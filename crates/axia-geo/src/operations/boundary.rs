//! ADR-148 — Point-Localized BoundaryTool (B-γ' Engine implementation).
//!
//! CAD 표준 BOUNDARY 명령 equivalent — 사용자가 영역 내부의 한 점을 클릭
//! 하면 그 점을 둘러싼 가장 작은 boundary loop 검출 → face 합성.
//!
//! ADR-139 (Boundary tool 명시 only) 직계 후속 — full mesh sweep
//! (`Scene::resynthesize_orphan_faces`) 보다 정밀한 *국지적* 명시 trigger.
//!
//! **메타-원칙 #16 정합**: 휴리스틱 자동 activation 0, 사용자 클릭 =
//! 명시 의도 canonical.
//!
//! # β-2 scope (current commit — algorithm 본체 + happy path)
//!
//! - `BoundaryError` enum (4 variant — AlgorithmDeferred sentinel β-1
//!   에서 본 commit 으로 제거됨)
//! - `boundary_from_point(&mut Mesh, ...)` — 4 validation + 본체 algorithm
//!   (Q1=c Hybrid: search_radius 내 orphan edges → DFS cycle finder →
//!   point-in-polygon 2D → smallest enclosing → face 합성)
//!
//! Validation 4단계 (ADR-148 §2.1):
//! 1. point 가 plane 에 평면적 (LOCKED #5 ε=1.5μm)
//! 2. orphan edges 수집 (active edges with no face, search_radius 내)
//! 3. 점을 둘러싼 cycle 발견 → 없음 → NoEnclosingCycle
//! 4. cycle 이 이미 face 인지 검사 → CycleAlreadyFaced
//!
//! Algorithm (β-2, Q1=c Hybrid):
//! - Linear scan with search_radius filter (β-2 MVP — BVH spatial accel
//!   은 future optimization, ADR-148 §3 trade-off matrix Q1=(c) 정합)
//! - DFS cycle finder on orphan edge subgraph
//! - point-in-polygon 2D (signed area + ray-casting)
//! - smallest enclosing cycle (area minimum)
//! - face 합성 (`add_face` existing API)
//!
//! # Cross-link
//!
//! - ADR-148 α spec (docs/adr/148-point-localized-boundary-tool.md)
//! - ADR-139 (LOCKED #64 Boundary tool 명시) — 직계 predecessor
//! - 메타-원칙 #5 / #14 / #16
//! - LOCKED #5 (1.5μm spatial-hash — proximity tolerance)
//! - LOCKED #44 / #63 / #64 / #65 / #66

use crate::mesh::Mesh;
use crate::{FaceId, VertId, EdgeId, MaterialId};
use crate::operations::boolean_geo::Plane;
use glam::DVec3;
use std::collections::{HashMap, HashSet};

/// ADR-148 β-1 — Point-Localized BoundaryTool errors.
///
/// Returned by `boundary_from_point`. Each variant 은 명시 validation
/// failure (silent skip 차단, 메타-원칙 #16 정합). β-1 의 `AlgorithmDeferred`
/// variant 는 β-2 진입 시 제거 예정 (skeleton 단계 표시).
#[derive(Debug, Clone, PartialEq)]
pub enum BoundaryError {
    /// 점이 plane 에 평면적 (LOCKED #5 ε=1.5μm 초과).
    /// `distance_mm` 은 plane 까지의 부호없는 거리 (Toast 표시용).
    PointNotOnPlane { distance_mm: f64 },

    /// search_radius 내 orphan edges 0 (작업 영역 비어 있음).
    /// `search_radius_mm` 은 caller 지정 또는 default 1000mm.
    NoOrphanEdgesInRadius { search_radius_mm: f64 },

    /// 점을 둘러싼 simple closed cycle 없음 (free space click 또는
    /// 모든 cycle 이 점을 포함하지 않음).
    NoEnclosingCycle,

    /// 발견된 cycle 이 이미 active face (중복 합성 차단).
    /// `existing_face_id` 는 Toast 에서 사용자에게 알림.
    CycleAlreadyFaced { existing_face_id: u32 },

}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoundaryError::PointNotOnPlane { distance_mm } => {
                write!(f, "PointNotOnPlane (distance {:.3}mm)", distance_mm)
            }
            BoundaryError::NoOrphanEdgesInRadius { search_radius_mm } => {
                write!(f, "NoOrphanEdgesInRadius (radius {:.1}mm)", search_radius_mm)
            }
            BoundaryError::NoEnclosingCycle => write!(f, "NoEnclosingCycle"),
            BoundaryError::CycleAlreadyFaced { existing_face_id } => {
                write!(f, "CycleAlreadyFaced (face {})", existing_face_id)
            }
        }
    }
}

impl std::error::Error for BoundaryError {}

/// Point-plane proximity tolerance (1.5μm) — "is this point ON the plane?".
///
/// ADR-274 — re-exports the canonical `plane::EPS_PLANE_OFFSET` (SSOT) rather
/// than duplicating the literal, so there is one source of truth for the
/// plane-offset ε. (Distinct from the 0.15μm vertex-dedup tolerance; the old
/// "LOCKED #5" attribution conflated the two — dedup is 0.15μm, plane offset
/// is 1.5μm.)
pub const POINT_ON_PLANE_TOL_MM: f64 = crate::plane::EPS_PLANE_OFFSET;

/// Default search radius (10×10×10m 작업 공간 표준).
/// caller 가 0 또는 negative 전달 시 본 값 사용.
pub const DEFAULT_SEARCH_RADIUS_MM: f64 = 1000.0;

/// ADR-148 β-1 — Point-localized boundary face detection (skeleton).
///
/// Given a 3D point and a plane, find the smallest enclosing orphan
/// edge cycle on that plane containing the point, and synthesize a
/// face from that cycle.
///
/// # Parameters
/// - `mesh`: target mesh (mutable — face 합성 시 update)
/// - `point`: 3D world-space click point
/// - `plane`: target plane (cardinal projection or face plane)
/// - `search_radius`: BVH spatial query radius (mm). ≤0 시 default 1000mm.
///
/// # Returns
/// - `Ok(FaceId)`: 새로 합성된 boundary face
/// - `Err(BoundaryError)`: 4 validation failure 또는 β-1 의 AlgorithmDeferred
///
/// # β-2 scope (current commit — happy path 활성)
///
/// 4 validation 모두 활성 + face 합성 본체. Returns `Ok(FaceId)` 시 새
/// boundary face 합성. β-1 의 `AlgorithmDeferred` sentinel 본 commit 에서
/// 제거됨.
pub fn boundary_from_point(
    mesh: &mut Mesh,
    point: DVec3,
    plane: Plane,
    search_radius: f64,
) -> Result<FaceId, BoundaryError> {
    // ─── ADR-171 β-2: absorb_boundary_input SSOT (Phase 2) ───────────────
    // Engine-internal robustness — project drift point onto the plane
    // BEFORE Validation #1 (robust 흡수, 사용자 정책 2026-05-30). Covers
    // ALL call paths (MCP / import / script / 내부호출) that bypass the
    // Tool layer (ADR-170 Phase 1).
    //
    // Behavior:
    //   - Point within MAX_DRIFT_MM of plane → projected onto plane (NEW:
    //     previously the 1.5μm-1mm drift gap returned PointNotOnPlane).
    //   - Point beyond MAX_DRIFT_MM → DriftBeyondTolerance → PointNotOnPlane
    //     (preserves the far-off rejection — 10mm test 정합).
    let canonical_plane = crate::plane::Plane {
        normal: plane.normal,
        offset: plane.dist,
    };
    let point = match crate::operations::boundary_input::absorb_boundary_input(
        mesh,
        crate::operations::boundary_input::BoundaryInput::Point {
            point,
            plane: canonical_plane,
        },
        Some(canonical_plane),
    ) {
        Ok(normalized) => {
            if let crate::operations::boundary_input::BoundaryInput::Point {
                point: projected,
                ..
            } = normalized.input
            {
                projected
            } else {
                point // unreachable — defensive
            }
        }
        Err(crate::operations::boundary_input::AbsorbReason::DriftBeyondTolerance {
            distance,
        }) => {
            // Beyond absorb tolerance — preserve original rejection semantics.
            return Err(BoundaryError::PointNotOnPlane {
                distance_mm: distance,
            });
        }
        Err(_) => point, // other reasons N/A to single Point — defensive
    };

    // Validation #1 — point 가 plane 에 평면적 (LOCKED #5 ε=1.5μm).
    // (post-absorb: projected point is on-plane within numerical precision.)
    let signed_dist = plane.normal.dot(point) - plane.dist;
    let distance_mm = signed_dist.abs();
    if distance_mm > POINT_ON_PLANE_TOL_MM {
        return Err(BoundaryError::PointNotOnPlane { distance_mm });
    }

    // Validation #2 — search_radius 내 orphan edges 수집.
    let radius_mm = if search_radius <= 0.0 {
        DEFAULT_SEARCH_RADIUS_MM
    } else {
        search_radius
    };
    let orphan_edges = collect_orphan_edges_in_radius(mesh, point, radius_mm);
    if orphan_edges.is_empty() {
        return Err(BoundaryError::NoOrphanEdgesInRadius {
            search_radius_mm: radius_mm,
        });
    }

    // Algorithm body — DFS cycle finder + point-in-polygon 2D + smallest
    // enclosing cycle selection.
    let cycles = find_all_cycles(mesh, &orphan_edges);
    if cycles.is_empty() {
        return Err(BoundaryError::NoEnclosingCycle);
    }

    // Build 2D plane basis for point-in-polygon test.
    let (u_axis, v_axis) = plane_basis(plane.normal);
    let point_2d = project_to_plane_2d(point, plane, u_axis, v_axis);

    // Project every cycle once. The outer selection below and the island
    // detection after it both need these, and projecting twice would let the
    // two drift.
    let projected: Vec<(Vec<VertId>, Vec<(f64, f64)>, f64)> = cycles
        .into_iter()
        .filter_map(|cycle_verts| {
            let poly_2d: Vec<(f64, f64)> = cycle_verts
                .iter()
                .filter_map(|&vid| {
                    if mesh.verts.contains(vid) && mesh.verts[vid].is_active() {
                        Some(project_to_plane_2d(mesh.verts[vid].pos(), plane, u_axis, v_axis))
                    } else {
                        None
                    }
                })
                .collect();
            if poly_2d.len() != cycle_verts.len() {
                return None;
            }
            let area = polygon_area_2d(&poly_2d).abs();
            if area <= 1e-9 {
                return None; // degenerate
            }
            Some((cycle_verts, poly_2d, area))
        })
        .collect();

    // Filter cycles enclosing the point + select smallest area.
    let mut best: Option<(Vec<VertId>, f64)> = None;
    for (cycle_verts, poly_2d, area) in projected.iter() {
        if !point_in_polygon_2d(point_2d, poly_2d) {
            continue;
        }
        match &best {
            None => best = Some((cycle_verts.clone(), *area)),
            Some((_, current_area)) => {
                if *area < *current_area {
                    best = Some((cycle_verts.clone(), *area));
                }
            }
        }
    }

    let (cycle_verts, outer_area) = best.ok_or(BoundaryError::NoEnclosingCycle)?;

    // ── Islands (ADR-148 §5 multi-loop) ──────────────────────────────────
    //
    // AutoCAD BPOLY makes a ring when you click between an outer boundary and
    // an island. We were making a solid face over the island: the outer square
    // is the only cycle CONTAINING the click, so it won.
    //
    // Direct children only. A cycle is a hole here when it is inside the outer
    // loop, smaller than it, does not contain the click (that one is already
    // the outer, by smallest-area selection), and is not nested inside another
    // hole — the innermost of a stack belongs to the ring one level down, not
    // to this face. That is BPOLY's "Outer" island style, and it keeps each
    // face's holes disjoint, which is what add_face_with_holes needs.
    let outer_poly: &Vec<(f64, f64)> = projected
        .iter()
        .find(|(cv, _, _)| *cv == cycle_verts)
        .map(|(_, poly, _)| poly)
        .expect("outer cycle came from projected");

    let candidates: Vec<&(Vec<VertId>, Vec<(f64, f64)>, f64)> = projected
        .iter()
        .filter(|(cv, poly, area)| {
            *cv != cycle_verts
                && *area < outer_area
                && polygon_inside_polygon(poly, outer_poly)
        })
        .collect();

    let holes: Vec<Vec<VertId>> = candidates
        .iter()
        .filter(|(_, poly, _)| {
            // Drop the nested ones: if another candidate contains this, this is
            // a grandchild, not a hole of ours.
            !candidates
                .iter()
                .any(|(_, other, other_area)| other_area > &polygon_area_2d(poly).abs()
                    && polygon_inside_polygon(poly, other))
        })
        .map(|(cv, _, _)| cv.clone())
        .collect();

    // Validation #4 — cycle 이 이미 face 인지 검사. 모든 인접 edge 에
    // active face 있으면 CycleAlreadyFaced.
    if cycle_already_faced(mesh, &cycle_verts) {
        // Best-effort: report first face we find.
        let existing_face_id = find_existing_face_id(mesh, &cycle_verts)
            .unwrap_or(FaceId::new(u32::MAX));
        return Err(BoundaryError::CycleAlreadyFaced {
            existing_face_id: existing_face_id.raw(),
        });
    }

    // Face 합성. No holes → add_face, unchanged. Holes → add_face_with_holes,
    // which computes the normal and validates winding the same way
    // (ADR-007 Invariant 2), so the ring is built by the same rules as every
    // other face rather than a second path of its own.
    // Material = default (MaterialId::new(0) — FORM_MATERIAL sentinel).
    let face_id = if holes.is_empty() {
        mesh.add_face(&cycle_verts, MaterialId::new(0))
            .map_err(|_| BoundaryError::NoEnclosingCycle)?
    } else {
        let hole_refs: Vec<&[VertId]> = holes.iter().map(|h| h.as_slice()).collect();
        mesh.add_face_with_holes(&cycle_verts, &hole_refs, MaterialId::new(0))
            .map_err(|_| BoundaryError::NoEnclosingCycle)?
    };

    Ok(face_id)
}

/// Is every vertex of `inner` inside `outer`?
///
/// Enough here because the cycles come from `find_all_cycles` over orphan
/// edges, which cannot cross: two boundary loops either nest or are disjoint.
/// A general polygon-in-polygon test would also need edge-crossing checks.
fn polygon_inside_polygon(inner: &[(f64, f64)], outer: &[(f64, f64)]) -> bool {
    !inner.is_empty() && inner.iter().all(|&p| point_in_polygon_2d(p, outer))
}

/// β-2 helper — collect (edge_id, v_small, v_large) for orphan edges
/// whose any endpoint is within `radius_mm` of `point`.
///
/// Returns a Vec of (EdgeId, VertId, VertId). β-2 MVP uses linear scan;
/// BVH spatial accel is future optimization (Q1=(c) trade-off).
fn collect_orphan_edges_in_radius(
    mesh: &Mesh,
    point: DVec3,
    radius_mm: f64,
) -> Vec<(EdgeId, VertId, VertId)> {
    let r2 = radius_mm * radius_mm;
    let mut result = Vec::new();
    let edge_ids: Vec<EdgeId> = mesh.edges.iter().map(|(id, _)| id).collect();
    for eid in edge_ids {
        let edge = &mesh.edges[eid];
        if !edge.is_active() {
            continue;
        }
        let (faces, _) = mesh.get_faces_sharing_edge(eid);
        let any_face = faces
            .iter()
            .any(|&f| mesh.faces.contains(f) && mesh.faces[f].is_active());
        if any_face {
            continue;
        }
        let va = edge.v_small();
        let vb = edge.v_large();
        if !mesh.verts.contains(va) || !mesh.verts.contains(vb) {
            continue;
        }
        if !mesh.verts[va].is_active() || !mesh.verts[vb].is_active() {
            continue;
        }
        let pa = mesh.verts[va].pos();
        let pb = mesh.verts[vb].pos();
        if pa.distance_squared(point) <= r2 || pb.distance_squared(point) <= r2 {
            result.push((eid, va, vb));
        }
    }
    result
}

/// β-2 helper — DFS cycle finder on the orphan edge subgraph.
///
/// Returns all simple closed cycles (as Vec<VertId>). Each cycle is
/// closed (first vertex == last vertex implicit, not duplicated).
/// MAX_CYCLES = 16 to bound time on dense graphs.
fn find_all_cycles(
    _mesh: &Mesh,
    orphan_edges: &[(EdgeId, VertId, VertId)],
) -> Vec<Vec<VertId>> {
    const MAX_CYCLES: usize = 16;
    const MAX_CYCLE_LEN: usize = 64;

    // Build adjacency
    let mut adj: HashMap<VertId, Vec<VertId>> = HashMap::new();
    for &(_eid, va, vb) in orphan_edges {
        adj.entry(va).or_default().push(vb);
        adj.entry(vb).or_default().push(va);
    }
    let mut cycles: Vec<Vec<VertId>> = Vec::new();
    let mut seen_signatures: HashSet<Vec<u32>> = HashSet::new();

    // Iterate from each vertex as a potential cycle start.
    let mut starts: Vec<VertId> = adj.keys().copied().collect();
    starts.sort_by_key(|v| v.raw());

    for start in starts {
        if cycles.len() >= MAX_CYCLES {
            break;
        }
        let mut path: Vec<VertId> = vec![start];
        let mut visited: HashSet<VertId> = HashSet::new();
        visited.insert(start);
        dfs_find_cycle(start, start, &adj, &mut path, &mut visited, &mut cycles, &mut seen_signatures, MAX_CYCLE_LEN, MAX_CYCLES);
    }
    cycles
}

fn dfs_find_cycle(
    start: VertId,
    current: VertId,
    adj: &HashMap<VertId, Vec<VertId>>,
    path: &mut Vec<VertId>,
    visited: &mut HashSet<VertId>,
    cycles: &mut Vec<Vec<VertId>>,
    seen_signatures: &mut HashSet<Vec<u32>>,
    max_len: usize,
    max_cycles: usize,
) {
    if cycles.len() >= max_cycles || path.len() >= max_len {
        return;
    }
    if let Some(neighbors) = adj.get(&current) {
        for &next in neighbors {
            if next == start && path.len() >= 3 {
                // Cycle found — canonical signature for dedup.
                let mut sig: Vec<u32> = path.iter().map(|v| v.raw()).collect();
                sig.sort();
                if !seen_signatures.contains(&sig) {
                    seen_signatures.insert(sig);
                    cycles.push(path.clone());
                }
                continue;
            }
            if visited.contains(&next) {
                continue;
            }
            visited.insert(next);
            path.push(next);
            dfs_find_cycle(start, next, adj, path, visited, cycles, seen_signatures, max_len, max_cycles);
            path.pop();
            visited.remove(&next);
        }
    }
}

/// β-2 helper — build 2D plane basis from normal.
fn plane_basis(normal: DVec3) -> (DVec3, DVec3) {
    let n = normal.normalize();
    // Pick a reference axis not parallel to n.
    let ref_axis = if n.x.abs() < 0.9 {
        DVec3::X
    } else {
        DVec3::Y
    };
    let u = ref_axis.cross(n).normalize();
    let v = n.cross(u).normalize();
    (u, v)
}

/// β-2 helper — project 3D point to 2D plane coordinates.
fn project_to_plane_2d(point: DVec3, plane: Plane, u: DVec3, v: DVec3) -> (f64, f64) {
    let plane_origin = plane.normal * plane.dist;
    let local = point - plane_origin;
    (local.dot(u), local.dot(v))
}

/// β-2 helper — point-in-polygon 2D (ray-casting).
fn point_in_polygon_2d(point: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let (px, py) = point;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        let intersect = ((yi > py) != (yj > py))
            && (px < (xj - xi) * (py - yi) / (yj - yi + f64::EPSILON) + xi);
        if intersect {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// β-2 helper — signed area of 2D polygon (shoelace).
fn polygon_area_2d(poly: &[(f64, f64)]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        area += (xj + xi) * (yj - yi);
        j = i;
    }
    area * 0.5
}

/// β-2 helper — check if cycle is already an active face.
fn cycle_already_faced(mesh: &Mesh, cycle_verts: &[VertId]) -> bool {
    find_existing_face_id(mesh, cycle_verts).is_some()
}

/// β-2 helper — find existing face id matching cycle (if any).
fn find_existing_face_id(mesh: &Mesh, cycle_verts: &[VertId]) -> Option<FaceId> {
    let cycle_set: HashSet<u32> = cycle_verts.iter().map(|v| v.raw()).collect();
    let face_ids: Vec<FaceId> = mesh.faces.iter().map(|(id, _)| id).collect();
    for fid in face_ids {
        if !mesh.faces[fid].is_active() {
            continue;
        }
        let outer_start = mesh.faces[fid].outer().start;
        if let Ok(face_verts) = mesh.collect_loop_verts(outer_start) {
            if face_verts.len() != cycle_verts.len() {
                continue;
            }
            let face_set: HashSet<u32> = face_verts.iter().map(|v| v.raw()).collect();
            if face_set == cycle_set {
                return Some(fid);
            }
        }
    }
    None
}

// ════════════════════════════════════════════════════════════════════
// β-1 회귀 자산 — 4 tests (절대 #[ignore] 금지)
//
// L-148-7: 절대 #[ignore] 금지 — 회귀 자산 모두 enabled.
// β-1 scope: validation 1+2 + AlgorithmDeferred sentinel.
// β-2 commit 시 happy path (Ok(FaceId)) 회귀 자산 추가.
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mesh;

    fn make_plane_z0() -> Plane {
        Plane {
            normal: DVec3::new(0.0, 0.0, 1.0),
            dist: 0.0,
        }
    }

    #[test]
    fn adr148_beta1_rejects_point_not_on_plane() {
        // L-148-3 정합: point 가 plane 에서 1.5μm 초과 시 강제 reject.
        let mut mesh = Mesh::new();
        let plane = make_plane_z0();
        // Point at z=10mm (well above 1.5μm tolerance).
        let point = DVec3::new(0.0, 0.0, 10.0);
        let result = boundary_from_point(&mut mesh, point, plane, 1000.0);
        match result {
            Err(BoundaryError::PointNotOnPlane { distance_mm }) => {
                assert!(
                    (distance_mm - 10.0).abs() < 1e-6,
                    "expected distance ~10mm, got {}",
                    distance_mm,
                );
            }
            other => panic!("expected PointNotOnPlane, got {:?}", other),
        }
    }

    #[test]
    fn adr148_beta1_point_within_tolerance_passes_validation_1() {
        // Boundary edge case — distance just below POINT_ON_PLANE_TOL_MM.
        let mut mesh = Mesh::new();
        let plane = make_plane_z0();
        // Point at z = 1.4μm (just within 1.5μm tolerance).
        let point = DVec3::new(0.0, 0.0, 1.4e-3);
        let result = boundary_from_point(&mut mesh, point, plane, 1000.0);
        // Validation #1 passes, but validation #2 fails (empty mesh,
        // no orphan edges in radius).
        match result {
            Err(BoundaryError::NoOrphanEdgesInRadius { search_radius_mm }) => {
                assert!((search_radius_mm - 1000.0).abs() < 1e-9);
            }
            other => panic!("expected NoOrphanEdgesInRadius, got {:?}", other),
        }
    }

    #[test]
    fn adr171_beta2_drift_gap_point_projected_passes_validation_1() {
        // ADR-171 β-2 — point at z=0.5mm is in the (1.5μm, 1mm) drift gap.
        // BEFORE absorb: PointNotOnPlane. AFTER absorb: projected onto plane
        // → passes Validation #1 → fails Validation #2 (empty mesh, no
        // orphan edges). Proves the absorb projection (robust 흡수).
        let mut mesh = Mesh::new();
        let plane = make_plane_z0();
        let point = DVec3::new(0.0, 0.0, 0.5); // 0.5mm off-plane (within 1mm)
        let result = boundary_from_point(&mut mesh, point, plane, 1000.0);
        match result {
            // Validation #1 now passes (projected); Validation #2 fails.
            Err(BoundaryError::NoOrphanEdgesInRadius { .. }) => {}
            other => panic!(
                "expected NoOrphanEdgesInRadius (absorb projected the drift), got {:?}",
                other
            ),
        }
    }

    #[test]
    fn adr171_beta2_far_off_plane_still_rejected() {
        // ADR-171 β-2 — point 10mm off-plane is BEYOND MAX_DRIFT_MM (1.0).
        // absorb returns DriftBeyondTolerance → PointNotOnPlane (preserves
        // far-off rejection semantics, 메타-원칙 #15 contract).
        let mut mesh = Mesh::new();
        let plane = make_plane_z0();
        let point = DVec3::new(0.0, 0.0, 10.0);
        let result = boundary_from_point(&mut mesh, point, plane, 1000.0);
        match result {
            Err(BoundaryError::PointNotOnPlane { distance_mm }) => {
                assert!(
                    (distance_mm - 10.0).abs() < 1e-6,
                    "expected ~10mm, got {}",
                    distance_mm
                );
            }
            other => panic!("expected PointNotOnPlane, got {:?}", other),
        }
    }

    #[test]
    fn adr148_beta1_negative_radius_uses_default() {
        // search_radius ≤ 0 → DEFAULT_SEARCH_RADIUS_MM (1000mm).
        let mut mesh = Mesh::new();
        let plane = make_plane_z0();
        let point = DVec3::new(0.0, 0.0, 0.0);

        // Test both 0 and negative.
        for radius in [0.0, -1.0, -100.0] {
            let result = boundary_from_point(&mut mesh, point, plane, radius);
            match result {
                Err(BoundaryError::NoOrphanEdgesInRadius { search_radius_mm }) => {
                    assert!(
                        (search_radius_mm - DEFAULT_SEARCH_RADIUS_MM).abs() < 1e-9,
                        "expected default {} for input {}",
                        DEFAULT_SEARCH_RADIUS_MM,
                        radius,
                    );
                }
                other => panic!("expected default radius substitution, got {:?}", other),
            }
        }
    }

    #[test]
    fn adr148_beta2_isolated_open_chain_returns_no_enclosing_cycle() {
        // β-2: orphan edge present but no closed cycle → NoEnclosingCycle.
        // Single edge from (0,0,0) to (10,0,0). Point on plane within
        // radius. No cycle exists.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let _ = mesh.add_edge(v0, v1);

        let plane = make_plane_z0();
        let point = DVec3::new(5.0, 0.0, 0.0);
        let result = boundary_from_point(&mut mesh, point, plane, 100.0);
        assert_eq!(result, Err(BoundaryError::NoEnclosingCycle));
    }

    #[test]
    fn adr148_beta2_square_cycle_synthesizes_face() {
        // β-2 happy path: 4 orphan edges forming a square on Z=0 plane,
        // point at center → Ok(FaceId).
        let mut mesh = Mesh::new();
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        // 4 orphan edges (square boundary, no face yet).
        let _ = mesh.add_edge(v00, v10);
        let _ = mesh.add_edge(v10, v11);
        let _ = mesh.add_edge(v11, v01);
        let _ = mesh.add_edge(v01, v00);

        let plane = make_plane_z0();
        let center = DVec3::new(5.0, 5.0, 0.0); // inside square
        let result = boundary_from_point(&mut mesh, center, plane, 100.0);
        assert!(
            matches!(result, Ok(_)),
            "expected Ok(FaceId) for centered point in square cycle, got {:?}",
            result,
        );
    }

    #[test]
    fn adr148_beta2_point_outside_cycle_returns_no_enclosing() {
        // β-2: square cycle exists but point is outside → NoEnclosingCycle.
        let mut mesh = Mesh::new();
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let _ = mesh.add_edge(v00, v10);
        let _ = mesh.add_edge(v10, v11);
        let _ = mesh.add_edge(v11, v01);
        let _ = mesh.add_edge(v01, v00);

        let plane = make_plane_z0();
        // Point well outside (15, 5, 0) but still within search radius
        // (edges within 100mm).
        let outside = DVec3::new(15.0, 5.0, 0.0);
        let result = boundary_from_point(&mut mesh, outside, plane, 100.0);
        assert_eq!(result, Err(BoundaryError::NoEnclosingCycle));
    }

    /// Square inside a square, click in the ring between them — AutoCAD BPOLY
    /// makes a ring, and so do we now.
    ///
    /// This started as a de-risk: before the multi-loop work it asserted
    /// (4, 0), recording that the single-loop version swallowed the island
    /// because only the outer square contains the click. It is the same test,
    /// now asserting the fix.
    #[test]
    fn adr148_multiloop_island_in_ring_becomes_a_hole() {
        let mut mesh = Mesh::new();
        // Outer square 0..20
        let o00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let o10 = mesh.add_vertex(DVec3::new(20.0, 0.0, 0.0));
        let o11 = mesh.add_vertex(DVec3::new(20.0, 20.0, 0.0));
        let o01 = mesh.add_vertex(DVec3::new(0.0, 20.0, 0.0));
        let _ = mesh.add_edge(o00, o10);
        let _ = mesh.add_edge(o10, o11);
        let _ = mesh.add_edge(o11, o01);
        let _ = mesh.add_edge(o01, o00);
        // Inner square 8..12 — the island
        let i00 = mesh.add_vertex(DVec3::new(8.0, 8.0, 0.0));
        let i10 = mesh.add_vertex(DVec3::new(12.0, 8.0, 0.0));
        let i11 = mesh.add_vertex(DVec3::new(12.0, 12.0, 0.0));
        let i01 = mesh.add_vertex(DVec3::new(8.0, 12.0, 0.0));
        let _ = mesh.add_edge(i00, i10);
        let _ = mesh.add_edge(i10, i11);
        let _ = mesh.add_edge(i11, i01);
        let _ = mesh.add_edge(i01, i00);

        let plane = make_plane_z0();
        // (3, 3) — inside the outer square, outside the island. The ring.
        let in_ring = DVec3::new(3.0, 3.0, 0.0);
        let result = boundary_from_point(&mut mesh, in_ring, plane, 100.0);

        let face_id = result.expect("ring click should synthesize something");
        let inners = mesh.faces[face_id].inners().len();
        let outer_start = mesh.faces[face_id].outer().start;
        let outer_len = mesh
            .collect_loop_verts(outer_start)
            .expect("outer loop should walk")
            .len();

        assert_eq!(
            (outer_len, inners),
            (4, 1),
            "clicking the ring should give the outer square with the island as a \
             hole, got outer={} inners={}",
            outer_len,
            inners,
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "ring face must satisfy ADR-007 invariants",
        );
    }

    #[test]
    fn adr148_multiloop_click_inside_the_island_faces_the_island_only() {
        // Same geometry, click INSIDE the inner square. The island is now the
        // smallest cycle containing the click, so it is the outer loop — and
        // it has no holes of its own. Nothing about the ring case may leak in.
        let mut mesh = Mesh::new();
        let o00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let o10 = mesh.add_vertex(DVec3::new(20.0, 0.0, 0.0));
        let o11 = mesh.add_vertex(DVec3::new(20.0, 20.0, 0.0));
        let o01 = mesh.add_vertex(DVec3::new(0.0, 20.0, 0.0));
        let _ = mesh.add_edge(o00, o10);
        let _ = mesh.add_edge(o10, o11);
        let _ = mesh.add_edge(o11, o01);
        let _ = mesh.add_edge(o01, o00);
        let i00 = mesh.add_vertex(DVec3::new(8.0, 8.0, 0.0));
        let i10 = mesh.add_vertex(DVec3::new(12.0, 8.0, 0.0));
        let i11 = mesh.add_vertex(DVec3::new(12.0, 12.0, 0.0));
        let i01 = mesh.add_vertex(DVec3::new(8.0, 12.0, 0.0));
        let _ = mesh.add_edge(i00, i10);
        let _ = mesh.add_edge(i10, i11);
        let _ = mesh.add_edge(i11, i01);
        let _ = mesh.add_edge(i01, i00);

        let plane = make_plane_z0();
        let face_id = boundary_from_point(&mut mesh, DVec3::new(10.0, 10.0, 0.0), plane, 100.0)
            .expect("click inside the island should face the island");
        assert_eq!(
            mesh.faces[face_id].inners().len(),
            0,
            "the island itself has no holes",
        );
        let outer = mesh
            .collect_loop_verts(mesh.faces[face_id].outer().start)
            .expect("outer loop walks");
        assert_eq!(outer.len(), 4, "island outer loop is the 4-vert inner square");
    }

    #[test]
    fn adr148_multiloop_plain_square_still_has_no_holes() {
        // Regression guard for the no-island path: the projection refactor and
        // the island filter must leave the ordinary case exactly as it was.
        let mut mesh = Mesh::new();
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let _ = mesh.add_edge(v00, v10);
        let _ = mesh.add_edge(v10, v11);
        let _ = mesh.add_edge(v11, v01);
        let _ = mesh.add_edge(v01, v00);

        let plane = make_plane_z0();
        let face_id = boundary_from_point(&mut mesh, DVec3::new(5.0, 5.0, 0.0), plane, 100.0)
            .expect("plain square still faces");
        assert_eq!(mesh.faces[face_id].inners().len(), 0);
        assert!(mesh.verify_face_invariants().is_valid());
    }

    #[test]
    fn adr148_multiloop_nested_island_is_not_our_hole() {
        // Three rings: outer 0..30, middle 5..25, inner 10..20. Click between
        // outer and middle.
        //
        // The middle is our hole. The inner is the middle's business — it sits
        // inside the middle, one level down, and a hole inside a hole is not a
        // hole of this face. Without the nested filter both would be handed to
        // add_face_with_holes and the face would claim a region it does not
        // bound. (BPOLY calls this the "Outer" island style.)
        let mut mesh = Mesh::new();
        let sq = |mesh: &mut Mesh, lo: f64, hi: f64| {
            let a = mesh.add_vertex(DVec3::new(lo, lo, 0.0));
            let b = mesh.add_vertex(DVec3::new(hi, lo, 0.0));
            let c = mesh.add_vertex(DVec3::new(hi, hi, 0.0));
            let d = mesh.add_vertex(DVec3::new(lo, hi, 0.0));
            let _ = mesh.add_edge(a, b);
            let _ = mesh.add_edge(b, c);
            let _ = mesh.add_edge(c, d);
            let _ = mesh.add_edge(d, a);
        };
        sq(&mut mesh, 0.0, 30.0);
        sq(&mut mesh, 5.0, 25.0);
        sq(&mut mesh, 10.0, 20.0);

        let plane = make_plane_z0();
        // (2, 2) — between outer and middle.
        let face_id = boundary_from_point(&mut mesh, DVec3::new(2.0, 2.0, 0.0), plane, 200.0)
            .expect("outermost ring");
        assert_eq!(
            mesh.faces[face_id].inners().len(),
            1,
            "only the middle square is our hole; the innermost belongs to the \
             middle's own ring, not to this face",
        );
        assert!(mesh.verify_face_invariants().is_valid());
    }

    #[test]
    fn adr148_multiloop_two_islands_become_two_holes() {
        // Two disjoint islands in one outer square — both are direct children,
        // so both are holes of the same ring (LOCKED #1 P7: disjoint inner
        // components → multi-hole ring).
        let mut mesh = Mesh::new();
        let o00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let o10 = mesh.add_vertex(DVec3::new(30.0, 0.0, 0.0));
        let o11 = mesh.add_vertex(DVec3::new(30.0, 20.0, 0.0));
        let o01 = mesh.add_vertex(DVec3::new(0.0, 20.0, 0.0));
        let _ = mesh.add_edge(o00, o10);
        let _ = mesh.add_edge(o10, o11);
        let _ = mesh.add_edge(o11, o01);
        let _ = mesh.add_edge(o01, o00);
        for (x0, x1) in [(5.0_f64, 10.0_f64), (20.0, 25.0)] {
            let a = mesh.add_vertex(DVec3::new(x0, 5.0, 0.0));
            let b = mesh.add_vertex(DVec3::new(x1, 5.0, 0.0));
            let c = mesh.add_vertex(DVec3::new(x1, 15.0, 0.0));
            let d = mesh.add_vertex(DVec3::new(x0, 15.0, 0.0));
            let _ = mesh.add_edge(a, b);
            let _ = mesh.add_edge(b, c);
            let _ = mesh.add_edge(c, d);
            let _ = mesh.add_edge(d, a);
        }

        let plane = make_plane_z0();
        // (15, 10) — between the two islands, inside the outer square.
        let face_id = boundary_from_point(&mut mesh, DVec3::new(15.0, 10.0, 0.0), plane, 200.0)
            .expect("ring with two islands");
        assert_eq!(
            mesh.faces[face_id].inners().len(),
            2,
            "both disjoint islands are holes of this ring",
        );
        assert!(mesh.verify_face_invariants().is_valid());
    }
}
