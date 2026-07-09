//! ADR-145 — Circle Annulus 명시 활성 (옵션 B).
//!
//! Circle 두 별개 face → 사용자 명시 trigger ("annulus 만들기" 우클릭)
//! 시 outer face 의 hole 로 inner Circle 명시 promote.
//!
//! **메타-원칙 #16 정합**: 휴리스틱 자동 annulus promote 폐기, 사용자
//! 명시 의도 canonical. ADR-139 (Boundary tool 명시) pattern 1:1 mirror.
//!
//! # β-1+ scope (current commit — promote logic 활성)
//!
//! - `AnnulusError` enum (4 variant — validation only, PromoteLogicDeferred 제거)
//! - `promote_circles_to_annulus(&mut Mesh, ...)` — 4 validation +
//!   promote logic full implementation
//!
//! Validation 4단계:
//! 1. outer + inner 둘 다 active face
//! 2. 둘 다 closed-curve Circle face (outer loop = 1 self-loop edge with
//!    `AnalyticCurve::Circle`)
//! 3. outer + inner coplanar (normal parallel + 같은 plane 식 정합)
//! 4. inner Circle fully contained in outer Circle (center distance +
//!    inner.radius <= outer.radius)
//!
//! Promote logic (`create_solid.rs` annulus_face pattern 1:1 답습):
//! 1. inner face 의 outer LoopRef + HEs collect
//! 2. HEs reparent (face pointer → outer_face_id, set_outer(false))
//! 3. outer face 에 `add_inner(inner_outer_loop)` 호출
//! 4. inner face `set_active(false)` (HE/edge/vert 보존)
//!
//! # Cross-link
//!
//! - ADR-145 α spec (docs/adr/145-circle-annulus-explicit-activation.md)
//! - ADR-139 (Boundary tool 명시) — pattern 1:1 mirror
//! - ADR-089 Phase 2 (closed-curve face) — `add_face_closed_curve`
//! - 메타-원칙 #16 (자동화 antipattern)
//! - LOCKED #1 P7 (hole loop manifold)
//! - LOCKED #44 (Complete Meaning per Merge — sub-step atomic)
//! - LOCKED #66 (ADR-164 Sunset Policy — Status canonical)

use crate::entities::LoopRef;
use crate::mesh::Mesh;
use crate::FaceId;
use glam::DVec3;

/// ADR-145 β-1 — Circle annulus promote errors.
///
/// Returned by `promote_circles_to_annulus`. Each variant 은 명시
/// validation failure (silent skip 차단, 메타-원칙 #16 정합).
#[derive(Debug, Clone, PartialEq)]
pub enum AnnulusError {
    /// outer 또는 inner face 가 inactive 또는 not found.
    InactiveFace { face_id: u32, role: &'static str },

    /// outer 또는 inner 가 closed-curve Circle face 아님 (outer loop
    /// 가 1 self-loop edge with `AnalyticCurve::Circle` 형태 아님).
    NotCircleFace { face_id: u32, role: &'static str },

    /// outer + inner 가 다른 평면 (normal parallel 미달 또는 plane
    /// 식 distance 미달).
    NotCoplanar {
        outer_normal: DVec3,
        inner_normal: DVec3,
        plane_distance: f64,
    },

    /// inner Circle 이 outer Circle 안에 fully contained 안 됨
    /// (off-center distance + inner.radius > outer.radius).
    InnerNotContained {
        center_distance: f64,
        inner_radius: f64,
        outer_radius: f64,
    },
}

impl std::fmt::Display for AnnulusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InactiveFace { face_id, role } => write!(
                f,
                "ADR-145: {} face {} is inactive or not found",
                role, face_id,
            ),
            Self::NotCircleFace { face_id, role } => write!(
                f,
                "ADR-145: {} face {} is not a closed-curve Circle face \
                 (expected 1 self-loop edge with AnalyticCurve::Circle)",
                role, face_id,
            ),
            Self::NotCoplanar {
                outer_normal,
                inner_normal,
                plane_distance,
            } => write!(
                f,
                "ADR-145: outer + inner not coplanar (outer_normal={:?}, \
                 inner_normal={:?}, plane_distance={:.3e})",
                outer_normal, inner_normal, plane_distance,
            ),
            Self::InnerNotContained {
                center_distance,
                inner_radius,
                outer_radius,
            } => write!(
                f,
                "ADR-145: inner Circle not fully contained in outer Circle \
                 (center_distance={:.3} + inner_radius={:.3} > outer_radius={:.3})",
                center_distance, inner_radius, outer_radius,
            ),
        }
    }
}

impl std::error::Error for AnnulusError {}

// ADR-167 β-3 — `COPLANAR_TOL` alias removed; callsite now imports
// `crate::plane::EPS_PLANE_OFFSET` directly (canonical SSOT, 1.5μm,
// LOCKED #5 spatial-hash dedup). Pre-β-3 alias was `const COPLANAR_TOL:
// f64 = crate::plane::EPS_PLANE_OFFSET;` — identical value, redundant
// indirection sunset.
/// Normal direction parity tolerance (parallel — 1 - |dot| < 1e-6 = nearly parallel).
///
/// ADR-167 β-2 — *Stricter than* canonical `EPS_PLANE_NORMAL` (1e-4) —
/// annulus inherits its inner circle's plane via Plane attach, so the
/// parity check tolerates only numerical drift (1e-6), not modeling
/// slop. Preserved per-call override (L-167-3 "Per-call tolerance
/// overrides").
const NORMAL_PARITY_TOL: f64 = 1e-6;

/// ADR-145 — Circle annulus 명시 promote.
///
/// 두 coplanar Circle face (outer + inner) 를 annulus (outer with
/// inner hole) 로 명시 promote. inner face deactivate.
///
/// **사용자 명시 trigger only** (메타-원칙 #16) — 휴리스틱 자동 detect
/// 안 됨. ContextMenu "annulus 만들기" 우클릭 후 호출 (β-4).
///
/// # β-1+ scope (current — promote logic 활성)
///
/// Validation 4단계 + promote logic full implementation. `create_solid.rs`
/// 의 annulus_face 패턴 1:1 답습 — HE reparent (set_face/set_outer) +
/// outer face `add_inner(LoopRef)` + inner face deactivate.
///
/// # Errors
///
/// - `InactiveFace` — outer 또는 inner active 아님
/// - `NotCircleFace` — outer 또는 inner 가 closed-curve Circle 아님
/// - `NotCoplanar` — 다른 평면
/// - `InnerNotContained` — inner Circle 이 outer 안 contained 안 됨
pub fn promote_circles_to_annulus(
    mesh: &mut Mesh,
    outer_face: FaceId,
    inner_face: FaceId,
) -> Result<(), AnnulusError> {
    // === Validation 1: outer + inner active ===
    let outer = mesh.faces.get(outer_face).ok_or(AnnulusError::InactiveFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    if !outer.is_active() {
        return Err(AnnulusError::InactiveFace {
            face_id: outer_face.raw(),
            role: "outer",
        });
    }
    let inner = mesh.faces.get(inner_face).ok_or(AnnulusError::InactiveFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;
    if !inner.is_active() {
        return Err(AnnulusError::InactiveFace {
            face_id: inner_face.raw(),
            role: "inner",
        });
    }

    // === Validation 2: 둘 다 Circle face ===
    let outer_circle = extract_circle(mesh, outer_face).ok_or(AnnulusError::NotCircleFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    let inner_circle = extract_circle(mesh, inner_face).ok_or(AnnulusError::NotCircleFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;

    // === Validation 3: coplanar (normal parallel + plane distance) ===
    let n_outer = outer_circle.normal.normalize_or_zero();
    let n_inner = inner_circle.normal.normalize_or_zero();
    let dot = n_outer.dot(n_inner).abs();
    if (1.0 - dot) > NORMAL_PARITY_TOL {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: n_outer,
            inner_normal: n_inner,
            plane_distance: f64::INFINITY,
        });
    }
    // Plane equation distance: (inner.center - outer.center) · outer.normal
    let plane_distance = (inner_circle.center - outer_circle.center).dot(n_outer).abs();
    // ADR-167 β-3 — canonical SSOT (EPS_PLANE_OFFSET = 1.5μm).
    if plane_distance > crate::plane::EPS_PLANE_OFFSET {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: n_outer,
            inner_normal: n_inner,
            plane_distance,
        });
    }

    // === Validation 4: inner ⊂ outer ===
    let center_distance = (inner_circle.center - outer_circle.center).length();
    if center_distance + inner_circle.radius > outer_circle.radius {
        return Err(AnnulusError::InnerNotContained {
            center_distance,
            inner_radius: inner_circle.radius,
            outer_radius: outer_circle.radius,
        });
    }

    // === Promote logic (β-1+ — create_solid.rs annulus_face pattern 1:1 답습) ===

    // 1. Collect inner face 의 outer loop HEs (Circle face = 1 self-loop HE)
    //    Validation 2 (extract_circle) 가 이미 보장 — collect_loop_hes safe.
    let inner_outer_start = mesh.faces[inner_face].outer().start;
    let hes = mesh.collect_loop_hes(inner_outer_start)
        .expect("ADR-145 β-1+: validation 2 (Circle face) guarantees collect_loop_hes OK");

    // 2. Get inner outer LoopRef (Copy — LoopRef is small struct)
    let inner_outer_loop = mesh.faces[inner_face].outer();

    // 3. Reparent HEs (face pointer → outer_face_id, set_outer(false))
    //    create_solid.rs:917-928 pattern 답습.
    for he_id in &hes {
        mesh.hes[*he_id].set_face(outer_face);
        mesh.hes[*he_id].set_outer(false);  // 이제 inner loop (hole)
    }

    // 4. Add inner loop to outer face (Face::add_inner — ADR-061 Step 2:
    //    bumps boundary_version + invalidates normal_cache)
    mesh.faces[outer_face].add_inner(inner_outer_loop);

    // 5. Deactivate inner face (HE/edge/vert 보존 — manifold safe).
    //    inner face 의 outer LoopRef 가 outer face 의 inner 로 reparent 된
    //    상태이므로 inner face 자체는 dangling outer ref 가 있으나 inactive.
    mesh.faces[inner_face].set_active(false);

    Ok(())
}

/// ADR-185 — Circle containment → **ring + inner disk** (면분할).
///
/// `promote_circles_to_annulus` 와 달리 inner disk 를 **보존**한다. outer face
/// 를 ring 으로 (inner circle = hole) 만들되, inner face 는 disk 로 유지 →
/// 두 face (ring + disk). 사용자 "원 안에 원을 그려서 면분할" 의미.
///
/// 차이: annulus 는 inner 의 outer-loop HE (CCW) 를 reparent + inner deactivate
/// → ring + 빈 hole. ring+disk 는 inner edge 의 **twin HE** (CW, ring 측) 를
/// outer 의 hole 로 사용 → inner disk 의 HE (CCW) 와 분리, inner 유지. edge 는
/// 2 face-bearing HE (disk + ring) → manifold.
pub fn split_face_by_inner_circle(
    mesh: &mut Mesh,
    outer_face: FaceId,
    inner_face: FaceId,
) -> Result<(), AnnulusError> {
    // === Validation 1: outer + inner active ===
    let outer = mesh.faces.get(outer_face).ok_or(AnnulusError::InactiveFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    if !outer.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" });
    }
    let inner = mesh.faces.get(inner_face).ok_or(AnnulusError::InactiveFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;
    if !inner.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: inner_face.raw(), role: "inner" });
    }

    // === Validation 2: 둘 다 Circle face ===
    let outer_circle = extract_circle(mesh, outer_face).ok_or(AnnulusError::NotCircleFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    let inner_circle = extract_circle(mesh, inner_face).ok_or(AnnulusError::NotCircleFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;

    // === Validation 3: coplanar ===
    let n_outer = outer_circle.normal.normalize_or_zero();
    let n_inner = inner_circle.normal.normalize_or_zero();
    if (1.0 - n_outer.dot(n_inner).abs()) > NORMAL_PARITY_TOL {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: n_outer,
            inner_normal: n_inner,
            plane_distance: f64::INFINITY,
        });
    }
    let plane_distance = (inner_circle.center - outer_circle.center).dot(n_outer).abs();
    if plane_distance > crate::plane::EPS_PLANE_OFFSET {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: n_outer,
            inner_normal: n_inner,
            plane_distance,
        });
    }

    // === Validation 4: inner ⊂ outer ===
    let center_distance = (inner_circle.center - outer_circle.center).length();
    if center_distance + inner_circle.radius > outer_circle.radius {
        return Err(AnnulusError::InnerNotContained {
            center_distance,
            inner_radius: inner_circle.radius,
            outer_radius: outer_circle.radius,
        });
    }

    // === Ring + disk promote (inner disk 보존) ===
    // inner 의 outer-loop HE (HE1, CCW disk boundary).
    let he1 = mesh.faces[inner_face].outer().start;
    // twin HE (HE2, CW ring-side) via radial chain.
    let he2 = mesh.hes[he1].next_rad();
    if he2 == he1 || !mesh.hes.contains(he2) {
        // 2-manifold circle edge 면 twin 항상 존재 — 방어적 silent reject.
        return Err(AnnulusError::NotCircleFace { face_id: inner_face.raw(), role: "inner" });
    }
    // twin → outer face 의 hole 로 reparent.
    mesh.hes[he2].set_face(outer_face);
    mesh.hes[he2].set_outer(false);
    mesh.faces[outer_face].add_inner(LoopRef { start: he2, is_outer: false });
    // inner disk 는 active 유지 (HE1 그대로 inner face boundary).
    Ok(())
}

/// **시뮬레이션 (원-in-다각형 smooth hole)** — `split_face_by_inner_circle` 의
/// 일반화: outer 가 **임의 face (다각형/원)** 여도 inner Circle 을 smooth
/// self-loop hole 로 부여. outer-원 검증(2) + radius containment(4) 를
/// **point-in-polygon containment** 로 대체. reparent 메커니즘(inner disk 의
/// twin HE → outer hole)은 shape-agnostic 동일.
///
/// 결과: outer (다각형) 가 **매끈한 곡선 원 hole** 보유 + inner disk 보존
/// = 2 face, manifold (원 edge 2 face-bearing HE). circle-in-rect 한계 해소.
pub fn split_face_by_inner_circle_generic(
    mesh: &mut Mesh,
    outer_face: FaceId,
    inner_face: FaceId,
) -> Result<(), AnnulusError> {
    use crate::boundary_kernel::geom2::{point_in_polygon_even_odd, Pip, Vec2};

    // 1. active
    let outer = mesh.faces.get(outer_face).ok_or(AnnulusError::InactiveFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    if !outer.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" });
    }
    let inner = mesh.faces.get(inner_face).ok_or(AnnulusError::InactiveFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;
    if !inner.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: inner_face.raw(), role: "inner" });
    }

    // 2. inner 는 Circle (outer 는 임의 shape — 검증 안 함).
    let inner_circle = extract_circle(mesh, inner_face).ok_or(AnnulusError::NotCircleFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;

    // 3. coplanar (outer face normal vs inner circle normal).
    let outer_n = mesh.faces[outer_face].normal().normalize_or_zero();
    let inner_n = inner_circle.normal.normalize_or_zero();
    if (1.0 - outer_n.dot(inner_n).abs()) > NORMAL_PARITY_TOL {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: outer_n,
            inner_normal: inner_n,
            plane_distance: f64::INFINITY,
        });
    }

    // 4. inner center 가 outer polygon 안 (point-in-polygon, 2D projection).
    let outer_start = mesh.faces[outer_face].outer().start;
    let verts = mesh
        .collect_loop_verts(outer_start)
        .map_err(|_| AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" })?;
    if verts.is_empty() {
        return Err(AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" });
    }
    let origin = mesh.verts.get(verts[0]).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
    let plane_dist = (inner_circle.center - origin).dot(outer_n).abs();
    if plane_dist > crate::plane::EPS_PLANE_OFFSET {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: outer_n,
            inner_normal: inner_n,
            plane_distance: plane_dist,
        });
    }
    // 2D basis from outer normal.
    let aux = if outer_n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = outer_n.cross(aux).normalize_or_zero();
    let v = outer_n.cross(u).normalize_or_zero();
    let poly2d: Vec<Vec2> = verts
        .iter()
        .map(|&vid| {
            let p = mesh.verts.get(vid).map(|x| x.pos()).unwrap_or(DVec3::ZERO) - origin;
            Vec2::new(p.dot(u), p.dot(v))
        })
        .collect();
    let cp = inner_circle.center - origin;
    let c2d = Vec2::new(cp.dot(u), cp.dot(v));
    if point_in_polygon_even_odd(c2d, &poly2d, 1e-6) != Pip::Inside {
        return Err(AnnulusError::InnerNotContained {
            center_distance: cp.length(),
            inner_radius: inner_circle.radius,
            outer_radius: 0.0, // 다각형 outer — radius 무의미 (sentinel).
        });
    }

    // === reparent (split_face_by_inner_circle 와 동일, shape-agnostic) ===
    let he1 = mesh.faces[inner_face].outer().start;
    let he2 = mesh.hes[he1].next_rad();
    if he2 == he1 || !mesh.hes.contains(he2) {
        return Err(AnnulusError::NotCircleFace { face_id: inner_face.raw(), role: "inner" });
    }
    mesh.hes[he2].set_face(outer_face);
    mesh.hes[he2].set_outer(false);
    mesh.faces[outer_face].add_inner(LoopRef { start: he2, is_outer: false });
    Ok(())
}

/// ADR-279 β — Circle-hole containment "size" for innermost-parent ordering.
///
/// Circle face → π·r² (from metadata — `face_area` returns ~0 for a self-loop);
/// polygon face → `face_area` (shoelace of the outer loop). Monotonic enclosed
/// area, comparable across circle/polygon, used only to order containers
/// smallest-first.
fn face_containment_size(mesh: &Mesh, fid: FaceId) -> f64 {
    if let Some(c) = extract_circle(mesh, fid) {
        std::f64::consts::PI * c.radius * c.radius
    } else {
        mesh.face_area(fid)
    }
}

/// ADR-279 β — is this circle face ALREADY a "disk" whose rim is a hole of some
/// container (i.e., a ring+disk relationship already exists)?
///
/// A ring+disk split reparents the circle's twin half-edge to the container as an
/// INNER (hole) loop. On a scoped re-derive (drawing a 2nd concentric circle), the
/// outer container + its existing circle hole are preserved untouched, so this
/// circle's twin already points at an active container as a non-outer loop.
/// Re-assigning it would add a DUPLICATE hole → the rim edge gets a 3rd
/// face-bearing HE → non-manifold. So a circle already-a-hole is skipped as an
/// inner candidate (it may still serve as a CONTAINER for a smaller circle).
///
/// A fresh / standalone circle's twin has a null face → returns false.
fn circle_already_hole(mesh: &Mesh, fid: FaceId) -> bool {
    let Some(face) = mesh.faces.get(fid) else { return false };
    let he1 = face.outer().start;
    if he1.is_null() || !mesh.hes.contains(he1) {
        return false;
    }
    let he2 = mesh.hes[he1].next_rad();
    if he2 == he1 || !mesh.hes.contains(he2) {
        return false;
    }
    let tf = mesh.hes[he2].face();
    !tf.is_null()
        && tf != fid
        && mesh.faces.get(tf).map_or(false, |f| f.is_active())
        && !mesh.hes[he2].is_outer()
}

/// ADR-279 β — assign every circle hole to its **innermost** parent ONLY.
///
/// The old Scene post-process scanned coplanar face PAIRS with an order-dependent
/// `processed` guard. Because containment is transitive (R20 ⊂ disk40 ⊂ box-top),
/// a circle nested at depth ≥ 2 could be assigned as a hole to MULTIPLE enclosing
/// faces (immediate parent AND grandparent) → the self-loop edge ended up
/// referenced by 3 face-bearing half-edges → **non-manifold** (nm=1), solid opens
/// (the "곡선 annulus 한계", ADR-279).
///
/// Canonical single-parent assignment (메타-원칙 #4 SSOT): sort candidate faces
/// by enclosed area ASCENDING, then for each circle `inner` (smallest first)
/// assign it as a hole to the FIRST (⇒ smallest ⇒ innermost) larger face that
/// contains it, and STOP. A face can be both an inner (of a bigger face) and a
/// container (of a smaller circle) — only the inner side is de-duplicated, so
/// perfect nesting at any depth resolves to one-hole-per-parent (L-279-3/4).
///
/// Returns the number of circle holes assigned.
pub fn assign_circle_holes_innermost(mesh: &mut Mesh, faces: &[FaceId]) -> usize {
    use std::cmp::Ordering;
    let mut sized: Vec<(FaceId, f64)> = faces
        .iter()
        .copied()
        .filter(|&f| mesh.faces.get(f).map_or(false, |x| x.is_active()))
        .map(|f| (f, face_containment_size(mesh, f)))
        .collect();
    sized.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

    let mut processed: std::collections::HashSet<FaceId> = std::collections::HashSet::new();
    let mut count = 0usize;
    for i in 0..sized.len() {
        let inner = sized[i].0;
        if processed.contains(&inner) {
            continue;
        }
        // Only a circle face can become a smooth (self-loop) hole here.
        if extract_circle(mesh, inner).is_none() {
            continue;
        }
        // Skip a circle that is ALREADY a disk whose rim is a container's hole
        // (ring+disk already formed on a prior draw / preserved by the scoped
        // re-derive) — re-assigning it duplicates the hole → non-manifold. It can
        // still act as a CONTAINER for a smaller circle below.
        if circle_already_hole(mesh, inner) {
            continue;
        }
        // First (smallest ascending) larger face that contains `inner` = its
        // innermost parent. Assign the circle hole there ONLY, then break.
        for j in (i + 1)..sized.len() {
            let outer = sized[j].0;
            if !mesh.faces.get(inner).map_or(false, |x| x.is_active()) {
                break;
            }
            if !mesh.faces.get(outer).map_or(false, |x| x.is_active()) {
                continue;
            }
            let assigned = if extract_circle(mesh, outer).is_some() {
                // both circles — split_face_by_inner_circle validates containment.
                split_face_by_inner_circle(mesh, outer, inner).is_ok()
            } else {
                // polygon outer + circle inner — generic validates point-in-poly.
                split_face_by_inner_circle_generic(mesh, outer, inner).is_ok()
            };
            if assigned {
                processed.insert(inner);
                count += 1;
                break; // innermost container found — do NOT assign to grandparents.
            }
        }
    }
    count
}

/// ADR-283 β — assign every POLYGON inner (rect / N-gon) to its INNERMOST
/// containing coplanar face as a HOLE (reparent the inner's twin loop). The
/// polygon-inner mirror of `assign_circle_holes_innermost`, for the containment
/// the circle paths don't cover (a rect drawn inside a circle / rect on a solid
/// top). Area-ascending so a polygon nested at depth ≥ 2 binds to its innermost
/// parent only; the hole-aware `polygon_inside` (via `split_face_by_inner_
/// polygon`) additionally excludes a container whose HOLE already contains the
/// inner (e.g. a rect sitting in a ring's circle hole belongs to the disk, not
/// the ring). Re-running is safe: an already-integrated inner's twins bound a
/// face → the reparent rejects it. Returns the number assigned.
pub fn assign_polygon_holes(mesh: &mut Mesh, faces: &[FaceId]) -> usize {
    use std::cmp::Ordering;
    let mut sized: Vec<(FaceId, f64)> = faces
        .iter()
        .copied()
        .filter(|&f| mesh.faces.get(f).map_or(false, |x| x.is_active()))
        .map(|f| (f, face_containment_size(mesh, f)))
        .collect();
    sized.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));

    let mut processed: std::collections::HashSet<FaceId> = std::collections::HashSet::new();
    let mut count = 0usize;
    for i in 0..sized.len() {
        let inner = sized[i].0;
        if processed.contains(&inner) {
            continue;
        }
        // inner must be a POLYGON (≥3-vert loop; a circle self-loop = 1 is
        // handled by assign_circle_holes_innermost).
        let is_poly = mesh
            .faces
            .get(inner)
            .and_then(|f| mesh.collect_loop_verts(f.outer().start).ok())
            .map_or(false, |v| v.len() >= 3);
        if !is_poly {
            continue;
        }
        // First (smallest ascending) larger face that materially contains the
        // polygon = its innermost parent. split validates containment + hole-
        // awareness; on success reparent the twin loop there ONLY, then break.
        for j in (i + 1)..sized.len() {
            let outer = sized[j].0;
            if !mesh.faces.get(inner).map_or(false, |x| x.is_active()) {
                break;
            }
            if !mesh.faces.get(outer).map_or(false, |x| x.is_active()) {
                continue;
            }
            if split_face_by_inner_polygon(mesh, outer, inner).is_ok() {
                processed.insert(inner);
                count += 1;
                break; // innermost container — do NOT assign to grandparents.
            }
        }
    }
    count
}

/// ADR-185 — 두 face 가 coplanar Circle 이고 한쪽이 다른쪽을 완전 포함하면
/// `(outer, inner)` 반환. partial overlap / disjoint / non-circle → `None`.
///
/// auto-draw 파이프라인의 containment 감지용 (Scene `intersect_faces_inner`
/// 의 `Ok(None)` 분기에서 사용 — auto_intersect_coplanar 가 partial overlap
/// 만 처리하므로 containment 는 본 helper + `split_face_by_inner_circle`).
pub fn detect_circle_containment(
    mesh: &Mesh,
    fid_a: FaceId,
    fid_b: FaceId,
) -> Option<(FaceId, FaceId)> {
    let ca = extract_circle(mesh, fid_a)?;
    let cb = extract_circle(mesh, fid_b)?;
    // coplanar (normal parallel + same plane).
    let na = ca.normal.normalize_or_zero();
    let nb = cb.normal.normalize_or_zero();
    if (1.0 - na.dot(nb).abs()) > NORMAL_PARITY_TOL {
        return None;
    }
    if (cb.center - ca.center).dot(na).abs() > crate::plane::EPS_PLANE_OFFSET {
        return None;
    }
    // containment: d + r_inner <= r_outer.
    let d = (cb.center - ca.center).length();
    if d + cb.radius <= ca.radius {
        Some((fid_a, fid_b)) // a = outer, b = inner
    } else if d + ca.radius <= cb.radius {
        Some((fid_b, fid_a)) // b = outer, a = inner
    } else {
        None // partial overlap or disjoint
    }
}

/// Helper: face 가 closed-curve Circle face 인지 확인 + Circle 메타데이터 반환.
///
/// Circle face = outer loop 가 1 self-loop edge with
/// `AnalyticCurve::Circle` 형태 (ADR-089 Phase 2 canonical).
fn extract_circle(mesh: &Mesh, face_id: FaceId) -> Option<CircleData> {
    let face = mesh.faces.get(face_id)?;
    let outer_start = face.outer().start;
    if outer_start.is_null() {
        return None;
    }
    // Collect loop HEs — Circle face = exactly 1 HE (self-loop)
    let hes = mesh.collect_loop_hes(outer_start).ok()?;
    if hes.len() != 1 {
        return None;
    }
    let he = mesh.hes.get(hes[0])?;
    let curve = mesh.edge_curve(he.edge())?;  // Mesh API — Option<&AnalyticCurve>
    match curve {
        crate::curves::AnalyticCurve::Circle {
            center,
            radius,
            normal,
            ..
        } => Some(CircleData {
            center: *center,  // *&DVec3 → DVec3 (Copy)
            radius: *radius,  // *&f64 → f64 (Copy)
            normal: *normal,  // *&DVec3 → DVec3 (Copy)
        }),
        _ => None,
    }
}

/// Minimal Circle metadata extracted from a face's self-loop edge.
struct CircleData {
    center: DVec3,
    radius: f64,
    normal: DVec3,
}

/// ADR-186 A2 — 임의 closed-curve self-loop face 의 대표 내부점 + 법선.
/// Circle/Bezier/BSpline/NURBS 모두 처리 (containment 판정용).
struct ClosedCurveData {
    /// 대표 내부점 (Circle=center, freeform=polyline centroid).
    interior: DVec3,
    normal: DVec3,
}

fn polyline_centroid(pts: &[DVec3]) -> Option<DVec3> {
    if pts.is_empty() {
        return None;
    }
    let sum = pts.iter().copied().fold(DVec3::ZERO, |a, p| a + p);
    Some(sum / pts.len() as f64)
}

/// ADR-186 A2 — `extract_circle` 의 일반화. self-loop closed-curve face
/// (Circle/Bezier/BSpline/NURBS) 의 대표 내부점 + face 법선 추출.
/// freeform 은 polyline tessellate centroid (단순 loop 가정).
fn extract_closed_curve(mesh: &Mesh, face_id: FaceId) -> Option<ClosedCurveData> {
    let face = mesh.faces.get(face_id)?;
    let outer_start = face.outer().start;
    if outer_start.is_null() {
        return None;
    }
    let hes = mesh.collect_loop_hes(outer_start).ok()?;
    if hes.len() != 1 {
        return None; // self-loop only
    }
    let he = mesh.hes.get(hes[0])?;
    let curve = mesh.edge_curve(he.edge())?;
    let normal = face.normal();
    use crate::curves::AnalyticCurve as AC;
    let interior = match curve {
        AC::Circle { center, .. } => *center,
        AC::Bezier { control_pts } => {
            polyline_centroid(&crate::curves::bezier::tessellate(control_pts, 0.1).ok()?)?
        }
        AC::BSpline { control_pts, knots, degree } => polyline_centroid(
            &crate::curves::bspline::tessellate(control_pts, knots, *degree as usize, 0.1).ok()?,
        )?,
        AC::NURBS { control_pts, weights, knots, degree } => polyline_centroid(
            &crate::curves::nurbs::tessellate(
                control_pts,
                weights,
                knots,
                *degree as usize,
                0.1,
            )
            .ok()?,
        )?,
        _ => return None, // Arc/Line 은 closed-curve face 아님
    };
    Some(ClosedCurveData { interior, normal })
}

/// ADR-186 A2 — `split_face_by_inner_circle_generic` 의 곡선-일반화.
/// inner 가 임의 closed-curve (Circle/Bezier/BSpline/NURBS) self-loop 이고
/// polygon outer 안에 포함되면 reparent (inner self-loop twin HE → outer hole).
/// containment only (overlap 은 A3 — arrange curve 교차). Circle 도 동작하나
/// scene post-process 와 중복 방지 위해 caller (rebuild tail) 가 freeform 만 전달.
pub fn split_face_by_inner_closed_curve_generic(
    mesh: &mut Mesh,
    outer_face: FaceId,
    inner_face: FaceId,
) -> Result<(), AnnulusError> {
    use crate::boundary_kernel::geom2::{point_in_polygon_even_odd, Pip, Vec2};

    // 1. active
    let outer = mesh.faces.get(outer_face).ok_or(AnnulusError::InactiveFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    if !outer.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" });
    }
    let inner = mesh.faces.get(inner_face).ok_or(AnnulusError::InactiveFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;
    if !inner.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: inner_face.raw(), role: "inner" });
    }

    // 2. inner = closed curve (any) — 대표 내부점 + 법선.
    let inner_data = extract_closed_curve(mesh, inner_face).ok_or(AnnulusError::NotCircleFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;

    // 3. coplanar.
    let outer_n = mesh.faces[outer_face].normal().normalize_or_zero();
    let inner_n = inner_data.normal.normalize_or_zero();
    if (1.0 - outer_n.dot(inner_n).abs()) > NORMAL_PARITY_TOL {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: outer_n,
            inner_normal: inner_n,
            plane_distance: f64::INFINITY,
        });
    }

    // 4. inner 대표점이 outer polygon 안 (point-in-polygon, 2D).
    let outer_start = mesh.faces[outer_face].outer().start;
    let verts = mesh
        .collect_loop_verts(outer_start)
        .map_err(|_| AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" })?;
    if verts.len() < 3 {
        // polygon outer 만 (bezier-in-circle 등 곡선 outer 는 future).
        return Err(AnnulusError::InnerNotContained {
            center_distance: 0.0,
            inner_radius: 0.0,
            outer_radius: 0.0,
        });
    }
    let origin = mesh.verts.get(verts[0]).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
    let plane_dist = (inner_data.interior - origin).dot(outer_n).abs();
    if plane_dist > crate::plane::EPS_PLANE_OFFSET {
        return Err(AnnulusError::NotCoplanar {
            outer_normal: outer_n,
            inner_normal: inner_n,
            plane_distance: plane_dist,
        });
    }
    let aux = if outer_n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = outer_n.cross(aux).normalize_or_zero();
    let v = outer_n.cross(u).normalize_or_zero();
    let poly2d: Vec<Vec2> = verts
        .iter()
        .map(|&vid| {
            let p = mesh.verts.get(vid).map(|x| x.pos()).unwrap_or(DVec3::ZERO) - origin;
            Vec2::new(p.dot(u), p.dot(v))
        })
        .collect();
    let cp = inner_data.interior - origin;
    let c2d = Vec2::new(cp.dot(u), cp.dot(v));
    if point_in_polygon_even_odd(c2d, &poly2d, 1e-6) != Pip::Inside {
        return Err(AnnulusError::InnerNotContained {
            center_distance: cp.length(),
            inner_radius: 0.0,
            outer_radius: 0.0,
        });
    }

    // === reparent (shape-agnostic, split_face_by_inner_circle 와 동일) ===
    let he1 = mesh.faces[inner_face].outer().start;
    let he2 = mesh.hes[he1].next_rad();
    if he2 == he1 || !mesh.hes.contains(he2) {
        return Err(AnnulusError::NotCircleFace { face_id: inner_face.raw(), role: "inner" });
    }
    mesh.hes[he2].set_face(outer_face);
    mesh.hes[he2].set_outer(false);
    mesh.faces[outer_face].add_inner(LoopRef { start: he2, is_outer: false });
    Ok(())
}

// ════════════════════════════════════════════════════════════════════
// ADR-283 β — Containment auto-split for a POLYGON inner (rect / N-gon).
// ════════════════════════════════════════════════════════════════════

/// ADR-283 β-1 — is `inner` a POLYGON (multi-edge loop) fully contained in
/// `outer` (a circle self-loop OR another polygon)? Returns `(outer, inner)`.
///
/// This is the containment case the circle paths (`detect_circle_containment`,
/// self-loop-inner `split_face_by_inner_circle*`) do NOT cover: a rect/N-gon
/// drawn inside another coplanar shape on a solid top. `auto_intersect_coplanar`
/// treats it as a 0-crossing no-op → the inner stays a free sheet → the solid
/// opens (ADR-282). Here we detect it so `split_face_by_inner_polygon` can
/// integrate the inner as a hole.
///
/// A face is a "polygon" if its outer loop has ≥ 3 verts (a Circle self-loop has
/// 1 → not a polygon inner, handled by the circle paths). Containment = EVERY
/// inner vert lies inside `outer` (point-in-circle for a circle outer,
/// point-in-polygon otherwise). Requiring ALL verts (not just a centroid)
/// naturally rejects partial overlap — that stays with `auto_intersect_coplanar`.
pub fn detect_polygon_containment(
    mesh: &Mesh,
    fid_a: FaceId,
    fid_b: FaceId,
) -> Option<(FaceId, FaceId)> {
    // Try both orderings; the polygon inner must be fully inside the other.
    if polygon_inside(mesh, fid_b, fid_a) {
        Some((fid_a, fid_b)) // a = outer, b = inner
    } else if polygon_inside(mesh, fid_a, fid_b) {
        Some((fid_b, fid_a)) // b = outer, a = inner
    } else {
        None
    }
}

/// Helper: is `inner` a polygon (≥3-vert loop) whose every vertex is inside
/// the MATERIAL of coplanar face `outer` — i.e. inside `outer`'s outer loop AND
/// outside every one of `outer`'s HOLE loops? The hole exclusion is essential
/// on a solid top: the ring face (box-square outer + circle hole) "contains" a
/// rect by its outer square, but if the rect sits in the circle hole it really
/// belongs to the disk filling that hole, not the ring — without this the scan
/// could reparent the rect into the wrong (ring) face and leave the solid open.
fn polygon_inside(mesh: &Mesh, inner: FaceId, outer: FaceId) -> bool {
    let (Some(fi), Some(fo)) = (mesh.faces.get(inner), mesh.faces.get(outer)) else {
        return false;
    };
    if !fi.is_active() || !fo.is_active() {
        return false;
    }
    // inner must be a polygon (≥3-vert outer loop; a circle self-loop = 1).
    let Ok(inner_verts) = mesh.collect_loop_verts(fi.outer().start) else {
        return false;
    };
    if inner_verts.len() < 3 {
        return false;
    }
    // coplanar (normals parallel + inner on outer's plane).
    let outer_n = fo.normal().normalize_or_zero();
    let inner_n = fi.normal().normalize_or_zero();
    if (1.0 - outer_n.dot(inner_n).abs()) > NORMAL_PARITY_TOL {
        return false;
    }
    let inner_pts: Vec<DVec3> = inner_verts
        .iter()
        .filter_map(|&v| mesh.verts.get(v).map(|x| x.pos()))
        .collect();
    if inner_pts.len() != inner_verts.len() {
        return false;
    }
    // 2D basis + origin from the outer normal.
    let origin = extract_circle(mesh, outer)
        .map(|c| c.center)
        .or_else(|| {
            mesh.collect_loop_verts(fo.outer().start)
                .ok()
                .and_then(|vs| vs.first().and_then(|&v| mesh.verts.get(v)).map(|x| x.pos()))
        })
        .unwrap_or(DVec3::ZERO);
    // plane offset gate — inner must be ON outer's plane.
    for &p in &inner_pts {
        if (p - origin).dot(outer_n).abs() > crate::plane::EPS_PLANE_OFFSET {
            return false;
        }
    }
    let aux = if outer_n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = outer_n.cross(aux).normalize_or_zero();
    let v = outer_n.cross(u).normalize_or_zero();

    // inside the outer's OUTER loop …
    if !loop_contains_all(mesh, outer, fo.outer().start, &inner_pts, u, v) {
        return false;
    }
    // … and OUTSIDE every hole (else the inner belongs to the hole-filling face).
    for hole in fo.inners() {
        if hole.start.is_null() {
            continue;
        }
        if loop_contains_all(mesh, outer, hole.start, &inner_pts, u, v) {
            return false;
        }
    }
    true
}

/// Helper: are ALL `pts` inside the loop starting at `loop_start` on `owner`'s
/// plane? A self-loop Circle edge → analytic center/radius (point_in_face can't
/// test a 1-vert loop); a polygon loop (≥3 verts) → even-odd point-in-polygon.
fn loop_contains_all(
    mesh: &Mesh,
    _owner: FaceId,
    loop_start: crate::HeId,
    pts: &[DVec3],
    u: DVec3,
    v: DVec3,
) -> bool {
    use crate::boundary_kernel::geom2::{point_in_polygon_even_odd, Pip, Vec2};
    let Ok(hes) = mesh.collect_loop_hes(loop_start) else {
        return false;
    };
    // Circle self-loop (1 HE with an AnalyticCurve::Circle) → analytic disk test.
    if hes.len() == 1 {
        if let Some(curve) = mesh.edge_curve(mesh.hes[hes[0]].edge()) {
            if let crate::curves::AnalyticCurve::Circle { center, radius, .. } = curve {
                let (center, radius) = (*center, *radius);
                return pts.iter().all(|&p| {
                    let d = p - center;
                    let (du, dv) = (d.dot(u), d.dot(v));
                    (du * du + dv * dv).sqrt() <= radius + crate::plane::EPS_PLANE_OFFSET
                });
            }
        }
        return false; // 1-vert non-circle loop → can't test
    }
    // Polygon loop → point-in-polygon (even-odd).
    let Ok(verts) = mesh.collect_loop_verts(loop_start) else {
        return false;
    };
    if verts.len() < 3 {
        return false;
    }
    let origin = mesh.verts.get(verts[0]).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
    let poly2d: Vec<Vec2> = verts
        .iter()
        .filter_map(|&vid| mesh.verts.get(vid).map(|x| x.pos()))
        .map(|p| {
            let d = p - origin;
            Vec2::new(d.dot(u), d.dot(v))
        })
        .collect();
    pts.iter().all(|&p| {
        let d = p - origin;
        let p2 = Vec2::new(d.dot(u), d.dot(v));
        point_in_polygon_even_odd(p2, &poly2d, 1e-6) != Pip::Outside
    })
}

/// ADR-283 β-1 — split `outer` by a contained POLYGON `inner`: reparent the
/// inner's N-HE twin loop into `outer` as a HOLE. Generalizes the circle
/// `split_face_by_inner_circle*` (single self-loop twin, annulus.rs:420-428) to
/// a multi-edge inner loop. Both faces stay active; the inner's edges become
/// 2-face (inner + outer hole) → manifold; the inner fills its own hole → the
/// solid stays closed. No geometry is created and no analytic boundary is
/// polygonized (the outer's Circle self-loop, shared with a ring on a solid top,
/// is untouched — this is why the reparent avoids the ADR-282 open).
///
/// De-risk proven: `adr283_sim_rect_in_circle_reparent_manifold`.
pub fn split_face_by_inner_polygon(
    mesh: &mut Mesh,
    outer_face: FaceId,
    inner_face: FaceId,
) -> Result<(), AnnulusError> {
    // 1. active
    let outer = mesh.faces.get(outer_face).ok_or(AnnulusError::InactiveFace {
        face_id: outer_face.raw(),
        role: "outer",
    })?;
    if !outer.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: outer_face.raw(), role: "outer" });
    }
    let inner = mesh.faces.get(inner_face).ok_or(AnnulusError::InactiveFace {
        face_id: inner_face.raw(),
        role: "inner",
    })?;
    if !inner.is_active() {
        return Err(AnnulusError::InactiveFace { face_id: inner_face.raw(), role: "inner" });
    }

    // 2. inner must be a polygon fully contained in outer (all verts inside).
    if !polygon_inside(mesh, inner_face, outer_face) {
        return Err(AnnulusError::InnerNotContained {
            center_distance: 0.0,
            inner_radius: 0.0,
            outer_radius: 0.0,
        });
    }

    // 3. reparent the inner's N-HE twin loop → outer's hole. Generalizes the
    //    single-twin circle reparent to a multi-edge loop, and — critically —
    //    LINKS the twins' next/prev into a closed loop (inner face CCW ⇒ the
    //    host hole is CW). Byte-for-byte the canonical pattern used by the
    //    cylinder/cone porthole split (mesh.rs:4276-4293). Merely setting
    //    face/outer without wiring next/prev leaves a 1-vert (broken) inner loop.
    let inner_start = mesh.faces[inner_face].outer().start;
    let inner_hes = mesh
        .collect_loop_hes(inner_start)
        .map_err(|_| AnnulusError::InactiveFace { face_id: inner_face.raw(), role: "inner" })?;
    if inner_hes.len() < 3 {
        return Err(AnnulusError::NotCircleFace { face_id: inner_face.raw(), role: "inner" });
    }
    let twins: Vec<crate::HeId> = inner_hes.iter().map(|&h| mesh.hes[h].next_rad()).collect();
    let m = twins.len();
    // Validate every twin exists + is free (not already bounding another face)
    // BEFORE mutating, so a bad case leaves the mesh untouched.
    for (i, &twin) in twins.iter().enumerate() {
        if twin == inner_hes[i] || !mesh.hes.contains(twin) {
            return Err(AnnulusError::NotCircleFace { face_id: inner_face.raw(), role: "inner" });
        }
        if !mesh.hes[twin].face().is_null() {
            // twin already bounds a face → reparenting would make it non-manifold.
            return Err(AnnulusError::InnerNotContained {
                center_distance: 0.0,
                inner_radius: 0.0,
                outer_radius: 0.0,
            });
        }
    }
    for k in 0..m {
        let cur = twins[(m - k) % m];
        let nxt = twins[(m - k - 1 + m) % m];
        mesh.hes[cur].set_face(outer_face);
        mesh.hes[cur].set_outer(false);
        mesh.hes[cur].set_next(nxt);
        mesh.hes[nxt].set_prev(cur);
    }
    mesh.faces[outer_face].add_inner(LoopRef { start: twins[0], is_outer: false });
    Ok(())
}

// ════════════════════════════════════════════════════════════════════
// Tests (ADR-145 β-1 — 5 회귀 자산)
// ════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{curves::AnalyticCurve, MaterialId};
    use glam::DVec3;

    /// Helper: build a Circle face at (center, radius) on Z=0 plane.
    fn build_circle_face(
        mesh: &mut Mesh,
        center: DVec3,
        radius: f64,
        normal: DVec3,
    ) -> FaceId {
        // ADR-089 Phase 2: 1 anchor + 1 self-loop edge with AnalyticCurve::Circle
        let anchor_pos = center + DVec3::new(radius, 0.0, 0.0);  // anchor at θ=0
        let anchor = mesh.add_vertex(anchor_pos);
        let curve = AnalyticCurve::Circle {
            center,
            radius,
            normal,
            basis_u: DVec3::X,
        };
        mesh.add_face_closed_curve(anchor, curve, MaterialId::new(0))
            .expect("Circle face creation must succeed")
    }

    #[test]
    fn adr145_beta1plus_promote_concentric_circles_succeeds() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);

        // Pre-promote state
        assert_eq!(mesh.faces[outer].inners().len(), 0,
            "Pre-promote: outer has no inner loops");
        assert!(mesh.faces[inner].is_active(), "Pre-promote: inner is active");

        // Promote
        let result = promote_circles_to_annulus(&mut mesh, outer, inner);
        assert!(result.is_ok(), "expected Ok; got {:?}", result);

        // Post-promote: outer has 1 inner loop (hole), inner face deactivated
        assert_eq!(mesh.faces[outer].inners().len(), 1,
            "Post-promote: outer has 1 inner loop (annulus hole)");
        assert!(!mesh.faces[inner].is_active(),
            "Post-promote: inner face is deactivated");
    }

    #[test]
    fn adr185_split_keeps_inner_disk_ring_plus_disk() {
        // ADR-185 — 원 안에 원 → ring + disk (면분할). annulus 와 달리 inner
        // disk 보존.
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);

        let result = split_face_by_inner_circle(&mut mesh, outer, inner);
        assert!(result.is_ok(), "expected Ok; got {:?}", result);

        // outer = ring (1 inner loop hole), inner = disk STILL ACTIVE.
        assert_eq!(mesh.faces[outer].inners().len(), 1, "outer has 1 hole (ring)");
        assert!(mesh.faces[outer].is_active(), "outer ring active");
        assert!(
            mesh.faces[inner].is_active(),
            "ADR-185: inner DISK kept active (vs annulus deactivates)"
        );
        // manifold preserved (edge has 2 face-bearing HEs: disk + ring hole).
        let report = mesh.verify_face_invariants();
        assert_eq!(
            report.violations.len(),
            0,
            "ADR-185: ring+disk manifold-safe; violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn adr279_assign_innermost_three_level_nesting_manifold() {
        // ADR-279 β — 3 concentric circles (R30 ⊃ R20 ⊃ R10). Each inner circle
        // must be assigned as a hole to its INNERMOST parent ONLY (R10→R20,
        // R20→R30), never a grandparent. No face gains a duplicate hole → manifold.
        let mut mesh = Mesh::new();
        let r30 = build_circle_face(&mut mesh, DVec3::ZERO, 30.0, DVec3::Z);
        let r20 = build_circle_face(&mut mesh, DVec3::ZERO, 20.0, DVec3::Z);
        let r10 = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);

        let assigned = assign_circle_holes_innermost(&mut mesh, &[r30, r20, r10]);
        assert_eq!(assigned, 2, "two inner circles assigned (R10→R20, R20→R30)");

        // Each face has AT MOST one inner loop — no grandparent double-assignment.
        assert_eq!(mesh.faces[r30].inners().len(), 1, "R30 ring: 1 hole (R20)");
        assert_eq!(mesh.faces[r20].inners().len(), 1, "R20 ring: 1 hole (R10)");
        assert_eq!(mesh.faces[r10].inners().len(), 0, "R10 innermost disk: 0 holes");
        assert!(
            mesh.faces[r30].is_active() && mesh.faces[r20].is_active() && mesh.faces[r10].is_active(),
            "all three faces stay active (ring/ring/disk)"
        );

        // Manifold: every shared rim edge has exactly 2 face-bearing HEs.
        let report = mesh.verify_face_invariants();
        assert_eq!(
            report.violations.len(),
            0,
            "ADR-279: 3-level nested curve annulus manifold-safe; got {:?}",
            report.violations
        );
        let active: Vec<FaceId> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(
            mesh.face_set_manifold_info(&active).non_manifold_edge_count,
            0,
            "ADR-279: no non-manifold edge (authoritative ManifoldInfo)"
        );
    }

    #[test]
    fn adr279_assign_innermost_idempotent_skips_already_hole() {
        // ADR-279 β — running the assignment TWICE (mirrors a scoped re-derive that
        // preserves an existing ring+disk) must NOT re-assign an already-hole circle
        // → no duplicate hole, still manifold.
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 20.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        assert_eq!(assign_circle_holes_innermost(&mut mesh, &[outer, inner]), 1);
        // second pass — inner is already outer's hole → nothing re-assigned.
        assert_eq!(
            assign_circle_holes_innermost(&mut mesh, &[outer, inner]),
            0,
            "already-hole circle is skipped (no duplicate assignment)"
        );
        assert_eq!(mesh.faces[outer].inners().len(), 1, "outer keeps exactly 1 hole");
        let active: Vec<FaceId> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(mesh.face_set_manifold_info(&active).non_manifold_edge_count, 0);
    }

    /// Helper: build a RECT (polygon, 4-vert loop) face on the Z=0 plane,
    /// centered at `c`, half-extent `h`. A standalone free sheet (its edge twins
    /// have a null face) — models a rect drawn INSIDE a circle disk.
    fn build_rect_face(mesh: &mut Mesh, c: DVec3, h: f64) -> FaceId {
        let vids: Vec<crate::VertId> = [
            c + DVec3::new(-h, -h, 0.0),
            c + DVec3::new(h, -h, 0.0),
            c + DVec3::new(h, h, 0.0),
            c + DVec3::new(-h, h, 0.0),
        ]
        .iter()
        .map(|&p| mesh.add_vertex(p))
        .collect();
        mesh.add_face(&vids, MaterialId::new(0)).expect("rect face")
    }

    /// ADR-283 α (de-risk sim) — CHARACTERIZE the gap: a POLYGON (rect) inner
    /// fully inside a CIRCLE (self-loop) outer is NOT handled by any existing
    /// containment function → it would stay an un-integrated free sheet (the
    /// "옆면 사라짐" contained case). `detect_circle_containment` needs BOTH
    /// circles (rect inner → None); `split_face_by_inner_circle_generic` needs a
    /// Circle inner (rect → NotCircleFace). This is why ADR-283 is needed.
    #[test]
    fn adr283_sim_rect_in_circle_gap_uncovered() {
        let mut mesh = Mesh::new();
        let circle = build_circle_face(&mut mesh, DVec3::ZERO, 40.0, DVec3::Z);
        let rect = build_rect_face(&mut mesh, DVec3::ZERO, 10.0);

        // (a) circle-containment detection FAILS (rect is not a circle).
        assert!(
            detect_circle_containment(&mesh, circle, rect).is_none(),
            "gap: rect inner is not a Circle → detect_circle_containment None"
        );
        // (b) the circle-inner split FAILS (inner must be a Circle self-loop).
        let r = split_face_by_inner_circle_generic(&mut mesh, circle, rect);
        assert!(
            matches!(r, Err(AnnulusError::NotCircleFace { role: "inner", .. })),
            "gap: rect inner → NotCircleFace, got {r:?}"
        );
        // (c) the rect is still a standalone sheet: circle gained no hole, so the
        // rect's 4 edges remain free boundary (not integrated).
        assert_eq!(mesh.faces[circle].inners().len(), 0, "circle has no hole yet");
    }

    /// ADR-283 α (de-risk sim) — VALIDATE the fix direction: reparenting the rect
    /// inner's TWIN loop (N half-edges, generalizing the circle's single self-loop
    /// twin, annulus.rs:420-428) into the circle outer as a HOLE integrates the
    /// rect (its edges become 2-face: rect + circle-hole) → manifold, circle gains
    /// a rect-shaped hole, both faces stay active. This is the β implementation
    /// (`split_face_by_inner_polygon`) prototyped inline.
    #[test]
    fn adr283_sim_rect_in_circle_reparent_manifold() {
        let mut mesh = Mesh::new();
        let circle = build_circle_face(&mut mesh, DVec3::ZERO, 40.0, DVec3::Z);
        let rect = build_rect_face(&mut mesh, DVec3::ZERO, 10.0);

        // === prototype reparent (β = split_face_by_inner_polygon) ===
        // Generalize the self-loop twin reparent to a multi-HE loop: for each
        // boundary HE of the rect, take its radial twin (the free "outside"),
        // move it onto the circle, and register the twin loop as the circle's hole.
        let inner_start = mesh.faces[rect].outer().start;
        let inner_hes = mesh.collect_loop_hes(inner_start).expect("rect loop");
        assert_eq!(inner_hes.len(), 4, "rect outer loop = 4 HEs");
        let twins: Vec<crate::HeId> = inner_hes.iter().map(|&h| mesh.hes[h].next_rad()).collect();
        let m = twins.len();
        for k in 0..m {
            let cur = twins[(m - k) % m];
            let nxt = twins[(m - k - 1 + m) % m];
            mesh.hes[cur].set_face(circle);
            mesh.hes[cur].set_outer(false);
            mesh.hes[cur].set_next(nxt);
            mesh.hes[nxt].set_prev(cur);
        }
        mesh.faces[circle].add_inner(LoopRef { start: twins[0], is_outer: false });

        // === verify: manifold, circle has a rect hole, both faces active ===
        assert_eq!(mesh.faces[circle].inners().len(), 1, "circle gained a rect hole");
        assert!(mesh.faces[circle].is_active() && mesh.faces[rect].is_active(),
            "both faces stay active (outer-with-hole + inner)");
        let report = mesh.verify_face_invariants();
        assert_eq!(report.violations.len(), 0,
            "ADR-283: rect-in-circle reparent manifold-safe; violations {:?}", report.violations);
        let active: Vec<FaceId> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(mesh.face_set_manifold_info(&active).non_manifold_edge_count, 0,
            "ADR-283: no non-manifold edge after reparent (authoritative ManifoldInfo)");
    }

    /// ADR-283 β — the real `split_face_by_inner_polygon` integrates a rect
    /// inner into a circle outer (rect-in-circle, the failing ADR-282 case) →
    /// manifold, circle gains a rect hole, both faces active.
    #[test]
    fn adr283_split_rect_in_circle_manifold() {
        let mut mesh = Mesh::new();
        let circle = build_circle_face(&mut mesh, DVec3::ZERO, 40.0, DVec3::Z);
        let rect = build_rect_face(&mut mesh, DVec3::ZERO, 10.0);

        assert_eq!(
            detect_polygon_containment(&mesh, circle, rect),
            Some((circle, rect)),
            "detect: rect is a polygon inside the circle → (circle=outer, rect=inner)"
        );
        split_face_by_inner_polygon(&mut mesh, circle, rect).expect("rect-in-circle split");

        assert_eq!(mesh.faces[circle].inners().len(), 1, "circle gained a rect hole");
        assert!(mesh.faces[circle].is_active() && mesh.faces[rect].is_active(), "both active");
        assert_eq!(mesh.verify_face_invariants().violations.len(), 0, "manifold-safe");
        let active: Vec<FaceId> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(mesh.face_set_manifold_info(&active).non_manifold_edge_count, 0, "nm=0");
    }

    /// ADR-283 β — rect inside a RECT outer (polygon-in-polygon containment).
    #[test]
    fn adr283_split_rect_in_rect_manifold() {
        let mut mesh = Mesh::new();
        let big = build_rect_face(&mut mesh, DVec3::ZERO, 40.0);
        let small = build_rect_face(&mut mesh, DVec3::ZERO, 10.0);

        assert_eq!(
            detect_polygon_containment(&mesh, big, small),
            Some((big, small)),
            "detect: small rect inside big rect → (big=outer, small=inner)"
        );
        split_face_by_inner_polygon(&mut mesh, big, small).expect("rect-in-rect split");
        assert_eq!(mesh.faces[big].inners().len(), 1, "big rect gained a small-rect hole");
        assert!(mesh.faces[big].is_active() && mesh.faces[small].is_active());
        assert_eq!(mesh.verify_face_invariants().violations.len(), 0, "manifold-safe");
        let active: Vec<FaceId> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(mesh.face_set_manifold_info(&active).non_manifold_edge_count, 0, "nm=0");
    }

    /// ADR-283 β — a rect that only PARTIALLY overlaps the circle (not fully
    /// contained) is NOT a containment → detect None + split rejects, so it falls
    /// through to `auto_intersect_coplanar` (partial-overlap 3-split, ADR-101).
    #[test]
    fn adr283_split_rejects_partial_overlap() {
        let mut mesh = Mesh::new();
        let circle = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        // rect centered at (8,0) half-extent 6 → spans x∈[2,14]: pokes outside r=10.
        let rect = build_rect_face(&mut mesh, DVec3::new(8.0, 0.0, 0.0), 6.0);
        assert_eq!(detect_polygon_containment(&mesh, circle, rect), None,
            "partial overlap is not containment");
        assert!(matches!(
            split_face_by_inner_polygon(&mut mesh, circle, rect),
            Err(AnnulusError::InnerNotContained { .. })
        ), "partial overlap → InnerNotContained (falls through to auto_intersect)");
    }

    #[test]
    fn adr185_split_rejects_not_contained() {
        // inner 가 outer 밖이면 reject (silent skip 용).
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::new(20.0, 0.0, 0.0), 5.0, DVec3::Z);
        let result = split_face_by_inner_circle(&mut mesh, outer, inner);
        assert!(matches!(result, Err(AnnulusError::InnerNotContained { .. })),
            "expected InnerNotContained; got {:?}", result);
    }

    /// **시뮬레이션 (원-in-사각 smooth hole)** — 다각형 rect outer + inner Circle
    /// → split_face_by_inner_circle_generic → rect 가 **매끈 곡선 원 hole** 보유.
    /// circle-in-rect 한계 (polygon hole) 해소 가설 검증.
    #[test]
    fn sim_split_polygon_rect_by_inner_circle_smooth() {
        let mut mesh = Mesh::new();
        // rect (polygon face) 0,0 - 400,400, CCW
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(400.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(400.0, 400.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 400.0, 0.0));
        let rect = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).expect("rect");
        // inner circle (Path B smooth) center (200,200) r80
        let disk = build_circle_face(&mut mesh, DVec3::new(200.0, 200.0, 0.0), 80.0, DVec3::Z);

        let res = split_face_by_inner_circle_generic(&mut mesh, rect, disk);
        assert!(res.is_ok(), "generic split should succeed: {:?}", res);

        // rect 가 1 inner loop (hole), 둘 다 active (ring + disk)
        assert_eq!(mesh.faces[rect].inners().len(), 1, "rect has 1 smooth hole");
        assert!(mesh.faces[rect].is_active() && mesh.faces[disk].is_active(), "both active");

        // hole edge = 매끈 곡선 (curve), polygon line 아님
        let (mut hole_line, mut hole_curve) = (0usize, 0usize);
        for (_, f) in mesh.faces.iter() {
            if !f.is_active() {
                continue;
            }
            for inner in f.inners() {
                if let Ok(hes) = mesh.collect_loop_hes(inner.start) {
                    for he in hes {
                        match mesh.edge_curve(mesh.hes[he].edge()) {
                            Some(_) => hole_curve += 1,
                            None => hole_line += 1,
                        }
                    }
                }
            }
        }
        println!("  sim 원-in-사각: hole edges {} Line / {} curve", hole_line, hole_curve);
        assert_eq!(hole_curve, 1, "smooth circle hole = 1 curve edge");
        assert_eq!(hole_line, 0, "no polygon hole edges");

        // manifold (원 edge 2 face-bearing HE: disk + rect hole)
        let report = mesh.verify_face_invariants();
        assert_eq!(report.violations.len(), 0, "manifold: {:?}", report.violations);
    }

    /// 시뮬레이션 — 원이 rect 밖 → containment 거부 (InnerNotContained).
    #[test]
    fn sim_split_polygon_rect_rejects_circle_outside() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(100.0, 100.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 100.0, 0.0));
        let rect = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).expect("rect");
        let disk = build_circle_face(&mut mesh, DVec3::new(500.0, 500.0, 0.0), 50.0, DVec3::Z);
        let res = split_face_by_inner_circle_generic(&mut mesh, rect, disk);
        assert!(
            matches!(res, Err(AnnulusError::InnerNotContained { .. })),
            "expected InnerNotContained; got {:?}",
            res
        );
    }

    /// **render 회귀** — rect annulus (smooth 곡선 hole) → export_buffers 가
    /// **곡선 hole 을 tessellate** (full rect 2 tris 아님). circle-in-rect
    /// "면분할 안 보임" (render 가 self-loop 곡선 hole skip) 회귀 차단.
    #[test]
    fn adr186_render_annulus_smooth_hole_tessellates() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(400.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(400.0, 400.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 400.0, 0.0));
        let rect = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).expect("rect");
        let disk = build_circle_face(&mut mesh, DVec3::new(200.0, 200.0, 0.0), 80.0, DVec3::Z);
        split_face_by_inner_circle_generic(&mut mesh, rect, disk).expect("split");
        let (_pos, _nrm, _idx, face_map, _pf64) = mesh.export_buffers().expect("export");
        let rect_tris = face_map.iter().filter(|&&f| f == rect.raw()).count();
        // full rect (hole skip) = 2 tris. annulus with tessellated 곡선 hole = 다수.
        assert!(
            rect_tris > 2,
            "annulus rect 가 곡선 hole 포함 tessellate (full-rect 2 tris 아님), got {}",
            rect_tris
        );
    }

    /// **multi-hole 진단** — 한 rect 안에 disjoint 원 2개 → generic split 두 번 호출 시
    /// rect 가 **2 hole** 보유 (사용자 "면이 분할 안된것이 있음" — 2 circles in rect).
    /// API 가 multi-hole 지원하면 버그는 scene post-process processed set.
    #[test]
    fn split_polygon_rect_two_disjoint_circles_multihole() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(400.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(400.0, 400.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 400.0, 0.0));
        let rect = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).expect("rect");
        let disk1 = build_circle_face(&mut mesh, DVec3::new(200.0, 120.0, 0.0), 60.0, DVec3::Z);
        let disk2 = build_circle_face(&mut mesh, DVec3::new(200.0, 280.0, 0.0), 60.0, DVec3::Z);
        split_face_by_inner_circle_generic(&mut mesh, rect, disk1).expect("hole1");
        split_face_by_inner_circle_generic(&mut mesh, rect, disk2).expect("hole2");
        assert_eq!(
            mesh.faces[rect].inners().len(),
            2,
            "rect 가 2 smooth hole 보유 (multi-hole)"
        );
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "multi-hole manifold: {:?}",
            inv.violations.iter().take(3).collect::<Vec<_>>()
        );
    }

    #[test]
    fn adr145_beta1_rejects_inactive_outer() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);

        // Deactivate outer
        mesh.faces[outer].set_active(false);

        let result = promote_circles_to_annulus(&mut mesh, outer, inner);
        assert!(matches!(result, Err(AnnulusError::InactiveFace { role: "outer", .. })),
            "expected InactiveFace(outer); got {:?}", result);
    }

    #[test]
    fn adr145_beta1_rejects_not_coplanar() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        // Inner on Y-up plane (different normal)
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Y);

        let result = promote_circles_to_annulus(&mut mesh, outer, inner);
        assert!(matches!(result, Err(AnnulusError::NotCoplanar { .. })),
            "expected NotCoplanar; got {:?}", result);
    }

    #[test]
    fn adr145_beta1_rejects_inner_not_contained_off_center() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        // Inner at (8, 0, 0) with radius 5 — center_distance 8 + radius 5 = 13 > outer.radius 10
        let inner = build_circle_face(&mut mesh, DVec3::new(8.0, 0.0, 0.0), 5.0, DVec3::Z);

        let result = promote_circles_to_annulus(&mut mesh, outer, inner);
        assert!(matches!(result, Err(AnnulusError::InnerNotContained { .. })),
            "expected InnerNotContained; got {:?}", result);
    }

    #[test]
    fn adr145_beta1_rejects_inner_larger_than_outer() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);
        // Inner radius 10 > outer radius 5
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);

        let result = promote_circles_to_annulus(&mut mesh, outer, inner);
        assert!(matches!(result, Err(AnnulusError::InnerNotContained { .. })),
            "expected InnerNotContained (inner > outer); got {:?}", result);
    }

    /// ADR-145 β-1+ — annulus 가 manifold safe (verify_face_invariants 통과).
    /// promote 후 outer face 의 hole topology 가 LOCKED #1 P7 정합 검증.
    #[test]
    fn adr145_beta1plus_annulus_preserves_manifold_invariants() {
        let mut mesh = Mesh::new();
        let outer = build_circle_face(&mut mesh, DVec3::ZERO, 10.0, DVec3::Z);
        let inner = build_circle_face(&mut mesh, DVec3::ZERO, 5.0, DVec3::Z);

        promote_circles_to_annulus(&mut mesh, outer, inner).expect("promote OK");

        // ADR-145 L-145-8 — hole inheritance manifold-safe
        let report = mesh.verify_face_invariants();
        assert!(report.violations.is_empty(),
            "ADR-145 β-1+: annulus topology must preserve manifold invariants; \
             got {:?}", report.violations);
    }
}
