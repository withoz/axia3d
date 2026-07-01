//! PlanarGraph: 양자화 weld + edge split + lineage 추적.
//!
//! **ADR-186 Phase 3 β-2** — AixiAcad `boundary_kernel/planar.rs` (ADR-057)
//! 1:1 faithful port. geom2 만 의존, deterministic (BTreeMap), zero-dep.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::Hash;

use super::geom2::{point_almost_eq, Vec2};

/// Edge ID (kernel 내부 단조 증가).
pub type EdgeId = u32;

/// Vertex 항목.
#[derive(Clone, Debug)]
pub struct Vertex<V> {
    /// ID.
    pub id: V,
    /// 2D 좌표.
    pub p: Vec2,
}

/// Edge 항목.
#[derive(Clone, Debug)]
pub struct Edge<V> {
    /// ID.
    pub id: EdgeId,
    /// 시작 vertex.
    pub a: V,
    /// 끝 vertex.
    pub b: V,
    /// Root edge id — split 이전의 origin edge id (lineage tracking).
    pub root: EdgeId,
}

/// **Lineage**: root → descendants 매핑.
///
/// `split_edge` 발생 시 root에 left/right 추가. `has(root, eid)`로
/// edge가 특정 root의 후손인지 검사. robust_split의 3-branch
/// classification (sub-region vs containment) 결정에 핵심.
#[derive(Default, Clone, Debug)]
pub struct Lineage {
    /// root → 모든 후손 (root 포함).
    pub map: BTreeMap<EdgeId, BTreeSet<EdgeId>>,
}

impl Lineage {
    /// Root에 자기 자신만 포함된 entry 생성.
    pub fn ensure_root(&mut self, root: EdgeId) {
        self.map.entry(root).or_insert_with(|| {
            let mut s = BTreeSet::new();
            s.insert(root);
            s
        });
    }
    /// split 등록: old → left + right.
    pub fn register_split(&mut self, root: EdgeId, old: EdgeId, new_a: EdgeId, new_b: EdgeId) {
        self.ensure_root(root);
        let set = self.map.get_mut(&root).unwrap();
        set.insert(old);
        set.insert(new_a);
        set.insert(new_b);
    }
    /// edge가 root의 후손인지.
    pub fn has(&self, root: EdgeId, eid: EdgeId) -> bool {
        self.map.get(&root).map(|s| s.contains(&eid)).unwrap_or(false)
    }
    /// 어떤 root에라도 속하면 true.
    pub fn any_in_roots(&self, eid: EdgeId, roots: &[EdgeId]) -> bool {
        roots.iter().any(|r| self.has(*r, eid))
    }
}

/// **PlanarGraph<V>**: 양자화 weld 기반 vertex dedup + edge split.
///
/// ## Bug fix #2 (vs 원본): BTreeMap 사용
///
/// 원본은 `HashMap<EdgeId, Edge>` — iteration 순서 비결정적 → face id 발급
/// 비결정적. ADR-035 deterministic kernel invariant 위반.
/// `BTreeMap` 사용으로 EdgeId 오름차순 순회 보장.
///
/// V 는 vertex 식별자 타입 (`Copy + Ord + Hash`) — caller 가 mesh VertId 사용.
/// `get_or_create_vertex` / `split_edge` 는 caller closure `FnMut(Vec2) -> V`
/// 로 새 vid 생성 책임 (예: mesh.add_vertex). vid 가 1차 시민.
pub struct PlanarGraph<V: Copy + Ord + Hash> {
    /// 양자화 / 비교 eps.
    pub eps: f64,
    /// 다음 eid.
    eid: EdgeId,
    /// Vertex map (deterministic iteration).
    pub vertices: BTreeMap<V, Vertex<V>>,
    /// Edge map (deterministic iteration).
    pub edges: BTreeMap<EdgeId, Edge<V>>,
    /// 좌표 양자화 → V 인덱스 (dedup용).
    vkey: HashMap<(i64, i64), V>,
}

impl<V: Copy + Ord + Hash> PlanarGraph<V> {
    /// 새 PlanarGraph.
    pub fn new(eps: f64) -> Self {
        Self {
            eps,
            eid: 1,
            vertices: BTreeMap::new(),
            edges: BTreeMap::new(),
            vkey: HashMap::new(),
        }
    }

    fn key_of(&self, p: Vec2) -> (i64, i64) {
        let qx = (p.x / self.eps).round() as i64;
        let qy = (p.y / self.eps).round() as i64;
        (qx, qy)
    }

    /// 명시 V 등록 편의 helper.
    ///
    /// caller 가 V 직접 알고 있을 때 (예: V=mesh VertId — caller 가 이미
    /// mesh.add_vertex 로 만든 vid). vkey hit 시 기존 V 반환 (지정 vid 무시),
    /// miss 시 vid 등록.
    pub fn ensure_at(&mut self, vid: V, p: Vec2) -> V {
        self.get_or_create_vertex(p, |_| vid)
    }

    /// 좌표로 vertex 얻기 또는 새로 생성. 가까운 점은 양자화 weld dedup.
    ///
    /// vkey miss 시 caller closure `make(p)` 호출 → 새 vid 생성.
    /// vkey hit 시 closure 호출 안 함 (기존 V 반환).
    pub fn get_or_create_vertex<F: FnMut(Vec2) -> V>(&mut self, p: Vec2, mut make: F) -> V {
        let k = self.key_of(p);
        if let Some(&id) = self.vkey.get(&k) {
            return id;
        }
        let id = make(p);
        self.vertices.insert(id, Vertex { id, p });
        self.vkey.insert(k, id);
        id
    }

    /// Edge 생성. `root=None` 이면 자기 자신이 root.
    pub fn create_edge(&mut self, a: V, b: V, root: Option<EdgeId>) -> EdgeId {
        let id = self.eid;
        self.eid += 1;
        let root_id = root.unwrap_or(id);
        self.edges.insert(
            id,
            Edge {
                id,
                a,
                b,
                root: root_id,
            },
        );
        id
    }

    /// **겹침 엣지 정규화 (ADR-057 / error01 과병합 수정, 2026-06-01)**
    ///
    /// 같은 두 vertex 를 잇는 평행(중복) edge 를 1개로 병합. vertex 좌표는
    /// 전혀 건드리지 않는다 (no weld / no move).
    ///
    /// ## 배경
    ///
    /// 긴 collinear line 이 짧은 sketch_line 과 같은 구간을 덮을 때, 교차
    /// `SegIsect::Overlap` 분할과 T-junction 분할이 양쪽을 정확히 같은
    /// endpoint pair `{a,b}` 로 쪼갠다 → 중복 edge 가 남는다. region 추출의
    /// half-edge walk 는 이 중복을 degenerate 2-cycle (왕복 spur) 로 보고,
    /// 고차수 정점에서 인접 face 들을 하나의 self-touching cycle 로 잘못 엮는다
    /// (error01: 선 1개 삭제 → 인접 2면이 아닌 4면 결합). 우리 엔진의
    /// "면사라짐" 의 동일 메커니즘.
    ///
    /// 평면 분할의 face 추출에는 두 정점 사이 edge 가 최대 1개면 충분하므로,
    /// 중복 제거는 위상적으로 항상 안전하다.
    ///
    /// ## lineage 보존
    ///
    /// 유지되는 edge (최소 EdgeId) 를 제거되는 edge 의 root 후손으로도 등록 →
    /// robust_split 의 `any_in_roots` 기반 material/surface 상속 분류가 그대로
    /// 성립. 반환값은 제거한 edge 수.
    pub fn dedup_parallel_edges(&mut self, lineage: &mut Lineage) -> usize {
        let mut seen: BTreeMap<(V, V), EdgeId> = BTreeMap::new();
        let mut to_remove: Vec<EdgeId> = Vec::new();
        // edges 는 EdgeId 오름차순 (BTreeMap) → 항상 최소 EdgeId 를 유지 (결정적).
        for (&eid, e) in self.edges.iter() {
            let key = if e.a <= e.b { (e.a, e.b) } else { (e.b, e.a) };
            match seen.get(&key) {
                None => {
                    seen.insert(key, eid);
                }
                Some(&kept) => {
                    // 유지 edge 를 제거 edge 의 root 계보에도 등록.
                    let dropped_root = e.root;
                    lineage.ensure_root(dropped_root);
                    lineage.map.get_mut(&dropped_root).unwrap().insert(kept);
                    to_remove.push(eid);
                }
            }
        }
        let n = to_remove.len();
        for eid in to_remove {
            self.edges.remove(&eid);
        }
        n
    }

    /// Edge를 한 점에서 split → (left, right, mid_vid).
    ///
    /// `split_p`가 endpoint 가까우면 no-op으로 edge_id 그대로 반환 (`left==right==edge_id`).
    /// 그렇지 않으면 vkey 확인 → hit 시 기존 V 사용, miss 시 caller closure 호출.
    pub fn split_edge<F: FnMut(Vec2) -> V>(
        &mut self,
        edge_id: EdgeId,
        split_p: Vec2,
        lineage: &mut Lineage,
        make: F,
    ) -> (EdgeId, EdgeId, V) {
        let e = self.edges.get(&edge_id).cloned().expect("missing edge");
        let a = self.vertices.get(&e.a).unwrap().p;
        let b = self.vertices.get(&e.b).unwrap().p;
        // endpoint 가까운 경우 weld + no-op.
        if point_almost_eq(split_p, a, self.eps) {
            return (edge_id, edge_id, e.a);
        }
        if point_almost_eq(split_p, b, self.eps) {
            return (edge_id, edge_id, e.b);
        }
        let mid = self.get_or_create_vertex(split_p, make);
        self.edges.remove(&edge_id);
        let left = self.create_edge(e.a, mid, Some(e.root));
        let right = self.create_edge(mid, e.b, Some(e.root));
        lineage.register_split(e.root, edge_id, left, right);
        (left, right, mid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-186 β-2 — AixiAcad planar 회귀 4건 1:1 재현 (port 검증).

    /// V=u32 인스턴스에서 closure 로 dedup 검증.
    #[test]
    fn dedup_same_coord() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let a = g.get_or_create_vertex(Vec2::new(1.0, 2.0), &mut next);
        let b = g.get_or_create_vertex(Vec2::new(1.0, 2.0), &mut next);
        assert_eq!(a, b);
    }

    #[test]
    fn dedup_parallel_edges_collapses_same_pair() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let a = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let b = g.get_or_create_vertex(Vec2::new(10.0, 0.0), &mut next);
        let c = g.get_or_create_vertex(Vec2::new(10.0, 10.0), &mut next);
        let e_ab1 = g.create_edge(a, b, None);
        let _e_ab2 = g.create_edge(a, b, None); // 평행 중복.
        let _e_ba3 = g.create_edge(b, a, None); // 역방향도 같은 쌍.
        let e_bc = g.create_edge(b, c, None);
        assert_eq!(g.edges.len(), 4);
        let mut lineage = Lineage::default();
        let removed = g.dedup_parallel_edges(&mut lineage);
        assert_eq!(removed, 2, "a-b 중복 2개 제거 기대");
        assert_eq!(g.edges.len(), 2);
        // 최소 EdgeId 유지 (결정적).
        assert!(g.edges.contains_key(&e_ab1));
        assert!(g.edges.contains_key(&e_bc));
        // vertex 는 전혀 건드리지 않음.
        assert_eq!(g.vertices.len(), 3);
    }

    #[test]
    fn split_edge_creates_two() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let v0 = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let v1 = g.get_or_create_vertex(Vec2::new(10.0, 0.0), &mut next);
        let e = g.create_edge(v0, v1, None);
        let mut lin = Lineage::default();
        lin.ensure_root(e);
        let (la, lb, _) = g.split_edge(e, Vec2::new(5.0, 0.0), &mut lin, &mut next);
        assert_ne!(la, lb);
        assert!(!g.edges.contains_key(&e));
        assert!(g.edges.contains_key(&la));
        assert!(g.edges.contains_key(&lb));
        assert!(lin.has(e, la));
        assert!(lin.has(e, lb));
    }

    #[test]
    fn split_at_endpoint_is_noop() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let v0 = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let v1 = g.get_or_create_vertex(Vec2::new(10.0, 0.0), &mut next);
        let e = g.create_edge(v0, v1, None);
        let mut lin = Lineage::default();
        let (la, lb, mid) = g.split_edge(e, Vec2::new(0.0, 0.0), &mut lin, &mut next);
        assert_eq!(la, e);
        assert_eq!(lb, e);
        assert_eq!(mid, v0);
    }
}
