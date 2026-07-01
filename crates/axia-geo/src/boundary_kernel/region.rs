//! Half-edge 기반 region (face) 추출.
//!
//! **ADR-186 Phase 3 β-3** — AixiAcad `boundary_kernel/region.rs` (ADR-057 +
//! L-P1 containment nesting) 1:1 faithful port. geom2 + planar 의존.
//! 유도면 모델의 핵심 — edge graph → 모든 면 + containment hole 자동 부착.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::hash::Hash;

use super::geom2::{
    eps_from_scale, orient2d_sign, point_in_polygon_even_odd, polygon_centroid,
    polygon_signed_area, Pip, Vec2,
};
use super::planar::{EdgeId, PlanarGraph};

/// 평면 graph에서 추출된 face (single-loop boundary).
#[derive(Clone, Debug)]
pub struct Region<V = u32> {
    /// Boundary vertex 시퀀스 (CCW order for interior face).
    pub verts: Vec<V>,
    /// Boundary edge 시퀀스 (각 edge가 verts[i] → verts[i+1]).
    pub edges: Vec<EdgeId>,
    /// Signed area (CCW positive, CW negative).
    pub signed_area: f64,
    /// 중심점 (centroid).
    pub centroid: Vec2,
}

/// **L-P1 (2026-06-01)** — 구멍(hole)을 가진 면.
///
/// 닫힌 곡선(원 등)이 다른 면 내부에 완전히 포함되면, 바깥 면은 그 곡선을
/// inner-loop(구멍)으로 갖는 annulus 가 되고, 곡선 내부는 별도 면(disk)이 된다.
/// `outer` 는 CCW 외곽 boundary, `holes` 는 즉시 포함된 자식 region(곡선 boundary).
///
/// `extract_regions` 가 disconnected 컴포넌트(예: 사각형 안의 원)를 평면적으로
/// 추출하면 바깥 region 이 자식을 빼지 않은 *전체* 영역으로 나온다 — `holes` 로
/// 그 차집합을 표현한다 (DCEL `FaceData.inner_loops` 와 대응).
#[derive(Clone, Debug)]
pub struct RegionWithHoles<V = u32> {
    /// 외곽 면 (CCW).
    pub outer: Region<V>,
    /// 즉시 포함된 구멍 region (각각 자체로도 별도 면이 됨).
    pub holes: Vec<Region<V>>,
}

#[derive(Clone, Debug)]
struct HalfEdge<V> {
    from: V,
    to: V,
    edge_id: EdgeId,
    angle: f64,
    twin: usize,
    next: usize,
    used: bool,
}

fn angle_of(a: Vec2, b: Vec2) -> f64 {
    (b.y - a.y).atan2(b.x - a.x)
}

fn build_half_edges<V: Copy + Ord + Hash>(g: &PlanarGraph<V>) -> Vec<HalfEdge<V>> {
    let mut half = Vec::new();
    // **Bug fix #2**: BTreeMap iteration (deterministic).
    for e in g.edges.values() {
        let a = g.vertices.get(&e.a).unwrap().p;
        let b = g.vertices.get(&e.b).unwrap().p;
        let id0 = half.len();
        let id1 = id0 + 1;
        half.push(HalfEdge {
            from: e.a,
            to: e.b,
            edge_id: e.id,
            angle: angle_of(a, b),
            twin: id1,
            next: usize::MAX,
            used: false,
        });
        half.push(HalfEdge {
            from: e.b,
            to: e.a,
            edge_id: e.id,
            angle: angle_of(b, a),
            twin: id0,
            next: usize::MAX,
            used: false,
        });
    }
    half
}

fn compute_next_pointers<V: Copy + Ord + Hash>(g: &PlanarGraph<V>, half: &mut [HalfEdge<V>]) {
    // **Bug fix #2**: BTreeMap for deterministic iteration.
    let mut outgoing: BTreeMap<V, Vec<usize>> = BTreeMap::new();
    for (i, h) in half.iter().enumerate() {
        outgoing.entry(h.from).or_default().push(i);
    }
    // **ADR-187 β-3** — robust angular sort. atan2(f64) 의 near-collinear 부동
    // 소수 오정렬이 face cycle traversal 을 틀어지게 함 (면사라짐/오분할 잠재
    // 근원). half-plane 분류 + 같은 half-plane 은 orient2d_sign (Shewchuk exact
    // 부호) → CCW 각도 순서 *exact*. non-degenerate 는 atan2 와 동일 순환 순서
    // (38 회귀 보존), near-collinear 만 정확히 정정 (메타-원칙 #15 빠름+정확).
    for arr in outgoing.values_mut() {
        arr.sort_by(|&i, &j| robust_angular_cmp(g, &half[i], &half[j]));
    }
    for i in 0..half.len() {
        let to = half[i].to;
        let twin = half[i].twin;
        let arr = outgoing.get(&to).unwrap();
        let idx = arr.iter().position(|&x| x == twin).unwrap();
        // CCW predecessor of twin → next (keeps face on left).
        let next_idx = if idx == 0 { arr.len() - 1 } else { idx - 1 };
        half[i].next = arr[next_idx];
    }
}

/// **ADR-187 β-3** — 두 half-edge 를 공통 origin 기준 CCW 각도 순서로 비교 (robust).
///
/// half-plane (upper/lower) 분류 후 같은 half-plane 은 `orient2d_sign` (exact 부호)
/// 으로 정렬. atan2 의 near-collinear 오정렬 차단 → face cycle topology 정확.
/// non-degenerate 는 atan2 와 동일 CCW 순환 순서 (시작점만 +x 로 다름, 순환
/// predecessor/successor 동일 → 기존 동작 보존).
fn robust_angular_cmp<V: Copy + Ord + Hash>(
    g: &PlanarGraph<V>,
    hi: &HalfEdge<V>,
    hj: &HalfEdge<V>,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let center = g.vertices[&hi.from].p; // hi.from == hj.from (동일 outgoing 정점)
    let ti = g.vertices[&hi.to].p;
    let tj = g.vertices[&hj.to].p;
    let ha = half_plane(ti.sub(center));
    let hb = half_plane(tj.sub(center));
    if ha != hb {
        return ha.cmp(&hb);
    }
    // 같은 half-plane: orient2d 부호. tj 가 ray center→ti 의 좌(CCW) 면 ti 가
    // 더 작은 각도 → ti 먼저 (Less).
    match orient2d_sign(center, ti, tj) {
        1 => Ordering::Less,
        -1 => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

/// 방향 벡터의 half-plane: 0 = upper (angle ∈ [0, π)), 1 = lower (angle ∈ [π, 2π)).
/// +x 축(dy=0,dx>0) = upper, -x 축(dy=0,dx<0) = lower — atan2 ascending CCW 순환
/// 순서와 정합.
fn half_plane(d: Vec2) -> u8 {
    if d.y > 0.0 || (d.y == 0.0 && d.x > 0.0) {
        0
    } else {
        1
    }
}

/// 모든 planar face cycle 추출.
///
/// 결과는 interior + outer 모두 포함. 호출자가 signed_area로 필터.
pub fn extract_regions<V: Copy + Ord + Hash>(g: &PlanarGraph<V>, eps_area: f64) -> Vec<Region<V>> {
    let mut half = build_half_edges(g);
    compute_next_pointers(g, &mut half);
    let mut regions = Vec::new();
    for start in 0..half.len() {
        if half[start].used {
            continue;
        }
        let mut verts = Vec::new();
        let mut edges = Vec::new();
        let mut cur = start;
        let mut safe = 0usize;
        while safe < half.len() + 8 {
            safe += 1;
            if half[cur].used {
                break;
            }
            half[cur].used = true;
            verts.push(half[cur].from);
            edges.push(half[cur].edge_id);
            cur = half[cur].next;
            if cur == start {
                break;
            }
        }
        if verts.len() < 3 {
            continue;
        }
        let poly: Vec<Vec2> = verts
            .iter()
            .map(|v| g.vertices.get(v).unwrap().p)
            .collect();
        let area = polygon_signed_area(&poly);
        if area.abs() <= eps_area {
            continue;
        }
        let c = polygon_centroid(&poly);
        regions.push(Region {
            verts,
            edges,
            signed_area: area,
            centroid: c,
        });
    }
    // **P5.UX.63 Patch A (2026-05-19)** — Self-touching polygon split.
    //
    // deg(v) ≥ 4 인 vertex (4 사각형이 한 점에 모이는 등)에서 half-edge walk가
    // figure-8 형태의 cycle을 emit할 수 있다. verts 배열에 같은 V가 두 번
    // 등장 → CDT inside-test가 pinch point 안쪽 wedge 영역을 잘못 inside로 판정 →
    // wedge 가로지르는 삼각형이 살아남아 시각 artifact (쐐기).
    //
    // 정합 원칙: extract_regions가 emit하는 모든 Region은 simple polygon이어야 한다.
    // self-touching cycle은 vertex 분기점에서 sub-polygon으로 분할.
    regions
        .into_iter()
        .flat_map(|r| split_self_touching(r, g))
        .filter(|r| r.signed_area.abs() > eps_area)
        .collect()
}

/// **SPIKE fix (2026-06-02)** — vertex connected-component (BFS).
///
/// hole(구멍) 은 *disconnected* 컴포넌트(면 안의 별도 닫힌 루프)에서만 발생해야
/// 한다. 겹친(overlap) 면들은 교차점에서 vertex 를 공유 → **같은 컴포넌트** =
/// 인접(adjacent) → hole 아님. centroid 포함만으로 판정하면 비볼록(L자) 면이
/// 인접 면의 centroid 를 품어 *가짜 hole* → 면 중복 → non-manifold. component
/// 검사로 차단.
fn connected_component_of<V: Copy + Ord + Hash>(g: &PlanarGraph<V>) -> BTreeMap<V, usize> {
    let mut adj: BTreeMap<V, Vec<V>> = BTreeMap::new();
    for v in g.vertices.keys() {
        adj.entry(*v).or_default();
    }
    for e in g.edges.values() {
        adj.entry(e.a).or_default().push(e.b);
        adj.entry(e.b).or_default().push(e.a);
    }
    let mut comp: BTreeMap<V, usize> = BTreeMap::new();
    let mut cid = 0usize;
    for &start in g.vertices.keys() {
        if comp.contains_key(&start) {
            continue;
        }
        let mut stack = vec![start];
        comp.insert(start, cid);
        while let Some(v) = stack.pop() {
            if let Some(neighbors) = adj.get(&v) {
                let ns: Vec<V> = neighbors.clone();
                for w in ns {
                    if !comp.contains_key(&w) {
                        comp.insert(w, cid);
                        stack.push(w);
                    }
                }
            }
        }
        cid += 1;
    }
    comp
}

/// **L-P1 (2026-06-01)** — region 을 containment-nesting 해 구멍(hole)을 유도.
///
/// `extract_regions` 의 결과 중 **interior(CCW, signed_area > 0)** region 만 면 후보로
/// 삼고, 서로의 centroid 포함관계로 즉시-부모(immediate parent)를 계산한다:
///   - 각 region 은 그대로 자기 자신의 면(`outer`)이 된다.
///   - region X 의 즉시-부모가 P 이면, X 의 boundary 는 P 의 `holes` 로 등록된다.
///
/// 연결된(connected) 일반 분할에서는 nested interior 가 생기지 않으므로(면들이
/// edge 로 분리됨) 모든 region 의 hole 이 비어 = 기존 동작과 동일. nested interior 는
/// **disconnected 컴포넌트**(면 내부의 닫힌 곡선)에서만 발생한다.
///
/// 예: 사각형 + 내부 원 → `[{outer: 사각형, holes: [원]}, {outer: 원, holes: []}]`
/// = annulus(사각형−원) + disk(원). SketchUp 동작과 일치.
pub fn extract_regions_nested<V: Copy + Ord + Hash>(
    g: &PlanarGraph<V>,
    eps_area: f64,
) -> Vec<RegionWithHoles<V>> {
    let all = extract_regions(g, eps_area);
    let comp = connected_component_of(g);
    let comp_of = |r: &Region<V>| -> usize {
        r.verts.first().and_then(|v| comp.get(v).copied()).unwrap_or(usize::MAX)
    };
    // CCW(interior, signed_area > 0) region = 실제 면.
    let ccw: Vec<Region<V>> = all
        .iter()
        .filter(|r| r.signed_area > eps_area)
        .cloned()
        .collect();
    let ccw_polys: Vec<Vec<Vec2>> = ccw
        .iter()
        .map(|r| r.verts.iter().map(|v| g.vertices.get(v).unwrap().p).collect())
        .collect();
    let ccw_eps: Vec<f64> = ccw_polys.iter().map(|p| eps_from_scale(p)).collect();
    let ccw_comp: Vec<usize> = ccw.iter().map(|r| comp_of(r)).collect();

    let mut result: Vec<RegionWithHoles<V>> = ccw
        .iter()
        .cloned()
        .map(|outer| RegionWithHoles { outer, holes: Vec::new() })
        .collect();

    // SPIKE fix v2 (2026-06-02) — hole 은 **CW cycle** (signed_area < 0 =
    // disconnected 컴포넌트의 outer 경계). 다른 컴포넌트의 *더 큰* CCW 면 안에
    // 있으면 그 면의 hole. 컴포넌트 outline **1개만** hole (각 내부 sub-face 아님
    // → 포함된 blob 의 non-manifold 차단). global unbounded CW 는 자기보다 큰
    // CCW 면이 없어 hole 안 됨. 연결 겹침은 inner CW cycle 자체가 없어 hole 0.
    for h in all.iter().filter(|r| r.signed_area < -eps_area) {
        let h_comp = comp_of(h);
        let h_area = -h.signed_area; // |signed_area|
        // h.centroid 는 CW(음수 area) 에서 부호가 틀림 (polygon_centroid 가 음수
        // area 로 나눠 negate). vertex 평균(orientation-무관)으로 신뢰 가능한 내부점.
        let pt = {
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut cnt = 0.0;
            for v in &h.verts {
                if let Some(vx) = g.vertices.get(v) {
                    sx += vx.p.x;
                    sy += vx.p.y;
                    cnt += 1.0;
                }
            }
            if cnt > 0.0 {
                Vec2::new(sx / cnt, sy / cnt)
            } else {
                h.centroid
            }
        };
        let mut best: Option<usize> = None;
        for j in 0..ccw.len() {
            // 같은 컴포넌트(=hole 자신의 면) OR hole 보다 작거나 같은 면(unbounded
            // 차단) → 부모 아님.
            if ccw_comp[j] == h_comp || ccw[j].signed_area <= h_area {
                continue;
            }
            if point_in_polygon_even_odd(pt, &ccw_polys[j], ccw_eps[j]) == Pip::Inside {
                // 더 작은(=더 안쪽) 부모 우선.
                best = match best {
                    Some(b) if ccw[b].signed_area <= ccw[j].signed_area => Some(b),
                    _ => Some(j),
                };
            }
        }
        if let Some(j) = best {
            result[j].holes.push(h.clone());
        }
    }
    result
}

/// **P5.UX.63 Patch A** — self-touching cycle을 simple sub-polygon으로 분할.
///
/// verts 배열에서 같은 V가 [a..b) 구간에 두 번 등장하면 그 구간을 sub-loop로
/// 분리 + 본체에서 제거. 본체에 또 self-touch가 있으면 반복.
fn split_self_touching<V: Copy + Ord + Hash>(region: Region<V>, g: &PlanarGraph<V>) -> Vec<Region<V>> {
    let mut out = Vec::new();
    let mut work = region;
    loop {
        // verts에서 같은 vid의 첫 등장 위치 표.
        let mut first_seen: BTreeMap<V, usize> = BTreeMap::new();
        let mut found_split: Option<(usize, usize)> = None;
        for (i, vid) in work.verts.iter().enumerate() {
            if let Some(&prev) = first_seen.get(vid) {
                found_split = Some((prev, i));
                break;
            }
            first_seen.insert(*vid, i);
        }
        match found_split {
            None => {
                // simple polygon — 그대로 emit.
                out.push(work);
                return out;
            }
            Some((a, b)) => {
                // verts[a..b] = 한 sub-loop (a vertex가 시작점, b에서 다시 등장).
                let sub_verts: Vec<_> = work.verts[a..b].to_vec();
                let sub_edges: Vec<_> = work.edges[a..b].to_vec();
                out.push(rebuild_region(sub_verts, sub_edges, g));
                // 본체에서 a..b drain — 본체는 verts[..a] + verts[b..] 이어붙임.
                work.verts.drain(a..b);
                work.edges.drain(a..b);
                if work.verts.len() < 3 {
                    return out;
                }
                // 본체 area/centroid 재계산은 다음 iteration의 final emit에서 처리.
                work = rebuild_region(work.verts, work.edges, g);
            }
        }
    }
}

/// Sub-polygon Region 재구성 — signed_area + centroid를 verts 좌표에서 재계산.
fn rebuild_region<V: Copy + Ord + Hash>(
    verts: Vec<V>,
    edges: Vec<EdgeId>,
    g: &PlanarGraph<V>,
) -> Region<V> {
    let poly: Vec<Vec2> = verts
        .iter()
        .map(|v| g.vertices.get(v).unwrap().p)
        .collect();
    let signed_area = polygon_signed_area(&poly);
    let centroid = polygon_centroid(&poly);
    Region {
        verts,
        edges,
        signed_area,
        centroid,
    }
}

#[cfg(test)]
mod tests {
    use super::super::planar::Lineage;
    use super::*;

    // ADR-186 β-3 — AixiAcad region 회귀 5건 1:1 재현 (port 검증).
    // 핵심: nested_loop / three_level / connected_split = 유도면 모델의 containment.

    /// **ADR-057 과병합 수정 (2026-06-01)** — 평행 edge 정규화 + region 정합.
    ///
    /// 중복 edge 가 정확히 주입한 만큼 제거되는지도 함께 확인 (정규화 동작).
    #[test]
    fn dedup_then_extract_regions_topology() {
        let verts: [(u32, f64, f64); 8] = [
            (26, -244.0, -1027.0),
            (27, -365.0, -1027.0),
            (48, -190.0, -1124.0),
            (49, -190.0, -1107.0),
            (5, -443.0, -727.0),
            (4, -1985.0, -118.0),
            (51, -190.0, -1027.0),
            (52, -190.0, -827.0),
        ];
        // 기본 10 edge (26-49 제외) + 마지막에 26-49.
        let edges: [(u32, u32); 11] = [
            (26, 27),
            (27, 48),
            (48, 49),
            (26, 5),
            (5, 4),
            (4, 27),
            (26, 51),
            (51, 52),
            (52, 5),
            (49, 51),
            (26, 49),
        ];
        // 긴 collinear line 분할이 남긴 평행 중복 edge (26-27, 26-51 위).
        let parallel: [(u32, u32); 2] = [(26, 27), (26, 51)];

        // skip_last=true → 26-49 삭제. (parallel + dedup + extract).
        let build = |skip_last: bool| -> (usize, usize) {
            let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-6);
            for &(id, x, y) in &verts {
                let _ = g.get_or_create_vertex(Vec2::new(x, y), |_p| id);
            }
            let n = if skip_last { edges.len() - 1 } else { edges.len() };
            for &(a, b) in &edges[..n] {
                g.create_edge(a, b, None);
            }
            for &(a, b) in &parallel {
                g.create_edge(a, b, None); // 평행 중복 edge 주입.
            }
            let edges_before = g.edges.len();
            let mut lineage = Lineage::default();
            let removed = g.dedup_parallel_edges(&mut lineage);
            // 주입한 평행 edge 가 모두 제거되어야 한다.
            assert_eq!(
                removed,
                parallel.len(),
                "주입한 평행 edge {} 개 제거 기대 (실제 {removed}, edges_before={edges_before})",
                parallel.len()
            );
            let faces = extract_regions(&g, 1.0)
                .iter()
                .filter(|r| r.signed_area > 1.0)
                .count();
            (faces, removed)
        };

        let (before, _) = build(false);
        let (after, _) = build(true);
        assert_eq!(before, 4, "dedup 후 전체 edge: 4 interior 기대 (실제 {before})");
        assert_eq!(
            after, 3,
            "dedup 후 26-49 삭제 → 3 interior (drop 1) 정답. 실제 {after}"
        );
    }

    /// **L-P1 corpus** — closed-loop helper: CCW square 를 PlanarGraph 에 추가.
    /// vid 는 호출자 base 부터 4개 사용. (lo,hi) = 좌하/우상 코너.
    fn add_square(g: &mut PlanarGraph<u32>, base: u32, lo: (f64, f64), hi: (f64, f64)) {
        let pts = [
            (lo.0, lo.1),
            (hi.0, lo.1),
            (hi.0, hi.1),
            (lo.0, hi.1),
        ];
        let mut ids = [0u32; 4];
        for (k, &(x, y)) in pts.iter().enumerate() {
            let id = base + k as u32;
            ids[k] = g.get_or_create_vertex(Vec2::new(x, y), |_p| id);
        }
        for k in 0..4 {
            g.create_edge(ids[k], ids[(k + 1) % 4], None);
        }
    }

    /// **L-P1**: 사각형 + 내부의 disconnected 사각형(원 대용 닫힌 루프) →
    /// nesting 이 annulus(hole 1개) + inner disk(hole 0개) 2면을 만든다.
    #[test]
    fn nested_loop_creates_one_hole() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        add_square(&mut g, 0, (0.0, 0.0), (100.0, 100.0)); // outer
        add_square(&mut g, 10, (40.0, 40.0), (60.0, 60.0)); // inner (disconnected)
        let faces = extract_regions_nested(&g, 1e-6);
        assert_eq!(faces.len(), 2, "두 면(annulus + disk) 기대: {}", faces.len());
        // 큰 면 = annulus (hole 1), 작은 면 = disk (hole 0).
        let outer = faces
            .iter()
            .max_by(|a, b| a.outer.signed_area.partial_cmp(&b.outer.signed_area).unwrap())
            .unwrap();
        let inner = faces
            .iter()
            .min_by(|a, b| a.outer.signed_area.partial_cmp(&b.outer.signed_area).unwrap())
            .unwrap();
        assert_eq!(outer.holes.len(), 1, "바깥 면은 구멍 1개");
        assert_eq!(inner.holes.len(), 0, "안쪽 disk 는 구멍 0개");
        // hole 면적 ≈ inner 면적 (20×20=400).
        assert!((outer.holes[0].signed_area.abs() - 400.0).abs() < 1e-6);
    }

    /// **L-P1**: 3단 중첩 (100 > 원 > 작은사각형) → 즉시-부모만 hole 로 귀속.
    /// 가장 작은 사각형은 *원*의 구멍이지 100사각형의 구멍이 아님.
    #[test]
    fn three_level_nesting_immediate_parent_only() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        add_square(&mut g, 0, (0.0, 0.0), (100.0, 100.0)); // L0
        add_square(&mut g, 10, (30.0, 30.0), (70.0, 70.0)); // L1
        add_square(&mut g, 20, (45.0, 45.0), (55.0, 55.0)); // L2
        let mut faces = extract_regions_nested(&g, 1e-6);
        assert_eq!(faces.len(), 3);
        faces.sort_by(|a, b| b.outer.signed_area.partial_cmp(&a.outer.signed_area).unwrap());
        // L0 의 구멍은 L1 1개 (면적 1600), L2 아님.
        assert_eq!(faces[0].holes.len(), 1, "L0 구멍 1");
        assert!((faces[0].holes[0].signed_area.abs() - 1600.0).abs() < 1e-6);
        // L1 의 구멍은 L2 1개 (면적 100).
        assert_eq!(faces[1].holes.len(), 1, "L1 구멍 1");
        assert!((faces[1].holes[0].signed_area.abs() - 100.0).abs() < 1e-6);
        // L2 는 구멍 0.
        assert_eq!(faces[2].holes.len(), 0, "L2 구멍 0");
    }

    /// **L-P1**: connected 분할(공유 edge 로 나뉜 두 면)은 nesting 없음 — 기존 동작 유지.
    #[test]
    fn connected_split_has_no_holes() {
        // 사각형을 수직 chord 로 둘로 — 두 면 모두 연결, 포함관계 없음.
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let p = |x: f64, y: f64, id: u32, g: &mut PlanarGraph<u32>| {
            g.get_or_create_vertex(Vec2::new(x, y), |_q| id)
        };
        let a = p(0.0, 0.0, 1, &mut g);
        let b = p(50.0, 0.0, 2, &mut g);
        let c = p(100.0, 0.0, 3, &mut g);
        let d = p(100.0, 100.0, 4, &mut g);
        let e = p(50.0, 100.0, 5, &mut g);
        let f = p(0.0, 100.0, 6, &mut g);
        for &(u, v) in &[(a, b), (b, c), (c, d), (d, e), (e, f), (f, a), (b, e)] {
            g.create_edge(u, v, None);
        }
        let faces = extract_regions_nested(&g, 1e-6);
        assert_eq!(faces.len(), 2, "두 면");
        assert!(faces.iter().all(|f| f.holes.is_empty()), "구멍 없어야");
    }

    /// **ADR-187 β-3** — near-collinear (thin sliver) 삼각형도 robust 각도 정렬로
    /// 정확히 1 interior 면 추출. atan2 부동소수가 흔들리는 near-collinear 에서
    /// face cycle traversal 보존 (robust orient2d_sign 기반 각도 비교).
    #[test]
    fn robust_angular_sort_thin_sliver() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut c = 1u32;
        let mut next = |_p: Vec2| {
            let v = c;
            c += 1;
            v
        };
        let a = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let b = g.get_or_create_vertex(Vec2::new(1000.0, 0.0), &mut next);
        // a→b 와 near-collinear (angle ~0.0003 rad).
        let cc = g.get_or_create_vertex(Vec2::new(1000.0, 0.3), &mut next);
        g.create_edge(a, b, None);
        g.create_edge(b, cc, None);
        g.create_edge(cc, a, None);
        let regions = extract_regions(&g, 1e-9);
        let interior = regions.iter().filter(|r| r.signed_area > 1e-9).count();
        assert_eq!(interior, 1, "thin sliver = 1 interior 면: {}", interior);
    }

    /// **A0.5.1**: V=u32 default 인스턴스에서 quad 추출 검증.
    #[test]
    fn quad_extracts_one_interior_face() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let v0 = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let v1 = g.get_or_create_vertex(Vec2::new(1.0, 0.0), &mut next);
        let v2 = g.get_or_create_vertex(Vec2::new(1.0, 1.0), &mut next);
        let v3 = g.get_or_create_vertex(Vec2::new(0.0, 1.0), &mut next);
        g.create_edge(v0, v1, None);
        g.create_edge(v1, v2, None);
        g.create_edge(v2, v3, None);
        g.create_edge(v3, v0, None);
        let _ = Lineage::default();
        let regions = extract_regions(&g, 1e-12);
        // 두 region: interior CCW + outer CW.
        assert!(regions.len() >= 1);
        // 적어도 하나는 area 1.0 ± eps.
        let area_ok = regions.iter().any(|r| (r.signed_area.abs() - 1.0).abs() < 1e-9);
        assert!(area_ok, "regions: {:?}", regions);
    }
}
