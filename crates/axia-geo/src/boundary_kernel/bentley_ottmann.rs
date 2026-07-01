//! **Bentley-Ottmann sweep line** for boundary kernel intersection resolve.
//!
//! **ADR-186 Phase 3 β-4** — AixiAcad `boundary_kernel/bentley_ottmann.rs`
//! 1:1 faithful port (tracing 호출만 제거 — zero-dep kernel 유지).
//!
//! ## 목적
//!
//! O((N+K) log N) sweep-line intersection resolve — K = intersection count.
//! naive O(N²~N³) (edge pair × 매 intersection 재시작) 대비, 6+ Rectangle 누적
//! 시 hang 회피. pentagram 같은 closed-chain self-intersect 도 정확
//! (B4-fix: all-active pair check).
//!
//! ## 알고리즘 (B-O standard)
//!
//! ```text
//! 1. Edge endpoints → Event queue (X 기준 priority).
//! 2. Sweep line 이 event 들을 좌→우 순회.
//! 3. Active set (sweep line 과 교차하는 edges, Y sorted) 유지.
//! 4. 각 event 처리:
//!    - LeftEndpoint: active 에 insert + (B4-fix) 모든 active pair intersect check.
//!    - RightEndpoint: remove + 제거 후 새 인접 pair check.
//!    - Intersection: split + active 에서 swap + 새 인접 pair check.
//! ```

#![allow(dead_code)]

use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::hash::Hash;

use super::geom2::{seg_intersect, SegIsect, Vec2};
use super::planar::{EdgeId, Lineage, PlanarGraph};

// ════════════════════════════════════════════════════════════════
// Event — sweep line priority queue 항목
// ════════════════════════════════════════════════════════════════

/// Event 종류 (sweep direction = +X).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    /// Edge 의 왼쪽 endpoint 도달 — active set 에 insert.
    LeftEndpoint,
    /// Edge 두 개의 intersection — swap + new neighbor check.
    Intersection,
    /// Edge 의 오른쪽 endpoint 도달 — active set 에서 remove.
    /// (intersection 보다 *나중* 처리 — 같은 X 에서 RightEndpoint 가 마지막).
    RightEndpoint,
}

/// Sweep event.
///
/// Ordering: X 오름차순, tie-break Y 오름차순, tie-break kind (Left → Intersection → Right).
#[derive(Debug, Clone, Copy)]
pub struct Event {
    /// Sweep direction 의 위치 (sweep_x = self.p.x).
    pub p: Vec2,
    /// Event 종류.
    pub kind: EventKind,
    /// 관련 edge (primary).
    pub edge_a: EdgeId,
    /// Intersection 시 secondary edge. 그 외 `None`.
    pub edge_b: Option<EdgeId>,
}

impl Event {
    pub fn left_endpoint(edge_id: EdgeId, p: Vec2) -> Self {
        Self {
            p,
            kind: EventKind::LeftEndpoint,
            edge_a: edge_id,
            edge_b: None,
        }
    }
    pub fn right_endpoint(edge_id: EdgeId, p: Vec2) -> Self {
        Self {
            p,
            kind: EventKind::RightEndpoint,
            edge_a: edge_id,
            edge_b: None,
        }
    }
    pub fn intersection(ei: EdgeId, ej: EdgeId, p: Vec2) -> Self {
        Self {
            p,
            kind: EventKind::Intersection,
            edge_a: ei,
            edge_b: Some(ej),
        }
    }

    /// Total ordering key.
    fn cmp_key(&self) -> (f64, f64, EventKind, EdgeId, Option<EdgeId>) {
        (self.p.x, self.p.y, self.kind, self.edge_a, self.edge_b)
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.cmp_key() == other.cmp_key()
    }
}
impl Eq for Event {}
impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Event {
    fn cmp(&self, other: &Self) -> Ordering {
        let (ax, ay, ak, aea, aeb) = self.cmp_key();
        let (bx, by, bk, bea, beb) = other.cmp_key();
        // f64 → total_cmp (NaN handling 표준).
        ax.total_cmp(&bx)
            .then_with(|| ay.total_cmp(&by))
            .then(ak.cmp(&bk))
            .then(aea.cmp(&bea))
            .then(aeb.cmp(&beb))
    }
}

/// Min-heap wrapper — BinaryHeap 는 max-heap 이라 `Reverse` 로 변환.
pub type EventQueue = BinaryHeap<std::cmp::Reverse<Event>>;

// ════════════════════════════════════════════════════════════════
// ActiveEdge — sweep line 과 교차하는 edge 의 sorted set 항목
// ════════════════════════════════════════════════════════════════

/// Sweep line 위 active edge — Y at sweep_x 기준 sorted.
#[derive(Debug, Clone, Copy)]
pub struct ActiveEdge {
    pub edge_id: EdgeId,
    /// Left endpoint (smaller X).
    pub left: Vec2,
    /// Right endpoint (larger X).
    pub right: Vec2,
}

impl ActiveEdge {
    /// Edge 의 sweep_x 에서의 Y 좌표.
    ///
    /// Vertical edge (left.x == right.x) 의 경우 left.y 반환 (insert 시점 기준).
    pub fn y_at(&self, sweep_x: f64) -> f64 {
        let dx = self.right.x - self.left.x;
        if dx.abs() < 1e-12 {
            return self.left.y; // vertical
        }
        let t = (sweep_x - self.left.x) / dx;
        self.left.y + t * (self.right.y - self.left.y)
    }
}

// ════════════════════════════════════════════════════════════════
// ActiveSet — sweep line 활성 edge sorted Vec
// ════════════════════════════════════════════════════════════════

/// Sweep line 위 활성 edge 들의 Y-sorted vec.
#[derive(Debug, Default)]
pub struct ActiveSet {
    /// sweep_x 시점의 Y 오름차순 정렬.
    edges: Vec<ActiveEdge>,
}

impl ActiveSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Edge 를 sweep_x 기준 Y order 위치에 insert. 반환: 삽입 index.
    pub fn insert(&mut self, edge: ActiveEdge, sweep_x: f64) -> usize {
        let y = edge.y_at(sweep_x);
        let pos = self
            .edges
            .binary_search_by(|e| {
                e.y_at(sweep_x)
                    .total_cmp(&y)
                    .then(e.edge_id.cmp(&edge.edge_id))
            })
            .unwrap_or_else(|p| p);
        self.edges.insert(pos, edge);
        pos
    }

    /// Edge_id 로 찾아 제거. 반환: 제거된 index. 없으면 `None`.
    pub fn remove_by_id(&mut self, edge_id: EdgeId) -> Option<usize> {
        let pos = self.edges.iter().position(|e| e.edge_id == edge_id)?;
        self.edges.remove(pos);
        Some(pos)
    }

    /// Edge 의 위 (higher Y) 인접 edge.
    pub fn above(&self, index: usize) -> Option<&ActiveEdge> {
        self.edges.get(index + 1)
    }

    /// Edge 의 아래 (lower Y) 인접 edge.
    pub fn below(&self, index: usize) -> Option<&ActiveEdge> {
        if index == 0 {
            None
        } else {
            self.edges.get(index - 1)
        }
    }

    /// Edge_id 로 현재 index 찾기.
    pub fn find_index(&self, edge_id: EdgeId) -> Option<usize> {
        self.edges.iter().position(|e| e.edge_id == edge_id)
    }

    /// Index 로 edge 조회 (sweep main loop swap 후 neighbor check 용).
    pub fn get(&self, idx: usize) -> Option<&ActiveEdge> {
        self.edges.get(idx)
    }

    /// **B4-fix (2026-05-28)** — 모든 active edge snapshot (LeftEndpoint event 의
    /// all-active pair check 용). clone — 짧은 lived.
    pub fn snapshot(&self) -> Vec<ActiveEdge> {
        self.edges.clone()
    }

    /// 두 edge 의 sweep line 위 순서 swap (intersection event 처리 시).
    pub fn swap_by_ids(&mut self, ei: EdgeId, ej: EdgeId) -> bool {
        let pi = self.find_index(ei);
        let pj = self.find_index(ej);
        match (pi, pj) {
            (Some(i), Some(j)) => {
                self.edges.swap(i, j);
                true
            }
            _ => false,
        }
    }
}

// ════════════════════════════════════════════════════════════════
// Driver — initial event queue
// ════════════════════════════════════════════════════════════════

/// Input edge 들로부터 event queue 채우기.
pub fn build_initial_event_queue<V, F>(
    edges: impl IntoIterator<Item = (EdgeId, Vec2, Vec2)>,
    _make_vertex: &mut F,
) -> EventQueue
where
    V: Copy,
    F: FnMut(Vec2) -> V,
{
    let mut queue: EventQueue = BinaryHeap::new();
    for (edge_id, p1, p2) in edges {
        let (left, right) = if p1.x < p2.x || (p1.x == p2.x && p1.y < p2.y) {
            (p1, p2)
        } else {
            (p2, p1)
        };
        queue.push(Reverse(Event::left_endpoint(edge_id, left)));
        queue.push(Reverse(Event::right_endpoint(edge_id, right)));
    }
    queue
}

// ════════════════════════════════════════════════════════════════
// Sweep main loop + intersection detection
// ════════════════════════════════════════════════════════════════

/// Intersection 검출 결과 — split 은 caller 가 batch 로 호출.
#[derive(Debug, Clone, Copy)]
pub struct Intersection {
    /// 첫 edge id.
    pub edge_a: EdgeId,
    /// 두번째 edge id.
    pub edge_b: EdgeId,
    /// 교차점 좌표.
    pub p: Vec2,
}

/// 두 edge 사이 intersection check + event emit.
fn check_pair_and_emit(
    a: &ActiveEdge,
    b: &ActiveEdge,
    eps: f64,
    sweep_x: f64,
    events: &mut EventQueue,
    emitted: &mut std::collections::BTreeSet<(EdgeId, EdgeId)>,
) {
    // 인접 (공유 endpoint) edge 는 skip.
    if endpoint_shared(a, b, eps) {
        return;
    }
    let key = if a.edge_id < b.edge_id {
        (a.edge_id, b.edge_id)
    } else {
        (b.edge_id, a.edge_id)
    };
    if emitted.contains(&key) {
        return;
    }
    let isect = seg_intersect(a.left, a.right, b.left, b.right, eps);
    match isect {
        SegIsect::None => {}
        SegIsect::Point { p, .. } => {
            // sweep_x 이후 발생하는 intersection 만 event.
            if p.x >= sweep_x - eps {
                emitted.insert(key);
                events.push(Reverse(Event::intersection(a.edge_id, b.edge_id, p)));
            }
        }
        SegIsect::Overlap { p1, p2 } => {
            emitted.insert(key);
            // Overlap 양 끝점 모두 event emit.
            if p1.x >= sweep_x - eps {
                events.push(Reverse(Event::intersection(a.edge_id, b.edge_id, p1)));
            }
            if p2.x >= sweep_x - eps {
                events.push(Reverse(Event::intersection(a.edge_id, b.edge_id, p2)));
            }
        }
    }
}

fn endpoint_shared(a: &ActiveEdge, b: &ActiveEdge, eps: f64) -> bool {
    let close = |p: Vec2, q: Vec2| (p.x - q.x).abs() < eps && (p.y - q.y).abs() < eps;
    close(a.left, b.left) || close(a.left, b.right) || close(a.right, b.left) || close(a.right, b.right)
}

/// **B-O sweep** — Sweep line 으로 모든 intersection 검출 (split 안 함).
///
/// 시간복잡도: O((N+K) log N). 반환: 모든 unique intersection.
pub fn find_intersections_by_sweep<V: Copy + Ord + Hash>(g: &PlanarGraph<V>) -> Vec<Intersection> {
    let eps = g.eps;

    // 1. Initial events from edges.
    let mut events: EventQueue = BinaryHeap::new();
    for edge in g.edges.values() {
        let p1 = match g.vertices.get(&edge.a) {
            Some(v) => v.p,
            None => continue,
        };
        let p2 = match g.vertices.get(&edge.b) {
            Some(v) => v.p,
            None => continue,
        };
        let (left, right) = if p1.x < p2.x || (p1.x == p2.x && p1.y < p2.y) {
            (p1, p2)
        } else {
            (p2, p1)
        };
        events.push(Reverse(Event::left_endpoint(edge.id, left)));
        events.push(Reverse(Event::right_endpoint(edge.id, right)));
    }

    let mut active = ActiveSet::new();
    let mut result = Vec::new();
    let mut emitted: std::collections::BTreeSet<(EdgeId, EdgeId)> = std::collections::BTreeSet::new();

    while let Some(Reverse(event)) = events.pop() {
        let sweep_x = event.p.x;
        match event.kind {
            EventKind::LeftEndpoint => {
                let edge_id = event.edge_a;
                let edge = match g.edges.get(&edge_id) {
                    Some(e) => e,
                    None => continue,
                };
                let p1 = match g.vertices.get(&edge.a) {
                    Some(v) => v.p,
                    None => continue,
                };
                let p2 = match g.vertices.get(&edge.b) {
                    Some(v) => v.p,
                    None => continue,
                };
                let (left, right) = if p1.x < p2.x || (p1.x == p2.x && p1.y < p2.y) {
                    (p1, p2)
                } else {
                    (p2, p1)
                };
                let ae = ActiveEdge {
                    edge_id,
                    left,
                    right,
                };
                let _idx = active.insert(ae, sweep_x);
                // **B4-fix (2026-05-28)** — pentagram self-intersect 누락 시정.
                // 표준 B-O 의 인접 (위/아래) 만 check 는 closed chain 에서 일부
                // intersection 누락. 모든 active edge 와 pair check.
                let all_active = active.snapshot();
                for other in &all_active {
                    if other.edge_id == ae.edge_id {
                        continue;
                    }
                    check_pair_and_emit(&ae, other, eps, sweep_x, &mut events, &mut emitted);
                }
            }
            EventKind::RightEndpoint => {
                let edge_id = event.edge_a;
                if let Some(idx) = active.find_index(edge_id) {
                    let above = active.above(idx).copied();
                    let below = active.below(idx).copied();
                    active.remove_by_id(edge_id);
                    // 제거 후 새 인접 pair check.
                    if let (Some(a), Some(b)) = (above, below) {
                        check_pair_and_emit(&a, &b, eps, sweep_x, &mut events, &mut emitted);
                    }
                }
            }
            EventKind::Intersection => {
                let edge_b = match event.edge_b {
                    Some(b) => b,
                    None => continue,
                };
                result.push(Intersection {
                    edge_a: event.edge_a,
                    edge_b,
                    p: event.p,
                });
                // Active set 에서 두 edge swap.
                active.swap_by_ids(event.edge_a, edge_b);
                // Swap 후 새 인접 pair check.
                if let Some(i) = active.find_index(event.edge_a) {
                    if let Some(neighbor) = active.above(i).copied() {
                        let cur = *active.get(i).expect("edge_a still active after swap");
                        check_pair_and_emit(&cur, &neighbor, eps, sweep_x, &mut events, &mut emitted);
                    }
                    if let Some(neighbor) = active.below(i).copied() {
                        let cur = *active.get(i).expect("edge_a still active after swap");
                        check_pair_and_emit(&cur, &neighbor, eps, sweep_x, &mut events, &mut emitted);
                    }
                }
                if let Some(j) = active.find_index(edge_b) {
                    if let Some(neighbor) = active.above(j).copied() {
                        let cur = *active.get(j).expect("edge_b still active after swap");
                        check_pair_and_emit(&cur, &neighbor, eps, sweep_x, &mut events, &mut emitted);
                    }
                    if let Some(neighbor) = active.below(j).copied() {
                        let cur = *active.get(j).expect("edge_b still active after swap");
                        check_pair_and_emit(&cur, &neighbor, eps, sweep_x, &mut events, &mut emitted);
                    }
                }
            }
        }
    }
    result
}

/// **B-O resolve + B4-fixedpoint (2026-05-28)** — Sweep + batch split + fixed-point.
///
/// Intersection 검출 (sweep) → batch split → 반복 (split 으로 생긴 새 edge 의
/// 후속 intersection 도 검출). 0 intersection 도달 시 종료. safety bound 10.
pub fn bentley_ottmann_resolve<V, F>(g: &mut PlanarGraph<V>, lineage: &mut Lineage, mut make_vertex: F)
where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    const MAX_ITER: usize = 10;
    for _iter in 0..MAX_ITER {
        let intersections = find_intersections_by_sweep(g);
        if intersections.is_empty() {
            break;
        }
        for is in intersections {
            split_at_point_in_lineage(g, lineage, is.edge_a, is.p, &mut make_vertex);
            split_at_point_in_lineage(g, lineage, is.edge_b, is.p, &mut make_vertex);
        }
        // iter == MAX_ITER-1 도달 시 후속 intersection 누락 가능 (정상 case 1-3 iter).
    }
}

/// `edge_id` 또는 그 root 의 후손 중 `p` 가 internal 인 edge 를 찾아 split.
fn split_at_point_in_lineage<V, F>(
    g: &mut PlanarGraph<V>,
    lineage: &mut Lineage,
    edge_id: EdgeId,
    p: Vec2,
    next: &mut F,
) where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    let root_id = match g.edges.get(&edge_id) {
        Some(e) => e.root,
        None => {
            // 이미 split 된 edge_id — root 를 lineage 에서 찾기.
            lineage
                .map
                .iter()
                .find(|(_, set)| set.contains(&edge_id))
                .map(|(root, _)| *root)
                .unwrap_or(edge_id)
        }
    };
    let eps = g.eps;
    // root 의 후손 중 p 가 internal 위인 edge 찾기.
    let candidates: Vec<EdgeId> = g
        .edges
        .iter()
        .filter_map(|(eid, e)| if e.root == root_id { Some(*eid) } else { None })
        .collect();
    for eid in candidates {
        let e = match g.edges.get(&eid) {
            Some(x) => x.clone(),
            None => continue,
        };
        let a = match g.vertices.get(&e.a) {
            Some(v) => v.p,
            None => continue,
        };
        let b = match g.vertices.get(&e.b) {
            Some(v) => v.p,
            None => continue,
        };
        let (on, t) = super::geom2::point_on_segment(p, a, b, eps);
        if on && t > eps && t < 1.0 - eps {
            g.split_edge(eid, p, lineage, |q| next(q));
            return;
        }
    }
}

// ════════════════════════════════════════════════════════════════
// Tests — AixiAcad B2/B3 corpus 1:1 재현 (ADR-186 β-4).
// ════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b2_event_ordering_x_first() {
        let a = Event::left_endpoint(1, Vec2 { x: 0.0, y: 0.0 });
        let b = Event::left_endpoint(2, Vec2 { x: 1.0, y: 0.0 });
        assert!(a < b, "X 작은 event 가 작아야 한다 (min-heap 우선)");
    }

    #[test]
    fn b2_event_ordering_y_tie_break() {
        let a = Event::left_endpoint(1, Vec2 { x: 0.0, y: 0.0 });
        let b = Event::left_endpoint(2, Vec2 { x: 0.0, y: 1.0 });
        assert!(a < b);
    }

    #[test]
    fn b2_event_ordering_kind_tie_break() {
        let p = Vec2 { x: 0.0, y: 0.0 };
        let l = Event::left_endpoint(1, p);
        let i = Event::intersection(1, 2, p);
        let r = Event::right_endpoint(1, p);
        assert!(l < i);
        assert!(i < r);
    }

    #[test]
    fn b2_event_queue_pops_in_sweep_order() {
        let events = vec![
            Event::left_endpoint(1, Vec2 { x: 2.0, y: 0.0 }),
            Event::right_endpoint(2, Vec2 { x: 0.0, y: 1.0 }),
            Event::left_endpoint(3, Vec2 { x: 0.0, y: 0.0 }),
            Event::intersection(1, 2, Vec2 { x: 1.0, y: 0.5 }),
        ];
        let mut q: EventQueue = events.into_iter().map(std::cmp::Reverse).collect();
        let mut popped_x = Vec::new();
        while let Some(std::cmp::Reverse(e)) = q.pop() {
            popped_x.push(e.p.x);
        }
        assert_eq!(popped_x, vec![0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn b2_active_edge_y_at_horizontal() {
        let e = ActiveEdge {
            edge_id: 1,
            left: Vec2 { x: 0.0, y: 1.0 },
            right: Vec2 { x: 10.0, y: 1.0 },
        };
        assert!((e.y_at(0.0) - 1.0).abs() < 1e-12);
        assert!((e.y_at(5.0) - 1.0).abs() < 1e-12);
        assert!((e.y_at(10.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn b2_active_edge_y_at_sloped() {
        let e = ActiveEdge {
            edge_id: 1,
            left: Vec2 { x: 0.0, y: 0.0 },
            right: Vec2 { x: 10.0, y: 10.0 },
        };
        assert!((e.y_at(3.0) - 3.0).abs() < 1e-9);
        assert!((e.y_at(7.5) - 7.5).abs() < 1e-9);
    }

    #[test]
    fn b2_active_edge_y_at_vertical_returns_left_y() {
        let e = ActiveEdge {
            edge_id: 1,
            left: Vec2 { x: 5.0, y: 0.0 },
            right: Vec2 { x: 5.0, y: 10.0 },
        };
        assert!((e.y_at(5.0) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn b2_active_set_insert_keeps_y_order() {
        let mut s = ActiveSet::new();
        let e2 = ActiveEdge {
            edge_id: 2,
            left: Vec2 { x: 0.0, y: 5.0 },
            right: Vec2 { x: 10.0, y: 5.0 },
        };
        let e1 = ActiveEdge {
            edge_id: 1,
            left: Vec2 { x: 0.0, y: 2.0 },
            right: Vec2 { x: 10.0, y: 2.0 },
        };
        let e3 = ActiveEdge {
            edge_id: 3,
            left: Vec2 { x: 0.0, y: 8.0 },
            right: Vec2 { x: 10.0, y: 8.0 },
        };
        s.insert(e2, 5.0);
        s.insert(e3, 5.0);
        s.insert(e1, 5.0);
        assert_eq!(s.find_index(1), Some(0));
        assert_eq!(s.find_index(2), Some(1));
        assert_eq!(s.find_index(3), Some(2));
    }

    #[test]
    fn b2_active_set_neighbors() {
        let mut s = ActiveSet::new();
        for (eid, y) in [(1u32, 1.0), (2, 3.0), (3, 5.0)] {
            s.insert(
                ActiveEdge {
                    edge_id: eid,
                    left: Vec2 { x: 0.0, y },
                    right: Vec2 { x: 10.0, y },
                },
                5.0,
            );
        }
        assert_eq!(s.above(1).map(|e| e.edge_id), Some(3));
        assert_eq!(s.below(1).map(|e| e.edge_id), Some(1));
        assert!(s.above(2).is_none());
        assert!(s.below(0).is_none());
    }

    #[test]
    fn b2_active_set_remove_returns_index() {
        let mut s = ActiveSet::new();
        s.insert(
            ActiveEdge {
                edge_id: 1,
                left: Vec2 { x: 0.0, y: 1.0 },
                right: Vec2 { x: 10.0, y: 1.0 },
            },
            5.0,
        );
        s.insert(
            ActiveEdge {
                edge_id: 2,
                left: Vec2 { x: 0.0, y: 2.0 },
                right: Vec2 { x: 10.0, y: 2.0 },
            },
            5.0,
        );
        assert_eq!(s.remove_by_id(1), Some(0));
        assert_eq!(s.len(), 1);
        assert!(s.remove_by_id(99).is_none());
    }

    #[test]
    fn b2_active_set_swap_two_edges() {
        let mut s = ActiveSet::new();
        s.insert(
            ActiveEdge {
                edge_id: 1,
                left: Vec2 { x: 0.0, y: 1.0 },
                right: Vec2 { x: 10.0, y: 1.0 },
            },
            5.0,
        );
        s.insert(
            ActiveEdge {
                edge_id: 2,
                left: Vec2 { x: 0.0, y: 2.0 },
                right: Vec2 { x: 10.0, y: 2.0 },
            },
            5.0,
        );
        assert!(s.swap_by_ids(1, 2));
        assert_eq!(s.find_index(1), Some(1));
        assert_eq!(s.find_index(2), Some(0));
        assert!(!s.swap_by_ids(1, 99));
    }

    #[test]
    fn b2_build_initial_event_queue_emits_2_events_per_edge() {
        let edges = vec![
            (1, Vec2 { x: 0.0, y: 0.0 }, Vec2 { x: 10.0, y: 0.0 }),
            (2, Vec2 { x: 5.0, y: -5.0 }, Vec2 { x: 5.0, y: 5.0 }),
        ];
        let mut make_v = |_p: Vec2| 0u32;
        let q = build_initial_event_queue::<u32, _>(edges, &mut make_v);
        assert_eq!(q.len(), 4, "edge 2 개 → event 4 개 (Left + Right 각각)");
    }

    #[test]
    fn b2_build_initial_event_queue_left_is_smaller_x() {
        let edges = vec![(1, Vec2 { x: 10.0, y: 0.0 }, Vec2 { x: 0.0, y: 0.0 })];
        let mut make_v = |_p: Vec2| 0u32;
        let mut q = build_initial_event_queue::<u32, _>(edges, &mut make_v);
        let std::cmp::Reverse(first) = q.pop().unwrap();
        assert_eq!(first.kind, EventKind::LeftEndpoint);
        assert!((first.p.x - 0.0).abs() < 1e-12);
        let std::cmp::Reverse(second) = q.pop().unwrap();
        assert_eq!(second.kind, EventKind::RightEndpoint);
        assert!((second.p.x - 10.0).abs() < 1e-12);
    }

    #[test]
    fn b2_empty_event_queue_works() {
        let mut make_v = |_p: Vec2| 0u32;
        let q = build_initial_event_queue::<u32, _>(std::iter::empty(), &mut make_v);
        assert!(q.is_empty());
    }

    /// 두 edge 가 X 자로 한 점에서 교차 — Sweep 가 정확히 1 intersection 검출.
    #[test]
    fn b3_two_crossing_edges_emit_one_intersection() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 0u32;
        let mut next = |_p: Vec2| {
            counter += 1;
            counter
        };
        let v1 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 0.0 }, &mut next);
        let v2 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 10.0 }, &mut next);
        let v3 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 10.0 }, &mut next);
        let v4 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 0.0 }, &mut next);
        let _e1 = g.create_edge(v1, v2, None);
        let _e2 = g.create_edge(v3, v4, None);
        let intersections = find_intersections_by_sweep(&g);
        assert_eq!(intersections.len(), 1, "X 자 교차 = 1 intersection");
        let p = intersections[0].p;
        assert!((p.x - 5.0).abs() < 1e-6 && (p.y - 5.0).abs() < 1e-6);
    }

    #[test]
    fn b3_parallel_edges_no_intersection() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 0u32;
        let mut next = |_p: Vec2| {
            counter += 1;
            counter
        };
        let v1 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 0.0 }, &mut next);
        let v2 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 0.0 }, &mut next);
        let v3 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 5.0 }, &mut next);
        let v4 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 5.0 }, &mut next);
        let _e1 = g.create_edge(v1, v2, None);
        let _e2 = g.create_edge(v3, v4, None);
        let intersections = find_intersections_by_sweep(&g);
        assert!(intersections.is_empty());
    }

    #[test]
    fn b3_shared_endpoint_no_intersection() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 0u32;
        let mut next = |_p: Vec2| {
            counter += 1;
            counter
        };
        let v1 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 0.0 }, &mut next);
        let v2 = g.get_or_create_vertex(Vec2 { x: 5.0, y: 5.0 }, &mut next);
        let v3 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 0.0 }, &mut next);
        let _e1 = g.create_edge(v1, v2, None);
        let _e2 = g.create_edge(v2, v3, None);
        let intersections = find_intersections_by_sweep(&g);
        assert!(intersections.is_empty(), "공유 endpoint 는 intersection 아님");
    }

    #[test]
    fn b3_three_edges_three_intersections() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 0u32;
        let mut next = |_p: Vec2| {
            counter += 1;
            counter
        };
        let v1 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 0.0 }, &mut next);
        let v2 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 10.0 }, &mut next);
        let v3 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 10.0 }, &mut next);
        let v4 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 0.0 }, &mut next);
        let v5 = g.get_or_create_vertex(Vec2 { x: -1.0, y: 5.0 }, &mut next);
        let v6 = g.get_or_create_vertex(Vec2 { x: 11.0, y: 5.0 }, &mut next);
        let _e1 = g.create_edge(v1, v2, None);
        let _e2 = g.create_edge(v3, v4, None);
        let _e3 = g.create_edge(v5, v6, None);
        let intersections = find_intersections_by_sweep(&g);
        assert_eq!(intersections.len(), 3, "3 edge mutual intersect");
    }

    #[test]
    fn b3_resolve_splits_x_crossing_into_4_edges() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 0u32;
        let mut next = |_p: Vec2| {
            counter += 1;
            counter
        };
        let v1 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 0.0 }, &mut next);
        let v2 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 10.0 }, &mut next);
        let v3 = g.get_or_create_vertex(Vec2 { x: 0.0, y: 10.0 }, &mut next);
        let v4 = g.get_or_create_vertex(Vec2 { x: 10.0, y: 0.0 }, &mut next);
        let _e1 = g.create_edge(v1, v2, None);
        let _e2 = g.create_edge(v3, v4, None);
        let mut lineage = Lineage::default();
        for e in g.edges.values() {
            lineage.ensure_root(e.root);
        }
        bentley_ottmann_resolve(&mut g, &mut lineage, &mut next);
        assert_eq!(g.edges.len(), 4, "X 자 split 후 4 edge (실제 {})", g.edges.len());
        assert!(g.vertices.len() >= 5, "vertex >= 5 (4 corner + 1 center)");
    }

    #[test]
    fn b3_empty_graph_no_intersections() {
        let g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let intersections = find_intersections_by_sweep(&g);
        assert!(intersections.is_empty());
    }
}
