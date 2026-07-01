//! Robust 2D planar split (3-branch classification + multi-split fix).
//!
//! **ADR-186 Phase 3 β-4** — AixiAcad `boundary_kernel/robust_split.rs` 1:1
//! faithful port (tracing 호출만 제거 — zero-dep kernel 유지). 유도면 모델의
//! XIA 상속 layer — re-derive 한 region 을 부모 face 의 material/surface 로
//! 매핑 (3-branch: shares_edge / all_inside / centroid-fluke).

#![allow(dead_code)]

use std::hash::Hash;

use super::geom2::{
    all_inside, point_in_polygon_even_odd, point_on_segment, seg_intersect, Pip, SegIsect, Vec2,
};
use super::planar::{EdgeId, Lineage, PlanarGraph};
use super::region::{extract_regions, Region};

/// Unbind 전의 face snapshot.
///
/// 사용자 가시 face의 material / surface / boundary polygon 보존.
/// 결과 region이 이 polygon 안에 있으면 inheritance 결정 토대.
#[derive(Clone)]
pub struct DirtyFaceInfo<TMat: Clone, TSurf: Clone> {
    /// 재료 ID.
    pub material: TMat,
    /// 표면 정보.
    pub surface: TSurf,
    /// 원본 boundary polygon (single loop).
    pub orig_polygon: Vec<Vec2>,
    /// 원본 boundary edge들의 root EdgeId (split 추적 토대).
    pub orig_edge_roots: Vec<EdgeId>,
    /// 원본 polygon area.
    pub area: f64,
}

/// Robust split 결과 face — engine-friendly.
///
/// `boundary_vids: Vec<V>` 가 1차 source. boundary_positions / boundary_vids
/// 셋 다 같은 length, 같은 순서. V=mesh VertId 일 때 boundary_vids 가 자기
/// 자신이 mesh vid.
#[derive(Clone)]
pub struct FaceOut<TMat: Clone, TSurf: Clone, V> {
    /// Boundary 2D 좌표 (CCW interior).
    pub boundary_positions: Vec<Vec2>,
    /// Boundary V 시퀀스 (1차 source).
    pub boundary_vids: Vec<V>,
    /// Signed area (always > 0 for emitted faces).
    pub signed_area: f64,
    /// Centroid (2D).
    pub centroid: Vec2,
    /// 상속된 material.
    pub material: TMat,
    /// 상속된 surface.
    pub surface: TSurf,
}

/// O(n²) intersection resolve. Fixed-point. (B-O 로 대체됨 — 참고용 보존.)
///
/// ## Bug fix #1 (vs 원본): Overlap multi-split
///
/// `split_edge_points` helper로 현존 edge에만 split 적용 (첫 split 후 ei.id
/// 삭제로 인한 panic 회피).
fn resolve_intersections<V, F>(g: &mut PlanarGraph<V>, lineage: &mut Lineage, next: &mut F)
where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    let eps = g.eps;
    let mut changed = true;
    while changed {
        changed = false;
        let eids: Vec<_> = g.edges.keys().cloned().collect();
        'outer: for i in 0..eids.len() {
            for j in (i + 1)..eids.len() {
                let ei = match g.edges.get(&eids[i]).cloned() {
                    Some(x) => x,
                    None => continue,
                };
                let ej = match g.edges.get(&eids[j]).cloned() {
                    Some(x) => x,
                    None => continue,
                };
                // Skip adjacent edges sharing vertex.
                if ei.a == ej.a || ei.a == ej.b || ei.b == ej.a || ei.b == ej.b {
                    continue;
                }
                let a1 = g.vertices.get(&ei.a).unwrap().p;
                let a2 = g.vertices.get(&ei.b).unwrap().p;
                let b1 = g.vertices.get(&ej.a).unwrap().p;
                let b2 = g.vertices.get(&ej.b).unwrap().p;
                match seg_intersect(a1, a2, b1, b2, eps) {
                    SegIsect::None => {}
                    SegIsect::Point { p, .. } => {
                        g.split_edge(ei.id, p, lineage, |q| next(q));
                        g.split_edge(ej.id, p, lineage, |q| next(q));
                        changed = true;
                        break 'outer;
                    }
                    SegIsect::Overlap { p1, p2 } => {
                        split_edge_points(g, ei.id, &[p1, p2], lineage, next);
                        split_edge_points(g, ej.id, &[p1, p2], lineage, next);
                        changed = true;
                        break 'outer;
                    }
                }
            }
        }
    }
}

/// **Bug fix #1** helper: edge가 split되어 ID가 바뀔 수 있으므로, 각 split point에
/// 대해 현재 살아있는 edge들 중 그 point가 internal에 있는 edge를 찾아 split.
fn split_edge_points<V, F>(
    g: &mut PlanarGraph<V>,
    original_id: EdgeId,
    pts: &[Vec2],
    lineage: &mut Lineage,
    next: &mut F,
) where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    let root_id = match g.edges.get(&original_id) {
        Some(e) => e.root,
        None => return,
    };
    let eps = g.eps;
    for &p in pts {
        let candidates: Vec<EdgeId> = g
            .edges
            .iter()
            .filter_map(|(eid, e)| {
                if e.root == root_id {
                    Some(*eid)
                } else {
                    None
                }
            })
            .collect();
        for eid in candidates {
            let e = match g.edges.get(&eid).cloned() {
                Some(x) => x,
                None => continue,
            };
            let a = g.vertices.get(&e.a).unwrap().p;
            let b = g.vertices.get(&e.b).unwrap().p;
            let (on, t) = point_on_segment(p, a, b, eps);
            if on && t > eps && t < 1.0 - eps {
                g.split_edge(eid, p, lineage, |q| next(q));
                break;
            }
        }
    }
}

/// Fixed-point T-junction: vertex가 strict interior of some edge면 split.
fn resolve_tjunctions_fixed_point<V, F>(g: &mut PlanarGraph<V>, lineage: &mut Lineage, next: &mut F)
where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    let eps = g.eps;
    loop {
        let vids: Vec<_> = g.vertices.keys().cloned().collect();
        let eids: Vec<_> = g.edges.keys().cloned().collect();
        let mut split = false;
        'outer: for vid in vids {
            let p = g.vertices.get(&vid).unwrap().p;
            for eid in &eids {
                let e = match g.edges.get(eid).cloned() {
                    Some(x) => x,
                    None => continue,
                };
                if e.a == vid || e.b == vid {
                    continue;
                }
                let a = g.vertices.get(&e.a).unwrap().p;
                let b = g.vertices.get(&e.b).unwrap().p;
                let (on, t) = point_on_segment(p, a, b, eps);
                if on && t > eps && t < 1.0 - eps {
                    g.split_edge(e.id, p, lineage, |q| next(q));
                    split = true;
                    break 'outer;
                }
            }
        }
        if !split {
            break;
        }
    }
}

fn innermost_parent<'a, V, TMat: Clone, TSurf: Clone>(
    r: &Region<V>,
    dirty: &'a [DirtyFaceInfo<TMat, TSurf>],
    eps: f64,
) -> Option<&'a DirtyFaceInfo<TMat, TSurf>> {
    let mut cands: Vec<&DirtyFaceInfo<TMat, TSurf>> = Vec::new();
    for f in dirty {
        let pip = point_in_polygon_even_odd(r.centroid, &f.orig_polygon, eps);
        if pip == Pip::Inside || pip == Pip::Boundary {
            cands.push(f);
        }
    }
    if cands.is_empty() {
        return None;
    }
    cands.sort_by(|a, b| {
        a.area
            .abs()
            .partial_cmp(&b.area.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Some(cands[0])
}

/// **Robust 2D split kernel** — entry point.
///
/// ## 3-branch classification (P5.UX.53 핵심)
///
/// 각 region에 대해 innermost parent face가 있다면:
/// 1. **shares_edge**: region edge 중 하나라도 parent의 root에서 유래 → sub-region
///    (parent material + surface 상속).
/// 2. **all_inside**: region 모든 vertex가 parent polygon 안 + boundary 공유 안 함
///    → contained region (default material + parent surface).
/// 3. **centroid fluke**: 위 둘 다 false (centroid만 우연히 inside) → default 모두.
///
/// ## V generic
///
/// - `PlanarGraph<V>` 받음
/// - `make_vertex: F: FnMut(Vec2) -> V` — caller 가 split 시점 새 V 생성 책임
///   (예: V=mesh VertId 면 mesh.add_vertex 호출). vid 가 진정 1차 시민.
pub fn robust_split_2d<TMat, TSurf, F, V>(
    mut g: PlanarGraph<V>,
    dirty: Vec<DirtyFaceInfo<TMat, TSurf>>,
    default_material: TMat,
    default_surface: TSurf,
    mut make_vertex: F,
) -> Vec<FaceOut<TMat, TSurf, V>>
where
    V: Copy + Ord + Hash,
    TMat: Clone,
    TSurf: Clone,
    F: FnMut(Vec2) -> V,
{
    let eps = g.eps;
    // **Bug fix #5**: area는 distance² 단위.
    let eps_area = (eps * eps).max(1e-18);
    let mut lineage = Lineage::default();
    for e in g.edges.values() {
        lineage.ensure_root(e.root);
    }
    // 1) intersection resolve — B-O sweep line (closed-chain self-intersect 정확).
    super::bentley_ottmann::bentley_ottmann_resolve(&mut g, &mut lineage, &mut make_vertex);
    // 2) T-junction resolve.
    resolve_tjunctions_fixed_point(&mut g, &mut lineage, &mut make_vertex);
    // 2.5) 겹침 엣지 정규화 — B-O Overlap / T-junction 분할이 남긴 평행(중복)
    //      edge 를 1개로 병합 (no vertex move). error01 과병합 ("면사라짐") fix.
    g.dedup_parallel_edges(&mut lineage);
    // 3) region extraction.
    let regions = extract_regions(&g, eps_area);
    // 4) classification + inheritance.
    let mut out = Vec::new();
    for r in regions {
        // Bounded CCW (interior) region만 통과. sliver / CW outer region drop
        // (정상 동작에서도 자주 발생).
        if r.signed_area <= eps_area {
            continue;
        }
        let parent = innermost_parent(&r, &dirty, eps);
        let mut mat = default_material.clone();
        let mut surf = default_surface.clone();
        if let Some(p) = parent {
            let shares_edge = r
                .edges
                .iter()
                .any(|eid| lineage.any_in_roots(*eid, &p.orig_edge_roots));
            if shares_edge {
                // Branch 1: sub-region of parent.
                mat = p.material.clone();
                surf = p.surface.clone();
            } else {
                // Branch 2: containment candidate — all_inside guard.
                let r_pts: Vec<Vec2> = r
                    .verts
                    .iter()
                    .map(|vid| g.vertices.get(vid).unwrap().p)
                    .collect();
                if all_inside(&r_pts, &p.orig_polygon, eps) {
                    mat = default_material.clone();
                    surf = p.surface.clone();
                }
                // Branch 3: centroid fluke — keep defaults (already set).
            }
        }
        let boundary_positions: Vec<Vec2> = r
            .verts
            .iter()
            .map(|vid| g.vertices.get(vid).unwrap().p)
            .collect();
        let boundary_vids: Vec<V> = r.verts.iter().copied().collect();
        out.push(FaceOut {
            boundary_positions,
            boundary_vids,
            signed_area: r.signed_area,
            centroid: r.centroid,
            material: mat,
            surface: surf,
        });
    }
    out
}

/// **ADR-186 δ-1** — 유도면 모델 통합 entry: raw edge graph → 교차해결 →
/// 면 + containment hole 자동 부착.
///
/// `robust_split_2d` (flat `extract_regions`, 3-branch 상속) 의 **nested 변형** —
/// `extract_regions_nested` 를 써서 rect/circle containment 가 **annulus + disk**
/// (RegionWithHoles) 로 나오도록 한다. 사용자의 "사각형 안 사각형 → 면분할"
/// (rect containment GAP) + "원 안 원" 통합 처리의 핵심.
///
/// DCEL 통합 (δ-2 `Mesh::rebuild_coplanar_faces`) 가 이 함수를 호출 — coplanar
/// edge 를 PlanarGraph 로 빌드 후 호출하면 `RegionWithHoles` (outer + holes) 반환
/// → `add_face_with_holes` 로 매핑. `make_vertex` 는 B-O split point 의 새 V 생성
/// (DCEL 에서는 `mesh.add_vertex`).
///
/// 파이프라인 (robust_split_2d 의 1~3 단계와 동일, extraction 만 nested):
/// 1. B-O sweep intersection resolve
/// 2. T-junction resolve (fixed-point)
/// 3. dedup_parallel_edges (error01 과병합 = "면사라짐" fix)
/// 4. extract_regions_nested (containment → RegionWithHoles)
pub fn resolve_and_extract_nested<V, F>(
    g: &mut PlanarGraph<V>,
    mut make_vertex: F,
) -> Vec<super::region::RegionWithHoles<V>>
where
    V: Copy + Ord + Hash,
    F: FnMut(Vec2) -> V,
{
    let eps = g.eps;
    let eps_area = (eps * eps).max(1e-18);
    let mut lineage = Lineage::default();
    for e in g.edges.values() {
        lineage.ensure_root(e.root);
    }
    // 1) intersection resolve (B-O sweep — closed-chain self-intersect 정확).
    super::bentley_ottmann::bentley_ottmann_resolve(g, &mut lineage, &mut make_vertex);
    // 2) T-junction resolve.
    resolve_tjunctions_fixed_point(g, &mut lineage, &mut make_vertex);
    // 3) 겹침 엣지 정규화 (error01 과병합 = "면사라짐" fix, no vertex move).
    g.dedup_parallel_edges(&mut lineage);
    // 4) region 추출 + containment hole 자동 부착 (annulus+disk).
    super::region::extract_regions_nested(g, eps_area)
}

#[cfg(test)]
mod tests {
    use super::super::planar::PlanarGraph;
    use super::*;

    // ADR-186 β-4 — AixiAcad robust_split 회귀 2건 1:1 재현 (port 검증).
    // 핵심: rect partial overlap = 3 region / rect containment = 2 region.

    /// 두 사각형 partial overlap → 3 region.
    #[test]
    fn two_rects_partial_overlap_makes_three_regions() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        // Rect A: (0,0)-(2,0)-(2,2)-(0,2).
        let a0 = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let a1 = g.get_or_create_vertex(Vec2::new(2.0, 0.0), &mut next);
        let a2 = g.get_or_create_vertex(Vec2::new(2.0, 2.0), &mut next);
        let a3 = g.get_or_create_vertex(Vec2::new(0.0, 2.0), &mut next);
        g.create_edge(a0, a1, None);
        g.create_edge(a1, a2, None);
        g.create_edge(a2, a3, None);
        g.create_edge(a3, a0, None);
        // Rect B: (1,1)-(3,1)-(3,3)-(1,3) — A와 부분 겹침.
        let b0 = g.get_or_create_vertex(Vec2::new(1.0, 1.0), &mut next);
        let b1 = g.get_or_create_vertex(Vec2::new(3.0, 1.0), &mut next);
        let b2 = g.get_or_create_vertex(Vec2::new(3.0, 3.0), &mut next);
        let b3 = g.get_or_create_vertex(Vec2::new(1.0, 3.0), &mut next);
        g.create_edge(b0, b1, None);
        g.create_edge(b1, b2, None);
        g.create_edge(b2, b3, None);
        g.create_edge(b3, b0, None);
        let mut split_counter = counter;
        let split_next = move |_p: Vec2| {
            let v = split_counter;
            split_counter += 1;
            v
        };
        let out = robust_split_2d::<i32, i32, _, u32>(g, Vec::new(), 0, 0, split_next);
        // 3 interior region: A-B, A∩B, B-A.
        assert_eq!(out.len(), 3, "regions = {}", out.len());
    }

    /// 큰 사각형 안의 작은 사각형 → 2 region (annular + inner).
    #[test]
    fn rect_inside_rect_makes_two_regions() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        // Big: (0,0)-(4,0)-(4,4)-(0,4).
        let a0 = g.get_or_create_vertex(Vec2::new(0.0, 0.0), &mut next);
        let a1 = g.get_or_create_vertex(Vec2::new(4.0, 0.0), &mut next);
        let a2 = g.get_or_create_vertex(Vec2::new(4.0, 4.0), &mut next);
        let a3 = g.get_or_create_vertex(Vec2::new(0.0, 4.0), &mut next);
        g.create_edge(a0, a1, None);
        g.create_edge(a1, a2, None);
        g.create_edge(a2, a3, None);
        g.create_edge(a3, a0, None);
        // Small inside: (1,1)-(3,1)-(3,3)-(1,3).
        let b0 = g.get_or_create_vertex(Vec2::new(1.0, 1.0), &mut next);
        let b1 = g.get_or_create_vertex(Vec2::new(3.0, 1.0), &mut next);
        let b2 = g.get_or_create_vertex(Vec2::new(3.0, 3.0), &mut next);
        let b3 = g.get_or_create_vertex(Vec2::new(1.0, 3.0), &mut next);
        g.create_edge(b0, b1, None);
        g.create_edge(b1, b2, None);
        g.create_edge(b2, b3, None);
        g.create_edge(b3, b0, None);
        let mut split_counter = counter;
        let split_next = move |_p: Vec2| {
            let v = split_counter;
            split_counter += 1;
            v
        };
        let out = robust_split_2d::<i32, i32, _, u32>(g, Vec::new(), 0, 0, split_next);
        // Expect 2 interior face: small disk + big annular (DCEL face traversal).
        assert!(out.len() >= 2, "expected ≥2 regions, got {}", out.len());
    }

    // ─── ADR-186 δ-1: resolve_and_extract_nested (raw edge → 면 + containment hole) ───

    fn add_loop(g: &mut PlanarGraph<u32>, pts: &[(f64, f64)], next: &mut impl FnMut(Vec2) -> u32) {
        let ids: Vec<u32> = pts
            .iter()
            .map(|&(x, y)| g.get_or_create_vertex(Vec2::new(x, y), &mut *next))
            .collect();
        let n = ids.len();
        for k in 0..n {
            g.create_edge(ids[k], ids[(k + 1) % n], None);
        }
    }

    /// raw 사각형 partial overlap (교차 edge, 미분할) → resolve → 3 면, hole 0.
    /// 우리 엔진의 "사각형 겹침 분할" 통합 검증 (ADR-101 수렴).
    #[test]
    fn resolve_and_extract_nested_rect_partial_overlap_3_regions() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        add_loop(&mut g, &[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)], &mut next);
        add_loop(&mut g, &[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)], &mut next);
        let mut split_counter = counter;
        let split_next = move |_p: Vec2| {
            let v = split_counter;
            split_counter += 1;
            v
        };
        let faces = resolve_and_extract_nested(&mut g, split_next);
        assert_eq!(faces.len(), 3, "partial overlap = 3 면 (A-B, A∩B, B-A): {}", faces.len());
        let total_holes: usize = faces.iter().map(|f| f.holes.len()).sum();
        assert_eq!(total_holes, 0, "연결 분할 = hole 0");
    }

    /// raw 사각형 containment (disconnected, 미분할) → 2 면 (annulus + disk).
    /// 우리 엔진의 "사각형 안 사각형 GAP" 해결 검증.
    #[test]
    fn resolve_and_extract_nested_rect_containment_annulus_plus_disk() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        add_loop(&mut g, &[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)], &mut next);
        add_loop(&mut g, &[(1.0, 1.0), (3.0, 1.0), (3.0, 3.0), (1.0, 3.0)], &mut next);
        let mut split_counter = counter;
        let split_next = move |_p: Vec2| {
            let v = split_counter;
            split_counter += 1;
            v
        };
        let faces = resolve_and_extract_nested(&mut g, split_next);
        assert_eq!(faces.len(), 2, "containment = 2 면 (annulus + disk): {}", faces.len());
        let outer_face = faces
            .iter()
            .max_by(|a, b| a.outer.signed_area.partial_cmp(&b.outer.signed_area).unwrap())
            .unwrap();
        assert_eq!(outer_face.holes.len(), 1, "바깥 면 = hole 1 (annulus)");
        assert!(
            (outer_face.holes[0].signed_area.abs() - 4.0).abs() < 1e-6,
            "hole 면적 = inner (2×2=4): {}",
            outer_face.holes[0].signed_area.abs()
        );
    }

    /// connected split (수직 chord) → 2 면, hole 0 (가짜 hole 회귀 가드).
    #[test]
    fn resolve_and_extract_nested_connected_split_no_holes() {
        let mut g: PlanarGraph<u32> = PlanarGraph::new(1e-9);
        let mut counter = 1u32;
        let mut next = |_p: Vec2| {
            let v = counter;
            counter += 1;
            v
        };
        let pts = [
            (0.0, 0.0),
            (50.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (50.0, 100.0),
            (0.0, 100.0),
        ];
        let ids: Vec<u32> = pts
            .iter()
            .map(|&(x, y)| g.get_or_create_vertex(Vec2::new(x, y), &mut next))
            .collect();
        // outer loop + 수직 chord (ids[1]-ids[4]).
        for &(i, j) in &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0), (1, 4)] {
            g.create_edge(ids[i], ids[j], None);
        }
        let mut split_counter = counter;
        let split_next = move |_p: Vec2| {
            let v = split_counter;
            split_counter += 1;
            v
        };
        let faces = resolve_and_extract_nested(&mut g, split_next);
        assert_eq!(faces.len(), 2, "chord 분할 = 2 면: {}", faces.len());
        assert!(faces.iter().all(|f| f.holes.is_empty()), "연결 분할 = hole 0");
    }
}
