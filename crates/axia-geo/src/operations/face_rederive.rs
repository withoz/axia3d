//! **ADR-186 Phase 4 δ-2** — DCEL bridge: `rebuild_coplanar_faces`.
//!
//! 유도면 모델의 DCEL 연결 (small end-to-end proof). coplanar DCEL `Edge`
//! (면 독립 SSOT) → boundary_kernel (`resolve_and_extract_nested`) → reconcile
//! (dirty 면 제거 + `add_face_with_holes`, annulus 포함).
//!
//! **목적 (사용자 결재 2026-06-01)**: DCEL-as-SSOT ↔ 유도면 모델 impedance (ADR-186
//! §7.1 우려 A) 를 *작은 증명*으로 조기 노출. 큰 Scene wiring (δ-4) 전 게이트.
//!
//! ## kernel V = u32 (VertId 가 Ord 미derive — ADR-186 §1 발견)
//!
//! `PlanarGraph<V>` 는 `V: Ord` (BTreeMap deterministic) 필요하나 `VertId` 는
//! `Copy + Eq + Hash` 만. → kernel 에 `V = VertId::raw()` (u32) 사용 + raw↔VertId
//! map 으로 reconcile. (VertId 에 Ord 추가는 core type 변경이라 별도 결재 보류.)
//!
//! ## 한계 (δ-2 proof scope)
//! - polygon only (Path B 곡선 = δ-3)
//! - XIA/material 상속 없음 — 새 면은 FORM_MATERIAL (δ-3 FaceLineage)
//! - 3D solid 보호 미구현 (sheet only 가정) — δ-4
//! - eps fixed 1e-6 (scale-aware = δ-4)

use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Result;
use glam::DVec3;

use crate::boundary_kernel::geom2::{point_in_polygon_even_odd, polygon_signed_area, Pip};
use crate::boundary_kernel::{
    arrange, resolve_and_extract_nested, Freeform2D, InputCurve, PlanarGraph, SubCurve, Vec2,
};
use crate::curves::{circle, AnalyticCurve};
use crate::entities::MaterialId;
use crate::mesh::Mesh;
use crate::surfaces::AnalyticSurface;
use crate::{EdgeId, FaceId, VertId};

/// ADR-050 P-5e-β FORM_MATERIAL sentinel.
const FORM_MATERIAL: MaterialId = MaterialId::new(0);

/// **δ-3** — dirty 면 snapshot (material / surface / polygon) — rebuild 후 새 면이
/// 상속할 부모 정보. AixiAcad `DirtyFaceInfo` 답습 (Mesh-level 변형).
struct DirtyInfo {
    fid: FaceId,
    material: MaterialId,
    surface: Option<AnalyticSurface>,
    polygon: Vec<Vec2>,
    area: f64,
}

/// **δ-3** — probe 점을 포함하는 가장 작은(=가장 안쪽) dirty 면 = 상속 부모.
/// robust_split `innermost_parent` 의 Mesh-level 변형.
fn innermost_parent<'a>(probe: Vec2, dirty: &'a [DirtyInfo], eps: f64) -> Option<&'a DirtyInfo> {
    dirty
        .iter()
        .filter(|d| point_in_polygon_even_odd(probe, &d.polygon, eps) != Pip::Outside)
        .min_by(|a, b| a.area.partial_cmp(&b.area).unwrap_or(std::cmp::Ordering::Equal))
}

/// **δ-3** — region outer boundary 바로 안쪽 점 (hole 회피).
///
/// centroid 는 annulus(outer region with hole) 에서 hole 안에 떨어질 수 있어
/// 상속 부모를 잘못 선택 (centroid 발견: annulus outer 의 centroid 가 hole 내부
/// → innermost_parent 가 inner 면 선택). 대신 첫 outer edge midpoint 에서 interior
/// (CCW left normal) 방향으로 edge 길이의 1% 이동 → ring 안쪽 (hole 밖) 보장.
fn region_interior_point(poly: &[Vec2]) -> Vec2 {
    if poly.len() < 2 {
        return poly.first().copied().unwrap_or(Vec2::new(0.0, 0.0));
    }
    let v0 = poly[0];
    let v1 = poly[1];
    let mid = Vec2::new((v0.x + v1.x) * 0.5, (v0.y + v1.y) * 0.5);
    let edge = v1.sub(v0);
    let len = edge.len();
    if len < 1e-12 {
        return mid;
    }
    // CCW(signed_area > 0) interior = v0→v1 의 left normal.
    let nx = -edge.y / len;
    let ny = edge.x / len;
    let step = len * 0.01;
    Vec2::new(mid.x + nx * step, mid.y + ny * step)
}

/// Plane basis (u, v) ⊥ normal.
fn plane_basis(n: DVec3) -> (DVec3, DVec3) {
    let n = n.normalize_or_zero();
    let a = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = (a - n * a.dot(n)).normalize_or_zero();
    let v = n.cross(u).normalize_or_zero();
    (u, v)
}

/// `rebuild_coplanar_faces` 결과 요약.
#[derive(Debug, Clone, Default)]
pub struct RebuildReport {
    /// 제거된 dirty 면 수.
    pub removed_faces: usize,
    /// 생성된 면 수.
    pub created_faces: usize,
    /// 생성된 hole(inner loop) 수.
    pub created_holes: usize,
    /// 처리한 coplanar edge 수.
    pub coplanar_edges: usize,
}

/// **4-β helper** — 두 collinear+연결(touch/overlap) segment 병합 → spanning.
/// gap 있으면 None (분리 유지).
fn try_merge_two_segs(a: (Vec2, Vec2), b: (Vec2, Vec2), eps: f64) -> Option<(Vec2, Vec2)> {
    let d = a.1.sub(a.0);
    let dlen = d.len();
    if dlen < eps {
        return None;
    }
    let dn = Vec2::new(d.x / dlen, d.y / dlen);
    // b 의 두 점이 line a 위 (수직거리 < eps) — 공선.
    let perp = |p: Vec2| {
        let w = p.sub(a.0);
        (w.x * dn.y - w.y * dn.x).abs()
    };
    if perp(b.0) > eps || perp(b.1) > eps {
        return None;
    }
    let t = |p: Vec2| p.sub(a.0).dot(dn);
    let (alo, ahi) = (0.0_f64, dlen);
    let (tb0, tb1) = (t(b.0), t(b.1));
    let (blo, bhi) = (tb0.min(tb1), tb0.max(tb1));
    // 연결/중첩? (gap 이면 분리)
    if bhi < alo - eps || blo > ahi + eps {
        return None;
    }
    let lo = alo.min(blo);
    let hi = ahi.max(bhi);
    Some((a.0.add(dn.mul(lo)), a.0.add(dn.mul(hi))))
}

/// **4-β helper** — collinear 연결 segment 들을 full line 으로 병합 (직선 source 복원).
/// rect 가 split 된 subseg 들 → 원본 full edge 로. (arc→circle 와 동일 원리)
fn merge_collinear_segments(mut segs: Vec<(Vec2, Vec2)>, eps: f64) -> Vec<(Vec2, Vec2)> {
    let mut changed = true;
    while changed {
        changed = false;
        'outer: for i in 0..segs.len() {
            for j in (i + 1)..segs.len() {
                if let Some(m) = try_merge_two_segs(segs[i], segs[j], eps) {
                    segs[i] = m;
                    segs.remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    segs
}

// ════════════════════════════════════════════════════════════════════════
//  Option A (perf scope, 2026-06-05) — affected-region re-derive helpers.
//  A new shape can only change the coplanar faces its bounding box overlaps;
//  disjoint faces are independent. Restricting the O(N²) `arrange` to the
//  connected component (by 2D AABB) the new shape touches turns it into
//  O(affected²) — near O(1) for disjoint draws (사용자 "도구 작동이 매우 느림").
// ════════════════════════════════════════════════════════════════════════

/// 2D axis-aligned bounding box in the rederive plane's projected coordinates.
#[derive(Clone, Copy)]
struct Aabb2 {
    min: Vec2,
    max: Vec2,
}

impl Aabb2 {
    fn point(p: Vec2) -> Self {
        Aabb2 { min: p, max: p }
    }
    fn expand(&mut self, p: Vec2) {
        self.min = Vec2::new(self.min.x.min(p.x), self.min.y.min(p.y));
        self.max = Vec2::new(self.max.x.max(p.x), self.max.y.max(p.y));
    }
    /// Overlap with margin `m` (conservative — groups near-touching boxes).
    fn overlaps(&self, o: &Aabb2, m: f64) -> bool {
        self.min.x <= o.max.x + m
            && self.max.x >= o.min.x - m
            && self.min.y <= o.max.y + m
            && self.max.y >= o.min.y - m
    }
}

/// 2D AABB of an edge (endpoints + curve bounds: circle/arc center±r, freeform
/// control hull). `None` if a vert is missing.
fn edge_aabb_2d(mesh: &Mesh, eid: EdgeId, project: &impl Fn(DVec3) -> Vec2) -> Option<Aabb2> {
    let edge = mesh.edges.get(eid)?;
    let pa = mesh.verts.get(edge.v_small())?.pos();
    let pb = mesh.verts.get(edge.v_large())?.pos();
    let mut a = Aabb2::point(project(pa));
    a.expand(project(pb));
    match edge.curve() {
        Some(AnalyticCurve::Circle { center, radius, .. })
        | Some(AnalyticCurve::Arc { center, radius, .. }) => {
            let c = project(*center);
            let r = *radius;
            a.expand(Vec2::new(c.x - r, c.y - r));
            a.expand(Vec2::new(c.x + r, c.y + r));
        }
        Some(AnalyticCurve::Bezier { control_pts })
        | Some(AnalyticCurve::BSpline { control_pts, .. })
        | Some(AnalyticCurve::NURBS { control_pts, .. }) => {
            for cp in control_pts {
                a.expand(project(*cp));
            }
        }
        _ => {}
    }
    Some(a)
}

/// 2D AABB of a coplanar face (union of its outer-edge AABBs). `None` if the
/// face is inactive, null, or has any **off-plane** vert (3D solid wall —
/// excluded from the planar scope so it stays protected).
fn face_aabb_2d_coplanar(
    mesh: &Mesh,
    fid: FaceId,
    project: &impl Fn(DVec3) -> Vec2,
    on_plane: &impl Fn(DVec3) -> bool,
) -> Option<Aabb2> {
    let f = mesh.faces.get(fid)?;
    if !f.is_active() || f.outer().start.is_null() {
        return None;
    }
    if face_has_curved_surface(mesh, fid) {
        return None; // curved-surface face — never a planar coplanar region
    }
    let edges = mesh.face_outer_edges(fid).ok()?;
    let mut aabb: Option<Aabb2> = None;
    for &e in &edges {
        if let Some(edge) = mesh.edges.get(e) {
            for vid in [edge.v_small(), edge.v_large()] {
                let p = mesh.verts.get(vid)?.pos();
                if !on_plane(p) {
                    return None; // off-plane face → not in planar scope
                }
            }
        }
        if let Some(ea) = edge_aabb_2d(mesh, e, project) {
            match &mut aabb {
                Some(a) => {
                    a.expand(ea.min);
                    a.expand(ea.max);
                }
                None => aabb = Some(ea),
            }
        }
    }
    aabb
}

/// **Option A** — connected component of coplanar faces (by AABB overlap)
/// reachable from `seed`. BFS; conservative `margin` groups near-touching
/// faces. Returns the affected face set (empty if no seed face is coplanar).
fn affected_face_component(
    mesh: &Mesh,
    all_fids: &[FaceId],
    seed: &[FaceId],
    project: &impl Fn(DVec3) -> Vec2,
    on_plane: &impl Fn(DVec3) -> bool,
    margin: f64,
) -> HashSet<FaceId> {
    let face_aabbs: Vec<(FaceId, Aabb2)> = all_fids
        .iter()
        .filter_map(|&f| face_aabb_2d_coplanar(mesh, f, project, on_plane).map(|a| (f, a)))
        .collect();
    let aabb_of: HashMap<FaceId, Aabb2> = face_aabbs.iter().map(|&(f, a)| (f, a)).collect();
    let mut affected: HashSet<FaceId> = HashSet::new();
    let mut frontier: Vec<FaceId> = Vec::new();
    for &s in seed {
        if aabb_of.contains_key(&s) && affected.insert(s) {
            frontier.push(s);
        }
    }
    while let Some(f) = frontier.pop() {
        let fa = aabb_of[&f];
        for &(other, oa) in &face_aabbs {
            if affected.contains(&other) {
                continue;
            }
            if fa.overlaps(&oa, margin) {
                affected.insert(other);
                frontier.push(other);
            }
        }
    }
    affected
}

/// **4-β (ADR-186 A)** — coplanar DCEL edge → analytic `InputCurve`.
///
/// 🔑 idempotency 핵심: **arc edge 들을 같은 (center,radius) 기준 하나의 full Circle 로
/// 병합 → source(전체 원) 복원**. self-loop Circle 도 Circle. 직선은 그대로 Line.
/// (polygonize 안 함 — 폴리곤 잔재 제거). arc 가 split-circle 조각이라는 가정 (현
/// 입력=rect+원). genuine partial arc(DrawArc)는 future.
fn reconstruct_input_curves(
    mesh: &Mesh,
    plane_origin: DVec3,
    u: DVec3,
    v: DVec3,
    n_unit: DVec3,
    tol: f64,
    volume_edges: &HashSet<EdgeId>,
    scope_edges: Option<&HashSet<EdgeId>>,
) -> Vec<InputCurve> {
    let project = |p: DVec3| -> Vec2 {
        let d = p - plane_origin;
        Vec2::new(d.dot(u), d.dot(v))
    };
    let on_plane = |p: DVec3| (p - plane_origin).dot(n_unit).abs() < tol;
    // 양자화 (0.1μm) — 같은 원의 arc 들을 한 키로.
    let q = |x: f64| (x / 1e-4).round() as i64;
    let mut curves: Vec<InputCurve> = Vec::new();
    // ADR-199 (A2) coverage + ADR-200 (A1, β-2) arc input — arc 조각을 즉시 full
    // Circle 로 병합하지 않는다. (center,radius)별로 arc edge 수집 → **정점
    // incidence(closed loop)** 로 전체 원 여부 판정. 닫힌 고리(모든 끝점 degree 2)
    // = full circle → `InputCurve::Circle` 재구성(idempotency). dangling 끝점
    // (degree 1) = 부분 arc(사용자가 일부 삭제 / DrawArc) → **`InputCurve::Arc` 로
    // arrange 에 투입**(β-2 — A2 의 preserve 대체, 호가 1급으로 교차·분할·면화).
    // full circle 로 완성하지 않으므로 삭제분 부활 차단 유지. (C) chord-rejection
    // 중점검사는 per-edge sagitta 가 full/partial 동일이라 구분 불가 → coverage 정답.
    let mut full_circle_keys: HashSet<(i64, i64, i64)> = HashSet::new();
    let mut arc_groups: HashMap<(i64, i64, i64), Vec<(EdgeId, VertId, VertId)>> = HashMap::new();
    let mut key_geom: HashMap<(i64, i64, i64), (Vec2, f64)> = HashMap::new();
    let mut line_segs: Vec<(Vec2, Vec2)> = Vec::new();
    // B6 — freeform overlap owner-id restore (idempotency / P5 fix). Group all
    // fragments/self-loops of one original by curve_owner_id → feed the stored
    // source ONCE. Unifies first-rebuild (self-loop) + re-rebuild (sub-bezier
    // fragment) feeding into one SSOT path. Gate-implicit: owner-ids only exist
    // when Phase 0.5 detection ran (gate on).
    let mut freeform_owners_seen: HashSet<u32> = HashSet::new();
    for (eid, edge) in mesh.edges.iter() {
        if !edge.is_active() || volume_edges.contains(&eid) {
            continue;
        }
        // Option A — restrict to the affected region's edges (None = full plane).
        if let Some(s) = scope_edges {
            if !s.contains(&eid) {
                continue;
            }
        }
        let va = edge.v_small();
        let vb = edge.v_large();
        let pa = match mesh.verts.get(va) {
            Some(x) => x.pos(),
            None => continue,
        };
        let pb = match mesh.verts.get(vb) {
            Some(x) => x.pos(),
            None => continue,
        };
        if !on_plane(pa) || !on_plane(pb) {
            continue;
        }
        // B6 — overlap freeform (self-loop or sub-bezier fragment) with a stored
        // source → restore the original by owner-id (once per owner). The
        // `freeform_curve_source` guard distinguishes overlap freeforms from
        // other owner-id'd edges (e.g., ADR-088 circle segment groups).
        if let Some(owner) = edge.curve_owner_id() {
            if let Some(src) = mesh.freeform_curve_source(owner) {
                if freeform_owners_seen.insert(owner) {
                    if let Some(ff) = project_curve_to_freeform2d(src, &project, owner) {
                        curves.push(InputCurve::Freeform(ff));
                    }
                }
                continue; // skip line/circle treatment for this fragment
            }
        }
        match edge.curve() {
            // Full-circle self-loop edge (Path B circle) → 항상 full circle.
            Some(AnalyticCurve::Circle { center, radius, .. }) => {
                if on_plane(*center) {
                    let c2 = project(*center);
                    let key = (q(c2.x), q(c2.y), q(*radius));
                    full_circle_keys.insert(key);
                    key_geom.entry(key).or_insert((c2, *radius));
                }
            }
            // Arc 조각 → 키별 수집 (coverage 는 루프 후 판정).
            Some(AnalyticCurve::Arc { center, radius, .. }) => {
                if on_plane(*center) {
                    let c2 = project(*center);
                    let key = (q(c2.x), q(c2.y), q(*radius));
                    arc_groups.entry(key).or_default().push((eid, va, vb));
                    key_geom.entry(key).or_insert((c2, *radius));
                }
            }
            _ => {
                // 직선 (curve 없음 / Line). self-loop(va==vb) 은 스킵.
                if va != vb {
                    line_segs.push((project(pa), project(pb)));
                }
            }
        }
    }
    // ── ADR-199 (A2) coverage 판정 — 키 정렬로 deterministic emit.
    let mut emitted: HashSet<(i64, i64, i64)> = HashSet::new();
    // (1) full-circle self-loop 키 → 항상 Circle.
    let mut full_keys: Vec<_> = full_circle_keys.iter().copied().collect();
    full_keys.sort_unstable();
    for key in full_keys {
        if let Some((c2, r)) = key_geom.get(&key) {
            if emitted.insert(key) {
                curves.push(InputCurve::Circle {
                    center: *c2,
                    radius: *r,
                });
            }
        }
    }
    // (2) arc-only 키 → 정점 incidence(closed loop) 검사.
    let mut arc_keys: Vec<_> = arc_groups.keys().copied().collect();
    arc_keys.sort_unstable();
    for key in arc_keys {
        if full_circle_keys.contains(&key) {
            continue; // 이미 full circle self-loop 으로 emit.
        }
        let arcs = &arc_groups[&key];
        // 끝점 degree: 같은 원의 arc 조각끼리만 카운트. 모두 2 = 닫힌 고리(전체 원).
        // 하나라도 1 = dangling(부분 arc, gap 존재).
        let mut deg: HashMap<VertId, usize> = HashMap::new();
        for (_, a, b) in arcs {
            *deg.entry(*a).or_default() += 1;
            *deg.entry(*b).or_default() += 1;
        }
        let is_full_circle = !deg.is_empty() && deg.values().all(|&d| d == 2);
        if is_full_circle {
            if let Some((c2, r)) = key_geom.get(&key) {
                if emitted.insert(key) {
                    curves.push(InputCurve::Circle {
                        center: *c2,
                        radius: *r,
                    });
                }
            }
        } else {
            // ADR-200 (A1, β-2) — 부분 arc → **InputCurve::Arc 로 arrange 에 투입**
            // (A2 의 preserve 대체). 각 fragment 를 개별 arc 로 (인접 fragment 는
            // valence-2 vertex 공유, harmless). a0/a1 은 arrange 2D frame 의 끝점
            // 각도 + **arc 미드포인트**로 방향 결정 (평면 frame 무관, 반원 모호성
            // 해소). full circle 로 완성하지 않으므로 삭제분 부활 차단 유지.
            let Some((c2, r)) = key_geom.get(&key).copied() else {
                continue;
            };
            let nm = |x: f64| {
                let mut a = x % std::f64::consts::TAU;
                if a < 0.0 {
                    a += std::f64::consts::TAU;
                }
                a
            };
            for (eid, va, vb) in arcs {
                let Some(AnalyticCurve::Arc {
                    center: c3,
                    radius: r3,
                    normal,
                    basis_u,
                    start_angle,
                    end_angle,
                }) = mesh.edges.get(*eid).and_then(|e| e.curve())
                else {
                    continue;
                };
                let (Some(pa), Some(pb)) = (
                    mesh.verts.get(*va).map(|x| x.pos()),
                    mesh.verts.get(*vb).map(|x| x.pos()),
                ) else {
                    continue;
                };
                let pa2 = project(pa);
                let pb2 = project(pb);
                let ang_a = (pa2.y - c2.y).atan2(pa2.x - c2.x);
                let ang_b = (pb2.y - c2.y).atan2(pb2.x - c2.x);
                // arc 미드포인트(3D stored frame) → project → 어느 CCW 방향에 있나.
                let basis_v = normal.cross(*basis_u).normalize_or_zero();
                let mid_ang = (*start_angle + *end_angle) * 0.5;
                let mid3 = *c3 + (mid_ang.cos() * *basis_u + mid_ang.sin() * basis_v) * *r3;
                let mid2 = project(mid3);
                let mid_ang_2d = (mid2.y - c2.y).atan2(mid2.x - c2.x);
                let ccw_ab = nm(ang_b - ang_a);
                let ccw_am = nm(mid_ang_2d - ang_a);
                let (a0, a1) = if ccw_am <= ccw_ab {
                    (ang_a, ang_a + ccw_ab) // 미드포인트가 CCW a→b 위
                } else {
                    (ang_b, ang_b + nm(ang_a - ang_b)) // CCW b→a 위
                };
                if a1 - a0 > 1e-9 {
                    curves.push(InputCurve::Arc {
                        center: c2,
                        radius: r,
                        a0,
                        a1,
                    });
                }
            }
        }
    }
    // 직선 source 복원: collinear 연결 subseg → full line 병합 (rect 가 split 돼도
    // 원본 edge 로 → arrange 가 매번 fresh source → idempotent).
    for (a, b) in merge_collinear_segments(line_segs, 1e-4) {
        curves.push(InputCurve::Line { a, b });
    }
    curves
}

/// A face on a CURVED analytic surface (Sphere/Cylinder/Cone/Torus/NURBS-class)
/// is NOT a planar region. It must be protected from the coplanar re-derive even
/// when its boundary verts happen to lie on the scan plane — e.g. a Path B sphere
/// hemisphere whose only outer boundary is the equator self-loop anchor at z=0.
/// Without this guard, drawing a coplanar rect at a sphere's equator polygonises
/// + removes the equator edge, collapsing the whole sphere into flat polygons.
/// Faces with NO surface (legacy) or a Plane surface are planar.
fn face_has_curved_surface(mesh: &Mesh, fid: FaceId) -> bool {
    mesh.faces
        .get(fid)
        .and_then(|f| f.surface())
        .is_some_and(|s| !matches!(s, crate::surfaces::AnalyticSurface::Plane { .. }))
}

/// **ADR-186 δ-2** — 주어진 plane 위 모든 coplanar 면을 edge graph 에서 재유도.
///
/// 1. coplanar `Edge` (면 독립) → 2D 사영 수집 + dirty 면 (loop verts 모두 plane 위)
/// 2. `PlanarGraph<u32>` (VertId::raw) 빌드
/// 3. `resolve_and_extract_nested` (교차해결 + containment hole) — split point 는
///    `mesh.add_vertex` 로 새 vert (raw↔VertId map)
/// 4. reconcile — dirty 면 제거 + `add_face_with_holes` (annulus 포함)
pub fn rebuild_coplanar_faces(
    mesh: &mut Mesh,
    plane_origin: DVec3,
    plane_normal: DVec3,
    tol: f64,
) -> Result<RebuildReport> {
    let (u, v) = plane_basis(plane_normal);
    let n_unit = plane_normal.normalize_or_zero();
    let project = |p: DVec3| -> Vec2 {
        let d = p - plane_origin;
        Vec2::new(d.dot(u), d.dot(v))
    };
    let on_plane = |p: DVec3| -> bool { (p - plane_origin).dot(n_unit).abs() < tol };

    // ── Phase 0 (δ-4a) — 3D solid(volume) face 의 boundary edge 수집 → re-derive
    //    에서 배제 (solid wall 보호, manifold 무손상). is_sheet_face = !volume.
    let mut volume_edges: HashSet<EdgeId> = HashSet::new();
    let all_fids: Vec<FaceId> = mesh.faces.iter().map(|(fid, _)| fid).collect();
    for &fid in &all_fids {
        let active = mesh.faces.get(fid).map(|f| f.is_active()).unwrap_or(false);
        // Protect 3D solid walls AND curved-surface faces (Sphere/Cylinder/…) —
        // collect their outer boundary edges so the coplanar re-derive never
        // consumes them. This shields a sphere's equator self-loop (whose anchor
        // sits on the scan plane) from being polygonised + removed.
        let protect = active && (!mesh.is_sheet_face(fid) || face_has_curved_surface(mesh, fid));
        if protect {
            if let Ok(edges) = mesh.face_outer_edges(fid) {
                for e in edges {
                    volume_edges.insert(e);
                }
            }
        }
    }

    // ── Phase 1 (read) — coplanar edge + dirty 면 수집 ──
    let mut existing_map: HashMap<u32, VertId> = HashMap::new();
    let mut coplanar_edges: Vec<(u32, u32, Vec2, Vec2)> = Vec::new();
    // δ-circle (2026-06-02): Path B 원(self-loop edge) tessellation 3D 점. mesh
    // borrow 중이라 수집만 — Phase 1.5 에서 vert 생성 + segment.
    let mut circle_polys3d: Vec<Vec<DVec3>> = Vec::new();
    let mut circle_anchors: HashSet<VertId> = HashSet::new();
    let mut circle_edge_removals: Vec<EdgeId> = Vec::new();
    for (eid, edge) in mesh.edges.iter() {
        if !edge.is_active() || volume_edges.contains(&eid) {
            continue;
        }
        let va = edge.v_small();
        let vb = edge.v_large();
        let pa = match mesh.verts.get(va) {
            Some(x) => x.pos(),
            None => continue,
        };
        let pb = match mesh.verts.get(vb) {
            Some(x) => x.pos(),
            None => continue,
        };
        if !on_plane(pa) || !on_plane(pb) {
            continue;
        }
        // δ-circle: self-loop edge (va==vb) = Path B 원. polygonize 해서 planar
        // graph 에 포함 (skip 안 함) — 원이 다른 도형과 겹쳐도 분할/포함 정합.
        if va == vb {
            if let Some(AnalyticCurve::Circle { center, radius, normal, basis_u }) = edge.curve() {
                if on_plane(*center) {
                    let chord = (radius * 0.05).max(1e-3);
                    let pts = circle::tessellate_full(*center, *radius, *normal, *basis_u, chord);
                    if pts.len() >= 4 {
                        circle_polys3d.push(pts);
                        circle_anchors.insert(va);
                        circle_edge_removals.push(eid);
                    }
                }
            }
            continue;
        }
        existing_map.insert(va.raw(), va);
        existing_map.insert(vb.raw(), vb);
        coplanar_edges.push((va.raw(), vb.raw(), project(pa), project(pb)));
    }
    // ── Phase 1.5 (δ-circle) — 원 tessellation → 새 mesh vert + closed-loop segment ──
    for poly in &circle_polys3d {
        // tessellate_full 은 닫힌(last==first) → 마지막 중복 점 제거.
        let n = if poly.len() >= 2 && (poly[0] - poly[poly.len() - 1]).length() < 1e-9 {
            poly.len() - 1
        } else {
            poly.len()
        };
        if n < 3 {
            continue;
        }
        let mut ring: Vec<(u32, Vec2)> = Vec::with_capacity(n);
        for &p3d in &poly[..n] {
            let vid = mesh.add_vertex(p3d);
            existing_map.insert(vid.raw(), vid);
            ring.push((vid.raw(), project(p3d)));
        }
        for i in 0..n {
            let (a, uva) = ring[i];
            let (b, uvb) = ring[(i + 1) % n];
            coplanar_edges.push((a, b, uva, uvb));
        }
    }
    if coplanar_edges.is_empty() {
        return Ok(RebuildReport::default());
    }
    // dirty 면 — active sheet 면 중 outer loop verts 모두 plane 위.
    // δ-3: material / surface / polygon snapshot (rebuild 후 상속).
    let mut dirty_faces: Vec<DirtyInfo> = Vec::new();
    let mut circle_face_removals: Vec<FaceId> = Vec::new();
    for fid in &all_fids {
        let fid = *fid;
        // δ-4a: sheet 면만 re-derive (3D solid wall + 곡면(Sphere/Cylinder/…) 면 은
        // dirty 제외 — 보호).
        if !mesh.is_sheet_face(fid) || face_has_curved_surface(mesh, fid) {
            continue;
        }
        let face_info = match mesh.faces.get(fid) {
            Some(f) if f.is_active() && !f.outer().start.is_null() => {
                (f.material(), f.surface().cloned(), f.outer().start)
            }
            _ => continue,
        };
        let (material, surface, start) = face_info;
        let verts = match mesh.collect_loop_verts(start) {
            Ok(vv) => vv,
            Err(_) => continue,
        };
        // δ-circle: 원 face (self-loop, 1 vert = circle anchor) → 제거 대상
        // (dirty poly>=3 검출에서 빠지므로 명시 제거 — leftover overlap 방지).
        if verts.len() == 1 && circle_anchors.contains(&verts[0]) {
            circle_face_removals.push(fid);
            continue;
        }
        let mut poly: Vec<Vec2> = Vec::with_capacity(verts.len());
        let mut all_on = true;
        for &vid in &verts {
            let p = match mesh.verts.get(vid) {
                Some(x) => x.pos(),
                None => {
                    all_on = false;
                    break;
                }
            };
            if !on_plane(p) {
                all_on = false;
                break;
            }
            poly.push(project(p));
        }
        if !all_on || poly.len() < 3 {
            continue;
        }
        let area = polygon_signed_area(&poly).abs();
        dirty_faces.push(DirtyInfo {
            fid,
            material,
            surface,
            polygon: poly,
            area,
        });
    }

    // ── Phase 2 (build graph, V = VertId::raw) ──
    let eps = 1e-6;
    let mut g: PlanarGraph<u32> = PlanarGraph::new(eps);
    let mut edge_pairs: BTreeSet<(u32, u32)> = BTreeSet::new();
    for &(raw_a, raw_b, uva, uvb) in &coplanar_edges {
        let va = g.ensure_at(raw_a, uva);
        let vb = g.ensure_at(raw_b, uvb);
        if va == vb {
            continue;
        }
        let key = (va.min(vb), va.max(vb));
        if edge_pairs.insert(key) {
            g.create_edge(va, vb, None);
        }
    }
    let coplanar_edge_count = coplanar_edges.len();

    // ── Phase 3 (resolve + nested extract) — split point = 새 mesh vert ──
    let mut new_map: HashMap<u32, VertId> = HashMap::new();
    let regions = {
        let make_vertex = |uv: Vec2| -> u32 {
            let pos3d = plane_origin + u * uv.x + v * uv.y;
            let vid = mesh.add_vertex(pos3d);
            new_map.insert(vid.raw(), vid);
            vid.raw()
        };
        resolve_and_extract_nested(&mut g, make_vertex)
    };

    // ── Phase 4 (reconcile) — dirty 제거 + add_face_with_holes (δ-3 상속) ──
    let mut removed = 0usize;
    for d in &dirty_faces {
        if mesh.remove_face(d.fid).is_ok() {
            removed += 1;
        }
    }
    // δ-circle: 원 Path B face 도 제거 (polygonized 새 면으로 대체).
    for &fid in &circle_face_removals {
        if mesh.remove_face(fid).is_ok() {
            removed += 1;
        }
    }
    // δ-circle (idempotency): 원 self-loop edge 도 제거 — 안 그러면 다음 rebuild
    // 가 재-polygonize 해서 polygon 중첩 → non-manifold (incremental 누적 차단).
    for &eid in &circle_edge_removals {
        let _ = mesh.remove_edge_and_halfedges(eid);
    }
    let resolve_vid = |raw: u32| -> Option<VertId> {
        existing_map.get(&raw).or_else(|| new_map.get(&raw)).copied()
    };
    let mut created = 0usize;
    let mut created_holes = 0usize;
    for region in &regions {
        let outer: Option<Vec<VertId>> =
            region.outer.verts.iter().map(|&r| resolve_vid(r)).collect();
        let outer = match outer {
            Some(o) if o.len() >= 3 => o,
            _ => continue,
        };
        let holes_vids: Vec<Vec<VertId>> = region
            .holes
            .iter()
            .filter_map(|h| h.verts.iter().map(|&r| resolve_vid(r)).collect::<Option<Vec<_>>>())
            .filter(|h| h.len() >= 3)
            .collect();
        let hole_refs: Vec<&[VertId]> = holes_vids.iter().map(|h| h.as_slice()).collect();
        // δ-3: innermost parent 의 material / surface 상속 (없으면 FORM_MATERIAL).
        // probe = outer boundary 바로 안쪽 점 (annulus hole 회피, centroid 부정확).
        let outer_poly: Vec<Vec2> = region
            .outer
            .verts
            .iter()
            .filter_map(|&r| g.vertices.get(&r).map(|v| v.p))
            .collect();
        let probe = region_interior_point(&outer_poly);
        let parent = innermost_parent(probe, &dirty_faces, eps);
        let mat = parent.map(|p| p.material).unwrap_or(FORM_MATERIAL);
        if let Ok(new_fid) = mesh.add_face_with_holes(&outer, &hole_refs, mat) {
            created += 1;
            created_holes += hole_refs.len();
            if let Some(p) = parent {
                if let Some(surf) = &p.surface {
                    mesh.set_face_surface(new_fid, Some(surf.clone()));
                }
            }
        }
    }

    Ok(RebuildReport {
        removed_faces: removed,
        created_faces: created,
        created_holes,
        coplanar_edges: coplanar_edge_count,
    })
}

/// **helper** — hole loop 이 full circle (1 self-loop Arc 0→2π) 인지.
/// circle hole 은 Phase 4 polygonize 대신 post-process generic split (smooth) 위임.
fn hole_is_full_circle(lp: &[SubCurve]) -> bool {
    lp.len() == 1
        && matches!(&lp[0], SubCurve::Arc { a0, a1, .. }
            if (a1 - a0).abs() >= std::f64::consts::TAU - 1e-6)
}

/// **4-γ helper** — SubCurve loop → 2D polygon (sampling). 면적/centroid/hole용.
fn subcurves_to_poly2d(lp: &[SubCurve]) -> Vec<Vec2> {
    let mut poly = Vec::new();
    for s in lp {
        match s {
            SubCurve::Line { a, .. } => poly.push(*a),
            SubCurve::Arc { center, radius, a0, a1 } => {
                for k in 0..16 {
                    let t = a0 + (a1 - a0) * (k as f64) / 16.0;
                    poly.push(Vec2::new(center.x + radius * t.cos(), center.y + radius * t.sin()));
                }
            }
            // B2/B4 — freeform sub-curve sampled via Freeform2D (area/centroid).
            SubCurve::Freeform { f2d, t0, t1 } => {
                for k in 0..24 {
                    let t = t0 + (t1 - t0) * (k as f64) / 24.0;
                    poly.push(f2d.eval(t));
                }
            }
        }
    }
    poly
}

/// **4-γ helper (D2)** — edge 에 Arc 곡선 부착. edge canonical(v_small→v_large) 에
/// 각도 정렬. add_face 후 호출 (edge 재사용).
fn set_arc_on_edge(
    mesh: &mut Mesh,
    v_from: VertId,
    v_to: VertId,
    center: DVec3,
    radius: f64,
    normal: DVec3,
    basis_u: DVec3,
    a_from: f64,
    a_to: f64,
    owner: Option<u32>,
) {
    let (eid, _) = match mesh.add_edge(v_from, v_to) {
        Ok(x) => x,
        Err(_) => return,
    };
    let small_is_from = mesh.edges.get(eid).map(|e| e.v_small() == v_from).unwrap_or(true);
    let (sa, ea) = if small_is_from { (a_from, a_to) } else { (a_to, a_from) };
    if let Some(e) = mesh.edges.get_mut(eid) {
        e.set_curve(Some(AnalyticCurve::Arc {
            center,
            radius,
            normal,
            basis_u,
            start_angle: sa,
            end_angle: ea,
        }));
    }
    // ADR-186 — D7 중간점 분할로 생긴 한 arc 의 2 반호는 같은 owner 로 묶어
    // **하나의 선택 단위**가 되게 함 (교차점이 아닌 D7 midpoint 에서 잘려 보이는
    // 문제 해소). 진짜 교차로 분리된 별개 arc 끼리는 서로 다른 owner → 분리 유지
    // (작가 정책: 교차점 절단=별개 arc / 비-교차점 절단=한 arc). LOCKED #15 P22.5.
    if let Some(o) = owner {
        mesh.set_edge_curve_owner_id(eid, Some(o));
    }
}

/// **B6** — project a world-space freeform `AnalyticCurve` (the stored source
/// from `freeform_curve_to_source`) to a 2D `Freeform2D` for `arrange`, tagged
/// with `owner`. Inverse of [`unproject_curve`]; mirrors the projection in the
/// (removed) collect_overlap_freeform_inputs (unified SSOT feeding path).
fn project_curve_to_freeform2d(
    c: &AnalyticCurve,
    project: &impl Fn(DVec3) -> Vec2,
    owner: u32,
) -> Option<Freeform2D> {
    let ff = match c {
        AnalyticCurve::Bezier { control_pts } => {
            Freeform2D::bezier(control_pts.iter().map(|p| project(*p)).collect())
        }
        AnalyticCurve::BSpline { control_pts, knots, degree } => Freeform2D::bspline(
            control_pts.iter().map(|p| project(*p)).collect(),
            knots.clone(),
            *degree,
        ),
        AnalyticCurve::NURBS { control_pts, weights, knots, degree } => Freeform2D::nurbs(
            control_pts.iter().map(|p| project(*p)).collect(),
            weights.clone(),
            knots.clone(),
            *degree,
        ),
        _ => return None,
    };
    Some(ff.with_owner(Some(owner)))
}

/// **B4b-2b** — unproject a z=0 (plane-space) freeform curve to world coords by
/// mapping each control point `(x, y, 0)` → `plane_origin + u·x + v·y`.
/// Knots/weights/degree preserved (param-space invariant under the isometry).
fn unproject_curve(c: &AnalyticCurve, unproject: &impl Fn(Vec2) -> DVec3) -> AnalyticCurve {
    let up = |p: DVec3| unproject(Vec2::new(p.x, p.y));
    match c {
        AnalyticCurve::Bezier { control_pts } => AnalyticCurve::Bezier {
            control_pts: control_pts.iter().map(|p| up(*p)).collect(),
        },
        AnalyticCurve::BSpline { control_pts, knots, degree } => AnalyticCurve::BSpline {
            control_pts: control_pts.iter().map(|p| up(*p)).collect(),
            knots: knots.clone(),
            degree: *degree,
        },
        AnalyticCurve::NURBS { control_pts, weights, knots, degree } => AnalyticCurve::NURBS {
            control_pts: control_pts.iter().map(|p| up(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
            degree: *degree,
        },
        other => other.clone(),
    }
}

/// **B4b-2b** — extract the world-space sub-curve over `[t0, t1]` of a 2D
/// freeform (B1 `split_at` ×2; U-B4b-1 machine-ε exact). Bezier sub-ranges
/// re-parameterise to `[0,1]` (rescale `(t1-t0)/(r1-t0)`); BSpline/NURBS
/// preserve param. Returns the sub-curve in WORLD coords (unprojected).
fn extract_world_subcurve(
    f2d: &Freeform2D,
    t0: f64,
    t1: f64,
    unproject: &impl Fn(Vec2) -> DVec3,
) -> Option<AnalyticCurve> {
    let full = f2d.to_curve3d(); // z=0 plane-space curve
    let (_r0, r1) = f2d.param_range();
    let dummy = VertId::default(); // split_at mid_vert unused for freeform
    let (_, right) = full.split_at(t0, dummy).ok()?; // [t0, r1]
    let t1_in_right = if f2d.knots.is_empty() {
        // Bezier: `right` re-parameterised to [0,1].
        if (r1 - t0).abs() < 1e-12 {
            return None;
        }
        (t1 - t0) / (r1 - t0)
    } else {
        // BSpline/NURBS: split_at preserves param.
        t1
    };
    let (mid, _) = right.split_at(t1_in_right, dummy).ok()?; // [t0, t1]
    Some(unproject_curve(&mid, unproject))
}

/// **B4b-2b** — attach a world-space freeform sub-curve to the edge `v_from →
/// v_to` + tag its `curve_owner_id` (B6 idempotency link). Direction-agnostic:
/// the render samplers orient by endpoints, so no canonical reversal is needed.
fn set_freeform_on_edge(
    mesh: &mut Mesh,
    v_from: VertId,
    v_to: VertId,
    curve: AnalyticCurve,
    owner: Option<u32>,
) {
    if v_from == v_to {
        return;
    }
    let (eid, _) = match mesh.add_edge(v_from, v_to) {
        Ok(x) => x,
        Err(_) => return,
    };
    if let Some(e) = mesh.edges.get_mut(eid) {
        e.set_curve(Some(curve));
    }
    if let Some(o) = owner {
        mesh.set_edge_curve_owner_id(eid, Some(o));
    }
}

/// **ADR-186 A3 / Option B (B4b-1)** — freeform-freeform overlap detection.
///
/// Find active freeform (Bezier/BSpline/NURBS) self-loop faces that OVERLAP
/// another curve — another freeform (B3) OR a rect/circle (B5) — with ≥2 CCI
/// intersections via `intersect_curves`. For each overlapping freeform,
/// allocate a `curve_owner_id` (ADR-088, shared space), set it on the
/// self-loop edge, and store the ORIGINAL curve in `freeform_curve_to_source`
/// (B4a map) so B6's reconstruct can restore the source by owner-id (P5
/// idempotency trap fix).
///
/// **Detection-only**: does NOT feed `arrange`, override A1, or split faces —
/// face topology UNCHANGED. The owner-id makes the freeform non-A1-preserved
/// so reconstruct feeds it to `arrange`, where the B5-1 intersect arm splits.
/// Idempotent: a freeform that already has an owner-id is skipped.
/// **B5 (mixed)**: freeform×freeform (B3) AND freeform×(rect Line / circle)
/// overlap. Containment (0 crossings) → A2 hole path; disjoint → preserved.
fn detect_freeform_overlaps(
    mesh: &mut Mesh,
    plane_origin: DVec3,
    n_unit: DVec3,
    tol: f64,
    scope_edges: Option<&HashSet<EdgeId>>,
) {
    use crate::curves::AnalyticCurve;
    let on_plane = |p: DVec3| (p - plane_origin).dot(n_unit).abs() < tol;
    // Collect active freeform self-loop faces: (self-loop edge, curve clone).
    let fids: Vec<FaceId> = mesh.faces.iter().map(|(fid, _)| fid).collect();
    let mut freeforms: Vec<(EdgeId, AnalyticCurve)> = Vec::new();
    for fid in fids {
        let start = match mesh.faces.get(fid) {
            Some(f) if f.is_active() && !f.outer().start.is_null() => f.outer().start,
            _ => continue,
        };
        let single = mesh.collect_loop_hes(start).map_or(false, |h| h.len() == 1);
        if !single {
            continue;
        }
        let e0 = mesh.hes[start].edge();
        let curve = match mesh.edges.get(e0).and_then(|e| e.curve().cloned()) {
            Some(
                c @ (AnalyticCurve::Bezier { .. }
                | AnalyticCurve::BSpline { .. }
                | AnalyticCurve::NURBS { .. }),
            ) => c,
            _ => continue,
        };
        // Option A — restrict to the affected region (None = full plane).
        if let Some(s) = scope_edges {
            if !s.contains(&e0) {
                continue;
            }
        }
        // on-plane via anchor vertex.
        let anchor = match mesh.edges.get(e0).map(|e| e.v_small()) {
            Some(va) => va,
            None => continue,
        };
        match mesh.verts.get(anchor).map(|v| v.pos()) {
            Some(p) if on_plane(p) => {}
            _ => continue,
        }
        freeforms.push((e0, curve));
    }
    if freeforms.is_empty() {
        return;
    }
    // B5 — collect non-freeform coplanar curves (rect Line edges, Circle/Arc
    // edges) so a freeform overlapping a rect/circle is also detected. Straight
    // edge (no curve) → AnalyticCurve::Line{v_small,v_large}; Circle/Arc → the
    // stored curve. Freeform edges are handled via the `freeforms` vec.
    let eids: Vec<EdgeId> = mesh.edges.iter().map(|(eid, _)| eid).collect();
    let mut others: Vec<AnalyticCurve> = Vec::new();
    for eid in eids {
        // Option A — restrict to the affected region (None = full plane).
        if let Some(s) = scope_edges {
            if !s.contains(&eid) {
                continue;
            }
        }
        let (curve_opt, va, vb) = match mesh.edges.get(eid) {
            Some(e) if e.is_active() => (e.curve().cloned(), e.v_small(), e.v_large()),
            _ => continue,
        };
        // skip freeform edges (collected separately).
        if matches!(
            curve_opt,
            Some(
                AnalyticCurve::Bezier { .. }
                    | AnalyticCurve::BSpline { .. }
                    | AnalyticCurve::NURBS { .. }
            )
        ) {
            continue;
        }
        // both endpoints on-plane.
        match mesh.verts.get(va).map(|v| v.pos()) {
            Some(p) if on_plane(p) => {}
            _ => continue,
        }
        match mesh.verts.get(vb).map(|v| v.pos()) {
            Some(p) if on_plane(p) => {}
            _ => continue,
        }
        match curve_opt {
            Some(c @ (AnalyticCurve::Circle { .. } | AnalyticCurve::Arc { .. })) => {
                others.push(c)
            }
            None | Some(AnalyticCurve::Line { .. }) => {
                if va != vb {
                    others.push(AnalyticCurve::Line { start: va, end: vb });
                }
            }
            _ => {}
        }
    }
    // Phase A (read) — overlap indices (≥2 intersections). freeform×freeform
    // (B3) AND freeform×non-freeform (B5).
    let mut overlap: HashSet<usize> = HashSet::new();
    for i in 0..freeforms.len() {
        for j in (i + 1)..freeforms.len() {
            let hits = crate::curves::intersect::intersect_curves(
                &freeforms[i].1, &freeforms[j].1, mesh, tol,
            )
            .map(|v| v.len())
            .unwrap_or(0);
            if hits >= 2 {
                overlap.insert(i);
                overlap.insert(j);
            }
        }
    }
    // B5 — freeform × non-freeform (rect/circle). Sum crossings across all
    // coplanar non-freeform curves; ≥2 total = boundary crossed = overlap
    // (containment = 0 crossings → A2 hole path; disjoint = 0 → preserved).
    for i in 0..freeforms.len() {
        if overlap.contains(&i) {
            continue;
        }
        let mut crossings = 0usize;
        for oc in &others {
            crossings += crate::curves::intersect::intersect_curves(
                &freeforms[i].1, oc, mesh, tol,
            )
            .map(|v| v.len())
            .unwrap_or(0);
            if crossings >= 2 {
                break;
            }
        }
        if crossings >= 2 {
            overlap.insert(i);
        }
    }
    // Phase B (mutate) — allocate owner-id + store source (idempotent).
    for &k in &overlap {
        let eid = freeforms[k].0;
        if mesh.edge_curve_owner_id(eid).is_some() {
            continue; // already detected (idempotent)
        }
        let owner = mesh.next_curve_owner_id();
        mesh.set_edge_curve_owner_id(eid, Some(owner));
        mesh.set_freeform_curve_source(owner, freeforms[k].1.clone());
    }
}

/// **4-γ (ADR-186 A)** — analytic arrangement 기반 coplanar 면 재유도.
/// (B4b wrapper — freeform overlap gate OFF by default.)
///
/// `rebuild_coplanar_faces` 의 polygon 경로를 `arrange()` 로 교체. 원을 깎지 않고
/// arc 경계 면 직접 생성:
/// - **D7 중간점 분할**: 각 arc 를 중간점 1개로 2 반호 (DCEL multigraph 회피)
/// - **D1 polygon hole**: 원 hole 은 polygonize (smooth hole 은 후속 ADR)
/// - **idempotency**: 4-β arc→circle 병합으로 source 복원 → incremental==single
pub fn rebuild_coplanar_faces_analytic(
    mesh: &mut Mesh,
    plane_origin: DVec3,
    plane_normal: DVec3,
    tol: f64,
) -> Result<RebuildReport> {
    rebuild_coplanar_faces_analytic_with_overlap(mesh, plane_origin, plane_normal, tol, false)
}

/// **ADR-186 A3 / Option B (B4b)** — `rebuild_coplanar_faces_analytic` with the
/// freeform-overlap gate. `enable_freeform_overlap = true` activates Phase 0.5
/// freeform-freeform overlap detection (B4b-1: owner-id + source-curve storage)
/// and — once B4b-2 lands — lens routing. Production passes `false` until B6
/// flips it on (engine default OFF, ADR-049 P-5e-α pattern).
///
/// Full-plane re-derive (no scope). Delegates to `_scoped` with `seed = None`.
/// Tests + the 4-arg wrapper + idempotency exercise this path.
pub fn rebuild_coplanar_faces_analytic_with_overlap(
    mesh: &mut Mesh,
    plane_origin: DVec3,
    plane_normal: DVec3,
    tol: f64,
    enable_freeform_overlap: bool,
) -> Result<RebuildReport> {
    rebuild_coplanar_faces_analytic_scoped(
        mesh,
        plane_origin,
        plane_normal,
        tol,
        enable_freeform_overlap,
        None,
    )
}

/// **ADR-186 Option A (perf scope, 2026-06-05)** — affected-region re-derive.
///
/// `seed` = the just-drawn faces. When `Some(non-empty)` and at least one seed
/// face is coplanar, only the **connected component** (by 2D AABB overlap) the
/// new shape touches is re-derived; disjoint coplanar faces are left untouched
/// (their derivation is independent — a shape can only intersect what its
/// bounding box overlaps). Turns the O(coplanar-edges²) `arrange` into
/// O(affected²) — near O(1) for disjoint draws (사용자 "도구 작동이 매우 느림";
/// 6 disjoint pies measured 43→880 ms quadratic before).
///
/// `seed = None` / empty / no coplanar seed → full-plane re-derive (identical
/// to the historical behavior). Correctness vs full: untouched components are
/// byte-identical because the full path recreates them idempotently (B6) —
/// skipping == recreating.
pub fn rebuild_coplanar_faces_analytic_scoped(
    mesh: &mut Mesh,
    plane_origin: DVec3,
    plane_normal: DVec3,
    tol: f64,
    enable_freeform_overlap: bool,
    seed: Option<&[FaceId]>,
) -> Result<RebuildReport> {
    let (u, v) = plane_basis(plane_normal);
    let n_unit = plane_normal.normalize_or_zero();
    let project = |p: DVec3| -> Vec2 {
        let d = p - plane_origin;
        Vec2::new(d.dot(u), d.dot(v))
    };
    let unproject = |xy: Vec2| -> DVec3 { plane_origin + u * xy.x + v * xy.y };
    let on_plane = |p: DVec3| (p - plane_origin).dot(n_unit).abs() < tol;
    let cpt = |c: Vec2, r: f64, a: f64| -> Vec2 {
        Vec2::new(c.x + r * a.cos(), c.y + r * a.sin())
    };
    let two_pi = 2.0 * std::f64::consts::PI;

    // ── Phase 0 — volume(3D solid) edge 보호.
    // **coplanarity 기준** (is_sheet_face 대신): off-plane 면(vert 하나라도 plane 밖)
    // 의 edge 만 보호. is_sheet_face(=!is_face_in_volume)는 fully-surrounded 평면 면
    // (모든 edge 2-face 공유, 예: arc-bounded semi)을 volume 으로 오판 → 그 edge 가
    // 잘못 보호되어 re-rebuild 시 reconstruct 에서 누락되던 근본 버그.
    let mut volume_edges: HashSet<EdgeId> = HashSet::new();
    let all_fids: Vec<FaceId> = mesh.faces.iter().map(|(fid, _)| fid).collect();
    for &fid in &all_fids {
        let (active, start) = match mesh.faces.get(fid) {
            Some(f) => (f.is_active(), f.outer().start),
            None => continue,
        };
        if !active || start.is_null() {
            continue;
        }
        let off_plane = mesh.collect_loop_verts(start).ok().map_or(false, |vv| {
            vv.iter()
                .any(|&vid| mesh.verts.get(vid).map_or(false, |v| !on_plane(v.pos())))
        });
        // Protect off-plane faces (3D solid walls) AND curved-surface faces. A
        // curved face (e.g. a sphere hemisphere) has its boundary (the equator
        // self-loop) ON the plane but its surface is NOT planar — without this the
        // equator gets polygonised + consumed, splitting the drawn rect and
        // collapsing the sphere into flat polygons.
        if off_plane || face_has_curved_surface(mesh, fid) {
            if let Ok(edges) = mesh.face_outer_edges(fid) {
                for e in edges {
                    volume_edges.insert(e);
                }
            }
        }
    }

    // ── Option A scope (perf, 2026-06-05) — restrict to the affected region
    //    when `seed` (the just-drawn faces) is given. The new shape can only
    //    change the coplanar faces its bbox overlaps; disjoint faces stay
    //    untouched. `affected_faces` (BFS over AABB overlap) drives which
    //    faces are removed/re-derived; `affected_edges` (boxes overlapping the
    //    region) drives reconstruct / detect / edge removal. None = full plane.
    let scope_margin = (tol * 10.0).max(1e-3);
    let scope: Option<(HashSet<FaceId>, HashSet<EdgeId>)> = match seed {
        Some(s) if !s.is_empty() => {
            let affected_faces =
                affected_face_component(mesh, &all_fids, s, &project, &on_plane, scope_margin);
            if affected_faces.is_empty() {
                None // seed not coplanar / not found → safe full-plane fallback
            } else {
                let face_aabbs: Vec<Aabb2> = affected_faces
                    .iter()
                    .filter_map(|&f| face_aabb_2d_coplanar(mesh, f, &project, &on_plane))
                    .collect();
                let mut affected_edges: HashSet<EdgeId> = HashSet::new();
                for (eid, edge) in mesh.edges.iter() {
                    if !edge.is_active() || volume_edges.contains(&eid) {
                        continue;
                    }
                    let on = match (mesh.verts.get(edge.v_small()), mesh.verts.get(edge.v_large()))
                    {
                        (Some(a), Some(b)) => on_plane(a.pos()) && on_plane(b.pos()),
                        _ => false,
                    };
                    if !on {
                        continue;
                    }
                    if let Some(ea) = edge_aabb_2d(mesh, eid, &project) {
                        if face_aabbs.iter().any(|a| a.overlaps(&ea, scope_margin)) {
                            affected_edges.insert(eid);
                        }
                    }
                }
                Some((affected_faces, affected_edges))
            }
        }
        _ => None,
    };
    let scope_edges: Option<&HashSet<EdgeId>> = scope.as_ref().map(|(_, e)| e);

    // ── Draw-onto-solid guard (2026-06-09) — skip the re-derive when the
    //    affected coplanar region overlaps a solid. A solid face on this plane
    //    (an on-plane face sharing an edge with an off-plane wall) is protected
    //    from face removal (`part_of_solid`), but re-deriving the surrounding
    //    free region removes/re-arranges the cut edges it shares with that solid
    //    face — dangling the solid's loop (an "Entity HeId not found" panic that
    //    leaked the wasm-bindgen borrow → "recursive use" spam) or making those
    //    edges 3-way non-manifold. The draw itself already produced a valid mesh;
    //    leave it as-is. This is also what drawing a footprint onto an existing
    //    wall wants (a new sheet to extrude, not an annulus in the wall's face).
    //    No-op without a solid on this plane (`volume_edges` empty), so the
    //    flat-sheet annulus/containment rederive cases are unchanged.
    if !volume_edges.is_empty() {
        let region_touches_solid = match &scope {
            Some((affected_faces, _)) => affected_faces.iter().any(|&f| {
                mesh.face_outer_edges(f)
                    .ok()
                    .map_or(false, |es| es.iter().any(|e| volume_edges.contains(e)))
            }),
            // full-plane rederive with a solid present → conservatively skip.
            None => true,
        };
        if region_touches_solid {
            return Ok(RebuildReport::default());
        }
    }

    // ── Phase 0.5 (B4b-1, gated) — freeform-freeform overlap detection +
    //    owner-id + source-curve storage (B4a map). Detection-only: face
    //    topology UNCHANGED (no feeding / A1 override / split — those are
    //    B4b-2). Runs before reconstruct so B4b-2 can consume the owner-id.
    if enable_freeform_overlap {
        detect_freeform_overlaps(mesh, plane_origin, n_unit, tol, scope_edges);
    }

    // ── Phase 1 — InputCurve 재구성 (4-β) + dirty 면 + 제거할 coplanar edge.
    //    B6 — reconstruct also restores overlap freeform sources by owner-id
    //    (Phase 0.5 sets them when gate on), so feeding is unified into
    //    reconstruct (the separate B4b-2a collect step is removed). The
    //    `enable_freeform_overlap` gate now controls only Phase 0.5 detection;
    //    reconstruct / A1 override are gate-implicit (owner-id presence).
    let input_curves = reconstruct_input_curves(
        mesh,
        plane_origin,
        u,
        v,
        n_unit,
        tol,
        &volume_edges,
        scope_edges,
    );
    if input_curves.is_empty() {
        return Ok(RebuildReport::default());
    }
    // ADR-200 (A1, β-2) — 부분 arc 는 이제 `InputCurve::Arc` 로 arrange 에 투입
    // (reconstruct 내부)되어 정상 재유도되므로, A2 의 보존(removal 제외) 로직은
    // 불필요 — 제거. 부분 arc 면/엣지는 일반 dirty 면처럼 제거 후 arrange 가
    // 재생성 (호가 1급 참여).
    let mut dirty_faces: Vec<DirtyInfo> = Vec::new();
    let mut faces_to_remove: Vec<FaceId> = Vec::new();
    for &fid in &all_fids {
        // Option A — skip faces outside the affected region (None = full plane).
        if let Some((ref affected_faces, _)) = scope {
            if !affected_faces.contains(&fid) {
                continue;
            }
        }
        let (material, surface, start) = match mesh.faces.get(fid) {
            Some(f) if f.is_active() && !f.outer().start.is_null() => {
                (f.material(), f.surface().cloned(), f.outer().start)
            }
            _ => continue,
        };
        let verts = match mesh.collect_loop_verts(start) {
            Ok(vv) => vv,
            Err(_) => continue,
        };
        let mut poly: Vec<Vec2> = Vec::new();
        let mut all_on = true;
        for &vid in &verts {
            let p = match mesh.verts.get(vid) {
                Some(x) => x.pos(),
                None => {
                    all_on = false;
                    break;
                }
            };
            if !on_plane(p) {
                all_on = false;
                break;
            }
            poly.push(project(p));
        }
        if !all_on {
            continue; // off-plane 면 보호 (3D solid wall)
        }
        // 평면상이지만 solid 일부면 (edge 가 volume_edges 에 = off-plane 면과 공유,
        // 예: box bottom) → 보호 (re-derive 안 함).
        let part_of_solid = mesh
            .face_outer_edges(fid)
            .ok()
            .map_or(false, |edges| edges.iter().any(|e| volume_edges.contains(e)));
        if part_of_solid {
            continue;
        }
        // A1 (2026-06-03) — closed Bezier/BSpline/NURBS self-loop face 보존.
        // arrange 가 freeform closed curve 미지원 (reconstruct 가 self-loop skip)
        // → 제거하면 재생성 안 돼 소멸 (데이터 손실, 시뮬레이션 (B) rect+bezier).
        // Circle 은 arrange(InputCurve::Circle)가 재생성하므로 제외 안 함.
        if verts.len() == 1 {
            let e0 = mesh.hes[start].edge();
            let is_freeform = mesh.edges.get(e0).map_or(false, |e| {
                matches!(
                    e.curve(),
                    Some(AnalyticCurve::Bezier { .. })
                        | Some(AnalyticCurve::BSpline { .. })
                        | Some(AnalyticCurve::NURBS { .. })
                )
            });
            // B4b-2a — overlap freeform (curve_owner_id set by Phase 0.5) is
            // fed to arrange + REMOVED here so lens sub-faces replace it.
            // Standalone/contained freeform → preserve (A1, no owner-id).
            let is_overlap = mesh.edges.get(e0).and_then(|e| e.curve_owner_id()).is_some();
            if is_freeform && !is_overlap {
                continue;
            }
        }
        faces_to_remove.push(fid);
        if poly.len() >= 3 {
            let area = polygon_signed_area(&poly).abs();
            dirty_faces.push(DirtyInfo { fid, material, surface, polygon: poly, area });
        }
        // poly < 3 (self-loop 원 face) → 제거만.
    }
    let mut edges_to_remove: Vec<EdgeId> = Vec::new();
    for (eid, edge) in mesh.edges.iter() {
        if !edge.is_active() || volume_edges.contains(&eid) {
            continue;
        }
        // Option A — skip edges outside the affected region (None = full plane).
        if let Some((_, ref affected_edges)) = scope {
            if !affected_edges.contains(&eid) {
                continue;
            }
        }
        // A1 — closed Bezier/BSpline/NURBS self-loop edge 보존 (face 보존과 정합).
        // B4b-2a — UNLESS overlap-detected (curve_owner_id set by Phase 0.5):
        // overlap freeform is removed so lens sub-curves replace it.
        if edge.v_small() == edge.v_large()
            && matches!(
                edge.curve(),
                Some(AnalyticCurve::Bezier { .. })
                    | Some(AnalyticCurve::BSpline { .. })
                    | Some(AnalyticCurve::NURBS { .. })
            )
            && edge.curve_owner_id().is_none()
        {
            continue;
        }
        let pa = match mesh.verts.get(edge.v_small()) {
            Some(x) => x.pos(),
            None => continue,
        };
        let pb = match mesh.verts.get(edge.v_large()) {
            Some(x) => x.pos(),
            None => continue,
        };
        if on_plane(pa) && on_plane(pb) {
            edges_to_remove.push(eid);
        }
    }

    // Finding #1 (2026-06-16) — preserve free wires (edges bounding NO active
    // face). The rederive removes every coplanar edge then rebuilds only the
    // closed faces returned by `arrange`; free segments (e.g. a line's tails
    // outside a circle it was drawn over) are not faces, so they would be lost.
    // ADR-016 §2: wires stay until the user deletes them. Edges that DO bound a
    // face are still removed + rebuilt as before.
    edges_to_remove.retain(|&eid| {
        let (adj, _) = mesh.get_faces_sharing_edge(eid);
        adj.iter()
            .any(|&f| mesh.faces.get(f).map(|fc| fc.is_active()).unwrap_or(false))
    });

    // ── Phase 2 — analytic arrangement.
    let faces = arrange(&input_curves, 1e-4);

    // ── Phase 3 — clean slate.
    let mut candidate_verts: HashSet<VertId> = HashSet::new();
    for &eid in &edges_to_remove {
        if let Some(e) = mesh.edges.get(eid) {
            candidate_verts.insert(e.v_small());
            candidate_verts.insert(e.v_large());
        }
    }
    let mut removed = 0usize;
    for &fid in &faces_to_remove {
        if mesh.remove_face(fid).is_ok() {
            removed += 1;
        }
    }
    for &eid in &edges_to_remove {
        let _ = mesh.remove_edge_and_halfedges(eid);
    }
    // orphan vert deactivate — stale HE ref 가진 옛 vert 를 add_vertex dedup 이 재사용해
    // wiring 손상되는 것 방지 (counts 동일하나 non-manifold 되던 근본).
    let mut used: HashSet<VertId> = HashSet::new();
    for (_, e) in mesh.edges.iter() {
        if e.is_active() {
            used.insert(e.v_small());
            used.insert(e.v_large());
        }
    }
    for &vid in &candidate_verts {
        if !used.contains(&vid) {
            if let Some(vt) = mesh.verts.get_mut(vid) {
                vt.set_active(false);
            }
        }
    }

    // ── Phase 4 — ArrFace → DCEL.
    let mut created = 0usize;
    let mut created_holes = 0usize;
    // P0.1 (ADR-190 Phase 0) — every materialized face lies on the known
    // re-derive plane, so it MUST carry a Plane surface (ADR-079 L3: result faces
    // = AnalyticSurface attached). Without this, a face derived from a self-loop
    // (Circle) parent — whose 1-vertex boundary is excluded from `dirty_faces` —
    // ended up surfaceless and Push/Pull failed hard ("NoProfileSurface"); the
    // ADR-189 arc split exposed it. Inherit the parent's exact surface when
    // present, else synthesize one from the plane.
    let default_plane_surface = AnalyticSurface::Plane {
        origin: plane_origin,
        normal: plane_normal,
        basis_u: u,
        u_range: (-1.0e6, 1.0e6),
        v_range: (-1.0e6, 1.0e6),
    };
    for af in &faces {
        let outer_poly = subcurves_to_poly2d(&af.outer);
        if outer_poly.len() < 3 {
            // standalone full circle (단일 Arc 0→2π) 처리 아래에서.
        }
        let probe = region_interior_point(&outer_poly);
        let parent = innermost_parent(probe, &dirty_faces, 1e-6);
        let mat = parent.map(|p| p.material).unwrap_or(FORM_MATERIAL);
        let inherit_surface = parent
            .and_then(|p| p.surface.clone())
            .or_else(|| Some(default_plane_surface.clone()));

        // standalone full circle?
        if af.outer.len() == 1 {
            if let SubCurve::Arc { center, radius, a0, a1 } = &af.outer[0] {
                if (a1 - a0).abs() >= two_pi - 1e-6 {
                    let anchor = mesh.add_vertex(unproject(cpt(*center, *radius, *a0)));
                    let circ = AnalyticCurve::Circle {
                        center: unproject(*center),
                        radius: *radius,
                        normal: plane_normal,
                        basis_u: u,
                    };
                    if let Ok(fid) = mesh.add_face_closed_curve(anchor, circ, mat) {
                        created += 1;
                        if let Some(s) = &inherit_surface {
                            mesh.set_face_surface(fid, Some(s.clone()));
                        }
                    }
                    continue;
                }
            }
        }

        // 일반 면 — SubCurve → 세그먼트 (arc 는 D7 중간점 분할).
        // arc 튜플 5번째 = owner-id (D7 두 반호를 한 선택 단위로 묶음).
        let mut seg_verts: Vec<VertId> = Vec::new();
        let mut seg_arcs: Vec<Option<(DVec3, f64, f64, f64, Option<u32>)>> = Vec::new();
        // B4b-2b — parallel freeform spec per segment (world sub-curve +
        // owner-id). None for Line/Arc; Some for freeform sub-bezier edges.
        let mut seg_freeform: Vec<Option<(AnalyticCurve, Option<u32>)>> = Vec::new();
        for sc in &af.outer {
            match sc {
                SubCurve::Line { a, .. } => {
                    seg_verts.push(mesh.add_vertex(unproject(*a)));
                    seg_arcs.push(None);
                    seg_freeform.push(None);
                }
                SubCurve::Arc { center, radius, a0, a1 } => {
                    let amid = (a0 + a1) / 2.0;
                    let c3 = unproject(*center);
                    // 이 arc 의 2 반호(D7 분할)에 공유 owner — 한 선택 단위.
                    let arc_owner = mesh.next_curve_owner_id();
                    seg_verts.push(mesh.add_vertex(unproject(cpt(*center, *radius, *a0))));
                    seg_arcs.push(Some((c3, *radius, *a0, amid, Some(arc_owner))));
                    seg_freeform.push(None);
                    seg_verts.push(mesh.add_vertex(unproject(cpt(*center, *radius, amid))));
                    seg_arcs.push(Some((c3, *radius, amid, *a1, Some(arc_owner))));
                    seg_freeform.push(None);
                }
                // B4b-2b — smooth freeform attach: D7 midpoint split (lens
                // 2-vert → 4-vert + P-Q multigraph 회피) + sub-bezier per half.
                SubCurve::Freeform { f2d, t0, t1 } => {
                    let tmid = (t0 + t1) / 2.0;
                    let sub_a = extract_world_subcurve(f2d, *t0, tmid, &unproject);
                    let sub_b = extract_world_subcurve(f2d, tmid, *t1, &unproject);
                    seg_verts.push(mesh.add_vertex(unproject(f2d.eval(*t0))));
                    seg_arcs.push(None);
                    seg_freeform.push(sub_a.map(|c| (c, f2d.owner_id)));
                    seg_verts.push(mesh.add_vertex(unproject(f2d.eval(tmid))));
                    seg_arcs.push(None);
                    seg_freeform.push(sub_b.map(|c| (c, f2d.owner_id)));
                }
            }
        }
        if seg_verts.len() < 3 {
            continue;
        }
        // holes — **원형(full-circle) hole 은 skip** (circle-in-polygon containment 은
        // Scene post-process 의 split_face_by_inner_circle_generic 가 smooth self-loop
        // hole 로 위임). 그 외(peanut union 등 mixed/arc hole)는 **outer 와 동일한
        // arc-aware 구성** (D7 중간점 분할 + arc curve 부착) → sub-face(lens/초승달)와
        // 정점 공유 → smooth shared-edge hole (폴리곤 잔재 0, (A) fully-analytic 정합).
        let mut hole_vlists: Vec<Vec<VertId>> = Vec::new();
        let mut hole_arcs: Vec<Vec<Option<(DVec3, f64, f64, f64, Option<u32>)>>> = Vec::new();
        // B4b-2b — parallel freeform spec per hole segment.
        let mut hole_freeform: Vec<Vec<Option<(AnalyticCurve, Option<u32>)>>> = Vec::new();
        for h in af.holes.iter() {
            if hole_is_full_circle(h) {
                continue; // post-process 위임 (단일 원 smooth hole)
            }
            let mut hv: Vec<VertId> = Vec::new();
            let mut ha: Vec<Option<(DVec3, f64, f64, f64, Option<u32>)>> = Vec::new();
            let mut hf: Vec<Option<(AnalyticCurve, Option<u32>)>> = Vec::new();
            for sc in h {
                match sc {
                    SubCurve::Line { a, .. } => {
                        hv.push(mesh.add_vertex(unproject(*a)));
                        ha.push(None);
                        hf.push(None);
                    }
                    SubCurve::Arc { center, radius, a0, a1 } => {
                        let amid = (a0 + a1) / 2.0;
                        let c3 = unproject(*center);
                        let arc_owner = mesh.next_curve_owner_id();
                        hv.push(mesh.add_vertex(unproject(cpt(*center, *radius, *a0))));
                        ha.push(Some((c3, *radius, *a0, amid, Some(arc_owner))));
                        hf.push(None);
                        hv.push(mesh.add_vertex(unproject(cpt(*center, *radius, amid))));
                        ha.push(Some((c3, *radius, amid, *a1, Some(arc_owner))));
                        hf.push(None);
                    }
                    // B4b-2b — smooth freeform hole (D7 midpoint + sub-bezier).
                    SubCurve::Freeform { f2d, t0, t1 } => {
                        let tmid = (t0 + t1) / 2.0;
                        let sub_a = extract_world_subcurve(f2d, *t0, tmid, &unproject);
                        let sub_b = extract_world_subcurve(f2d, tmid, *t1, &unproject);
                        hv.push(mesh.add_vertex(unproject(f2d.eval(*t0))));
                        ha.push(None);
                        hf.push(sub_a.map(|c| (c, f2d.owner_id)));
                        hv.push(mesh.add_vertex(unproject(f2d.eval(tmid))));
                        ha.push(None);
                        hf.push(sub_b.map(|c| (c, f2d.owner_id)));
                    }
                }
            }
            if hv.len() >= 3 {
                hole_vlists.push(hv);
                hole_arcs.push(ha);
                hole_freeform.push(hf);
            }
        }
        let hole_refs: Vec<&[VertId]> = hole_vlists.iter().map(|h| h.as_slice()).collect();

        if let Ok(new_fid) = mesh.add_face_with_holes(&seg_verts, &hole_refs, mat) {
            created += 1;
            created_holes += hole_refs.len();
            if let Some(s) = &inherit_surface {
                mesh.set_face_surface(new_fid, Some(s.clone()));
            }
            // outer arc 곡선 부착 (D2) + B4b-2b freeform sub-bezier 부착 + owner-id.
            let nseg = seg_verts.len();
            for i in 0..nseg {
                let v_from = seg_verts[i];
                let v_to = seg_verts[(i + 1) % nseg];
                if let Some((c3, r, af0, af1, arc_owner)) = seg_arcs[i] {
                    set_arc_on_edge(mesh, v_from, v_to, c3, r, plane_normal, u, af0, af1, arc_owner);
                }
                if let Some((curve, owner)) = seg_freeform[i].clone() {
                    set_freeform_on_edge(mesh, v_from, v_to, curve, owner);
                }
            }
            // hole arc 곡선 부착 (smooth shared-edge hole) + B4b-2b freeform.
            for ((hv, ha), hf) in hole_vlists
                .iter()
                .zip(hole_arcs.iter())
                .zip(hole_freeform.iter())
            {
                let hn = hv.len();
                for i in 0..hn {
                    let v_from = hv[i];
                    let v_to = hv[(i + 1) % hn];
                    if let Some((c3, r, af0, af1, arc_owner)) = ha[i] {
                        set_arc_on_edge(mesh, v_from, v_to, c3, r, plane_normal, u, af0, af1, arc_owner);
                    }
                    if let Some((curve, owner)) = hf[i].clone() {
                        set_freeform_on_edge(mesh, v_from, v_to, curve, owner);
                    }
                }
            }
            // **분할 경계 가시화 (메타-원칙 #15, 사용자 보고 2026-06-03 "경계선을
            // 보이게")** — outer + hole division edge 에 HARD flag. coplanar 여도
            // wireframe emit (LOCKED #16 coplanar hide 우회, force_hard fast-path).
            // ADR-101 split path 가 이미 HARD 부여 → face_rederive 도 동일 contract.
            // 원/분할 경계가 보여야 derived-face 구조 (어디서 나뉘었는지) 확인 가능.
            let mut closed_outer = seg_verts.clone();
            closed_outer.push(seg_verts[0]);
            mesh.mark_chain_edges_hard(&closed_outer);
            for hv in &hole_vlists {
                if hv.is_empty() {
                    continue;
                }
                let mut closed = hv.clone();
                closed.push(hv[0]);
                mesh.mark_chain_edges_hard(&closed);
            }
        }
    }

    // ── A2 (2026-06-03) — freeform closed curve containment → smooth hole.
    // A1 으로 보존된 Bezier/BSpline/NURBS self-loop face 가 다른 polygon 면 안에
    // 완전 포함되면 reparent (inner self-loop twin HE → outer hole). Circle 은 scene
    // post-process(split_face_by_inner_circle_generic)가 처리하므로 제외 (중복 방지).
    // overlap 은 A3 (arrange curve 교차). containment only.
    let active_fids: Vec<FaceId> =
        mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(fid, _)| fid).collect();
    for &inner in &active_fids {
        // freeform (bezier/bspline/nurbs) self-loop 만 — circle 제외.
        let is_freeform = {
            let start = match mesh.faces.get(inner) {
                Some(f) if f.is_active() => f.outer().start,
                _ => continue,
            };
            if start.is_null() {
                continue;
            }
            // Defensive: an active face's cached `outer().start` can dangle to a
            // half-edge removed earlier in this rebuild (e.g. drawing onto a
            // solid's protected coplanar face leaves a stale loop pointer). The
            // rest of this function reads through `.get()`; mirror that here so a
            // stale id skips the face instead of panicking the whole engine (the
            // panic leaked the wasm-bindgen borrow → "recursive use" spam).
            let e0 = match mesh.hes.get(start) {
                Some(he) => he.edge(),
                None => continue,
            };
            mesh.collect_loop_hes(start).map_or(false, |h| h.len() == 1)
                && mesh.edges.get(e0).map_or(false, |e| {
                    matches!(
                        e.curve(),
                        Some(AnalyticCurve::Bezier { .. })
                            | Some(AnalyticCurve::BSpline { .. })
                            | Some(AnalyticCurve::NURBS { .. })
                    )
                })
        };
        if !is_freeform {
            continue;
        }
        // 포함하는 polygon outer 찾기 → reparent (containment 검증은 split 내부).
        for &outer in &active_fids {
            if outer == inner {
                continue;
            }
            if crate::operations::annulus::split_face_by_inner_closed_curve_generic(
                mesh, outer, inner,
            )
            .is_ok()
            {
                created_holes += 1;
                break;
            }
        }
    }

    Ok(RebuildReport {
        removed_faces: removed,
        created_faces: created,
        created_holes,
        coplanar_edges: input_curves.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_rect(mesh: &mut Mesh, lo: (f64, f64), hi: (f64, f64)) -> FaceId {
        add_rect_mat(mesh, lo, hi, FORM_MATERIAL)
    }

    fn add_rect_mat(mesh: &mut Mesh, lo: (f64, f64), hi: (f64, f64), mat: MaterialId) -> FaceId {
        let v0 = mesh.add_vertex(DVec3::new(lo.0, lo.1, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(hi.0, lo.1, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(hi.0, hi.1, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(lo.0, hi.1, 0.0));
        mesh.add_face(&[v0, v1, v2, v3], mat).unwrap()
    }

    fn active_face_count(mesh: &Mesh) -> usize {
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
    }

    /// **δ-2 small proof** — 큰 사각형 + 안에 작은 사각형 (둘 다 면, 겹침) →
    /// rebuild → annulus (outer, hole 1) + disk (inner). DCEL-source re-derive
    /// 가 rect containment GAP 을 DCEL 레벨에서 해결함을 end-to-end 증명.
    #[test]
    fn adr186_delta2_rect_containment_rebuilds_to_annulus() {
        let mut mesh = Mesh::new();
        let _outer = add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0)); // full rect
        let _inner = add_rect(&mut mesh, (1.0, 1.0), (3.0, 3.0)); // contained disk
        assert_eq!(active_face_count(&mesh), 2, "before: 2 면 (겹침)");

        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();

        assert_eq!(report.removed_faces, 2, "dirty 2 면 제거: {:?}", report);
        assert_eq!(report.created_faces, 2, "annulus + disk = 2 면: {:?}", report);
        assert_eq!(report.created_holes, 1, "annulus hole 1: {:?}", report);
        assert_eq!(active_face_count(&mesh), 2, "after: 2 면");
        // outer 면이 inner loop (hole) 1 개를 가져야 한다 (annulus).
        let with_hole = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
            .count();
        assert_eq!(with_hole, 1, "hole 가진 면 = 1 (annulus)");
    }

    /// **δ-2** — 단일 사각형 면 → rebuild → 그대로 1 면 (가짜 hole/분할 없음, 회귀 가드).
    #[test]
    fn adr186_delta2_single_rect_rebuilds_unchanged() {
        let mut mesh = Mesh::new();
        let _f = add_rect(&mut mesh, (0.0, 0.0), (10.0, 10.0));
        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        assert_eq!(report.created_faces, 1, "단일 면 = 1: {:?}", report);
        assert_eq!(report.created_holes, 0, "hole 0");
        assert_eq!(active_face_count(&mesh), 1);
    }

    /// **δ-4a 3D solid 보호** — box(volume, bottom z=0) + 별도 sheet rect 를 z=0
    /// 에서 rebuild → sheet rect 만 재유도, box 6 면 무손상 (volume_edges 배제 +
    /// is_sheet_face dirty 제외).
    #[test]
    fn adr186_delta4a_3d_solid_protected_on_rebuild() {
        let mut mesh = Mesh::new();
        // Box (volume) — center z=5, height 10 → bottom face at z=0.
        let box_faces = mesh
            .create_box(DVec3::new(0.0, 0.0, 5.0), 10.0, 10.0, 10.0, FORM_MATERIAL)
            .unwrap();
        let box_count = box_faces.len();
        assert_eq!(box_count, 6, "box = 6 면");
        // 별도 sheet rect on z=0 (box 와 떨어진 위치).
        add_rect(&mut mesh, (100.0, 100.0), (110.0, 110.0));

        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();

        // sheet rect 만 제거/재유도 (box volume 면 미제거).
        assert!(report.removed_faces <= 1, "box volume 미제거: removed={:?}", report);
        // box 6 면 모두 보존 (활성).
        let box_still_active = box_faces
            .iter()
            .filter(|&&f| mesh.faces.get(f).map(|x| x.is_active()).unwrap_or(false))
            .count();
        assert_eq!(box_still_active, box_count, "box {} 면 모두 보존 (3D solid 보호)", box_count);
    }

    /// **δ-3 상속 proof** — outer 면 mat1 + inner 면 mat2 (containment) → rebuild →
    /// annulus 가 mat1 (outer 부모), disk 가 mat2 (inner 부모) 상속. XIA/material
    /// 보존 검증 (innermost_parent 3-branch 변형).
    #[test]
    fn adr186_delta3_rect_containment_inherits_material() {
        let mat1 = MaterialId::new(11);
        let mat2 = MaterialId::new(22);
        let mut mesh = Mesh::new();
        add_rect_mat(&mut mesh, (0.0, 0.0), (4.0, 4.0), mat1); // outer
        add_rect_mat(&mut mesh, (1.0, 1.0), (3.0, 3.0), mat2); // inner (containment)

        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        assert_eq!(report.created_faces, 2, "annulus + disk: {:?}", report);

        // annulus (hole 가진 면) = outer 부모 mat1, disk (hole 없는 큰 면) = inner mat2.
        let mut annulus_mat = None;
        let mut disk_mat = None;
        for (_, f) in mesh.faces.iter() {
            if !f.is_active() {
                continue;
            }
            if f.inners().is_empty() {
                disk_mat = Some(f.material());
            } else {
                annulus_mat = Some(f.material());
            }
        }
        assert_eq!(annulus_mat, Some(mat1), "annulus 는 outer 부모 mat1 상속");
        assert_eq!(disk_mat, Some(mat2), "disk 는 inner 부모 mat2 상속");
    }

    /// **SPIKE 진단 (2026-06-02)** — 복잡 겹침 사각형 (여러 개 + 큰 rect) →
    /// `rebuild_coplanar_faces` 직접 호출 → non-manifold 발생 여부.
    /// 사용자 시연에서 큰 rect 추가 시 non-manifold 5건 발생. 이것이 rebuild
    /// **자체** 버그인지, scene wiring(auto_intersect+rederive 중복) 문제인지 격리.
    #[test]
    fn spike_complex_rects_nonmanifold_isolation() {
        let mut mesh = Mesh::new();
        // 연결된 겹침 (서로 교차 = 1 component). 실제 사용자 복잡 scene 의 대표.
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut mesh, (2.0, 0.0), (6.0, 4.0)); // crosses #1
        add_rect(&mut mesh, (0.0, 2.0), (4.0, 6.0)); // crosses #1
        add_rect(&mut mesh, (2.0, 2.0), (6.0, 6.0)); // crosses #1,#2,#3
        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        let inv = mesh.verify_face_invariants();
        println!("SPIKE report: {:?}", report);
        println!(
            "SPIKE faces={} valid={} violations={}",
            active_face_count(&mesh),
            inv.is_valid(),
            inv.violations.len()
        );
        for vmsg in &inv.violations {
            println!("  violation: {}", vmsg);
        }
        // 진단 핵심: rebuild 자체가 non-manifold 를 만드는가?
        assert!(
            inv.is_valid(),
            "rebuild_coplanar_faces ITSELF produced {} violations (= rebuild 버그, scene wiring 아님): {:?}",
            inv.violations.len(),
            inv.violations
        );
    }

    /// **SPIKE 진단 v2 (2026-06-02)** — 큰 rect 가 겹친 작은 rect blob 을 *안
    /// 건드리고 포함* (disconnected) → 포함된 blob 전체가 **outline 1개** hole 이어야.
    /// CW-cycle fix 검증 (이전: 각 sub-face 를 hole → non-manifold).
    #[test]
    fn spike_contained_blob_no_nonmanifold() {
        let mut mesh = Mesh::new();
        // blob: 3 overlapping rects (서로 교차 = connected).
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut mesh, (1.5, 0.0), (5.5, 4.0));
        add_rect(&mut mesh, (0.75, 1.5), (4.75, 5.5));
        // big rect: blob 을 안 건드리고 감쌈 (disconnected → blob = 1 hole).
        add_rect(&mut mesh, (-2.0, -2.0), (8.0, 8.0));
        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        let inv = mesh.verify_face_invariants();
        println!("CONTAINED report: {:?}", report);
        println!(
            "CONTAINED faces={} holes={} valid={} violations={}",
            active_face_count(&mesh),
            report.created_holes,
            inv.is_valid(),
            inv.violations.len()
        );
        for vmsg in &inv.violations {
            println!("  violation: {}", vmsg);
        }
        assert!(
            inv.is_valid(),
            "contained blob produced {} violations: {:?}",
            inv.violations.len(),
            inv.violations
        );
    }

    /// **SPIKE 진단 v3 (2026-06-02)** — 사각형 + 교차하는 원(Path B) → rederive.
    /// 원이 polygonize 되어 planar graph 에 포함 → 분할. 면사라짐 0 + manifold.
    /// (이전: self-loop edge skip → 원 미참여 → 면사라짐 8→6).
    #[test]
    fn spike_rect_plus_crossing_circle() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        // circle center (3,2) radius 2.5 → 사각형 우측과 교차 (connected).
        let center = DVec3::new(3.0, 2.0, 0.0);
        let radius = 2.5;
        let anchor = mesh.add_vertex(center + DVec3::new(radius, 0.0, 0.0));
        let circle = AnalyticCurve::Circle {
            center,
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        mesh.add_face_closed_curve(anchor, circle, FORM_MATERIAL).unwrap();
        let before = active_face_count(&mesh);
        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        let inv = mesh.verify_face_invariants();
        println!("CIRCLE before={} report={:?}", before, report);
        println!(
            "CIRCLE faces={} valid={} violations={}",
            active_face_count(&mesh),
            inv.is_valid(),
            inv.violations.len()
        );
        for vmsg in &inv.violations {
            println!("  violation: {}", vmsg);
        }
        assert!(
            report.coplanar_edges > 8,
            "원 polygonize 안 됨 (coplanar_edges={})",
            report.coplanar_edges
        );
        assert!(
            inv.is_valid(),
            "circle+rect produced {} violations: {:?}",
            inv.violations.len(),
            inv.violations
        );
        assert!(
            active_face_count(&mesh) >= 3,
            "rect+circle 교차 → ≥3 면: {}",
            active_face_count(&mesh)
        );
    }

    /// **SPIKE 진단 v4 (2026-06-02)** — 2 겹친 원 (single rebuild) → manifold?
    /// 브라우저 incremental 시연에서 2번째 원 추가 시 non-manifold 16건 발생.
    /// kernel(2-원 arrangement) 문제인지 incremental(매 draw 재유도) 문제인지 격리.
    #[test]
    fn spike_two_overlapping_circles() {
        use crate::curves::AnalyticCurve;
        let mk = |mesh: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let center = DVec3::new(cx, cy, 0.0);
            let anchor = mesh.add_vertex(center + DVec3::new(r, 0.0, 0.0));
            let c = AnalyticCurve::Circle {
                center,
                radius: r,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            };
            mesh.add_face_closed_curve(anchor, c, FORM_MATERIAL).unwrap();
        };
        let mut mesh = Mesh::new();
        mk(&mut mesh, 0.0, 0.0, 3.0);
        mk(&mut mesh, 3.0, 0.0, 3.0); // overlaps (centers 3 apart, r 3 each)
        let report = rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap();
        let inv = mesh.verify_face_invariants();
        println!(
            "2CIRCLE faces={} valid={} viol={} report={:?}",
            active_face_count(&mesh),
            inv.is_valid(),
            inv.violations.len(),
            report
        );
        for vmsg in inv.violations.iter().take(6) {
            println!("  {}", vmsg);
        }
        assert!(
            inv.is_valid(),
            "2 overlapping circles single-rebuild: {} violations",
            inv.violations.len()
        );
    }

    /// **SPIKE 진단 v5 (2026-06-02)** — INCREMENTAL 재현: rebuild 1 (rect+circle1)
    /// → circle2 추가 → rebuild 2. 브라우저 incremental non-manifold 의 pure repro.
    /// 가설: circle1 self-loop edge 가 face 제거 후에도 남아 rebuild2 에서 재-
    /// polygonize → circle1 polygon 2개 중첩 → non-manifold.
    #[test]
    fn spike_incremental_two_circles_rebuild() {
        use crate::curves::AnalyticCurve;
        let mk = |mesh: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let center = DVec3::new(cx, cy, 0.0);
            let anchor = mesh.add_vertex(center + DVec3::new(r, 0.0, 0.0));
            let c = AnalyticCurve::Circle {
                center,
                radius: r,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            };
            mesh.add_face_closed_curve(anchor, c, FORM_MATERIAL).unwrap();
        };
        let self_loops = |mesh: &Mesh| -> usize {
            mesh.edges
                .iter()
                .filter(|(_, e)| e.is_active() && e.v_small() == e.v_large())
                .count()
        };
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        mk(&mut mesh, 2.0, 1.0, 1.8); // circle1
        rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap(); // rebuild 1
        let inv1 = mesh.verify_face_invariants();
        println!(
            "INCR rebuild1: faces={} valid={} self_loops={}",
            active_face_count(&mesh),
            inv1.is_valid(),
            self_loops(&mesh)
        );
        mk(&mut mesh, 0.5, 0.5, 1.5); // circle2
        rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-4).unwrap(); // rebuild 2
        let inv2 = mesh.verify_face_invariants();
        println!(
            "INCR rebuild2: faces={} valid={} viol={} self_loops={}",
            active_face_count(&mesh),
            inv2.is_valid(),
            inv2.violations.len(),
            self_loops(&mesh)
        );
        assert!(
            inv2.is_valid(),
            "incremental 2-circle rebuild: {} violations",
            inv2.violations.len()
        );
    }

    /// **SPIKE 진단 v7 (2026-06-02)** — 갈림길 진단: 3사각형+원1 SINGLE rebuild.
    /// PASS → 잔존은 incremental(누적/source degrade) 문제. FAIL → 복잡 arrangement
    /// robustness 문제 (kernel). incremental 버전 (v6) 은 circle1 에서 viol 7.
    #[test]
    fn spike_single_three_rects_one_circle() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut mesh, (1.5, 0.0), (5.5, 4.0));
        add_rect(&mut mesh, (0.75, 1.5), (4.75, 5.5));
        let center = DVec3::new(2.0, 1.0, 0.0);
        let anchor = mesh.add_vertex(center + DVec3::new(1.8, 0.0, 0.0));
        let c = AnalyticCurve::Circle {
            center,
            radius: 1.8,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        mesh.add_face_closed_curve(anchor, c, FORM_MATERIAL).unwrap();
        // SINGLE rebuild (모든 도형 추가 후 1회).
        rebuild_coplanar_faces(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = mesh.verify_face_invariants();
        println!(
            "SINGLE 3rect+circle: faces={} valid={} viol={}",
            active_face_count(&mesh),
            inv.is_valid(),
            inv.violations.len()
        );
        assert!(
            inv.is_valid(),
            "single 3-rect+circle: {} violations",
            inv.violations.len()
        );
    }

    /// **(A) Step 1 PROOF (2026-06-02)** — tangent 기반 arc-aware region 추출 검증.
    /// 직선(secant y=0.5) + 원(r=2) → 교차 2점 → arc 2 + chord → region 추출이
    /// 면 2개(각 chord+arc 경계)를 정확히 뽑는지 *실제 코드* 로 증명. boundary_kernel
    /// 의 chord-angular-sort 를 **departing tangent** 로 바꾸면 arc 면이 정확히
    /// 추출됨 = full (A) 의 가장 큰 불확실성 de-risk.
    ///
    /// 위상: 원을 가로지르는 직선 → chord + top arc + bottom arc.
    /// → 면 A {chord, top arc} (작은 cap) + 면 B {chord, bottom arc} (큰 부분)
    /// + outer (full circle CW). chord 는 두 면이 공유 (manifold).
    #[test]
    fn proof_a_step1_tangent_region_line_circle_two_arc_faces() {
        use std::f64::consts::PI;
        let r = 2.0_f64;
        let cy = 0.5_f64;
        let cx = (r * r - cy * cy).sqrt();
        // verts: 0 = left intersection, 1 = right intersection.
        let p = [(-cx, cy), (cx, cy)];
        let ang = |x: f64, y: f64| {
            let a = y.atan2(x);
            if a < 0.0 { a + 2.0 * PI } else { a }
        };
        let th_l = ang(p[0].0, p[0].1); // ~2.889 (upper-left)
        let th_r = ang(p[1].0, p[1].1); // ~0.253 (lower-right)
        // departing tangent: CCW(+θ) = (-sinθ, cosθ), CW(-θ) = (sinθ, -cosθ).
        let tan_ccw = |t: f64| (-t.sin(), t.cos());
        let tan_cw = |t: f64| (t.sin(), -t.cos());
        let nrm = |v: (f64, f64)| {
            let l = (v.0 * v.0 + v.1 * v.1).sqrt();
            (v.0 / l, v.1 / l)
        };
        let aang = |v: (f64, f64)| {
            let a = v.1.atan2(v.0);
            if a < 0.0 { a + 2.0 * PI } else { a }
        };

        // half-edge: from, to, twin, departing tangent, optional arc (from_angle, to_angle).
        struct He {
            from: usize,
            to: usize,
            twin: usize,
            tang: (f64, f64),
            arc: Option<(f64, f64)>,
        }
        let chord_t01 = nrm((p[1].0 - p[0].0, p[1].1 - p[0].1));
        let chord_t10 = nrm((p[0].0 - p[1].0, p[0].1 - p[1].1));
        let hes = vec![
            He { from: 0, to: 1, twin: 1, tang: chord_t01, arc: None }, // 0 chord 0→1
            He { from: 1, to: 0, twin: 0, tang: chord_t10, arc: None }, // 1 chord 1→0
            He { from: 1, to: 0, twin: 3, tang: tan_ccw(th_r), arc: Some((th_r, th_l)) }, // 2 top CCW
            He { from: 0, to: 1, twin: 2, tang: tan_cw(th_l), arc: Some((th_l, th_r)) },  // 3 top CW
            He { from: 0, to: 1, twin: 5, tang: tan_ccw(th_l), arc: Some((th_l, th_r + 2.0 * PI)) }, // 4 bot CCW
            He { from: 1, to: 0, twin: 4, tang: tan_cw(th_r), arc: Some((th_r + 2.0 * PI, th_l)) },  // 5 bot CW
        ];
        // per-vertex departing half-edges, sorted CCW by tangent angle.
        let mut depart: Vec<Vec<usize>> = vec![Vec::new(); 2];
        for (i, h) in hes.iter().enumerate() {
            depart[h.from].push(i);
        }
        for d in depart.iter_mut() {
            d.sort_by(|&a, &b| aang(hes[a].tang).partial_cmp(&aang(hes[b].tang)).unwrap());
        }
        // next(h) = at h.to, CW-rotate (prev in CCW order) from twin(h). → CCW faces.
        let next = |h: usize| -> usize {
            let tw = hes[h].twin;
            let list = &depart[hes[h].to];
            let idx = list.iter().position(|&x| x == tw).unwrap();
            list[(idx + list.len() - 1) % list.len()]
        };
        // extract cycles.
        let mut visited = vec![false; hes.len()];
        let mut cycles: Vec<Vec<usize>> = Vec::new();
        for start in 0..hes.len() {
            if visited[start] {
                continue;
            }
            let mut cyc = Vec::new();
            let mut h = start;
            loop {
                visited[h] = true;
                cyc.push(h);
                h = next(h);
                if h == start || visited[h] {
                    break;
                }
            }
            cycles.push(cyc);
        }
        // signed area (sample arcs into polyline).
        let sample = |h: &He| -> Vec<(f64, f64)> {
            match h.arc {
                None => vec![p[h.from]],
                Some((a0, a1)) => (0..24)
                    .map(|k| {
                        let t = a0 + (a1 - a0) * (k as f64) / 24.0;
                        (r * t.cos(), r * t.sin())
                    })
                    .collect(),
            }
        };
        let area = |cyc: &Vec<usize>| -> f64 {
            let mut poly: Vec<(f64, f64)> = Vec::new();
            for &h in cyc {
                poly.extend(sample(&hes[h]));
            }
            let mut s = 0.0;
            for i in 0..poly.len() {
                let j = (i + 1) % poly.len();
                s += poly[i].0 * poly[j].1 - poly[j].0 * poly[i].1;
            }
            s * 0.5
        };
        let areas: Vec<f64> = cycles.iter().map(area).collect();
        let bounded: Vec<&Vec<usize>> = cycles
            .iter()
            .zip(&areas)
            .filter(|(_, &a)| a > 1e-6)
            .map(|(c, _)| c)
            .collect();

        println!("PROOF cycles={} areas={:?}", cycles.len(), areas);
        for (i, c) in cycles.iter().enumerate() {
            let kinds: Vec<&str> = c
                .iter()
                .map(|&h| if hes[h].arc.is_none() { "chord" } else { "arc" })
                .collect();
            println!("  cycle{} area={:.3} edges={:?}", i, areas[i], kinds);
        }

        assert_eq!(cycles.len(), 3, "3 cycles (2 bounded + 1 outer)");
        assert_eq!(bounded.len(), 2, "2 bounded arc faces");
        for f in &bounded {
            let has_chord = f.iter().any(|&h| hes[h].arc.is_none());
            let n_arc = f.iter().filter(|&&h| hes[h].arc.is_some()).count();
            assert!(has_chord && n_arc == 1, "bounded face = chord + 1 arc: {:?}", f);
        }
        // chord 가 두 bounded 면 모두에 (manifold).
        let chord_in = bounded
            .iter()
            .filter(|f| f.iter().any(|&h| hes[h].arc.is_none()))
            .count();
        assert_eq!(chord_in, 2, "chord 가 두 면 공유 (manifold)");
        // 두 면 면적 합 ≈ 원판 (chord 가 disk 정확 분할).
        let total: f64 = bounded.iter().map(|c| area(c)).sum();
        let disk = PI * r * r;
        assert!(
            (total - disk).abs() < 0.05 * disk,
            "두 면 합 ≈ 원판: {:.3} vs {:.3}",
            total,
            disk
        );
    }

    /// **(A) Step 2 PROOF (2026-06-02)** — arc-arc region 추출 검증 (원-원).
    /// 원 A(0,0,r2) ∩ B(2,0,r2) → 교차 2점 → arc 4개 → 면 3 (lens + crescent 2)
    /// + outer. Step 1(arc-chord 3방향) 보다 어려운 **arc-arc 4방향 꼭짓점** 을
    /// tangent angular sort 가 정확히 처리함을 *실제 코드* 로 증명.
    /// 기대: lens≈4.913, crescent 각≈7.653, union 합≈20.219.
    #[test]
    fn proof_a_step2_tangent_region_circle_circle_three_arc_faces() {
        use std::f64::consts::PI;
        let r = 2.0_f64;
        let circles = [(0.0_f64, 0.0_f64), (2.0_f64, 0.0_f64)]; // centers (r 공통)
        let s3 = 3.0_f64.sqrt();
        let pts = [(1.0, s3), (1.0, -s3)]; // vert 0 = P1, vert 1 = P2 (교차점)
        let angle_on = |ci: usize, pi: usize| {
            let a = (pts[pi].1 - circles[ci].1).atan2(pts[pi].0 - circles[ci].0);
            if a < 0.0 { a + 2.0 * PI } else { a }
        };
        let ccw = |t: f64| (-t.sin(), t.cos());
        let cw = |t: f64| (t.sin(), -t.cos());
        let aang = |v: (f64, f64)| {
            let a = v.1.atan2(v.0);
            if a < 0.0 { a + 2.0 * PI } else { a }
        };

        // 각 원을 두 교차점에서 2 arc 으로 분할 → 4 arc.
        // arc = (center, vert_α, angle_α, vert_β, angle_β) — α→β CCW (β>α).
        let mut arcs: Vec<((f64, f64), usize, f64, usize, f64)> = Vec::new();
        for ci in 0..2 {
            let c = circles[ci];
            let (a0, a1) = (angle_on(ci, 0), angle_on(ci, 1));
            let (lo_v, lo_a, hi_v, hi_a) =
                if a0 < a1 { (0, a0, 1, a1) } else { (1, a1, 0, a0) };
            arcs.push((c, lo_v, lo_a, hi_v, hi_a)); // arc1: lo→hi
            arcs.push((c, hi_v, hi_a, lo_v, lo_a + 2.0 * PI)); // arc2: hi→lo+2π
        }

        struct He {
            from: usize,
            to: usize,
            twin: usize,
            tang: (f64, f64),
            center: (f64, f64),
            a0: f64, // 이 half-edge traversal 의 시작 각
            a1: f64, // 끝 각
        }
        let mut hes: Vec<He> = Vec::new();
        for &(c, va, aa, vb, ab) in &arcs {
            let fwd = hes.len();
            hes.push(He { from: va, to: vb, twin: fwd + 1, tang: ccw(aa), center: c, a0: aa, a1: ab });
            hes.push(He { from: vb, to: va, twin: fwd, tang: cw(ab), center: c, a0: ab, a1: aa });
        }

        let mut depart: Vec<Vec<usize>> = vec![Vec::new(); 2];
        for (i, h) in hes.iter().enumerate() {
            depart[h.from].push(i);
        }
        for d in depart.iter_mut() {
            d.sort_by(|&a, &b| aang(hes[a].tang).partial_cmp(&aang(hes[b].tang)).unwrap());
        }
        let next = |h: usize| -> usize {
            let tw = hes[h].twin;
            let list = &depart[hes[h].to];
            let idx = list.iter().position(|&x| x == tw).unwrap();
            list[(idx + list.len() - 1) % list.len()]
        };
        let mut visited = vec![false; hes.len()];
        let mut cycles: Vec<Vec<usize>> = Vec::new();
        for start in 0..hes.len() {
            if visited[start] {
                continue;
            }
            let mut cyc = Vec::new();
            let mut h = start;
            loop {
                visited[h] = true;
                cyc.push(h);
                h = next(h);
                if h == start || visited[h] {
                    break;
                }
            }
            cycles.push(cyc);
        }
        let area = |cyc: &Vec<usize>| -> f64 {
            let mut poly: Vec<(f64, f64)> = Vec::new();
            for &h in cyc {
                let he = &hes[h];
                for k in 0..48 {
                    let t = he.a0 + (he.a1 - he.a0) * (k as f64) / 48.0;
                    poly.push((he.center.0 + r * t.cos(), he.center.1 + r * t.sin()));
                }
            }
            let mut s = 0.0;
            for i in 0..poly.len() {
                let j = (i + 1) % poly.len();
                s += poly[i].0 * poly[j].1 - poly[j].0 * poly[i].1;
            }
            s * 0.5
        };
        let areas: Vec<f64> = cycles.iter().map(area).collect();
        let mut bounded: Vec<f64> = areas.iter().cloned().filter(|&a| a > 1e-6).collect();
        bounded.sort_by(|a, b| a.partial_cmp(b).unwrap());

        println!("PROOF2 cycles={} areas={:?}", cycles.len(), areas);
        for (i, c) in cycles.iter().enumerate() {
            println!("  cycle{} area={:.3} edges(arcs)={}", i, areas[i], c.len());
        }

        assert_eq!(cycles.len(), 4, "4 cycles (3 bounded + 1 outer)");
        let n_bounded = bounded.len();
        assert_eq!(n_bounded, 3, "3 bounded arc faces (lens + 2 crescent)");
        // 각 bounded 면 = 정확히 arc 2개.
        for (c, &a) in cycles.iter().zip(&areas) {
            if a > 1e-6 {
                assert_eq!(c.len(), 2, "bounded face = 2 arcs: {:?}", c);
            }
        }
        // 면적: 최소 = lens ≈ 4.913, 합 = union ≈ 20.219.
        let lens_exp = 8.0 * 0.5_f64.acos() - 12.0_f64.sqrt();
        let union_exp = 2.0 * PI * r * r - lens_exp;
        assert!(
            (bounded[0] - lens_exp).abs() < 0.10 * lens_exp,
            "lens ≈ {:.3}: got {:.3}",
            lens_exp,
            bounded[0]
        );
        let total: f64 = bounded.iter().sum();
        assert!(
            (total - union_exp).abs() < 0.05 * union_exp,
            "union 합 ≈ {:.3}: got {:.3}",
            union_exp,
            total
        );
    }

    /// **4-β 회귀 (2026-06-02)** — DCEL edge → InputCurve, arc→circle 병합 (source 복원).
    #[test]
    fn beta_reconstruct_input_curves_arc_merge() {
        use crate::curves::AnalyticCurve;
        use std::f64::consts::PI;
        let mut mesh = Mesh::new();
        // 1) self-loop Circle (Path B) — center (0,0) r=2.
        let anchor = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        mesh.add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: 2.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            FORM_MATERIAL,
        )
        .unwrap();
        // 2) 다른 원 (center (10,0) r=3) 이 4 quarter-arc 로 split 된 상태 (D7 중간점 스타일,
        //    각 arc 가 distinct vert 쌍). → 1 Circle 로 병합돼야.
        let p1 = mesh.add_vertex(DVec3::new(13.0, 0.0, 0.0)); // E
        let mn = mesh.add_vertex(DVec3::new(10.0, 3.0, 0.0)); // N
        let p2 = mesh.add_vertex(DVec3::new(7.0, 0.0, 0.0)); // W
        let ms = mesh.add_vertex(DVec3::new(10.0, -3.0, 0.0)); // S
        let arc = |a0: f64, a1: f64| AnalyticCurve::Arc {
            center: DVec3::new(10.0, 0.0, 0.0),
            radius: 3.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: a0,
            end_angle: a1,
        };
        mesh.add_edge_with_curve(p1, mn, arc(0.0, PI / 2.0)).unwrap();
        mesh.add_edge_with_curve(mn, p2, arc(PI / 2.0, PI)).unwrap();
        mesh.add_edge_with_curve(p2, ms, arc(PI, 1.5 * PI)).unwrap();
        mesh.add_edge_with_curve(ms, p1, arc(1.5 * PI, 2.0 * PI)).unwrap();
        // 3) 직선 edge.
        let l1 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let l2 = mesh.add_vertex(DVec3::new(5.0, 10.0, 0.0));
        mesh.add_edge(l1, l2).unwrap();

        let (u, v) = plane_basis(DVec3::Z);
        let curves = reconstruct_input_curves(
            &mesh,
            DVec3::ZERO,
            u,
            v,
            DVec3::Z,
            1e-4,
            &HashSet::new(),
            None,
        );
        let n_circ = curves
            .iter()
            .filter(|c| matches!(c, InputCurve::Circle { .. }))
            .count();
        let n_line = curves
            .iter()
            .filter(|c| matches!(c, InputCurve::Line { .. }))
            .count();
        println!("4-β: circles={} lines={}", n_circ, n_line);
        assert_eq!(n_circ, 2, "self-loop원 1 + arc4개→병합 1 = 2 circle");
        assert_eq!(n_line, 1, "1 line");
    }

    /// **4-γ 회귀** — analytic rebuild 단일 rect → 1 면.
    #[test]
    fn gamma_analytic_single_rect() {
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (10.0, 10.0));
        let r = rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        println!("4-γ single rect: created={} active={}", r.created_faces, active_face_count(&mesh));
        assert_eq!(active_face_count(&mesh), 1, "단일 rect → 1면");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
    }

    /// **4-γ 회귀** — rect + 겹친 원 (analytic) → manifold, arc 경계.
    #[test]
    fn gamma_analytic_rect_plus_circle() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        let a = mesh.add_vertex(DVec3::new(4.0 + 1.5, 2.0, 0.0));
        mesh.add_face_closed_curve(
            a,
            AnalyticCurve::Circle {
                center: DVec3::new(4.0, 2.0, 0.0),
                radius: 1.5,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            FORM_MATERIAL,
        )
        .unwrap();
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = mesh.verify_face_invariants();
        println!(
            "4-γ rect+circle: active={} valid={} viol={}",
            active_face_count(&mesh),
            inv.is_valid(),
            inv.violations.len()
        );
        assert!(inv.is_valid(), "manifold: {:?}", inv.violations.iter().take(3).collect::<Vec<_>>());
        assert!(active_face_count(&mesh) >= 2, "겹침 → 면 분할");
    }

    /// **Tangency degeneracy (2026-06-18)** — engine-level (scoped re-derive). A
    /// circle centered on a rect edge with `radius` = half-height passes EXACTLY
    /// through the two corners (4,±3) and is **tangent** to the top/bottom edges
    /// there. Pre-fix the `arrange()` cycle walk could not order the coincident
    /// (tangent) half-edges → 1 degenerate face. Fixed by the `bend` (signed
    /// curvature) tie-break. Mirrors `exec_draw_circle_as_curve` (pre-split + scoped
    /// re-derive). Expect 3 sub-faces (rect∖circle / rect∩circle / circle∖rect).
    #[test]
    fn tangent_corner_rect_circle_scoped_three_faces() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (-4.0, -3.0), (4.0, 3.0));
        let center = DVec3::new(4.0, 0.0, 0.0); // on the right edge
        let radius = 3.0; // = half-height → through corners (4,±3), tangent to top/bottom
        // scene order: split existing straight edges first, then add the circle face.
        mesh.split_edges_at_circle_crossings(center, radius, DVec3::Z, DVec3::X);
        let anchor = mesh.add_vertex(center + DVec3::new(radius, 0.0, 0.0));
        let cface = mesh
            .add_face_closed_curve(
                anchor,
                AnalyticCurve::Circle { center, radius, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            )
            .unwrap();
        let origin = center + DVec3::new(radius, 0.0, 0.0); // anchor = first loop vert
        rebuild_coplanar_faces_analytic_scoped(&mut mesh, origin, DVec3::Z, 1e-3, false, Some(&[cface]))
            .unwrap();
        assert_eq!(
            active_face_count(&mesh),
            3,
            "원이 모서리 접(tangent) 통과해도 3 sub-face"
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "manifold valid after tangent-corner re-derive"
        );
    }

    /// **2026-06-03** — rect 안 겹치는 원 2개 rebuild → rect hole 이 **smooth arc**
    /// (polygon 잔재 0). 수정 전 hole 은 polygonize (~32 line edge). sub-face 와
    /// 정점 공유 → manifold + fully-analytic.
    #[test]
    fn rect_two_overlapping_circles_hole_is_smooth_arc_not_polygon() {
        use crate::curves::AnalyticCurve;
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (12.0, 8.0));
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle {
                    center: DVec3::new(cx, cy, 0.0),
                    radius: r,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
                FORM_MATERIAL,
            )
            .unwrap();
        };
        circ(&mut m, 5.0, 4.0, 2.2);
        circ(&mut m, 7.0, 4.0, 2.2);
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
        assert_eq!(active_face_count(&m), 4, "rect + lens + 초승달2 = 4면");
        // 직선(None) edge 는 rect 4변 뿐 — peanut hole 은 arc (polygon 잔재 0, 수정 전 36).
        let mut n_arc = 0usize;
        let mut n_line = 0usize;
        for (_eid, e) in m.edges.iter() {
            if !e.is_active() {
                continue;
            }
            match e.curve() {
                Some(AnalyticCurve::Arc { .. }) => n_arc += 1,
                _ => n_line += 1,
            }
        }
        assert!(n_line <= 4, "직선 edge = rect 4변 뿐 (polygon hole 잔재 0): {} (수정 전 ~36)", n_line);
        assert!(n_arc >= 8, "peanut hole + sub-face 가 arc edge: {}", n_arc);
    }

    /// **2026-06-03** — rect 안 겹치는 원 2개 render → **rect 의 peanut hole 이
    /// 직선 chord(마름모) 아닌 arc-sampled smooth**. 사용자 보고 "외곽 사각형이
    /// 마름모". export_buffers 의 per-face 삼각형 수로 검증 (마름모 hole 면 = 8 tris,
    /// arc-sampled 면 = 수십+). inner-loop arc 샘플링 (mesh_export) 회귀.
    #[test]
    fn rect_two_overlap_circles_render_no_diamond_hole() {
        use crate::curves::AnalyticCurve;
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (12.0, 8.0));
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            )
            .unwrap();
        };
        circ(&mut m, 5.0, 4.0, 2.2);
        circ(&mut m, 7.0, 4.0, 2.2);
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let (_p, _n, idx, fmap, _) = m.export_buffers().unwrap();
        use std::collections::BTreeMap;
        let mut tc: BTreeMap<u32, usize> = BTreeMap::new();
        for t in 0..idx.len() / 3 {
            *tc.entry(fmap[t]).or_default() += 1;
        }
        assert_eq!(tc.len(), 4, "4 면 모두 렌더: {:?}", tc);
        // 마름모 hole 면(수정 전 rect = 8 tris)이 없어야 — 모든 면 arc-sampled.
        let min_tris = tc.values().copied().min().unwrap_or(0);
        assert!(
            min_tris >= 16,
            "모든 면 arc-sampled (마름모 hole 8 tris 차단): per-face {:?}",
            tc
        );
    }

    /// **2026-06-03** — rect 안 겹치는 원 분할 경계가 wireframe 에 **보임**
    /// (메타-원칙 #15 HARD flag). 사용자 보고 "경계선을 보이게". 수정 전 coplanar
    /// arc 경계는 hide → rect 4변만 (4 segments). 수정 후 arc 경계 HARD → emit.
    #[test]
    fn rect_two_overlap_circles_division_edges_visible() {
        use crate::curves::AnalyticCurve;
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (12.0, 8.0));
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            )
            .unwrap();
        };
        circ(&mut m, 5.0, 4.0, 2.2);
        circ(&mut m, 7.0, 4.0, 2.2);
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        // wireframe segment 수 (positions 6 floats/segment). 수정 전 4 (rect 4변).
        let (lines, _map) = m.export_edge_lines_with_map(20.1);
        let segs = lines.len() / 6;
        assert!(
            segs > 20,
            "분할 arc 경계가 wireframe 에 emit (HARD flag, 수정 전 ~4): {}",
            segs
        );
    }

    /// **A1 2026-06-03** — 닫힌 Bezier 가 다른 coplanar 도형(rect)과 함께 있어도
    /// rebuild 에서 **소멸하지 않음** (데이터 손실 방지). arrange 미지원 freeform
    /// closed curve self-loop 을 clean-slate 에서 보존. 시뮬레이션 (B) 회귀.
    #[test]
    fn rect_with_bezier_bezier_survives_rebuild() {
        use crate::curves::AnalyticCurve;
        let cps = vec![
            DVec3::new(6.0, 3.0, 0.0),
            DVec3::new(9.0, 5.0, 0.0),
            DVec3::new(6.0, 8.0, 0.0),
            DVec3::new(3.0, 5.0, 0.0),
            DVec3::new(6.0, 3.0, 0.0),
        ];
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (12.0, 8.0));
        let a = m.add_vertex(cps[0]);
        m.add_face_closed_curve(a, AnalyticCurve::Bezier { control_pts: cps }, FORM_MATERIAL)
            .unwrap();
        let bez_before = m
            .edges
            .iter()
            .filter(|(_, e)| e.is_active() && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. })))
            .count();
        assert_eq!(bez_before, 1, "사전: bezier edge 1");
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        // A1 핵심: rebuild 후에도 bezier 보존 (수정 전 0 = 소멸).
        let bez_after = m
            .edges
            .iter()
            .filter(|(_, e)| e.is_active() && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. })))
            .count();
        assert_eq!(bez_after, 1, "rebuild 후 bezier 보존 (소멸 차단, 수정 전 0): {}", bez_after);
        assert!(active_face_count(&m) >= 2, "rect + bezier 면 보존: {}", active_face_count(&m));
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
    }

    /// **B4b-2b PROOF (ADR-186 A3 / Option B)** — gated freeform overlap
    /// SMOOTH split. 2 overlapping closed beziers + gate ON → detection +
    /// A1 override + feeding → arrange → **3 manifold faces** with **smooth
    /// sub-bezier boundaries** (D7 midpoint), each sub-bezier edge tagged with
    /// the source owner-id (B6 link). gate OFF → preserved. disjoint → no split.
    #[test]
    fn adr186_b4b2b_overlap_beziers_smooth_lens() {
        use crate::curves::AnalyticCurve;
        let blob = |cx: f64| -> Vec<DVec3> {
            vec![
                DVec3::new(cx, 0.0, 0.0),
                DVec3::new(cx + 7.0, 5.0, 0.0),
                DVec3::new(cx, 10.0, 0.0),
                DVec3::new(cx - 7.0, 5.0, 0.0),
                DVec3::new(cx, 0.0, 0.0),
            ]
        };
        let add_blob = |m: &mut Mesh, cx: f64| {
            let a = m.add_vertex(blob(cx)[0]);
            m.add_face_closed_curve(a, AnalyticCurve::Bezier { control_pts: blob(cx) }, FORM_MATERIAL)
                .unwrap();
        };

        // gate ON + overlapping → split to 3 manifold faces + map populated.
        let mut m = Mesh::new();
        add_blob(&mut m, 5.0);
        add_blob(&mut m, 8.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(m.freeform_curve_to_source.len(), 2, "detection: 2 sources stored");
        assert_eq!(
            active_face_count(&m),
            3,
            "B4b-2b: overlap beziers → lens + 2 crescent = 3 faces"
        );
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "3-face smooth lens manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
        // original overlap bezier self-loops removed (A1 override).
        let self_loop_bez = m
            .edges
            .iter()
            .filter(|(_, e)| {
                e.is_active()
                    && e.is_self_loop()
                    && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. }))
            })
            .count();
        assert_eq!(self_loop_bez, 0, "overlap bezier self-loops removed (A1 override)");
        // B4b-2b — lens boundary = smooth sub-bezier regular edges, each tagged
        // with the source owner-id (B6 link).
        let smooth_owned = m
            .edges
            .iter()
            .filter(|(_, e)| {
                e.is_active()
                    && !e.is_self_loop()
                    && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. }))
                    && e.curve_owner_id().is_some()
            })
            .count();
        assert!(
            smooth_owned >= 6,
            "smooth sub-bezier edges with owner-id (B6 link): {}",
            smooth_owned
        );

        // gate OFF — beziers preserved, no split, no detection.
        let mut m2 = Mesh::new();
        add_blob(&mut m2, 5.0);
        add_blob(&mut m2, 8.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m2, DVec3::ZERO, DVec3::Z, 1e-3, false)
            .unwrap();
        assert_eq!(active_face_count(&m2), 2, "gate OFF: beziers preserved (no split)");
        assert!(m2.freeform_curve_to_source.is_empty(), "gate OFF: no detection");

        // disjoint + gate ON → no overlap → no split.
        let mut m3 = Mesh::new();
        add_blob(&mut m3, 5.0);
        add_blob(&mut m3, 40.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m3, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m3), 2, "disjoint: no split");
        assert!(m3.freeform_curve_to_source.is_empty(), "disjoint: no detection");
    }

    /// **B6 PROOF (ADR-186 A3 / Option B)** — re-rebuild idempotency. gate on,
    /// rebuild ×3: reconstruct restores the source beziers by owner-id (from the
    /// freeform_curve_to_source map) → same 3 smooth faces every time. WITHOUT
    /// B6 the sub-bezier fragments degrade to Line on rebuild #2 (smooth → poly,
    /// bez 12→0; the P5 trap). Now the smooth lens survives every rebuild.
    #[test]
    fn adr186_b6_rerebuild_idempotent_smooth_lens() {
        use crate::curves::AnalyticCurve;
        let add_blob = |m: &mut Mesh, cx: f64| {
            let cps = vec![
                DVec3::new(cx, 0.0, 0.0),
                DVec3::new(cx + 7.0, 5.0, 0.0),
                DVec3::new(cx, 10.0, 0.0),
                DVec3::new(cx - 7.0, 5.0, 0.0),
                DVec3::new(cx, 0.0, 0.0),
            ];
            let a = m.add_vertex(cps[0]);
            m.add_face_closed_curve(a, AnalyticCurve::Bezier { control_pts: cps }, FORM_MATERIAL)
                .unwrap();
        };
        let bez_regular = |m: &Mesh| -> usize {
            m.edges
                .iter()
                .filter(|(_, e)| {
                    e.is_active()
                        && !e.is_self_loop()
                        && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. }))
                })
                .count()
        };
        let mut m = Mesh::new();
        add_blob(&mut m, 5.0);
        add_blob(&mut m, 8.0);

        // rebuild #1 → 3 smooth faces, sub-bezier edges.
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m), 3, "rebuild#1: 3 faces");
        assert!(bez_regular(&m) >= 6, "rebuild#1: smooth sub-bezier: {}", bez_regular(&m));

        // rebuild #2 (idempotent) — B6 restores originals by owner-id; smooth
        // PRESERVED (without B6 this was bez 0 = polygon, the P5 trap).
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m), 3, "rebuild#2: still 3 faces (idempotent)");
        assert!(
            bez_regular(&m) >= 6,
            "rebuild#2: smooth PRESERVED (P5 trap fixed, was 0): {}",
            bez_regular(&m)
        );
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "rebuild#2 manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );

        // rebuild #3 — still idempotent; map stays at 2 originals.
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m), 3, "rebuild#3: still 3 faces");
        assert!(bez_regular(&m) >= 6, "rebuild#3: smooth preserved");
        assert_eq!(m.freeform_curve_to_source.len(), 2, "map stable (2 originals)");
    }

    /// **B5-2 PROOF (ADR-186 A3 mixed)** — freeform × rect overlap detection +
    /// split + idempotency. A bezier partially overlapping a rect (≥2 boundary
    /// crossings) is detected → owner-id → fed to arrange → B5-1 arm splits it
    /// into smooth-bezier-bounded lens sub-faces. Re-rebuild restores the source
    /// by owner-id (B6) → idempotent. Containment (0 crossings) → NOT detected
    /// (A2 hole path). Gate OFF → preserved.
    #[test]
    fn adr186_b5_2_freeform_x_rect_overlap_detects_splits_idempotent() {
        use crate::curves::AnalyticCurve;
        let blob = |cx: f64| -> Vec<DVec3> {
            vec![
                DVec3::new(cx, 0.0, 0.0),
                DVec3::new(cx + 7.0, 5.0, 0.0),
                DVec3::new(cx, 10.0, 0.0),
                DVec3::new(cx - 7.0, 5.0, 0.0),
                DVec3::new(cx, 0.0, 0.0),
            ]
        };
        let add_blob = |m: &mut Mesh, cx: f64| {
            let a = m.add_vertex(blob(cx)[0]);
            m.add_face_closed_curve(a, AnalyticCurve::Bezier { control_pts: blob(cx) }, FORM_MATERIAL)
                .unwrap();
        };
        let bez_regular = |m: &Mesh| -> usize {
            m.edges
                .iter()
                .filter(|(_, e)| {
                    e.is_active()
                        && !e.is_self_loop()
                        && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. }))
                })
                .count()
        };

        // partial overlap (gate ON): rect straddles the blob's right half (left
        // edge x=6.5 cuts vertically through the blob, off the (6,0) anchor).
        let mut m = Mesh::new();
        add_rect(&mut m, (6.5, -5.0), (20.0, 15.0));
        add_blob(&mut m, 6.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert!(
            !m.freeform_curve_to_source.is_empty(),
            "B5-2 detection: freeform×rect overlap → owner-id"
        );
        let f1 = active_face_count(&m);
        assert!(f1 >= 3, "B5-2: lens split → >=3 faces, got {}", f1);
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "B5-2 manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
        assert!(bez_regular(&m) >= 2, "B5-2: smooth sub-bezier lens boundary: {}", bez_regular(&m));

        // re-rebuild ×2 (idempotent) — B6 restores source by owner-id; smooth
        // lens preserved (without B6/feed this would degrade — the P5 trap).
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m), f1, "B5-2 idempotent #2: same face count");
        assert!(bez_regular(&m) >= 2, "B5-2 idempotent #2: smooth preserved: {}", bez_regular(&m));
        rebuild_coplanar_faces_analytic_with_overlap(&mut m, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert_eq!(active_face_count(&m), f1, "B5-2 idempotent #3: same face count");
        let inv2 = m.verify_face_invariants();
        assert!(inv2.is_valid(), "B5-2 re-rebuild manifold");

        // gate OFF — no detection (freeform preserved).
        let mut m2 = Mesh::new();
        add_rect(&mut m2, (6.5, -5.0), (20.0, 15.0));
        add_blob(&mut m2, 6.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m2, DVec3::ZERO, DVec3::Z, 1e-3, false)
            .unwrap();
        assert!(m2.freeform_curve_to_source.is_empty(), "gate OFF: no detection");

        // containment (gate ON) — blob fully inside rect → 0 boundary crossings
        // → NOT B5-detected (≥2 bound; A2 hole path handles containment).
        let mut m3 = Mesh::new();
        add_rect(&mut m3, (-10.0, -10.0), (25.0, 20.0));
        add_blob(&mut m3, 6.0);
        rebuild_coplanar_faces_analytic_with_overlap(&mut m3, DVec3::ZERO, DVec3::Z, 1e-3, true)
            .unwrap();
        assert!(
            m3.freeform_curve_to_source.is_empty(),
            "containment: 0 crossings → no B5 detection (A2 hole path)"
        );
    }

    /// **A2 2026-06-03** — 닫힌 Bezier 가 polygon(rect) 안에 포함되면 **smooth
    /// hole** 이 됨 (containment → reparent). + render 에서 hole tessellate (직선
    /// chord 아님). circle smooth-hole 경로의 곡선 일반화.
    #[test]
    fn rect_with_bezier_bezier_becomes_hole() {
        use crate::curves::AnalyticCurve;
        let cps = vec![
            DVec3::new(6.0, 3.0, 0.0),
            DVec3::new(9.0, 5.0, 0.0),
            DVec3::new(6.0, 8.0, 0.0),
            DVec3::new(3.0, 5.0, 0.0),
            DVec3::new(6.0, 3.0, 0.0),
        ];
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (12.0, 8.0));
        let a = m.add_vertex(cps[0]);
        m.add_face_closed_curve(a, AnalyticCurve::Bezier { control_pts: cps }, FORM_MATERIAL)
            .unwrap();
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        // A2 핵심: bezier 가 rect 의 hole 이 됨 (포함 → reparent).
        let faces_with_holes = m
            .faces
            .iter()
            .filter(|(_, f)| {
                f.is_active() && f.inners().iter().any(|i| !i.start.is_null())
            })
            .count();
        assert_eq!(faces_with_holes, 1, "rect 가 bezier hole 1개 보유: {}", faces_with_holes);
        // bezier edge 보존 (disk 면).
        let bez = m
            .edges
            .iter()
            .filter(|(_, e)| e.is_active() && matches!(e.curve(), Some(AnalyticCurve::Bezier { .. })))
            .count();
        assert_eq!(bez, 1, "bezier edge 보존");
        let inv = m.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
        // render (Layer 3): bezier hole 이 tessellate → 모든 면 smooth (직선 chord 아님).
        let (_p, _n, idx, fmap, _) = m.export_buffers().expect("export_buffers");
        use std::collections::BTreeMap;
        let mut tc: BTreeMap<u32, usize> = BTreeMap::new();
        for t in 0..idx.len() / 3 {
            *tc.entry(fmap[t]).or_default() += 1;
        }
        let min_tris = tc.values().copied().min().unwrap_or(0);
        assert!(min_tris >= 8, "bezier hole + disk smooth render: {:?}", tc);
    }

    /// **잔존 시뮬레이션** — 복잡 arrangement viol 의 최소 repro 좁히기. assert 없음.
    #[test]
    fn sim_complex_residual_narrow() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(a, AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X }, FORM_MATERIAL).unwrap();
        };
        let reb = |m: &mut Mesh| { rebuild_coplanar_faces_analytic(m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap(); };
        let rpt = |m: &Mesh, lbl: &str| {
            let i = m.verify_face_invariants();
            println!("  {}: faces={} valid={} viol={}", lbl, active_face_count(m), i.is_valid(), i.violations.len());
        };

        // (A) 2 rect + 원, incremental.
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0)); reb(&mut m);
        add_rect(&mut m, (1.5, 0.0), (5.5, 4.0)); reb(&mut m);
        circ(&mut m, 2.0, 1.0, 1.8); reb(&mut m);
        rpt(&m, "A) 2rect+circle incr");

        // (B) 3 rect + 원, SINGLE rebuild (모든 도형 추가 후 1회).
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut m, (1.5, 0.0), (5.5, 4.0));
        add_rect(&mut m, (0.75, 1.5), (4.75, 5.5));
        circ(&mut m, 2.0, 1.0, 1.8);
        reb(&mut m);
        rpt(&m, "B) 3rect+circle SINGLE");

        // (C) 1 rect + 원, incremental (대조군 — 통과해야).
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0)); reb(&mut m);
        circ(&mut m, 2.0, 1.0, 1.8); reb(&mut m);
        rpt(&m, "C) 1rect+circle incr");

        // (D) 2 rect + 원, SINGLE rebuild.
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut m, (1.5, 0.0), (5.5, 4.0));
        circ(&mut m, 2.0, 1.0, 1.8);
        reb(&mut m);
        rpt(&m, "D) 2rect+circle SINGLE");
    }

    /// **잔존 시뮬레이션 2** — 2rect+circle (최소 repro) viol edge 추적.
    #[test]
    fn sim_2rect_circle_trace() {
        use crate::curves::AnalyticCurve;
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut m, (1.5, 0.0), (5.5, 4.0));
        let a = m.add_vertex(DVec3::new(3.8, 1.0, 0.0));
        m.add_face_closed_curve(a, AnalyticCurve::Circle { center: DVec3::new(2.0, 1.0, 0.0), radius: 1.8, normal: DVec3::Z, basis_u: DVec3::X }, FORM_MATERIAL).unwrap();
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = m.verify_face_invariants();
        println!("  faces={} valid={} viol={}", active_face_count(&m), inv.is_valid(), inv.violations.len());
        for v in inv.violations.iter() {
            println!("  VIOL: {}", v);
        }
        for (eid, e) in m.edges.iter() {
            if !e.is_active() {
                continue;
            }
            let pa = m.verts.get(e.v_small()).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            let pb = m.verts.get(e.v_large()).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            let ct = match e.curve() {
                Some(AnalyticCurve::Arc { .. }) => "Arc",
                Some(_) => "C",
                None => "Line",
            };
            println!("  {:?}: ({:.2},{:.2})-({:.2},{:.2}) {}", eid, pa.x, pa.y, pb.x, pb.y, ct);
        }
        // viol edge x=4 (4,0)-(4,4) 의 실제 공유 face 추적.
        for (eid, e) in m.edges.iter() {
            if !e.is_active() {
                continue;
            }
            let pa = m.verts.get(e.v_small()).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            let pb = m.verts.get(e.v_large()).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            if (pa.x - 4.0).abs() < 0.01 && (pb.x - 4.0).abs() < 0.01 {
                let (faces, hes) = m.get_faces_sharing_edge(eid);
                println!(
                    "  >> x=4 edge {:?} (v_small={:?} v_large={:?}) → {} faces, {} hes:",
                    eid, e.v_small(), e.v_large(), faces.len(), hes.len()
                );
                for &he in &hes {
                    if let Some(h) = m.hes.get(he) {
                        println!(
                            "     he {:?}: edge={:?} dst={:?} face={:?}",
                            he, h.edge(), h.dst(), h.face()
                        );
                    }
                }
                // 각 face 의 실제 outer loop half-edge 덤프 (HeId78 이 FaceId6 loop 에 있나?)
                for f in &faces {
                    if let Some(s) = m.faces.get(*f).map(|fc| fc.outer().start) {
                        if let Ok(loop_hes) = m.collect_loop_hes(s) {
                            println!("     {:?} loop hes: {:?}", f, loop_hes);
                        }
                    }
                }
            }
        }
    }

    /// **잔존 격리 3** — 6면을 fresh mesh 에 add_face 로 직접 (arrange/arc/clean-slate 배제).
    /// add_face_with_holes 자체가 이 공유-정점 구성에서 phantom he 를 만드는가?
    #[test]
    fn sim_six_faces_addface_manifold() {
        let mut m = Mesh::new();
        let f = |m: &mut Mesh, pts: &[(f64, f64)]| {
            let vs: Vec<crate::VertId> =
                pts.iter().map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0))).collect();
            let _ = m.add_face(&vs, FORM_MATERIAL);
        };
        f(&mut m, &[(1.5, 2.7), (0.3, 1.6), (0.5, 0.0), (1.5, 0.0)]);
        f(&mut m, &[(0.5, 0.0), (0.3, 1.6), (1.5, 2.7), (1.5, 4.0), (0.0, 4.0), (0.0, 0.0)]);
        f(&mut m, &[(0.5, 0.0), (2.0, -0.8), (3.5, 0.0), (1.5, 0.0)]);
        f(&mut m, &[(3.5, 0.0), (3.5, 2.1), (1.5, 2.7), (1.5, 0.0)]);
        f(&mut m, &[(1.5, 2.7), (3.5, 2.1), (3.5, 0.0), (4.0, 0.0), (4.0, 4.0), (1.5, 4.0)]);
        f(&mut m, &[(4.0, 0.0), (5.5, 0.0), (5.5, 4.0), (4.0, 4.0)]);
        let inv = m.verify_face_invariants();
        println!(
            "  6면 add_face: faces={} valid={} viol={}",
            active_face_count(&m),
            inv.is_valid(),
            inv.violations.len()
        );
        for v in inv.violations.iter().take(4) {
            println!("    {}", v);
        }
    }

    /// **잔존 격리 4 (결정적)** — 도형 생성→제거→6면 재생성. remove+recreate(slot 재사용)
    /// 가 wiring 손상시키는지 순수 core-mesh 로 검증 (arrange/arc/clean-slate 로직 배제).
    #[test]
    fn sim_remove_then_six_faces() {
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (4.0, 4.0));
        add_rect(&mut m, (1.5, 0.0), (5.5, 4.0));
        let a = m.add_vertex(DVec3::new(3.8, 1.0, 0.0));
        m.add_face_closed_curve(a, crate::curves::AnalyticCurve::Circle { center: DVec3::new(2.0, 1.0, 0.0), radius: 1.8, normal: DVec3::Z, basis_u: DVec3::X }, FORM_MATERIAL).unwrap();
        // clean-slate 식 제거.
        let fids: Vec<_> = m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
        for fid in fids {
            let _ = m.remove_face(fid);
        }
        let eids: Vec<_> = m.edges.iter().filter(|(_, e)| e.is_active()).map(|(id, _)| id).collect();
        for eid in eids {
            let _ = m.remove_edge_and_halfedges(eid);
        }
        // 6면 재생성 (sim_six_faces 와 동일).
        let f = |m: &mut Mesh, pts: &[(f64, f64)]| {
            let vs: Vec<crate::VertId> =
                pts.iter().map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0))).collect();
            let _ = m.add_face(&vs, FORM_MATERIAL);
        };
        f(&mut m, &[(1.5, 2.7), (0.3, 1.6), (0.5, 0.0), (1.5, 0.0)]);
        f(&mut m, &[(0.5, 0.0), (0.3, 1.6), (1.5, 2.7), (1.5, 4.0), (0.0, 4.0), (0.0, 0.0)]);
        f(&mut m, &[(0.5, 0.0), (2.0, -0.8), (3.5, 0.0), (1.5, 0.0)]);
        f(&mut m, &[(3.5, 0.0), (3.5, 2.1), (1.5, 2.7), (1.5, 0.0)]);
        f(&mut m, &[(1.5, 2.7), (3.5, 2.1), (3.5, 0.0), (4.0, 0.0), (4.0, 4.0), (1.5, 4.0)]);
        f(&mut m, &[(4.0, 0.0), (5.5, 0.0), (5.5, 4.0), (4.0, 4.0)]);
        let inv = m.verify_face_invariants();
        println!(
            "  remove-then-6면: faces={} valid={} viol={}",
            active_face_count(&m), inv.is_valid(), inv.violations.len()
        );
        for v in inv.violations.iter().take(4) {
            println!("    {}", v);
        }
    }

    /// **4-γ 회귀** — 1st rebuild 결과 모든 edge 가 평면(z=0) 위 + manifold.
    /// (off-plane edge 생성 없음 — re-rebuild reconstruct 가 전부 다시 수집 가능 보장.)
    #[test]
    fn gamma_analytic_first_rebuild_all_planar() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        let a = mesh.add_vertex(DVec3::new(5.5, 2.0, 0.0));
        mesh.add_face_closed_curve(
            a,
            AnalyticCurve::Circle { center: DVec3::new(4.0, 2.0, 0.0), radius: 1.5, normal: DVec3::Z, basis_u: DVec3::X },
            FORM_MATERIAL,
        ).unwrap();
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let mut n = 0;
        for (_, e) in mesh.edges.iter() {
            if !e.is_active() {
                continue;
            }
            n += 1;
            for vid in [e.v_small(), e.v_large()] {
                let z = mesh.verts.get(vid).map(|v| v.pos().z).unwrap_or(99.0);
                assert!(z.abs() < 1e-6, "edge vert off-plane: z={}", z);
            }
        }
        assert_eq!(n, 10, "rect+circle → 10 edge");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
    }

    /// **4-γ 격리 — idempotency** — rect+원 1회 rebuild 후 다시 rebuild. 2nd 가 1st 와
    /// 동일해야 (rect-subseg + arc feedback 이 idempotent 인지).
    #[test]
    fn gamma_analytic_double_rebuild_idempotent() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        let a = mesh.add_vertex(DVec3::new(4.0 + 1.5, 2.0, 0.0));
        mesh.add_face_closed_curve(
            a,
            AnalyticCurve::Circle { center: DVec3::new(4.0, 2.0, 0.0), radius: 1.5, normal: DVec3::Z, basis_u: DVec3::X },
            FORM_MATERIAL,
        ).unwrap();
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let f1 = active_face_count(&mesh);
        let e1 = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        // 2nd rebuild (동일 입력 재유도) — idempotent + manifold 이어야.
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let f2 = active_face_count(&mesh);
        let e2 = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        let inv = mesh.verify_face_invariants();
        println!("  double-rebuild: 1st {}f/{}e → 2nd {}f/{}e valid={}", f1, e1, f2, e2, inv.is_valid());
        // **idempotency 해결** (coplanarity 면 판정 fix): 2nd rebuild manifold + counts 불변.
        assert!(inv.is_valid(), "2nd rebuild manifold: {:?}", inv.violations.iter().take(3).collect::<Vec<_>>());
        assert_eq!(f1, f2, "idempotent face count");
        assert_eq!(e1, e2, "idempotent edge count");
    }

    /// **Step 6 사전검토 (kernel-native smooth hole)** — split_face_by_inner_circle (ADR-185)
    /// 가 동심원을 smooth 곡선 hole annulus 로 만드는지 (hole edge = curve, manifold).
    #[test]
    fn sim_kernel_native_smooth_hole() {
        use crate::curves::AnalyticCurve;
        let disk = |m: &mut Mesh, r: f64| -> crate::FaceId {
            let a = m.add_vertex(DVec3::new(r, 0.0, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::ZERO, radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            ).unwrap()
        };
        let mut m = Mesh::new();
        let outer = disk(&mut m, 10.0);
        let inner = disk(&mut m, 4.0);
        let res = crate::operations::annulus::split_face_by_inner_circle(&mut m, outer, inner);
        let inv = m.verify_face_invariants();
        let nf = active_face_count(&m);
        let with_hole = m.faces.iter().filter(|(_, f)| f.is_active() && !f.inners().is_empty()).count();
        let (mut hl, mut hc) = (0usize, 0usize);
        for (_, f) in m.faces.iter() {
            if !f.is_active() {
                continue;
            }
            for inn in f.inners() {
                if let Ok(hes) = m.collect_loop_hes(inn.start) {
                    for he in hes {
                        match m.edges.get(m.hes[he].edge()).and_then(|x| x.curve()) {
                            Some(_) => hc += 1,
                            None => hl += 1,
                        }
                    }
                }
            }
        }
        println!(
            "  smooth hole: split={:?} faces={} valid={} hole면={} (hole edges: {} Line / {} curve)",
            res.is_ok(), nf, inv.is_valid(), with_hole, hl, hc
        );
    }

    /// **Step 6 사전검토** — 기존경로 교체 시뮬: 원-in-원 containment 을 analytic rebuild 로.
    /// 옛 δ-Path B 는 원 hole 을 kernel-native 보존. analytic 은 hole 을 polygonize(D1)?
    /// → hole edge 가 Line(polygon)인지 curve(보존)인지 측정.
    #[test]
    fn sim_swap_circle_containment() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, r: f64| {
            let a = m.add_vertex(DVec3::new(r, 0.0, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::ZERO, radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            ).unwrap();
        };
        let mut m = Mesh::new();
        circ(&mut m, 10.0);
        circ(&mut m, 4.0);
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = m.verify_face_invariants();
        let nf = active_face_count(&m);
        let with_hole = m.faces.iter().filter(|(_, f)| f.is_active() && !f.inners().is_empty()).count();
        let (mut hole_line, mut hole_curve) = (0usize, 0usize);
        for (_, f) in m.faces.iter() {
            if !f.is_active() {
                continue;
            }
            for inner in f.inners() {
                if let Ok(hes) = m.collect_loop_hes(inner.start) {
                    for he in hes {
                        let e = m.hes[he].edge();
                        match m.edges.get(e).and_then(|x| x.curve()) {
                            Some(_) => hole_curve += 1,
                            None => hole_line += 1,
                        }
                    }
                }
            }
        }
        println!(
            "  원-in-원 analytic: faces={} valid={} hole면={} (hole edges: {} Line / {} curve)",
            nf, inv.is_valid(), with_hole, hole_line, hole_curve
        );
    }

    /// **Step 6 사전검토** — 기존경로 교체 시뮬: 사각형 containment (δ-4b) 을 analytic 로.
    #[test]
    fn sim_swap_rect_containment() {
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (400.0, 400.0));
        add_rect(&mut m, (100.0, 100.0), (300.0, 300.0));
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = m.verify_face_invariants();
        let nf = active_face_count(&m);
        let with_hole = m.faces.iter().filter(|(_, f)| f.is_active() && !f.inners().is_empty()).count();
        println!(
            "  사각형 containment analytic: faces={} valid={} hole면={}",
            nf, inv.is_valid(), with_hole
        );
    }

    /// **Step 5** — 진짜 disjoint hole: rect 안에 안 닿는 원 → annulus(rect+hole) + disk.
    /// nest_loops fix 후에도 D1 hole polygonize 경로가 *진짜 홀* 엔 정상 manifold 인지.
    #[test]
    fn gamma_analytic_disjoint_circle_annulus() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (10.0, 10.0));
        let a = mesh.add_vertex(DVec3::new(7.0, 5.0, 0.0));
        mesh.add_face_closed_curve(
            a,
            AnalyticCurve::Circle { center: DVec3::new(5.0, 5.0, 0.0), radius: 2.0, normal: DVec3::Z, basis_u: DVec3::X },
            FORM_MATERIAL,
        ).unwrap();
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = mesh.verify_face_invariants();
        let nf = active_face_count(&mesh);
        println!("  disjoint annulus: faces={} valid={} viol={}", nf, inv.is_valid(), inv.violations.len());
        assert!(inv.is_valid(), "disjoint annulus manifold: {:?}", inv.violations.iter().take(3).collect::<Vec<_>>());
        assert_eq!(nf, 2, "annulus(rect+hole) + disk = 2 faces");
    }

    /// **Step 5** — 멀티-홀: 큰 rect 안에 안 닿는 원 2개 → annulus(2 hole) + disk 2.
    #[test]
    fn gamma_analytic_two_disjoint_circles_multihole() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            ).unwrap();
        };
        let mut mesh = Mesh::new();
        add_rect(&mut mesh, (0.0, 0.0), (12.0, 8.0));
        circ(&mut mesh, 3.0, 4.0, 1.5);
        circ(&mut mesh, 9.0, 4.0, 1.5);
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let inv = mesh.verify_face_invariants();
        let nf = active_face_count(&mesh);
        println!("  멀티홀: faces={} valid={} viol={}", nf, inv.is_valid(), inv.violations.len());
        assert!(inv.is_valid(), "멀티홀 manifold: {:?}", inv.violations.iter().take(3).collect::<Vec<_>>());
        assert_eq!(nf, 3, "annulus(2hole) + disk×2 = 3 faces");
    }

    /// **Step 5** — 그리기 순서 무관성: 원 먼저 vs rect 먼저 → 같은 manifold 결과.
    #[test]
    fn gamma_analytic_draw_order_independence() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            ).unwrap();
        };
        let mut a = Mesh::new();
        add_rect(&mut a, (0.0, 0.0), (4.0, 4.0));
        circ(&mut a, 2.0, 1.0, 1.8);
        rebuild_coplanar_faces_analytic(&mut a, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let fa = active_face_count(&a);
        let va = a.verify_face_invariants();
        let mut b = Mesh::new();
        circ(&mut b, 2.0, 1.0, 1.8);
        add_rect(&mut b, (0.0, 0.0), (4.0, 4.0));
        rebuild_coplanar_faces_analytic(&mut b, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let fb = active_face_count(&b);
        let vb = b.verify_face_invariants();
        println!("  순서A rect→circ: faces={} valid={} / 순서B circ→rect: faces={} valid={}", fa, va.is_valid(), fb, vb.is_valid());
        assert!(va.is_valid() && vb.is_valid(), "양 순서 manifold");
        assert_eq!(fa, fb, "그리기 순서 무관 동일 면 수");
    }

    /// **4-γ 격리** — 2 원만 incremental (rect 없이). 원 incremental 자체 검증.
    #[test]
    fn gamma_analytic_two_circles_incremental() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle { center: DVec3::new(cx, cy, 0.0), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
                FORM_MATERIAL,
            ).unwrap();
        };
        let mut mesh = Mesh::new();
        circ(&mut mesh, 0.0, 0.0, 2.0);
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let i1 = mesh.verify_face_invariants();
        let e1 = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        println!("  2circ after c1: faces={} edges={} valid={}", active_face_count(&mesh), e1, i1.is_valid());
        circ(&mut mesh, 2.0, 0.0, 2.0);
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let i2 = mesh.verify_face_invariants();
        let e2 = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        println!("  2circ after c2: faces={} edges={} valid={} viol={}", active_face_count(&mesh), e2, i2.is_valid(), i2.violations.len());
        assert!(i2.is_valid(), "2원 incremental manifold: {:?}", i2.violations.iter().take(3).collect::<Vec<_>>());
    }

    /// **4-γ 핵심 회귀 — idempotency** — 3 rect + 2 원 incremental (매 draw rebuild).
    /// 구 polygon rebuild 는 viol 7 (spike_incremental_complex_rects_circles).
    /// analytic rebuild 는 arc→circle source 복원 → manifold 목표.
    #[test]
    fn gamma_analytic_incremental_manifold() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle {
                    center: DVec3::new(cx, cy, 0.0),
                    radius: r,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
                FORM_MATERIAL,
            )
            .unwrap();
        };
        let mut mesh = Mesh::new();
        let dump = |m: &Mesh, lbl: &str| {
            let inv = m.verify_face_invariants();
            let ne = m.edges.iter().filter(|(_, e)| e.is_active()).count();
            println!(
                "  4-γ {}: faces={} edges={} valid={} viol={}",
                lbl,
                active_face_count(m),
                ne,
                inv.is_valid(),
                inv.violations.len()
            );
        };
        add_rect(&mut mesh, (0.0, 0.0), (4.0, 4.0));
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        dump(&mesh, "rect1");
        add_rect(&mut mesh, (1.5, 0.0), (5.5, 4.0));
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        dump(&mesh, "rect2");
        add_rect(&mut mesh, (0.75, 1.5), (4.75, 5.5));
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        dump(&mesh, "rect3");
        // **확인된 성과**: 3 겹친 rect incremental (매 draw 재유도) → manifold.
        let inv_rects = mesh.verify_face_invariants();
        assert!(
            inv_rects.is_valid(),
            "3 overlapping rects incremental manifold: {:?}",
            inv_rects.violations.iter().take(3).collect::<Vec<_>>()
        );
        circ(&mut mesh, 2.0, 1.0, 1.8);
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        dump(&mesh, "circle1");
        let inv_c1 = mesh.verify_face_invariants();
        assert!(
            inv_c1.is_valid(),
            "3rect+circle1 manifold: {:?}",
            inv_c1.violations.iter().take(3).collect::<Vec<_>>()
        );
        circ(&mut mesh, 0.5, 0.5, 1.5);
        rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        dump(&mesh, "circle2");
        // **핵심 해결**: 3 rect + 2 원 incremental 전 단계 manifold. 구 polygon rebuild 는
        // circle1 에서 viol 7 (spike_incremental_complex_rects_circles). analytic rebuild +
        // nest_loops 정점-공유 가드(가짜 hole 차단)로 해결.
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "3rect+2circle incremental manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
    }

    /// **Step 7 (보강) — many-shape stress** — 많은 도형 (중첩 rect 6 + circle 4)
    /// incremental 재유도 시 **매 단계 manifold** 유지 ("도형이 많아져도 변함없이").
    #[test]
    fn gamma_analytic_stress_many_overlapping_shapes() {
        use crate::curves::AnalyticCurve;
        let circ = |m: &mut Mesh, cx: f64, cy: f64, r: f64| {
            let a = m.add_vertex(DVec3::new(cx + r, cy, 0.0));
            m.add_face_closed_curve(
                a,
                AnalyticCurve::Circle {
                    center: DVec3::new(cx, cy, 0.0),
                    radius: r,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
                FORM_MATERIAL,
            )
            .unwrap();
        };
        let mut mesh = Mesh::new();
        let mut step = 0;
        // 6 staggered partially-overlapping rects (2 rows × 3 cols, ~40 overlap)
        for row in 0..2 {
            for col in 0..3 {
                let x0 = col as f64 * 80.0;
                let y0 = row as f64 * 80.0;
                add_rect(&mut mesh, (x0, y0), (x0 + 120.0, y0 + 120.0));
                rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
                step += 1;
                let inv = mesh.verify_face_invariants();
                assert!(
                    inv.is_valid(),
                    "step {} rect: {:?}",
                    step,
                    inv.violations.iter().take(3).collect::<Vec<_>>()
                );
            }
        }
        // 4 circles overlapping rects + each other
        for k in 0..4 {
            circ(&mut mesh, 60.0 + k as f64 * 50.0, 100.0, 45.0);
            rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
            step += 1;
            let inv = mesh.verify_face_invariants();
            assert!(
                inv.is_valid(),
                "step {} circle: {:?}",
                step,
                inv.violations.iter().take(3).collect::<Vec<_>>()
            );
        }
        // "변함없이 유지" — re-derive 3× → face count + manifold 안정 (idempotency).
        let nf0 = active_face_count(&mesh);
        for r in 0..3 {
            rebuild_coplanar_faces_analytic(&mut mesh, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
            let inv = mesh.verify_face_invariants();
            assert!(
                inv.is_valid(),
                "re-derive {} manifold: {:?}",
                r,
                inv.violations.iter().take(3).collect::<Vec<_>>()
            );
            let nf_i = active_face_count(&mesh);
            assert_eq!(nf_i, nf0, "re-derive {} idempotent (face count): {} vs {}", r, nf_i, nf0);
        }
        let nf = active_face_count(&mesh);
        println!("stress: {} draws → {} faces, all manifold + idempotent", step, nf);
        assert!(nf > 6, "many sub-faces from overlaps: {}", nf);
    }

    // **KNOWN RESIDUAL + 진단 결론 (2026-06-02)** — incremental(매 draw rebuild) 이
    // 자기 split 결과(pre-split arrangement)를 되먹어 non-manifold. 결정적 진단:
    //   - spike_single_three_rects_one_circle (SINGLE rebuild, fresh source) = 12면
    //     manifold ✅ → **커널 정확**.
    //   - 동일 기하 incremental (3사각형 각 rebuild → 7면 split, 그 위에 circle1
    //     rebuild) = viol 7 ❌.
    // → 순수 idempotency 문제. 커널은 fresh full-edge 입력엔 정확하나 pre-split
    //   arrangement + 원 입력엔 non-manifold (split edge 와 원 polygon 교차점이
    //   기존 split vert 와 근접/중복 → degeneracy).
    // **완전 fix = derived 를 되먹이지 말고 source 에서 재유도** — source(원본 rect
    //   edge + 원 self-loop) 보존/추적 + 매 rebuild 시 derived(split·polygon) 제거 후
    //   fresh 재유도. = axia-sketch 진짜 derived-face. 다음 focused 세션.

    // ════════════════════════════════════════════════════════════════════
    //  Option A (perf scope, 2026-06-05) 회귀 — affected-region re-derive.
    //  사용자 "도구 작동이 매우 느림" → seed(새 면) 의 connected component(2D
    //  AABB)만 재유도. disjoint 면은 무손상, overlapping 은 full 과 동일 결과.
    // ════════════════════════════════════════════════════════════════════

    /// Disjoint scope — a new disjoint shape re-derives ONLY its own region;
    /// the other (far-apart) faces' FaceIds persist (not removed/recreated).
    #[test]
    fn option_a_disjoint_scope_leaves_others_untouched() {
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (2.0, 2.0));
        add_rect(&mut m, (10.0, 0.0), (12.0, 2.0));
        add_rect(&mut m, (20.0, 0.0), (22.0, 2.0));
        rebuild_coplanar_faces_analytic(&mut m, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        assert_eq!(active_face_count(&m), 3, "3 disjoint faces derived");
        let before: Vec<FaceId> = m
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(f, _)| f)
            .collect();
        assert_eq!(before.len(), 3);
        // add a 4th disjoint rect + scoped re-derive seeded by it.
        let d = add_rect(&mut m, (30.0, 0.0), (32.0, 2.0));
        rebuild_coplanar_faces_analytic_scoped(
            &mut m,
            DVec3::ZERO,
            DVec3::Z,
            1e-3,
            false,
            Some(&[d]),
        )
        .unwrap();
        // the 3 original FaceIds are UNTOUCHED (proof the scope skipped them).
        for &f in &before {
            assert!(
                m.faces.contains(f) && m.faces[f].is_active(),
                "disjoint face {:?} untouched by scoped re-derive",
                f
            );
        }
        assert_eq!(active_face_count(&m), 4, "4 disjoint faces total");
    }

    /// Overlapping scope == full — the AABB scope captures the overlapping
    /// neighbor, so scoped (seed = new rect) yields the SAME faces as full.
    #[test]
    fn option_a_overlapping_scope_matches_full() {
        let build = || {
            let mut m = Mesh::new();
            add_rect(&mut m, (0.0, 0.0), (10.0, 10.0));
            let new = add_rect(&mut m, (5.0, 5.0), (15.0, 15.0)); // partial overlap
            (m, new)
        };
        let (mut full, _) = build();
        rebuild_coplanar_faces_analytic(&mut full, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let nf_full = active_face_count(&full);

        let (mut scoped, new) = build();
        rebuild_coplanar_faces_analytic_scoped(
            &mut scoped,
            DVec3::ZERO,
            DVec3::Z,
            1e-3,
            false,
            Some(&[new]),
        )
        .unwrap();
        let nf_scoped = active_face_count(&scoped);

        assert_eq!(nf_scoped, nf_full, "scoped overlap == full: {nf_scoped} vs {nf_full}");
        assert!(nf_full >= 3, "partial overlap → ≥3 sub-faces: {nf_full}");
        let inv = scoped.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "scoped manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
    }

    /// Empty seed → full-plane fallback (None semantics).
    #[test]
    fn option_a_empty_seed_falls_back_to_full() {
        let mut m = Mesh::new();
        add_rect(&mut m, (0.0, 0.0), (10.0, 10.0));
        add_rect(&mut m, (5.0, 5.0), (15.0, 15.0));
        rebuild_coplanar_faces_analytic_scoped(
            &mut m,
            DVec3::ZERO,
            DVec3::Z,
            1e-3,
            false,
            Some(&[]),
        )
        .unwrap();
        assert!(
            active_face_count(&m) >= 3,
            "empty seed = full re-derive: {}",
            active_face_count(&m)
        );
    }

    /// Circle AABB bounds — a circle straddling a rect edge must use its
    /// center±radius box (NOT just its anchor vertex) so the scope captures
    /// the rect. Scoped (seed = circle) == full proves the bound is correct.
    #[test]
    fn option_a_overlapping_circle_scope_uses_curve_bounds() {
        use crate::curves::AnalyticCurve;
        let build = || {
            let mut m = Mesh::new();
            add_rect(&mut m, (0.0, 0.0), (10.0, 10.0));
            let anchor = m.add_vertex(DVec3::new(13.0, 5.0, 0.0)); // on circle
            let cf = m
                .add_face_closed_curve(
                    anchor,
                    AnalyticCurve::Circle {
                        center: DVec3::new(10.0, 5.0, 0.0),
                        radius: 3.0,
                        normal: DVec3::Z,
                        basis_u: DVec3::X,
                    },
                    FORM_MATERIAL,
                )
                .unwrap();
            (m, cf)
        };
        let (mut full, _) = build();
        rebuild_coplanar_faces_analytic(&mut full, DVec3::ZERO, DVec3::Z, 1e-3).unwrap();
        let nf_full = active_face_count(&full);

        let (mut scoped, cf) = build();
        rebuild_coplanar_faces_analytic_scoped(
            &mut scoped,
            DVec3::ZERO,
            DVec3::Z,
            1e-3,
            false,
            Some(&[cf]),
        )
        .unwrap();
        let nf_scoped = active_face_count(&scoped);

        assert_eq!(
            nf_scoped, nf_full,
            "scoped circle-overlap == full (center±r AABB used): {nf_scoped} vs {nf_full}"
        );
    }
}
