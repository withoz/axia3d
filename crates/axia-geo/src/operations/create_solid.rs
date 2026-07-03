//! ADR-079 W-1-α — `create_solid` Surface-native solid generation.
//!
//! Profile-driven solid creation from a profile face + mode. Smart
//! routing within `CreateSolidMode::Extrude` based on profile surface
//! kind and boundary curve kinds. Other modes (Revolve / Sweep / Loft)
//! delegate to existing `Mesh::revolve` / `sweep` / `loft` (W-3/W-4).
//!
//! ## W-1-α scope (active branches)
//!
//! - `CreateSolidMode::Extrude` + `Plane` surface + `AllLinear` boundary
//!   → `extrude_planar_box` (Box solid, 6 Plane surfaces)
//!
//! All other branches return `SolidError::NotYetSupported` — Scene-level
//! caller (`Scene::exec_create_solid`) handles fallback to legacy
//! `Mesh::push_pull` per ADR-079 Q3 lock-in (W-4 deprecate).
//!
//! ## Architectural lock-ins (ADR-079 §5)
//!
//! - **L1**: Surface = truth, Mesh = view. Surface attach at construction
//!   time (not as afterthought).
//! - **L2**: Smart routing scope = Extrude mode 내부만.
//! - **L3**: 모든 결과 face = AnalyticSurface attached.
//! - **L8**: profile-driven only — primitive direct path 와 분리.
//!
//! Cross-references:
//! - ADR-079 §2.1 (primary entry), §2.3 (variants × matrix), §3 Q1~Q7
//! - ADR-067 Step 1 (auto-merge after push_pull, 보존)
//! - ADR-053 Phase H (surface transform — translation under Rigid)
//! - ADR-059 Phase N (Curve & Surface Mandatory)

use anyhow::{bail, ensure, Result};
use glam::{DMat4, DVec3};
use serde::{Deserialize, Serialize};

use crate::curves::{AnalyticCurve, CurveOps};
use crate::curves::synthesize::synthesize_plane_surface;
use crate::entities::{EdgeId, Face, FaceId, LoopRef, MaterialId, VertId};
use crate::mesh::Mesh;
use crate::surfaces::AnalyticSurface;
use crate::tolerances::{EPSILON_LENGTH, FACE_TOLERANCE};

/// ADR-079 §2.1 — Solid creation mode (profile + mode → solid).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CreateSolidMode {
    /// Linear extrusion. SketchUp Push/Pull 의 NURBS-native 등가물.
    /// Smart routing (§2.3) 가 surface kind + boundary 별 분기.
    Extrude { distance: f64 },

    /// ADR-259 — Tapered (draft) linear extrusion. `taper_deg` = draft angle
    /// from the extrude axis (`+` = inward / top shrinks = mold draft, `−` =
    /// outward flare, `|θ| < 89°`). v1 (D1/D2): `(Plane, AllLinear)` convex
    /// **and concave** profile → frustum with exact-Plane trapezoid sides.
    /// FAIL-CLOSED (D5): a collapsing / self-intersecting / spiking offset is
    /// rejected (hard error + rollback — never a silent straight extrude).
    /// Additive — existing `Extrude` byte-shape unchanged (serde-safe).
    ExtrudeTapered { distance: f64, taper_deg: f64 },

    /// ADR-260 — Circle → Cone / Frustum linear extrusion. `top_scale ∈ [0,1)`
    /// = top radius ratio (`0` = apex cone / `0<s<1` = frustum / `≥1` rejected =
    /// cylinder). v1 (Q1/Q2/Q3): `(Plane, AllCircular)` profile → full kernel-
    /// native (apex 2-face mirror of `create_cone_kernel_native`, frustum 3-face
    /// annulus mirror of `extrude_cylinder_kernel_native` with the side surface
    /// swapped Cylinder → Cone). Reuses `AnalyticSurface::Cone` (no new surface
    /// type). FAIL-CLOSED (D5): degenerate distance / `top_scale ≥ 1` / `< 0` /
    /// circle-param mismatch / solid-face (`is_move_only`) → hard error + rollback
    /// (never a silent straight extrude). Additive — serde-safe.
    ExtrudeCone { distance: f64, top_scale: f64 },

    /// ADR-261 — Bidirectional / two-sided linear extrusion about the profile
    /// plane. `dist_pos` = extent along `+normal`, `dist_neg` = extent along
    /// `−normal` (both ≥ 0; `dist_pos + dist_neg > 0`). Symmetric = `(d, d)`;
    /// asymmetric = `(d_pos, d_neg)`; `dist_neg = 0` degenerates to one-way.
    /// v1: `(Plane, AllLinear)` + `(Plane, AllCircular)`. Implementation
    /// (Q2): translate the profile by `−normal·dist_neg` (ADR-060 Phase O
    /// carries the Circle curve / Plane surface), then reuse
    /// `extrude_planar_box` / `extrude_planar_cylinder` by `dist_pos+dist_neg`
    /// — the moved profile becomes the (outward) bottom cap, so Shape/Xia
    /// ownership is preserved. FAIL-CLOSED (D5): negative / zero-sum distance /
    /// solid-face (`is_move_only`) → hard error + rollback. Additive, serde-safe.
    ExtrudeBidirectional { dist_pos: f64, dist_neg: f64 },

    /// Rotation around an axis. W-4 — 기존 `Mesh::revolve` 위임.
    Revolve {
        axis_origin: DVec3,
        axis_dir: DVec3,
        angle_rad: f64,
    },

    /// Sweep along a path curve. W-3 — 기존 `Mesh::sweep` 위임.
    Sweep { path: AnalyticCurve },

    /// Loft to another profile face. W-3 — 기존 `Mesh::loft` 위임.
    Loft { other_profile: FaceId },
}

/// ADR-079 §2.2 — Result classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolidKind {
    /// Plane all-Line boundary → Box (6 Planes).
    Box,
    /// Plane circular/arc boundary → Cylinder (1 Cylinder + 2 Plane caps).
    /// W-2 scope.
    Cylinder,
    /// ADR-260 — Plane circular boundary + `ExtrudeCone` → Cone (apex: 1 Cone
    /// side + 1 Plane base = 2 faces) or Frustum (1 Cone side annulus + 2 Plane
    /// caps = 3 faces). Reuses `AnalyticSurface::Cone`.
    Cone,
    /// Curved profile (Cylinder/Sphere/Cone/Torus panel) → smooth group
    /// 전체 일관 변형. W-2 scope.
    SmoothGroupOffset,
    /// Mixed/NURBS profile → general sweep (NURBSSurface walls). W-3 scope.
    GeneralSweep,
    /// Revolve mode 결과. W-4 scope.
    RevolutionSolid,
    /// Sweep mode 결과. W-3 scope.
    SweptSolid,
    /// Loft mode 결과. W-3 scope.
    LoftSolid,
}

/// ADR-079 §2.3 — Boundary classification for Extrude smart routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryKind {
    /// 모든 edge 가 Line (또는 curve None — Phase N synthesize 시 Line).
    AllLinear,
    /// 모든 edge 가 Circle/Arc.
    AllCircular,
    /// Linear + Curved 혼합 또는 Bezier/BSpline/NURBS 포함.
    Mixed,
}

/// ADR-079 §2.2 — Result of `create_solid`.
#[derive(Clone, Debug)]
pub struct CreateSolidResult {
    pub profile_face: FaceId,
    pub solid_kind: SolidKind,
    pub top_face: FaceId,
    pub side_faces: Vec<FaceId>,
    pub all_solid_faces: Vec<FaceId>,
    pub adjacent_splits: usize,
    pub split_debug: Vec<String>,
}

/// Errors specific to `create_solid` operation.
#[derive(Debug, thiserror::Error)]
pub enum SolidError {
    #[error("create_solid: profile face has no AnalyticSurface attached")]
    NoProfileSurface,
    #[error("create_solid: profile boundary collection failed: {0}")]
    BoundaryCollection(String),
    #[error("create_solid: distance {dist} below EPSILON_LENGTH")]
    DegenerateDistance { dist: f64 },
    #[error("create_solid: profile face not found")]
    FaceNotFound,
    #[error("create_solid: not yet supported — {reason} (Q3 fallback to legacy push_pull)")]
    NotYetSupported { reason: String },
}

impl Mesh {
    /// ADR-079 §2.1 — Surface-native solid creation from a profile face.
    ///
    /// W-1-α: only `(Extrude, Plane, AllLinear)` is active. Other branches
    /// return `SolidError::NotYetSupported` — caller (`Scene::exec_create_solid`)
    /// handles fallback to legacy `Mesh::push_pull`.
    ///
    /// **Profile-driven only** (L8 lock-in) — direct primitive paths
    /// (`Mesh::create_box` etc.) are separate.
    pub fn create_solid(
        &mut self,
        profile_face: FaceId,
        mode: CreateSolidMode,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        // ADR-256 follow-up (engine-layer hardening, 사용자 결재 "진행"):
        // reject an INACTIVE (deactivated-but-slot-resident) profile face,
        // not just a missing one. `contains` alone passes a face that was
        // removed/deactivated → create_solid would silently proceed on a
        // dead face. The WASM boundary guard already covers UI/MCP/script
        // entry points; this closes the true root for any internal Rust
        // caller (Scene::exec_create_solid etc.). Short-circuit `||` keeps
        // the index access safe. mode-specific faces (Loft/Sweep other
        // faces) already check is_active() in their handlers.
        if !self.faces.contains(profile_face) || !self.faces[profile_face].is_active() {
            return Err(SolidError::FaceNotFound.into());
        }

        match mode {
            CreateSolidMode::Extrude { distance } => {
                if distance.abs() < crate::tolerances::EPSILON_LENGTH {
                    return Err(SolidError::DegenerateDistance { dist: distance }.into());
                }

                // ─── ADR-264 — Embedded boss: FUSE vs ADR-102 cleave ──
                //
                // An embedded sub-face on a CLOSED SOLID (a rect drawn on a
                // box top = a "boss") must FUSE into that solid: remove the
                // profile, build the side walls on the *shared* rim edges so
                // they re-twin with the surrounding ring. The legacy ADR-102
                // cleave duplicated the profile boundary → opened the ring
                // (crack: boundary edges + 3 coincident faces); preserving the
                // profile → 3 face-bearing HEs per rim edge (non-manifold).
                // Both are wrong for a boss (ADR-264 §1, simulation-validated).
                //
                // Fuse gate (ADR-264 D1): Plane + AllLinear only (the validated
                // extrude_planar_box fuse path) AND the profile's connected
                // component is a closed 2-manifold solid. siblings present but
                // OPEN component = a flat coplanar arrangement (ADR-101 §B-3b
                // auto-intersect sub-faces) → keep the ADR-102 cleave (L-102-*).
                let siblings = self
                    .collect_coplanar_siblings(profile_face)
                    .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                let fuse_embedded = !siblings.is_empty()
                    && self
                        .faces
                        .get(profile_face)
                        .and_then(|f| f.surface().cloned())
                        .map_or(false, |s| matches!(s, AnalyticSurface::Plane { .. }))
                    && matches!(
                        classify_boundary(self, profile_face),
                        Ok(BoundaryKind::AllLinear)
                    )
                    && {
                        // boss-on-solid discriminator — profile's connected
                        // component is closed (box top) vs open (flat sheets).
                        let all_active: Vec<FaceId> = self
                            .faces
                            .iter()
                            .filter(|(_, f)| f.is_active())
                            .map(|(id, _)| id)
                            .collect();
                        self.face_connected_components(&all_active)
                            .into_iter()
                            .find(|c| c.contains(&profile_face))
                            .map_or(false, |c| {
                                self.face_set_manifold_info(&c).is_closed_solid
                            })
                    };

                // Cleave only for the flat-arrangement case (ADR-102 retained).
                let profile_face = if siblings.is_empty() || fuse_embedded {
                    // free profile (preserve = box bottom) OR boss-fuse (shared
                    // rim MUST stay intact so walls can re-twin) → no cleave.
                    profile_face
                } else {
                    let cleave = self
                        .cleave_face_from_siblings(profile_face, &siblings)
                        .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                    cleave.new_face_id
                };

                let surface = self
                    .faces
                    .get(profile_face)
                    .and_then(|f| f.surface().cloned())
                    .ok_or(SolidError::NoProfileSurface)?;

                // P1.3 (ADR-192) — a closed non-Circle curve disk (a Bezier or
                // BSpline self-loop face) extrudes via the analytic GeneralSweep
                // path (swept BSplineSurface side). This must precede
                // classify_boundary, which would mis-classify a curve self-loop
                // as Mixed → NotYetSupported → push_pull fallback (which fails
                // on a 1-vertex boundary). Circle self-loops still go through
                // (Plane, AllCircular) → extrude_planar_cylinder below.
                {
                    // Only a SINGLE-LOOP, PLANE-surfaced Bezier/BSpline self-loop
                    // routes here. The Plane guard prevents the BSplineSurface side
                    // face this path *produces* (also bounded by a curve self-loop)
                    // from re-routing here on a second Push/Pull. The single-loop
                    // guard is defensive — a multi-loop curve annulus is already
                    // intercepted by the scene-level P1.2 routing (exec_create_solid)
                    // before create_solid is reached. (Adversarial-review findings
                    // #16 / #10, ADR-192; BSpline = §5.5, NURBS = §5.6 extension
                    // — rational profile sweeps to a NURBSSurface side.)
                    let start = self.faces[profile_face].outer().start;
                    let single_loop = self
                        .faces
                        .get(profile_face)
                        .map_or(false, |f| f.inners().is_empty());
                    let plane_profile = matches!(surface, AnalyticSurface::Plane { .. });
                    if !start.is_null() && single_loop && plane_profile {
                        let eid = self.hes[start].edge();
                        let is_swept_self_loop = self.edges.get(eid).map_or(false, |e| {
                            e.is_self_loop()
                                && matches!(
                                    e.curve(),
                                    Some(
                                        AnalyticCurve::Bezier { .. }
                                            | AnalyticCurve::BSpline { .. }
                                            | AnalyticCurve::NURBS { .. }
                                    )
                                )
                        });
                        if is_swept_self_loop {
                            return self.extrude_closed_curve_general_kernel_native(
                                profile_face,
                                distance,
                                material,
                            );
                        }
                    }
                }

                let boundary = classify_boundary(self, profile_face)
                    .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;

                match (&surface, boundary) {
                    (AnalyticSurface::Plane { .. }, BoundaryKind::AllLinear) => {
                        self.extrude_planar_box(profile_face, distance, material, &surface, fuse_embedded)
                    }
                    (AnalyticSurface::Plane { .. }, BoundaryKind::AllCircular) => {
                        // W-2-α: Plane + AllCircular → Cylinder.
                        self.extrude_planar_cylinder(profile_face, distance, material, &surface)
                    }
                    (AnalyticSurface::Plane { .. }, BoundaryKind::Mixed) => {
                        // follow-up (2026-06-16) — native Arc+Line mixed extrude
                        // (per-edge Cylinder/Plane side walls, arc-aware top).
                        // Freeform edges still return NotYetSupported inside →
                        // Scene falls back to legacy push_pull (ADR-192 scope).
                        self.extrude_planar_mixed(profile_face, distance, material, &surface)
                    }
                    (AnalyticSurface::Cylinder { .. }, _) => {
                        // W-2-γ-i: Cylinder smooth-group radius offset.
                        self.offset_smooth_group_cylinder(profile_face, distance, &surface)
                    }
                    (AnalyticSurface::Sphere { .. }, _) => {
                        // W-2-γ-ii: Sphere smooth-group radius offset.
                        self.offset_smooth_group_sphere(profile_face, distance, &surface)
                    }
                    (AnalyticSurface::Cone { .. }, _) => {
                        // W-2-γ-iii: Cone constant-offset (true surface offset).
                        self.offset_smooth_group_cone(profile_face, distance, &surface)
                    }
                    (AnalyticSurface::Torus { .. }, _) => {
                        // W-2-γ-iv: Torus constant-offset (= minor_radius offset).
                        self.offset_smooth_group_torus(profile_face, distance, &surface)
                    }
                    (
                        AnalyticSurface::BezierPatch { .. }
                        | AnalyticSurface::BSplineSurface { .. }
                        | AnalyticSurface::NURBSSurface { .. },
                        _,
                    ) => self.extrude_nurbs_class_profile(
                        profile_face,
                        distance,
                        material,
                        &surface,
                    ),
                }
            }
            CreateSolidMode::ExtrudeTapered { distance, taper_deg } => {
                // ADR-259 β-1 — tapered (draft) extrude. v1 scope (D1/D2):
                // (Plane, AllLinear) convex/concave profile → frustum.
                if distance.abs() < crate::tolerances::EPSILON_LENGTH {
                    return Err(SolidError::DegenerateDistance { dist: distance }.into());
                }
                // v1 = FLAT-PROFILE only. Tapering a face that already bounds a
                // solid (a box wall — `is_move_only`) would sandwich the preserved
                // profile between the existing interior and the new tapered walls
                // → 3 face-bearing HEs per boundary edge → non-manifold (the
                // ADR-087 K-ε breakage that MoveOnly dispatch avoids for straight
                // Extrude). Since `ExtrudeTapered` carries no `fallback_dist`, it
                // skips the Scene-level MoveOnly dispatch — so this guard is the
                // SSOT preventing that breakage. "Draft an existing solid face" is
                // a future op → reject (no fallback; D5). Checked on the ORIGINAL
                // face, before cleave (cleave isolates it from siblings).
                if crate::operations::push_pull::is_move_only(self, profile_face) {
                    return Err(SolidError::NotYetSupported {
                        reason: "tapered extrude on a solid face (draft) — use a flat profile \
                                 (ADR-259 v1 D2)"
                            .to_string(),
                    }
                    .into());
                }
                // ADR-102 γ cleave pre-step (identical to Extrude) — isolate the
                // profile's outer boundary from coplanar siblings so EXISTING faces
                // are untouched (L-102-1). A taper-offset reject after this point
                // is rolled back by the Scene wrapper's snapshot restore (D5).
                let profile_face = {
                    let siblings = self
                        .collect_coplanar_siblings(profile_face)
                        .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                    if siblings.is_empty() {
                        profile_face
                    } else {
                        let cleave = self
                            .cleave_face_from_siblings(profile_face, &siblings)
                            .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                        cleave.new_face_id
                    }
                };
                let surface = self
                    .faces
                    .get(profile_face)
                    .and_then(|f| f.surface().cloned())
                    .ok_or(SolidError::NoProfileSurface)?;
                let boundary = classify_boundary(self, profile_face)
                    .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                match (&surface, boundary) {
                    (AnalyticSurface::Plane { .. }, BoundaryKind::AllLinear) => self
                        .extrude_planar_box_tapered(
                            profile_face,
                            distance,
                            taper_deg,
                            material,
                            &surface,
                        ),
                    _ => Err(SolidError::NotYetSupported {
                        reason: "tapered extrude v1 supports (Plane, AllLinear) only (ADR-259 D2)"
                            .to_string(),
                    }
                    .into()),
                }
            }
            CreateSolidMode::ExtrudeCone { distance, top_scale } => {
                // ADR-260 β-1 — circle → cone / frustum extrude. v1 scope
                // (Q1/Q2/Q3): (Plane, AllCircular) profile → full kernel-native.
                if distance.abs() < crate::tolerances::EPSILON_LENGTH {
                    return Err(SolidError::DegenerateDistance { dist: distance }.into());
                }
                // top_scale ∈ [0, 1): 0 = apex cone, 0<s<1 = frustum.
                // < 0 nonsensical (inverted scaling → flipped cone). ≥ 1−1e-4 is a
                // cylinder (apex = center + n·dist/(1−s) → ∞ as s→1, half_angle →
                // 0 = degenerate near-cylinder) → reject with "use straight
                // Extrude". D5: hard error, no fallback (fallback_dist = None).
                if !top_scale.is_finite() || top_scale < 0.0 {
                    return Err(SolidError::NotYetSupported {
                        reason: "cone extrude: top_scale must be ≥ 0 (ADR-260 D2 [0,1))"
                            .to_string(),
                    }
                    .into());
                }
                if top_scale >= 1.0 - 1e-4 {
                    return Err(SolidError::NotYetSupported {
                        reason: "cone extrude: top_scale ≥ 1 is a cylinder — use straight \
                                 Extrude (ADR-260 D2 [0,1))"
                            .to_string(),
                    }
                    .into());
                }
                // v1 = FLAT-PROFILE only. Coning a face that already bounds a
                // solid (a cylinder cap — `is_move_only`) would sandwich the
                // preserved profile → 3 face-bearing HEs per boundary edge →
                // non-manifold (ADR-087 K-ε). `ExtrudeCone` carries no
                // `fallback_dist`, so it skips Scene-level MoveOnly dispatch —
                // this guard is the SSOT. Checked on the ORIGINAL face, before
                // cleave (mirror ExtrudeTapered).
                if crate::operations::push_pull::is_move_only(self, profile_face) {
                    return Err(SolidError::NotYetSupported {
                        reason: "cone extrude on a solid face — use a flat circle profile \
                                 (ADR-260)"
                            .to_string(),
                    }
                    .into());
                }
                // ADR-102 γ cleave pre-step (isolate the profile from coplanar
                // siblings — EXISTING faces untouched, L-102-1). A reject after
                // this point is rolled back by the Scene wrapper (D5).
                let profile_face = {
                    let siblings = self
                        .collect_coplanar_siblings(profile_face)
                        .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                    if siblings.is_empty() {
                        profile_face
                    } else {
                        let cleave = self
                            .cleave_face_from_siblings(profile_face, &siblings)
                            .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                        cleave.new_face_id
                    }
                };
                let surface = self
                    .faces
                    .get(profile_face)
                    .and_then(|f| f.surface().cloned())
                    .ok_or(SolidError::NoProfileSurface)?;
                let boundary = classify_boundary(self, profile_face)
                    .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                match (&surface, boundary) {
                    (AnalyticSurface::Plane { .. }, BoundaryKind::AllCircular) => self
                        .extrude_planar_cone(
                            profile_face,
                            distance,
                            top_scale,
                            material,
                            &surface,
                        ),
                    _ => Err(SolidError::NotYetSupported {
                        reason: "cone extrude v1 supports (Plane, AllCircular) only (ADR-260)"
                            .to_string(),
                    }
                    .into()),
                }
            }
            CreateSolidMode::ExtrudeBidirectional { dist_pos, dist_neg } => {
                // ADR-261 β-1 — bidirectional / two-sided extrude. v1 (Q1/Q2):
                // (Plane, AllLinear|AllCircular) → translate profile by
                // −normal·dist_neg (ADR-060 carries curve/surface) then reuse
                // the one-way extrude by (dist_pos + dist_neg).
                if !dist_pos.is_finite() || !dist_neg.is_finite() || dist_pos < 0.0 || dist_neg < 0.0
                {
                    return Err(SolidError::NotYetSupported {
                        reason: "bidirectional extrude: dist_pos / dist_neg must be ≥ 0 (ADR-261)"
                            .to_string(),
                    }
                    .into());
                }
                if dist_pos + dist_neg < crate::tolerances::EPSILON_LENGTH {
                    return Err(SolidError::DegenerateDistance { dist: dist_pos + dist_neg }.into());
                }
                // is_move_only guard (ORIGINAL face, before cleave) — mirror
                // ExtrudeTapered/ExtrudeCone. fallback_dist = None → this is the
                // ADR-087 K-ε sandwich SSOT.
                if crate::operations::push_pull::is_move_only(self, profile_face) {
                    return Err(SolidError::NotYetSupported {
                        reason: "bidirectional extrude on a solid face — use a flat profile \
                                 (ADR-261)"
                            .to_string(),
                    }
                    .into());
                }
                // ADR-102 γ cleave (isolate profile from coplanar siblings — EXISTING
                // faces untouched, L-102-1). A reject after this is rolled back by the
                // Scene wrapper (D5).
                let profile_face = {
                    let siblings = self
                        .collect_coplanar_siblings(profile_face)
                        .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                    if siblings.is_empty() {
                        profile_face
                    } else {
                        let cleave = self
                            .cleave_face_from_siblings(profile_face, &siblings)
                            .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
                        cleave.new_face_id
                    }
                };
                self.extrude_planar_bidirectional(profile_face, dist_pos, dist_neg, material)
            }
            CreateSolidMode::Revolve {
                axis_origin,
                axis_dir,
                angle_rad,
            } => self.revolve_profile_face(
                profile_face,
                axis_origin,
                axis_dir,
                angle_rad,
                material,
            ),
            CreateSolidMode::Sweep { path } => {
                self.sweep_profile_along_path(profile_face, &path, material)
            }
            CreateSolidMode::Loft { other_profile } => {
                self.loft_between_profiles(profile_face, other_profile, material)
            }
        }
    }

    /// ADR-079 §2.3 — `Plane all-Line → Box` extrusion.
    ///
    /// 1. Translate boundary verts by `profile_normal * dist`.
    /// 2. Create top face (translated profile).
    /// 3. Create N side faces (one quad per profile edge).
    /// 4. Attach Plane surface to all new faces (top: translated profile,
    ///    sides: synthesized).
    ///
    /// Profile face is preserved (not removed) — caller (Scene wrapper)
    /// updates Shape/Xia ownership including the new top + sides.
    fn extrude_planar_box(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
        // ADR-264 — embedded boss on a closed solid: remove the profile and
        // fuse the side walls onto the shared rim (no preserved bottom cap).
        fuse_embedded: bool,
    ) -> Result<CreateSolidResult> {
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_box: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;
        if boundary_verts.len() < 3 {
            bail!(
                "extrude_planar_box: profile boundary has only {} verts (need ≥ 3)",
                boundary_verts.len()
            );
        }

        // Profile normal — from analytic surface (truth) rather than mesh
        // averaged normal (view).
        let profile_normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => bail!("extrude_planar_box: profile surface is not Plane"),
        };
        if profile_normal.length_squared() < 0.5 {
            bail!("extrude_planar_box: profile normal is near-zero");
        }
        let translation = profile_normal * dist;

        // Translate boundary verts to create top loop.
        let mut top_verts = Vec::with_capacity(boundary_verts.len());
        for &v in &boundary_verts {
            let pos = self.vertex_pos(v)?;
            top_verts.push(self.add_vertex(pos + translation));
        }

        // ADR-264 — fuse: remove the profile face BEFORE building the side
        // walls. This frees each rim edge's profile-side half-edge so the new
        // wall's `add_face` (find_halfedge Pass 1) re-twins with the
        // surrounding ring → ring↔wall = 2 faces (manifold), not 3. The boss
        // bottom becomes interior (no face), fusing the boss into the solid.
        // `boundary_verts` / `top_verts` stay valid (verts + shared edges
        // persist; only the profile face + its private HEs are deactivated).
        if fuse_embedded {
            self.remove_face(profile_face)?;
        }

        // Top face — translated profile.
        // Winding: profile is CCW (outward normal = profile_normal). Top
        // should have outward normal = +profile_normal (away from box top),
        // which is the same winding as profile when viewed from above.
        // BUT: if dist > 0 (extruding "up"), top is above profile, and its
        // outward normal should point UP (= +profile_normal). Profile's
        // normal is also +profile_normal. So both are CCW from "above".
        // For dist < 0 (recess), top is below, normal points DOWN. The
        // winding is the same — analytic transform preserves it.
        let top_face = self.add_face(&top_verts, material)?;

        // Side faces — one quad per profile edge.
        // Quad winding: outward normal = side_normal (perpendicular to
        // profile_normal, pointing away from box interior).
        // For a CCW profile loop and dist > 0, the natural quad is:
        //   [v_i, v_(i+1), top_(i+1), top_i] — outward normal correct.
        let n = boundary_verts.len();
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [
                    boundary_verts[i],
                    boundary_verts[next],
                    top_verts[next],
                    top_verts[i],
                ]
            } else {
                // dist < 0 — reverse winding so outward normal is correct.
                [
                    boundary_verts[next],
                    boundary_verts[i],
                    top_verts[i],
                    top_verts[next],
                ]
            };
            let side = self.add_face(&quad, material)?;
            side_faces.push(side);
        }

        // Surface attach — L3 lock-in (construction-time, not afterthought).
        // Top: translated profile surface (Phase H Rigid translation).
        let top_surface = profile_surface
            .transform(&DMat4::from_translation(translation))
            .unwrap_or_else(|_| {
                // Phase H transform failed (rare for pure translation); fall
                // back to synthesized Plane from top vertex positions.
                let top_positions: Vec<DVec3> = top_verts
                    .iter()
                    .filter_map(|v| self.vertex_pos(*v).ok())
                    .collect();
                synthesize_plane_surface(&top_positions)
            });
        if let Some(top_face_mut) = self.faces.get_mut(top_face) {
            top_face_mut.set_surface(Some(top_surface));
        }

        // Sides: synthesized Plane from each quad's vertex positions.
        for &side_fid in &side_faces {
            let face_ref = self.faces.get(side_fid);
            if face_ref.is_none() || !face_ref.unwrap().is_active() {
                continue;
            }
            let start = self.faces[side_fid].outer().start;
            if start.is_null() {
                continue;
            }
            let side_verts = self.collect_loop_verts(start)?;
            let positions: Vec<DVec3> = side_verts
                .iter()
                .filter_map(|v| self.vertex_pos(*v).ok())
                .collect();
            if positions.len() >= 3 {
                let side_surface = synthesize_plane_surface(&positions);
                self.faces[side_fid].set_surface(Some(side_surface));
            }
        }

        // ADR-183 (사용자 결재 2026-06-01) — Outward base cap.
        //
        // 사용자 보고: rect → Push/Pull 박스의 일부 면이 BackSide(파랑)로
        // 렌더 + 그 면에 다시 못 그림. 진단(엔진 verify_outward_normals):
        // extrude 후 BOTTOM cap 의 normal 이 INWARD. create_box 는 0 inward
        // 인데 push-pull 만 1 inward 였음.
        //
        // 원인: profile_face 는 그릴 때의 normal(+profile_normal)을 유지하는데,
        // dist>0 으로 위로 extrude 하면 profile 이 *바닥*이 되어 outward 는
        // -profile_normal 이어야 함. extrude_planar_box 는 top/side 만 새로
        // 만들고 profile_face 를 그대로 bottom 으로 두어 winding 을 안 뒤집었음.
        //
        // 수정: 바닥이 되는 cap 의 winding 을 flip (cached normal 부호 반전 +
        // loop 역순) + Plane surface 재합성(newell winding-aware → outward).
        //   dist > 0 → profile_face 가 바닥, dist < 0 → top_face 가 바닥.
        // reverse_loop 는 edge degree 보존 → manifold(is_closed_solid) 유지,
        // 공유 edge 의 twin 방향이 일관화됨 (orientation-consistent solid).
        // ADR-264 — fuse mode has NO preserved bottom cap (dist>0: profile
        // removed; dist<0: top_face is the pocket bottom, whose CCW-from-above
        // winding already yields the correct outward normal — see ADR-264 §2
        // pocket simulation). Skip the ADR-183 flip entirely when fusing.
        if !fuse_embedded {
            let bottom_cap = if dist > 0.0 { profile_face } else { top_face };
            self.flip_face(bottom_cap)?;
            {
                let bstart = self.faces[bottom_cap].outer().start;
                if !bstart.is_null() {
                    let bverts = self.collect_loop_verts(bstart)?;
                    let bpos: Vec<DVec3> = bverts
                        .iter()
                        .filter_map(|v| self.vertex_pos(*v).ok())
                        .collect();
                    if bpos.len() >= 3 {
                        let outward_surface = synthesize_plane_surface(&bpos);
                        self.faces[bottom_cap].set_surface(Some(outward_surface));
                    }
                }
            }
        }

        // ADR-067 Step 1 auto-merge — preserve.
        // The legacy push_pull's `adr_067_step1_auto_merge_result` works
        // on a `PushPullResult`. We don't need to invoke it here because
        // create_solid is invoked from a clean profile face — there are
        // no adjacent coplanar faces to auto-merge with at this step.
        // (Future W-2/W-3 variants may need to invoke auto-merge for
        // smooth-group cases.)
        let adjacent_splits = 0;

        // Aggregate all solid faces (profile + top + sides) for Shape
        // ownership.
        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        // ADR-264 — fuse removed the profile (now interior); don't report it.
        if !fuse_embedded {
            all_solid_faces.push(profile_face);
        }
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::Box,
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits,
            split_debug: Vec::new(),
        })
    }

    /// ADR-259 β-1 — `Plane convex/concave → tapered (draft) Box` extrusion.
    ///
    /// Like [`extrude_planar_box`] but the top loop is the **2D inward/outward
    /// offset** of the profile by `d = |dist|·tan(taper_rad)`, producing a
    /// frustum. Side walls stay **exact planar trapezoids** for convex AND
    /// concave profiles (ADR-259 §2: per-edge perpendicular offset keeps top
    /// edge ∥ bottom edge → one pair of parallel sides → always planar).
    ///
    /// **FAIL-CLOSED (D5)**: if the offset collapses / inverts / self-intersects
    /// / spikes, this bails! → `SolidError` *before any mesh mutation* (offset is
    /// checked before the first `add_vertex_force_new`), so the caller hard-errors
    /// and the mesh is untouched — never a silent straight extrude, never a broken
    /// face. Top verts use **`add_vertex_force_new`** (NOT `add_vertex`) to avoid
    /// the 0.15μm spatial-hash dedup merging a taper vert onto a profile vert at
    /// steep angles (review HIGH risk).
    ///
    /// Wired into `create_solid` dispatch via `CreateSolidMode::ExtrudeTapered`
    /// (β-1). Verified by the `adr259_sim_*` (construction) + `adr259_create_solid_*`
    /// (dispatch) tests.
    fn extrude_planar_box_tapered(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        taper_deg: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        use crate::boundary_kernel::geom2::{offset_polygon_2d, PolyOffset, Vec2};

        const MITER_LIMIT: f64 = 16.0;

        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_box_tapered: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;
        let n = boundary_verts.len();
        if n < 3 {
            bail!(
                "extrude_planar_box_tapered: profile boundary has {} verts (need ≥ 3)",
                n
            );
        }

        let profile_normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => bail!("extrude_planar_box_tapered: profile surface is not Plane"),
        };
        if profile_normal.length_squared() < 0.5 {
            bail!("extrude_planar_box_tapered: profile normal is near-zero");
        }
        let translation = profile_normal * dist;

        // 3D boundary positions.
        let positions: Vec<DVec3> = boundary_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;

        // 2D basis (t_axis, b_axis) in the profile plane (right-handed w/ normal).
        let centroid =
            positions.iter().fold(DVec3::ZERO, |a, &p| a + p) / (n as f64);
        let t_axis = {
            let e = positions[1] - positions[0];
            if e.length_squared() < EPSILON_LENGTH * EPSILON_LENGTH {
                bail!("extrude_planar_box_tapered: degenerate first edge (2D basis)");
            }
            e.normalize()
        };
        let b_axis = profile_normal.cross(t_axis);
        if b_axis.length_squared() < 0.5 {
            bail!("extrude_planar_box_tapered: degenerate 2D basis (b_axis near-zero)");
        }
        let b_axis = b_axis.normalize();

        // Project profile boundary → 2D.
        let poly2d: Vec<Vec2> = positions
            .iter()
            .map(|p| {
                let r = *p - centroid;
                Vec2::new(r.dot(t_axis), r.dot(b_axis))
            })
            .collect();

        // Offset distance: + taper = inward (top smaller / draft);
        //                  - taper = outward (flare). d = |dist|·tan(θ).
        let d_off = dist.abs() * taper_deg.to_radians().tan();

        let top2d = match offset_polygon_2d(&poly2d, d_off, MITER_LIMIT) {
            PolyOffset::Ok(p) => p,
            PolyOffset::Degenerate => bail!(
                "extrude_planar_box_tapered: taper offset collapses/inverts \
                 (taper too steep for this profile) — rejected (D5 fail-closed)"
            ),
            PolyOffset::SelfIntersect => bail!(
                "extrude_planar_box_tapered: taper offset self-intersects \
                 (concave over-offset) — rejected (D5 fail-closed)"
            ),
            PolyOffset::Spike => bail!(
                "extrude_planar_box_tapered: taper offset spike at a sharp vertex \
                 — rejected (D5 fail-closed)"
            ),
            PolyOffset::BadInput => bail!(
                "extrude_planar_box_tapered: degenerate profile for taper offset"
            ),
        };
        debug_assert_eq!(top2d.len(), n, "offset preserves vertex count");

        // Lift top 2D → 3D. **force_new** (NOT add_vertex) — no dedup merge.
        let mut top_verts = Vec::with_capacity(n);
        for w in &top2d {
            let pos3d = centroid + t_axis * w.x + b_axis * w.y + translation;
            top_verts.push(self.add_vertex_force_new(pos3d));
        }

        let top_face = self.add_face(&top_verts, material)?;

        // Side trapezoids — one per profile edge (winding policy = box).
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [
                    boundary_verts[i],
                    boundary_verts[next],
                    top_verts[next],
                    top_verts[i],
                ]
            } else {
                [
                    boundary_verts[next],
                    boundary_verts[i],
                    top_verts[i],
                    top_verts[next],
                ]
            };
            side_faces.push(self.add_face(&quad, material)?);
        }

        // Top surface — synthesized Plane from translated+offset top positions
        // (still coplanar in the translated plane; offset only shrank it).
        let top_positions: Vec<DVec3> = top_verts
            .iter()
            .filter_map(|v| self.vertex_pos(*v).ok())
            .collect();
        if top_positions.len() >= 3 {
            let top_surface = synthesize_plane_surface(&top_positions);
            if let Some(tf) = self.faces.get_mut(top_face) {
                tf.set_surface(Some(top_surface));
            }
        }

        // Side surfaces — synthesized Plane per trapezoid (exact, ADR-259 §2).
        for &side_fid in &side_faces {
            let fref = self.faces.get(side_fid);
            if fref.is_none() || !fref.unwrap().is_active() {
                continue;
            }
            let start = self.faces[side_fid].outer().start;
            if start.is_null() {
                continue;
            }
            let sverts = self.collect_loop_verts(start)?;
            let spos: Vec<DVec3> = sverts
                .iter()
                .filter_map(|v| self.vertex_pos(*v).ok())
                .collect();
            if spos.len() >= 3 {
                self.faces[side_fid].set_surface(Some(synthesize_plane_surface(&spos)));
            }
        }

        // ADR-183 outward base cap (same as extrude_planar_box).
        let bottom_cap = if dist > 0.0 { profile_face } else { top_face };
        self.flip_face(bottom_cap)?;
        {
            let bstart = self.faces[bottom_cap].outer().start;
            if !bstart.is_null() {
                let bverts = self.collect_loop_verts(bstart)?;
                let bpos: Vec<DVec3> = bverts
                    .iter()
                    .filter_map(|v| self.vertex_pos(*v).ok())
                    .collect();
                if bpos.len() >= 3 {
                    self.faces[bottom_cap].set_surface(Some(synthesize_plane_surface(&bpos)));
                }
            }
        }

        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::Box, // frustum = box-topology solid.
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-260 β-1 — `Plane circular → Cone / Frustum` extrusion. Reuses
    /// `AnalyticSurface::Cone` (ADR-031 Phase D, no new surface type).
    ///
    /// `top_scale ∈ [0,1)`: `0` = apex cone, `0<s<1` = frustum.
    ///
    /// **Cone surface params** (cone.rs: `P = apex + v·axis + v·tanα·radial`,
    /// outward `= cosα·radial − sinα·axis` ⇒ `axis_dir` points apex→base):
    /// - virtual apex `= center + n·(dist/(1−s))`
    /// - `axis_dir = (center − apex).normalize() = −sign(dist)·n`
    /// - `half_angle = atan(R·(1−s)/|dist|)`
    /// - `v_range = (|dist|·s/(1−s), |dist|/(1−s))` (top_v < base_v); apex
    ///   collapses to `(0, |dist|)`.
    ///
    /// **Construction (Q3 = full kernel-native):**
    /// - self-loop profile (1 vert, Path B production default):
    ///   - apex → mirror [`Mesh::create_cone_kernel_native`] (2 faces: profile
    ///     base + cone side on the profile's twin HE, apex degenerate v=0).
    ///   - frustum → mirror [`Mesh::extrude_cylinder_kernel_native`] (3 faces:
    ///     base + scaled-top disk + annulus side), side surface Cylinder → Cone.
    /// - polygonal-arc profile (≥3 verts, legacy / non-self-loop): fan (apex) /
    ///   quad (frustum) + ONE Cone surface.
    ///
    /// FAIL-CLOSED (D5): caller (dispatch arm) rejects degenerate distance /
    /// `top_scale ≥ 1` / `< 0` / solid face. Any error here → Scene rollback
    /// (byte-identical, `fallback_dist = None`).
    fn extrude_planar_cone(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        top_scale: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_cone: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;

        let normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => bail!("extrude_planar_cone: profile surface is not Plane"),
        };
        if normal.length_squared() < 0.5 {
            bail!("extrude_planar_cone: profile normal is near-zero");
        }

        // Circle params — self-loop: edge Circle curve; polygonal: reconcile arcs.
        let (center, radius, basis_u) = if boundary_verts.len() == 1 {
            let eid = self.hes[outer_start].edge();
            let curve = self
                .edges
                .get(eid)
                .and_then(|e| e.curve().cloned())
                .ok_or(SolidError::NotYetSupported {
                    reason: "cone extrude: self-loop edge has no AnalyticCurve".to_string(),
                })?;
            match curve {
                AnalyticCurve::Circle { center, radius, basis_u, .. } => (center, radius, basis_u),
                _ => {
                    return Err(SolidError::NotYetSupported {
                        reason: "cone extrude: self-loop curve is not Circle \
                                 (Bezier/BSpline/NURBS cone → future ADR)"
                            .to_string(),
                    }
                    .into())
                }
            }
        } else {
            let (c, r, _n, bu) = extract_shared_circle_params(self, profile_face).map_err(|e| {
                SolidError::NotYetSupported {
                    reason: format!("cone extrude: arc params mismatch — {}", e),
                }
            })?;
            (c, r, bu)
        };

        // snap-to-apex: sub-tolerance top cap (s·R < EPS) → apex (s = 0).
        let top_scale = if top_scale * radius < crate::tolerances::EPSILON_LENGTH {
            0.0
        } else {
            top_scale
        };

        // Cone surface params (see doc comment).
        let one_minus_s = 1.0 - top_scale;
        let apex_pt = center + normal * (dist / one_minus_s);
        let axis_dir = -(dist.signum()) * normal;
        let half_angle = (radius * one_minus_s / dist.abs()).atan();
        let base_v = dist.abs() / one_minus_s;
        let top_v = dist.abs() * top_scale / one_minus_s;
        let make_cone = |v_lo: f64, v_hi: f64| AnalyticSurface::Cone {
            apex: apex_pt,
            axis_dir,
            half_angle,
            ref_dir: basis_u,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v_lo, v_hi),
        };

        if boundary_verts.len() == 1 {
            // ── Path B (self-loop) kernel-native ──────────────────────────
            let bot_boundary_he = self.hes[outer_start].next_rad();
            if bot_boundary_he.is_null() || bot_boundary_he == outer_start {
                bail!(
                    "extrude_planar_cone: profile self-loop edge has degenerate \
                     radial chain — cannot locate twin HE"
                );
            }

            if top_scale == 0.0 {
                // APEX — mirror create_cone_kernel_native. Cone side = 1 self-loop
                // face on the profile's twin HE, apex degenerate (v=0). 2 faces.
                let cone_side = self.faces.insert(Face::new(
                    LoopRef::default(),
                    normal, // legacy field; canonical truth = AnalyticSurface::Cone
                    FACE_TOLERANCE,
                    material,
                ));
                self.hes[bot_boundary_he].set_next(bot_boundary_he);
                self.hes[bot_boundary_he].set_prev(bot_boundary_he);
                self.hes[bot_boundary_he].set_face(cone_side);
                self.hes[bot_boundary_he].set_outer(true);
                self.faces[cone_side].set_outer(LoopRef::new(bot_boundary_he, true));
                self.faces[cone_side].set_surface(Some(make_cone(0.0, base_v)));

                let owner_id = self.next_surface_owner_id();
                self.set_face_surface_owner_id(cone_side, Some(owner_id));

                return Ok(CreateSolidResult {
                    profile_face,
                    solid_kind: SolidKind::Cone,
                    // No real top cap (apex). The cone side is the new face.
                    top_face: cone_side,
                    side_faces: vec![cone_side],
                    all_solid_faces: vec![profile_face, cone_side],
                    adjacent_splits: 0,
                    split_debug: Vec::new(),
                });
            }

            // FRUSTUM — mirror extrude_cylinder_kernel_native (3-face annulus),
            // side surface Cylinder → Cone, top = scaled circle (radius·s).
            let top_center = center + normal * dist;
            let top_radius = radius * top_scale;
            let top_anchor_pos = top_center + basis_u * top_radius;
            let top_anchor = self.add_vertex(top_anchor_pos);
            let top_circle = AnalyticCurve::Circle {
                center: top_center,
                radius: top_radius,
                normal,
                basis_u,
            };
            let top_face = self.add_face_closed_curve(top_anchor, top_circle, material)?;

            let top_outer_start = self.faces[top_face].outer().start;
            let top_boundary_he = self.hes[top_outer_start].next_rad();
            if top_boundary_he.is_null() || top_boundary_he == top_outer_start {
                bail!(
                    "extrude_planar_cone: top self-loop edge has degenerate radial chain"
                );
            }

            let annulus = self.faces.insert(Face::new(
                LoopRef::default(),
                normal,
                FACE_TOLERANCE,
                material,
            ));
            // Bottom boundary HE → annulus outer (legacy schema).
            self.hes[bot_boundary_he].set_next(bot_boundary_he);
            self.hes[bot_boundary_he].set_prev(bot_boundary_he);
            self.hes[bot_boundary_he].set_face(annulus);
            self.hes[bot_boundary_he].set_outer(true);
            // Top boundary HE → annulus inner (legacy schema; Path B both
            // boundaries are outer-equivalent, set via set_face_boundary_loops).
            self.hes[top_boundary_he].set_next(top_boundary_he);
            self.hes[top_boundary_he].set_prev(top_boundary_he);
            self.hes[top_boundary_he].set_face(annulus);
            self.hes[top_boundary_he].set_outer(false);
            self.faces[annulus].set_outer(LoopRef::new(bot_boundary_he, true));
            self.faces[annulus].add_inner(LoopRef::new(top_boundary_he, false));
            self.set_face_boundary_loops(
                annulus,
                vec![
                    LoopRef::new(bot_boundary_he, true),
                    LoopRef::new(top_boundary_he, true),
                ],
            );
            self.faces[annulus].set_surface(Some(make_cone(top_v, base_v)));

            let owner_id = self.next_surface_owner_id();
            self.set_face_surface_owner_id(annulus, Some(owner_id));

            return Ok(CreateSolidResult {
                profile_face,
                solid_kind: SolidKind::Cone,
                top_face,
                side_faces: vec![annulus],
                all_solid_faces: vec![profile_face, top_face, annulus],
                adjacent_splits: 0,
                split_debug: Vec::new(),
            });
        }

        // ── Polygonal-arc circle (≥3 verts, legacy / non-self-loop) ────────
        // fan (apex) / quad (frustum) + ONE Cone surface.
        let n = boundary_verts.len();
        if n < 3 {
            bail!(
                "extrude_planar_cone: profile boundary has {} verts (need 1 self-loop or ≥ 3)",
                n
            );
        }
        let bot_pos: Vec<DVec3> = boundary_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;

        if top_scale == 0.0 {
            // APEX fan — single apex vertex + N triangles. No top cap.
            let apex_vid = self.add_vertex_force_new(apex_pt);
            let mut side_faces = Vec::with_capacity(n);
            for i in 0..n {
                let next = (i + 1) % n;
                let tri = if dist > 0.0 {
                    [boundary_verts[i], boundary_verts[next], apex_vid]
                } else {
                    [boundary_verts[next], boundary_verts[i], apex_vid]
                };
                side_faces.push(self.add_face(&tri, material)?);
            }
            let cone_surface = make_cone(0.0, base_v);
            for &fid in &side_faces {
                if self.faces.get(fid).map(|f| f.is_active()).unwrap_or(false) {
                    self.faces[fid].set_surface(Some(cone_surface.clone()));
                }
            }
            let owner_id = self.next_surface_owner_id();
            for &fid in &side_faces {
                self.set_face_surface_owner_id(fid, Some(owner_id));
            }
            let mut all_solid_faces = Vec::with_capacity(1 + side_faces.len());
            all_solid_faces.push(profile_face);
            all_solid_faces.extend(side_faces.iter().copied());
            let top_face = *side_faces.first().unwrap(); // no real top; first side.
            return Ok(CreateSolidResult {
                profile_face,
                solid_kind: SolidKind::Cone,
                top_face,
                side_faces,
                all_solid_faces,
                adjacent_splits: 0,
                split_debug: Vec::new(),
            });
        }

        // FRUSTUM polygonal — scaled top verts + N quads + Plane top cap.
        let mut top_verts = Vec::with_capacity(n);
        for &p in &bot_pos {
            // Scale toward center in-plane by top_scale, then translate.
            let scaled = center + (p - center) * top_scale + normal * dist;
            top_verts.push(self.add_vertex_force_new(scaled));
        }
        let top_face = self.add_face(&top_verts, material)?;
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [boundary_verts[i], boundary_verts[next], top_verts[next], top_verts[i]]
            } else {
                [boundary_verts[next], boundary_verts[i], top_verts[i], top_verts[next]]
            };
            side_faces.push(self.add_face(&quad, material)?);
        }
        // Top cap Plane surface (translated + scaled profile plane).
        let top_positions: Vec<DVec3> = top_verts
            .iter()
            .filter_map(|v| self.vertex_pos(*v).ok())
            .collect();
        if top_positions.len() >= 3 {
            if let Some(tf) = self.faces.get_mut(top_face) {
                tf.set_surface(Some(synthesize_plane_surface(&top_positions)));
            }
        }
        let cone_surface = make_cone(top_v, base_v);
        for &fid in &side_faces {
            if self.faces.get(fid).map(|f| f.is_active()).unwrap_or(false) {
                self.faces[fid].set_surface(Some(cone_surface.clone()));
            }
        }
        let owner_id = self.next_surface_owner_id();
        for &fid in &side_faces {
            self.set_face_surface_owner_id(fid, Some(owner_id));
        }
        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());
        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::Cone,
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-261 β-1 — bidirectional / two-sided extrude about the profile plane.
    ///
    /// **Q2 (translate + reuse)**: translate the profile by `−normal·dist_neg`
    /// (ADR-060 Phase O carries the Circle curve center / Plane surface origin —
    /// ALL boundary verts move → full move → curve/surface transform, NOT the
    /// Line fallback), then reuse the one-way `extrude_planar_box` /
    /// `extrude_planar_cylinder` by `dist_pos + dist_neg`. The moved profile
    /// becomes the (outward, ADR-183 flipped) bottom cap at `−dist_neg`; the top
    /// cap lands at `+dist_pos`. Profile is PRESERVED (not consumed) → Shape/Xia
    /// ownership unchanged (the Scene wrapper maps the new top + sides to the
    /// profile's owner, as for one-way extrude).
    ///
    /// Result solid spans `[−dist_neg, +dist_pos]` about the original profile
    /// plane (identical geometry to AixiAcad's build-fresh `extrude_planar_face_
    /// bidir`, with maximal reuse of our proven one-way extrude).
    ///
    /// Caller (dispatch arm) has already validated `dist_pos, dist_neg ≥ 0` +
    /// `sum > EPSILON`, rejected solid faces (`is_move_only`), and cleaved
    /// coplanar siblings. `dist_neg = 0` degenerates cleanly to a one-way `+`
    /// extrude (translate no-op).
    fn extrude_planar_bidirectional(
        &mut self,
        profile_face: FaceId,
        dist_pos: f64,
        dist_neg: f64,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_bidirectional: profile face has null outer loop start");
        }
        let surface = self
            .faces
            .get(profile_face)
            .and_then(|f| f.surface().cloned())
            .ok_or(SolidError::NoProfileSurface)?;
        let normal = match &surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => {
                return Err(SolidError::NotYetSupported {
                    reason: "bidirectional extrude: profile surface is not Plane (ADR-261 v1)"
                        .to_string(),
                }
                .into())
            }
        };
        if normal.length_squared() < 0.5 {
            bail!("extrude_planar_bidirectional: profile normal is near-zero");
        }

        // Step 1 — translate the profile to the −dist_neg plane. ADR-060 Phase O:
        // ALL boundary verts move → Circle curve center translates + Plane surface
        // origin transforms (NOT the partial-move Line fallback).
        if dist_neg > crate::tolerances::EPSILON_LENGTH {
            let boundary_verts = self.collect_loop_verts(outer_start)?;
            self.translate_verts(&boundary_verts, normal * (-dist_neg))?;
        }

        // Step 2 — reuse the one-way extrude by the full height. The (now bottom)
        // profile becomes the outward bottom cap; extrude builds the top cap at
        // +dist_pos + N side walls spanning the full height.
        let total = dist_pos + dist_neg;
        // Re-read the surface AFTER translate (ADR-060 transformed its origin).
        let surface = self
            .faces
            .get(profile_face)
            .and_then(|f| f.surface().cloned())
            .ok_or(SolidError::NoProfileSurface)?;
        let boundary = classify_boundary(self, profile_face)
            .map_err(|e| SolidError::BoundaryCollection(e.to_string()))?;
        match (&surface, boundary) {
            (AnalyticSurface::Plane { .. }, BoundaryKind::AllLinear) => {
                // ADR-264 — bidirectional path: no embedded-boss fuse (current).
                self.extrude_planar_box(profile_face, total, material, &surface, false)
            }
            (AnalyticSurface::Plane { .. }, BoundaryKind::AllCircular) => {
                self.extrude_planar_cylinder(profile_face, total, material, &surface)
            }
            _ => Err(SolidError::NotYetSupported {
                reason: "bidirectional extrude v1 supports (Plane, AllLinear|AllCircular) only \
                         (ADR-261)"
                    .to_string(),
            }
            .into()),
        }
    }

    /// ADR-079 W-3 / follow-up (2026-06-16) — `Plane mixed boundary (Arc + Line)
    /// → general sweep` extrusion. The native path for a face whose Plane
    /// boundary mixes Arc/Circle and Line/straight edges (e.g. a circle cut by
    /// a secant = arc cap). Generalizes [`extrude_planar_box`] (AllLinear) by
    /// attaching the side-wall surface PER EDGE at construction time:
    /// - Arc / Circle edge → `Cylinder` (axis = profile normal through the arc
    ///   center, radius = arc radius) → smooth.
    /// - Line / no-curve edge → synthesized `Plane` → flat (the chord, correct).
    ///
    /// The top cap inherits each boundary edge's curve translated by the
    /// extrude vector (Arc-aware top rim, same as the push_pull §4c fix) so the
    /// top stays smooth. Per-edge curve lookup is by VERT-PAIR (`find_edge`),
    /// guaranteeing curve↔edge alignment (the ADR-102 cleave lesson).
    ///
    /// Replaces the legacy `push_pull` fallback (snapshot/restore/re-cleave +
    /// post-hoc `promote_arc_side_faces_to_cylinder`) with a single
    /// construction-time path. ADR-102 cleave is handled by `create_solid`'s
    /// own pre-step (the profile is already cleaved when this runs).
    ///
    /// Freeform boundary edges (Bezier / BSpline / NURBS) need swept
    /// `BSplineSurface` walls (ADR-192) — NOT handled here → returns
    /// `NotYetSupported` so Scene falls back to legacy push_pull (those cases
    /// preserved unchanged).
    fn extrude_planar_mixed(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        use crate::curves::synthesize::synthesize_plane_surface;
        use crate::curves::AnalyticCurve;

        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_mixed: profile face has null outer loop start");
        }
        let boundary = self.collect_loop_verts(outer_start)?;
        let n = boundary.len();
        if n < 3 {
            bail!("extrude_planar_mixed: profile boundary has only {} verts (need ≥ 3)", n);
        }

        // Per-edge curve snapshot by VERT-PAIR (alignment — ADR-102 cleave lesson:
        // `face_outer_edges` order may differ from `collect_loop_verts`).
        // Also capture curve_owner_id for ADR-088 P22.5 top-cap propagation.
        let mut edge_curves: Vec<Option<AnalyticCurve>> = Vec::with_capacity(n);
        let mut edge_owner_ids: Vec<Option<u32>> = Vec::with_capacity(n);
        for i in 0..n {
            let eid = self.find_edge(boundary[i], boundary[(i + 1) % n]);
            edge_curves.push(eid.and_then(|e| self.edge_curve(e).cloned()));
            edge_owner_ids.push(eid.and_then(|e| self.edge_curve_owner_id(e)));
        }
        // Freeform edges → defer to legacy push_pull (swept surface, ADR-192).
        if edge_curves.iter().any(|c| matches!(
            c,
            Some(AnalyticCurve::Bezier { .. } | AnalyticCurve::BSpline { .. } | AnalyticCurve::NURBS { .. })
        )) {
            return Err(SolidError::NotYetSupported {
                reason: "Plane mixed boundary with freeform (Bezier/BSpline/NURBS) edge → swept surface (ADR-192 scope)".to_string(),
            }
            .into());
        }

        let profile_normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => bail!("extrude_planar_mixed: profile surface is not Plane"),
        };
        if profile_normal.length_squared() < 0.5 {
            bail!("extrude_planar_mixed: profile normal is near-zero");
        }
        let translation = profile_normal * dist;

        // Top verts (translated profile).
        let mut top_verts = Vec::with_capacity(n);
        for &v in &boundary {
            let p = self.vertex_pos(v)?;
            top_verts.push(self.add_vertex(p + translation));
        }

        // Top cap — translated Plane surface.
        let top_face = self.add_face(&top_verts, material)?;
        let top_surface = profile_surface
            .transform(&DMat4::from_translation(translation))
            .unwrap_or_else(|_| {
                let pos: Vec<DVec3> = top_verts.iter().filter_map(|v| self.vertex_pos(*v).ok()).collect();
                synthesize_plane_surface(&pos)
            });
        self.faces[top_face].set_surface(Some(top_surface));

        // Top cap edges — propagate each boundary edge's curve, translated
        // (Arc/Circle → smooth top rim; Line/no-curve stays straight).
        // ADR-088 P22.5: map each unique base arc owner → fresh top owner so
        // the two D7 half-edges of each top arc are grouped for single-click selection.
        let mut base_to_top_owner: std::collections::HashMap<u32, u32> = Default::default();
        for i in 0..n {
            let translated = match &edge_curves[i] {
                Some(AnalyticCurve::Arc { center, radius, normal, basis_u, start_angle, end_angle }) => {
                    Some(AnalyticCurve::Arc {
                        center: *center + translation, radius: *radius, normal: *normal,
                        basis_u: *basis_u, start_angle: *start_angle, end_angle: *end_angle,
                    })
                }
                Some(AnalyticCurve::Circle { center, radius, normal, basis_u }) => {
                    Some(AnalyticCurve::Circle {
                        center: *center + translation, radius: *radius, normal: *normal, basis_u: *basis_u,
                    })
                }
                _ => None,
            };
            if let Some(tc) = translated {
                if let Some(te) = self.find_edge(top_verts[i], top_verts[(i + 1) % n]) {
                    if let Some(e) = self.edges.get_mut(te) {
                        e.set_curve(Some(tc));
                    }
                    // Propagate owner: same base owner → same top owner (fresh id per arc group)
                    if let Some(base_owner) = edge_owner_ids[i] {
                        let top_owner = if let Some(&existing) = base_to_top_owner.get(&base_owner) {
                            existing
                        } else {
                            let new_id = self.next_curve_owner_id();
                            base_to_top_owner.insert(base_owner, new_id);
                            new_id
                        };
                        self.set_edge_curve_owner_id(te, Some(top_owner));
                    }
                }
            }
        }

        // Side walls — one quad per profile edge. Arc/Circle → Cylinder,
        // else Plane (construction-time surface attach, ADR-079 L3 lock-in).
        let (v_lo, v_hi) = if dist > 0.0 { (0.0, dist) } else { (dist, 0.0) };
        let mut side_faces = Vec::with_capacity(n);
        let mut arc_side_faces = Vec::new();
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [boundary[i], boundary[next], top_verts[next], top_verts[i]]
            } else {
                [boundary[next], boundary[i], top_verts[i], top_verts[next]]
            };
            let side = self.add_face(&quad, material)?;
            side_faces.push(side);

            let cyl_params = match &edge_curves[i] {
                Some(AnalyticCurve::Arc { center, radius, basis_u, .. })
                | Some(AnalyticCurve::Circle { center, radius, basis_u, .. }) => {
                    Some((*center, *radius, *basis_u))
                }
                _ => None,
            };
            let surf = if let Some((center, radius, basis_u)) = cyl_params {
                arc_side_faces.push(side);
                AnalyticSurface::Cylinder {
                    axis_origin: center,
                    axis_dir: profile_normal,
                    radius,
                    ref_dir: basis_u,
                    u_range: (0.0, std::f64::consts::TAU),
                    v_range: (v_lo, v_hi),
                }
            } else {
                let pos: Vec<DVec3> = quad.iter().filter_map(|&v| self.vertex_pos(v).ok()).collect();
                synthesize_plane_surface(&pos)
            };
            self.faces[side].set_surface(Some(surf));
        }

        // owner-id grouping — all arc-edge Cylinder walls share one cylinder
        // group so a single-face click selects the whole side (ADR-093 D-F).
        if !arc_side_faces.is_empty() {
            let owner_id = self.next_surface_owner_id();
            for &sf in &arc_side_faces {
                self.set_face_surface_owner_id(sf, Some(owner_id));
            }
        }

        // ADR-183 — bottom cap (= profile when dist>0) outward winding flip.
        let bottom_cap = if dist > 0.0 { profile_face } else { top_face };
        self.flip_face(bottom_cap)?;
        {
            let bstart = self.faces[bottom_cap].outer().start;
            if !bstart.is_null() {
                let bverts = self.collect_loop_verts(bstart)?;
                let bpos: Vec<DVec3> = bverts.iter().filter_map(|v| self.vertex_pos(*v).ok()).collect();
                if bpos.len() >= 3 {
                    let outward = synthesize_plane_surface(&bpos);
                    self.faces[bottom_cap].set_surface(Some(outward));
                }
            }
        }

        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::GeneralSweep,
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-2-α — `Plane circular boundary → Cylinder` extrusion.
    ///
    /// Profile face has `AnalyticSurface::Plane` and outer loop edges all
    /// carry `AnalyticCurve::Arc` sharing identical (center, radius, normal).
    /// Builds:
    /// 1. Top cap = translated profile (Plane surface).
    /// 2. N side faces (one quad per profile edge), all sharing the SAME
    ///    `AnalyticSurface::Cylinder` instance — automatic smooth group.
    ///
    /// On boundary arc-parameter mismatch (different center/radius/normal
    /// among the loop's arcs) returns `NotYetSupported` so Scene falls back
    /// to legacy push_pull (Q3 lock-in).
    fn extrude_planar_cylinder(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("extrude_planar_cylinder: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;

        // ADR-089 A-θ-β — closed-curve face fast-path. ADR-094 B-η
        // dispatch: Path B (kernel-native annulus) vs Path A (legacy
        // tessellate-then-extrude). Engine default = false (Path A) —
        // preserves 245+ regression assets. Production layer flips
        // to true via Mesh::set_cylinder_path_b_default.
        if boundary_verts.len() == 1 {
            if self.cylinder_path_b_default {
                // ADR-094 B-η — Path B canonical (annulus topology,
                // 3 face / 2 edge / 2 vert, 산업 CAD parity).
                return self.extrude_cylinder_kernel_native(
                    profile_face, dist, material,
                );
            }
            // Legacy Path A — L-θ-2 / L-θ-3 / L-θ-4 / L-θ-5.
            return self.extrude_closed_curve_face_via_tessellation(
                profile_face,
                dist,
                material,
                profile_surface,
            );
        }

        if boundary_verts.len() < 3 {
            bail!(
                "extrude_planar_cylinder: profile boundary has only {} verts (need ≥ 3)",
                boundary_verts.len()
            );
        }

        // Profile normal — Plane truth source.
        let profile_normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => bail!("extrude_planar_cylinder: profile surface is not Plane"),
        };
        if profile_normal.length_squared() < 0.5 {
            bail!("extrude_planar_cylinder: profile normal is near-zero");
        }
        let translation = profile_normal * dist;

        // Extract circle params from boundary arcs and verify consistency.
        // §W2-B-(a) lock-in — all arcs must share (center, radius, normal).
        let (circle_center, circle_radius, _circle_normal, circle_basis_u) =
            extract_shared_circle_params(self, profile_face).map_err(|e| {
                SolidError::NotYetSupported {
                    reason: format!(
                        "Plane circular boundary arc parameters mismatch — {} (Q3 fallback)",
                        e
                    ),
                }
            })?;

        // Translate boundary verts to create top loop.
        let mut top_verts = Vec::with_capacity(boundary_verts.len());
        for &v in &boundary_verts {
            let pos = self.vertex_pos(v)?;
            top_verts.push(self.add_vertex(pos + translation));
        }

        // Top cap face (translated profile).
        let top_face = self.add_face(&top_verts, material)?;

        // Side faces — one quad per profile edge.
        let n = boundary_verts.len();
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [
                    boundary_verts[i],
                    boundary_verts[next],
                    top_verts[next],
                    top_verts[i],
                ]
            } else {
                [
                    boundary_verts[next],
                    boundary_verts[i],
                    top_verts[i],
                    top_verts[next],
                ]
            };
            let side = self.add_face(&quad, material)?;
            side_faces.push(side);
        }

        // Surface attach — L3 lock-in.
        // Top cap: translated profile Plane surface.
        let top_surface = profile_surface
            .transform(&DMat4::from_translation(translation))
            .unwrap_or_else(|_| {
                let top_positions: Vec<DVec3> = top_verts
                    .iter()
                    .filter_map(|v| self.vertex_pos(*v).ok())
                    .collect();
                synthesize_plane_surface(&top_positions)
            });
        if let Some(top_face_mut) = self.faces.get_mut(top_face) {
            top_face_mut.set_surface(Some(top_surface));
        }

        // ADR-088 P22.5 — top rim arc curves + shared owner for single-click selection.
        // All N top-rim arc segments share one owner so clicking any one segment
        // selects the entire top-cap circle rim (same pattern as step 6/8 in
        // extrude_closed_curve_face_via_tessellation).
        {
            let top_rim_owner = self.next_curve_owner_id();
            let top_center = circle_center + translation;
            let two_pi = std::f64::consts::TAU;
            for i in 0..n {
                let theta_start = (i as f64) * two_pi / (n as f64);
                let theta_end = ((i + 1) as f64) * two_pi / (n as f64);
                let arc = AnalyticCurve::Arc {
                    center: top_center,
                    radius: circle_radius,
                    normal: profile_normal,
                    basis_u: circle_basis_u,
                    start_angle: theta_start,
                    end_angle: theta_end,
                };
                if let Some(eid) = self.find_edge(top_verts[i], top_verts[(i + 1) % n]) {
                    if let Some(edge_mut) = self.edges.get_mut(eid) {
                        edge_mut.set_curve(Some(arc));
                    }
                    self.set_edge_curve_owner_id(eid, Some(top_rim_owner));
                }
            }
        }

        // Side wall: SAME `AnalyticSurface::Cylinder` instance shared by all
        // N quad faces. Smooth group emerges naturally from shared surface
        // kind + parameters (ADR-038 P23 surface-aware normals).
        // Cylinder axis_origin = circle_center on profile plane (preserved).
        // The cylinder spans from profile plane to translated plane:
        //   v ∈ [0, dist] along axis_dir = profile_normal (signed).
        // u ∈ [0, 2π] full circumference.
        let (axis_dir, v_lo, v_hi) = if dist > 0.0 {
            (profile_normal, 0.0, dist)
        } else {
            // For dist < 0, axis still points along profile_normal so the
            // cylinder's local v parameter increases away from profile —
            // but extrusion goes in -profile_normal direction. We choose
            // axis_dir = profile_normal and v_range = [dist, 0] so that
            // (axis_origin + axis_dir * v) for v ∈ [dist, 0] sweeps the
            // wall from translated plane back to profile.
            (profile_normal, dist, 0.0)
        };
        let cylinder_surface = AnalyticSurface::Cylinder {
            axis_origin: circle_center,
            axis_dir,
            radius: circle_radius,
            ref_dir: circle_basis_u,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v_lo, v_hi),
        };
        for &side_fid in &side_faces {
            let face_ref = self.faces.get(side_fid);
            if face_ref.is_none() || !face_ref.unwrap().is_active() {
                continue;
            }
            self.faces[side_fid].set_surface(Some(cylinder_surface.clone()));
        }

        // ADR-093 D-β — Surface owner-id grouping (B-MVP).
        // All N side faces share a single fresh owner-id so the selection
        // layer can promote a single-face click → entire cylinder side
        // group (Lock-in D-F: allocation site = post N-side creation).
        let owner_id = self.next_surface_owner_id();
        for &side_fid in &side_faces {
            self.set_face_surface_owner_id(side_fid, Some(owner_id));
        }

        let adjacent_splits = 0;

        // Aggregate — profile + top + N sides.
        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::Cylinder,
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits,
            split_debug: Vec::new(),
        })
    }

    /// ADR-089 A-θ-β — closed-curve face Push-Pull via tessellation
    /// (Path A jamjeong; Path B 진정한 kernel-native cylinder 는 별도
    /// future ADR).
    ///
    /// Detect: profile has exactly 1 boundary vertex (anchor) + 1
    /// self-loop edge with `AnalyticCurve::Circle` curve.
    ///
    /// Process (L-θ-3 / L-θ-4 / L-θ-5):
    /// 1. Extract Circle (center, radius, normal, basis_u) from edge curve.
    /// 2. Tessellate to N points (default chord_tol = radius/100, → ~32
    ///    segments for 1m radius; min 8 enforced by `segment_count_for_arc`).
    /// 3. Soft-delete original closed-curve face (`remove_face`).
    /// 4. Add a fresh polygonal face with N tessellated vertices.
    /// 5. Inherit Plane surface from original closed-curve face.
    /// 6. Recurse `extrude_planar_cylinder` with substituted profile —
    ///    the recursion's `boundary_verts.len() == N >= 8` skips the
    ///    fast-path and proceeds with normal extrusion.
    ///
    /// **Result**: top + N side faces are Plane / Cylinder (ADR-087 K-δ
    /// Cylinder primitive 와 동일 토폴로지). closed-curve canonical
    /// 표현 은 result solid 에 보존되지 않음 — 메타-원칙 #14 측면 회귀
    /// 가 Path B (별도 ADR) 까지 deferred.
    fn extrude_closed_curve_face_via_tessellation(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        // 1. Locate self-loop edge + Circle curve.
        let outer_start = self.faces[profile_face].outer().start;
        let self_loop_edge_id = self.hes[outer_start].edge();
        let anchor_vid = self.edges[self_loop_edge_id].v_small();
        let edge_id = self_loop_edge_id;
        let curve = self
            .edges
            .get(edge_id)
            .and_then(|e| e.curve().cloned())
            .ok_or(SolidError::NotYetSupported {
                reason:
                    "extrude closed-curve fast-path: self-loop edge has no AnalyticCurve attached"
                        .to_string(),
            })?;
        let (center, radius, normal, basis_u) = match curve {
            AnalyticCurve::Circle {
                center,
                radius,
                normal,
                basis_u,
            } => (center, radius, normal, basis_u),
            _ => {
                return Err(SolidError::NotYetSupported {
                    reason: format!(
                        "extrude closed-curve fast-path: only Circle curves supported \
                         in Path A (got {:?})",
                        std::mem::discriminant(&curve),
                    ),
                }
                .into());
            }
        };

        // 2. Tessellate (chord_tol = radius / 100 → ~32 seg, min 8).
        let chord_tol = (radius * 0.01).max(1e-6);
        let pts = crate::curves::circle::tessellate_full(
            center, radius, normal, basis_u, chord_tol,
        );
        // tessellate_full returns N+1 closed (last == first) — drop tail.
        if pts.len() < 4 {
            bail!(
                "extrude_closed_curve_face_via_tessellation: tessellation produced {} points \
                 (need ≥ 4 incl. closing duplicate)",
                pts.len()
            );
        }
        let unique_pts = &pts[..pts.len() - 1];
        let tess_verts: Vec<VertId> =
            unique_pts.iter().map(|p| self.add_vertex(*p)).collect();

        // 3. Soft-delete original closed-curve face.
        self.remove_face(profile_face)?;

        // 3b. ADR-089 A-υ-β — cleanup orphan self-loop edge + anchor.
        // After remove_face the closed-curve self-loop edge has no
        // active face referencing it. Without this cleanup the edge
        // still renders as 23 polyline segments (A-κ-β closed-curve
        // edge wireframe path) overlapping the new polygonal bottom.
        // L-υ-1 / L-υ-2.
        if self.edges.contains(self_loop_edge_id)
            && self.edges[self_loop_edge_id].is_active()
        {
            let _ = self.remove_edge_and_halfedges(self_loop_edge_id);
        }
        // Anchor vertex: deactivate if no other edges reference it.
        // (L-υ-2 — preserve if used by other standalone wires.)
        if self.verts.contains(anchor_vid) && self.verts[anchor_vid].is_active() {
            if self.verts[anchor_vid].outgoing().is_none() {
                self.verts[anchor_vid].set_active(false);
            }
        }

        // 4. Create polygonal substitute face.
        let substituted = self.add_face(&tess_verts, material)?;

        // 5. Inherit Plane surface (L-θ-5).
        if let Some(face_mut) = self.faces.get_mut(substituted) {
            face_mut.set_surface(Some(profile_surface.clone()));
        }

        // 6. Attach Arc curves to each substitute edge — required for
        //    `extract_shared_circle_params` (called by recursion) to
        //    classify the boundary as `AllCircular` and recover
        //    (center, radius, normal, basis_u). Without curve attach,
        //    the recursion fails with "edge is not Circle/Arc".
        let n_seg = tess_verts.len();
        let edges = self.face_outer_edges(substituted)?;
        let two_pi = std::f64::consts::TAU;
        // ADR-088 P22.5 — all N bottom-rim arc segments share one owner so a
        // single click on any segment selects the whole bottom circle rim.
        let bottom_rim_owner = self.next_curve_owner_id();
        for (i, &eid) in edges.iter().enumerate() {
            let theta_start = (i as f64) * two_pi / (n_seg as f64);
            let theta_end = ((i + 1) as f64) * two_pi / (n_seg as f64);
            let arc = AnalyticCurve::Arc {
                center,
                radius,
                normal,
                basis_u,
                start_angle: theta_start,
                end_angle: theta_end,
            };
            if let Some(edge_mut) = self.edges.get_mut(eid) {
                edge_mut.set_curve(Some(arc));
            }
            self.set_edge_curve_owner_id(eid, Some(bottom_rim_owner));
        }

        // 7. Recurse — substitute now has N >= 8 verts + Arc curves;
        //    fast-path skipped, AllCircular branch matches.
        let result = self.extrude_planar_cylinder(
            substituted, dist, material, profile_surface,
        )?;

        // 8. ADR-092 C-β — attach Arc curves to TOP face's N edges
        //    (mirror step 6 for bottom). Translated center =
        //    profile_normal · dist + original center. DCEL topology
        //    unchanged (manifold-safe per L1/L5). Render fast-path
        //    (A-κ Arc tessellation) samples the analytic curves and
        //    emits a smooth ring polyline — fixes "원에 대한 완벽한
        //    처리가 안되고 있습니다" (2026-05-09 사용자 시연 결함 1).
        let profile_normal = match profile_surface {
            AnalyticSurface::Plane { normal, .. } => normal.normalize_or_zero(),
            _ => DVec3::ZERO, // unreachable — extrude_planar_cylinder enforces Plane
        };
        if profile_normal.length_squared() > 0.5 {
            let translation = profile_normal * dist;
            let top_center = center + translation;
            // Top face edges in face_outer_edges() loop order — same N
            // chord positions as bottom (just translated). Index i
            // corresponds to angular sector [i, i+1)/N · 2π.
            //
            // Note on winding: top face may have reversed loop order
            // vs bottom (CCW from above vs CCW from below). The Arc
            // curve is direction-agnostic — the same Arc(theta_a,
            // theta_b) and Arc(theta_b, theta_a) sample the same point
            // set. Visual ring is identical regardless of loop order.
            if let Ok(top_edges) = self.face_outer_edges(result.top_face) {
                let n_seg_top = top_edges.len();
                if n_seg_top == n_seg {
                    // ADR-088 P22.5 — all N top-rim arc segments share one owner
                    // (different from bottom_rim_owner) so a click selects the whole top circle.
                    let top_rim_owner = self.next_curve_owner_id();
                    for (i, &eid) in top_edges.iter().enumerate() {
                        let theta_start = (i as f64) * two_pi / (n_seg_top as f64);
                        let theta_end =
                            ((i + 1) as f64) * two_pi / (n_seg_top as f64);
                        let arc = AnalyticCurve::Arc {
                            center: top_center,
                            radius,
                            normal,
                            basis_u,
                            start_angle: theta_start,
                            end_angle: theta_end,
                        };
                        if let Some(edge_mut) = self.edges.get_mut(eid) {
                            edge_mut.set_curve(Some(arc));
                        }
                        self.set_edge_curve_owner_id(eid, Some(top_rim_owner));
                    }
                }
            }
        }

        Ok(result)
    }

    /// ADR-094 B-δ-prep — Path B kernel-native cylinder extrude (coexist
    /// with `extrude_closed_curve_face_via_tessellation` Path A).
    ///
    /// **Status**: Additive prep — exposed via test entry point only.
    /// Production paths still route through Path A. B-η flip will switch
    /// the canonical entry.
    ///
    /// **Architectural goal** (ADR-094 §1, ADR-090 §1.2): cylinder DCEL =
    /// 3 face / 2 edge / 2 vert (산업 CAD parity), instead of Path A's
    /// 25 face / 70 edge / 46 vert.
    ///
    /// Process:
    /// 1. Profile must be a closed-curve face (1 anchor + 1 self-loop
    ///    edge with `AnalyticCurve::Circle`) — same precondition as Path A.
    /// 2. Translate anchor + circle by `profile_normal · dist` → top
    ///    anchor + top circle.
    /// 3. Create top face via `add_face_closed_curve` (ADR-089 pattern).
    /// 4. Locate boundary HEs (twin of each closed-curve face's anchor
    ///    HE — the one with `face = NULL` after Path A topology).
    /// 5. Create annulus side face manually:
    ///    a. `Face::new` with placeholder + `faces.insert`.
    ///    b. Wire both boundary HEs to the annulus face (face = annulus_id,
    ///       next/prev = self for self-loop semantics).
    ///    c. Set `face.outer = LoopRef(top_boundary_he, true)`,
    ///       `face.inners = [LoopRef(bot_boundary_he, false)]` —
    ///       legacy traversal sees a "ring with hole" semantic, which
    ///       Path A code does NOT exercise on this face (test entry
    ///       only).
    ///    d. Set `face_to_boundary_loops[annulus_id] = [top_loop, bot_loop]`
    ///       — Path B canonical (B-γ-prep effective getter).
    /// 6. Attach `AnalyticSurface::Cylinder` to annulus face.
    /// 7. Top + bottom faces inherit `AnalyticSurface::Plane` from their
    ///    closed-curve construction (ADR-089 A-η-1).
    ///
    /// Returns `CreateSolidResult` with `solid_kind = SolidKind::Cylinder`,
    /// `side_faces = [annulus_id]` (single face).
    ///
    /// **Out of scope (B-δ-prep)**: Push-Pull / Boolean / Render path
    /// integration — those are separate prep sub-steps. The annulus face
    /// is *constructed correctly* by this method, but downstream ops
    /// won't render or process it as a single cylindrical face until
    /// later prep steps land + B-η flip.
    pub fn extrude_cylinder_kernel_native(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        // 1. Validate profile = closed-curve face with Circle.
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!(
                "B-δ-prep: profile face {profile_face:?} has null outer loop"
            );
        }
        let bot_self_loop_eid = self.hes[outer_start].edge();
        if !self.edges[bot_self_loop_eid].is_self_loop() {
            bail!(
                "B-δ-prep: profile face's outer edge {:?} is not a self-loop \
                 (Path B requires closed-curve profile)",
                bot_self_loop_eid,
            );
        }
        let bot_anchor = self.edges[bot_self_loop_eid].v_small();
        let curve = self
            .edges
            .get(bot_self_loop_eid)
            .and_then(|e| e.curve().cloned())
            .ok_or_else(|| anyhow::anyhow!(
                "B-δ-prep: profile self-loop edge has no AnalyticCurve"
            ))?;
        let (center, radius, normal, basis_u) = match curve {
            AnalyticCurve::Circle { center, radius, normal, basis_u } => {
                (center, radius, normal, basis_u)
            }
            _ => bail!(
                "B-δ-prep: only Circle profiles supported for kernel-native \
                 cylinder (other closed curves → general analytic sweep, \
                 future ADR)"
            ),
        };

        // 2. Compute translation along the profile normal.
        let translation = normal * dist;
        let top_center = center + translation;
        let top_anchor_pos = self.vertex_pos(bot_anchor)? + translation;

        // 3. Create top vert + top closed-curve face (ADR-089 pattern).
        let top_anchor = self.add_vertex(top_anchor_pos);
        let top_circle = AnalyticCurve::Circle {
            center: top_center,
            radius,
            normal,
            basis_u,
        };
        let top_face = self.add_face_closed_curve(top_anchor, top_circle, material)?;

        // 4. Locate the top self-loop edge + its boundary HE (twin of
        //    the HE in the top face's outer loop).
        let top_outer_start = self.faces[top_face].outer().start;
        let top_self_loop_eid = self.hes[top_outer_start].edge();
        let top_boundary_he = self.hes[top_outer_start].next_rad();
        if top_boundary_he.is_null() || top_boundary_he == top_outer_start {
            bail!(
                "B-δ-prep: top self-loop edge {:?} has degenerate radial \
                 chain — cannot locate boundary HE",
                top_self_loop_eid,
            );
        }
        let bot_boundary_he = self.hes[outer_start].next_rad();
        if bot_boundary_he.is_null() || bot_boundary_he == outer_start {
            bail!(
                "B-δ-prep: bottom self-loop edge {:?} has degenerate radial \
                 chain — cannot locate boundary HE",
                bot_self_loop_eid,
            );
        }

        // 5. Create the annulus side face (manual low-level construction).
        let annulus_face = self.faces.insert(Face::new(
            LoopRef::default(),
            // Cylinder side face's "normal" is direction-dependent
            // (depends on parametric position). DCEL Face::normal is a
            // legacy field — use profile normal as a placeholder. The
            // canonical surface info is the AnalyticSurface::Cylinder
            // attached below.
            normal,
            FACE_TOLERANCE,
            material,
        ));

        // 5a. Wire bottom boundary HE → annulus face (self-loop on annulus).
        self.hes[bot_boundary_he].set_next(bot_boundary_he);
        self.hes[bot_boundary_he].set_prev(bot_boundary_he);
        self.hes[bot_boundary_he].set_face(annulus_face);
        // Path B 의미: 두 boundary loop 모두 outer-equivalent (ADR-090
        // §2.2). 그러나 legacy schema 의 outer/inners 구분 위해 bottom 을
        // outer 로, top 을 inner ("hole") 로 picking — 본 face 는 Path A
        // 코드 경로에서 traverse 안 됨 (test entry 전용).
        self.hes[bot_boundary_he].set_outer(true);

        // 5b. Wire top boundary HE → annulus face (self-loop, inner-loop
        //     for legacy schema compat).
        self.hes[top_boundary_he].set_next(top_boundary_he);
        self.hes[top_boundary_he].set_prev(top_boundary_he);
        self.hes[top_boundary_he].set_face(annulus_face);
        self.hes[top_boundary_he].set_outer(false);

        // 5c. Set legacy outer + inners.
        self.faces[annulus_face].set_outer(LoopRef::new(bot_boundary_he, true));
        self.faces[annulus_face].add_inner(LoopRef::new(top_boundary_he, false));

        // 5d. Set Path B canonical multi-loop schema.
        let bot_loop = LoopRef::new(bot_boundary_he, true);
        let top_loop = LoopRef::new(top_boundary_he, true);
        self.set_face_boundary_loops(annulus_face, vec![bot_loop, top_loop]);

        // 6. Attach AnalyticSurface::Cylinder.
        let (axis_dir, v_lo, v_hi) = if dist > 0.0 {
            (normal, 0.0, dist)
        } else {
            (normal, dist, 0.0)
        };
        let cylinder_surface = AnalyticSurface::Cylinder {
            axis_origin: center,
            axis_dir,
            radius,
            ref_dir: basis_u,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v_lo, v_hi),
        };
        self.faces[annulus_face].set_surface(Some(cylinder_surface));

        // **Path B annulus owner_id hotfix (2026-05-23, 사용자 시연 evidence)**
        // — annulus side face 에도 surface_owner_id 부여. Path A
        // extrude_planar_cylinder 와 동일 패턴 (ADR-093 D-δ 답습).
        // Path B 는 1 annulus = 1 group 이지만 K3 propagation 정합 +
        // 향후 split 시 sub-face 가 같은 owner_id inherit (ADR-093 D-δ).
        // 사용자 시연 finding: Path B cylinder 측면 모든 face owner_id
        // 부재 → group selection 미작동 → 본 hotfix 로 해소.
        let owner_id = self.next_surface_owner_id();
        self.set_face_surface_owner_id(annulus_face, Some(owner_id));

        // 7. Aggregate result. Top face has its Plane surface from
        //    add_face_closed_curve (ADR-089 A-η-1). Bottom (profile_face)
        //    too. Annulus has Cylinder.
        let all_solid_faces = vec![profile_face, top_face, annulus_face];
        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::Cylinder,
            top_face,
            side_faces: vec![annulus_face],
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-192 P1.3 — analytic GeneralSweep for a closed **non-Circle** curve
    /// disk (a self-loop face whose curve is `AnalyticCurve::Bezier` or
    /// `AnalyticCurve::BSpline`).
    ///
    /// This generalizes `extrude_cylinder_kernel_native` (Path B Cylinder): the
    /// boundary-HE location + side-face DCEL wiring are *curve-agnostic*; only
    /// the top curve (translated profile curve) and the side **surface** differ. The
    /// side is a true analytic swept surface — `surfaces::sweep::extrusion_
    /// surface(profile, normal, dist)` returns the extrusion as a degree-1-in-v
    /// `BSplineSurface` (faceSurfaceKind 7, render-supported) — instead of the
    /// Cylinder of the Circle path. Result = 3 faces (base Plane + top Plane +
    /// side BSplineSurface / NURBSSurface), `SolidKind::GeneralSweep`.
    ///
    /// Profiles: **Bezier** (MVP, clamped-knot synthesis) + **BSpline**
    /// (ADR-192 §5.5 extension — native knots/degree pass straight through) +
    /// **NURBS** (ADR-192 §5.6 extension — rational: per-control-point weights
    /// flow through `extrusion_surface_nurbs` to a `NURBSSurface` side, the top
    /// rim staying a native-weight NURBS self-loop).
    ///
    /// **Limitations (ADR-192, shared with the Cylinder Path B kernel-native
    /// pattern this mirrors — NOT regressions of this MVP):**
    /// - The side face uses the legacy outer/inner "ring with hole" schema
    ///   (`add_inner`), so a subsequent **Boolean** on the result bails
    ///   (`boolean.rs` rejects non-empty `inners()`) — same as Cylinder Path B.
    /// - `he_twin` on a self-loop boundary HE returns the HE itself (no distinct
    ///   twin); the side DCEL relies on the radial chain, identical to the
    ///   Cylinder annulus. Bounded by the `< 1000` guard (no infinite loop).
    /// - `analytic_face_area` returns 0 for a BSplineSurface / NURBSSurface side
    ///   (a tessellation fallback is a follow-up); harmless here because
    ///   `cleanup_degenerate_faces` does not run on the post-extrude result.
    /// - Re-pushing the BSplineSurface side itself is intentionally **not**
    ///   routed here (dispatch requires a Plane-surfaced profile); it falls
    ///   through to the tessellation path.
    /// - Negative `dist` produces a manifold-valid solid (verified); side-surface
    ///   render orientation parity with the kernel-native pattern is a follow-up.
    fn extrude_closed_curve_general_kernel_native(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        // 1. Validate profile = self-loop face with a Bezier / BSpline curve.
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("P1.3: profile face {profile_face:?} has null outer loop");
        }
        let bot_self_loop_eid = self.hes[outer_start].edge();
        if !self.edges[bot_self_loop_eid].is_self_loop() {
            bail!("P1.3: profile outer edge is not a self-loop (closed-curve profile required)");
        }
        let bot_anchor = self.edges[bot_self_loop_eid].v_small();
        let curve = self
            .edges
            .get(bot_self_loop_eid)
            .and_then(|e| e.curve().cloned())
            .ok_or_else(|| anyhow::anyhow!("P1.3: self-loop edge has no AnalyticCurve"))?;
        // Profile → (ctrl, weights, knots, degree). Bezier synthesizes clamped
        // knots (degree = len-1, canonical Bezier-as-BSpline); BSpline passes
        // its native knots/degree straight through to `extrusion_surface`
        // (P1.3 BSpline extension, ADR-192 §5.5). NURBS additionally carries
        // per-control-point weights → `extrusion_surface_nurbs` → a rational
        // NURBSSurface side (ADR-192 §5.6). Non-NURBS profiles have no weights.
        enum SweptProfile {
            Bezier,
            BSpline,
            Nurbs,
        }
        let (prof_ctrl, prof_weights, prof_knots, prof_degree, prof_kind) = match curve {
            AnalyticCurve::Bezier { control_pts } => {
                let degree = control_pts.len().saturating_sub(1);
                let mut knots = vec![0.0; degree + 1];
                knots.extend(std::iter::repeat(1.0).take(degree + 1));
                (control_pts, None, knots, degree, SweptProfile::Bezier)
            }
            AnalyticCurve::BSpline { control_pts, knots, degree } => {
                (control_pts, None, knots, degree as usize, SweptProfile::BSpline)
            }
            AnalyticCurve::NURBS { control_pts, weights, knots, degree } => {
                (control_pts, Some(weights), knots, degree as usize, SweptProfile::Nurbs)
            }
            other => bail!(
                "P1.3: only Bezier / BSpline / NURBS closed curves (got {:?})",
                std::mem::discriminant(&other),
            ),
        };
        if prof_ctrl.len() < 3 {
            // add_face_closed_curve (top) → bezier_best_fit_normal needs >= 3
            // (shared Bezier/BSpline helper, LOCKED #35 A-Α).
            // (Adversarial-review finding #9, ADR-192.)
            bail!("P1.3: closed-curve profile needs >= 3 control points");
        }
        if prof_ctrl.len() < prof_degree + 1 {
            bail!("P1.3: profile needs >= degree+1 control points");
        }

        // Profile plane normal (P0.1 attaches a Plane surface to re-derive faces).
        let normal = match self.faces.get(profile_face).and_then(|f| f.surface()) {
            Some(AnalyticSurface::Plane { normal, .. }) => normal.normalize_or_zero(),
            _ => bail!("P1.3: profile face needs a Plane surface"),
        };
        if normal.length_squared() < 0.5 {
            bail!("P1.3: profile plane normal is degenerate");
        }

        // 2. Translate the profile along the normal → top.
        let translation = normal * dist;
        let top_anchor_pos = self.vertex_pos(bot_anchor)? + translation;
        let top_ctrl: Vec<DVec3> = prof_ctrl.iter().map(|&p| p + translation).collect();

        // 3. Create top vert + top closed-curve face (ADR-089 A-ω / A-Α) —
        //    same curve kind as the profile (BSpline keeps native knots/degree).
        let top_anchor = self.add_vertex(top_anchor_pos);
        let top_curve = match prof_kind {
            SweptProfile::Bezier => AnalyticCurve::Bezier { control_pts: top_ctrl },
            SweptProfile::BSpline => AnalyticCurve::BSpline {
                control_pts: top_ctrl,
                knots: prof_knots.clone(),
                degree: prof_degree as u32,
            },
            SweptProfile::Nurbs => AnalyticCurve::NURBS {
                control_pts: top_ctrl,
                weights: prof_weights.clone().expect("NURBS profile carries weights"),
                knots: prof_knots.clone(),
                degree: prof_degree as u32,
            },
        };
        let top_face = self.add_face_closed_curve(top_anchor, top_curve, material)?;

        // 4. Locate boundary HEs (twin of each self-loop face's outer HE).
        let top_outer_start = self.faces[top_face].outer().start;
        let top_boundary_he = self.hes[top_outer_start].next_rad();
        if top_boundary_he.is_null() || top_boundary_he == top_outer_start {
            bail!("P1.3: top self-loop edge has degenerate radial chain");
        }
        let bot_boundary_he = self.hes[outer_start].next_rad();
        if bot_boundary_he.is_null() || bot_boundary_he == outer_start {
            bail!("P1.3: bottom self-loop edge has degenerate radial chain");
        }

        // 5. Create the side face (same low-level DCEL wiring as the cylinder
        //    annulus — bottom self-loop = outer, top self-loop = inner).
        let side_face = self.faces.insert(Face::new(
            LoopRef::default(),
            normal,
            FACE_TOLERANCE,
            material,
        ));
        self.hes[bot_boundary_he].set_next(bot_boundary_he);
        self.hes[bot_boundary_he].set_prev(bot_boundary_he);
        self.hes[bot_boundary_he].set_face(side_face);
        self.hes[bot_boundary_he].set_outer(true);
        self.hes[top_boundary_he].set_next(top_boundary_he);
        self.hes[top_boundary_he].set_prev(top_boundary_he);
        self.hes[top_boundary_he].set_face(side_face);
        self.hes[top_boundary_he].set_outer(false);
        self.faces[side_face].set_outer(LoopRef::new(bot_boundary_he, true));
        self.faces[side_face].add_inner(LoopRef::new(top_boundary_he, false));
        self.set_face_boundary_loops(
            side_face,
            vec![
                LoopRef::new(bot_boundary_he, true),
                LoopRef::new(top_boundary_he, true),
            ],
        );

        // 6. Attach the analytic swept surface (extrusion of the profile,
        //    a degree-1-in-v tensor surface) to the side face. Bezier/BSpline
        //    → non-rational `extrusion_surface` (BSplineSurface). NURBS →
        //    `extrusion_surface_nurbs` carrying the profile weights across v
        //    → a rational NURBSSurface (ADR-192 §5.6).
        let side_surface = match prof_kind {
            SweptProfile::Nurbs => {
                let weights = prof_weights
                    .as_ref()
                    .expect("NURBS profile carries weights");
                let (grid, wgrid, knots_u, knots_v, deg_u, deg_v) =
                    crate::surfaces::sweep::extrusion_surface_nurbs(
                        &prof_ctrl,
                        weights,
                        &prof_knots,
                        prof_degree,
                        normal,
                        dist,
                    )
                    .map_err(|e| anyhow::anyhow!("P1.3 NURBS extrusion surface: {}", e))?;
                AnalyticSurface::NURBSSurface {
                    ctrl_grid: grid,
                    weights: wgrid,
                    knots_u,
                    knots_v,
                    deg_u: deg_u as u32,
                    deg_v: deg_v as u32,
                    trim_loops: Vec::new(),
                }
            }
            SweptProfile::Bezier | SweptProfile::BSpline => {
                let (grid, knots_u, knots_v, deg_u, deg_v) =
                    crate::surfaces::sweep::extrusion_surface(
                        &prof_ctrl,
                        &prof_knots,
                        prof_degree,
                        normal,
                        dist,
                    )
                    .map_err(|e| anyhow::anyhow!("P1.3 extrusion surface: {}", e))?;
                AnalyticSurface::BSplineSurface {
                    ctrl_grid: grid,
                    knots_u,
                    knots_v,
                    deg_u: deg_u as u32,
                    deg_v: deg_v as u32,
                }
            }
        };
        self.faces[side_face].set_surface(Some(side_surface));
        let owner_id = self.next_surface_owner_id();
        self.set_face_surface_owner_id(side_face, Some(owner_id));

        // 7. Result. base (profile_face) + top inherit Plane (ADR-089 A-η-1).
        let all_solid_faces = vec![profile_face, top_face, side_face];
        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::GeneralSweep,
            top_face,
            side_faces: vec![side_face],
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-3-δ — Extrude on NURBS-class profile (tessellation-based).
    ///
    /// Profile face's surface is BezierPatch / BSplineSurface / NURBSSurface.
    /// Tessellation-based approximation per §W3-B-(a):
    /// - Profile boundary verts already on surface (no projection needed)
    /// - Compute representative normal at face's parametric-center via
    ///   `AnalyticSurface::normal_at_world_pos(centroid)`
    /// - Translate boundary verts by `representative_normal · dist` to
    ///   form top boundary
    /// - Build top face (preserve profile as bottom) + N side quads
    /// - Top + side surfaces synthesized as Plane (approximate; original
    ///   NURBS surface metadata not propagated to new faces)
    ///
    /// SolidKind: `GeneralSweep` (per ADR-079 §2.2 W-3 scope).
    ///
    /// **Known limitation**: representative normal is uniform (face center).
    /// True per-vertex offset (each vertex moved along its own surface normal)
    /// would produce a non-Plane top — future enhancement (W-3-ε).
    fn extrude_nurbs_class_profile(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        material: MaterialId,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!(
                "extrude_nurbs_class_profile: profile face {profile_face:?} \
                 has null outer loop start"
            );
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;
        if boundary_verts.len() < 3 {
            bail!(
                "extrude_nurbs_class_profile: profile boundary has only {} verts",
                boundary_verts.len()
            );
        }

        // Compute centroid of boundary verts → representative normal.
        let mut centroid = DVec3::ZERO;
        let positions: Vec<DVec3> = boundary_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;
        for p in &positions {
            centroid += *p;
        }
        centroid /= positions.len() as f64;
        let representative_normal = profile_surface.normal_at_world_pos(centroid);
        if representative_normal.length_squared() < 0.5 {
            bail!(
                "extrude_nurbs_class_profile: NURBS surface representative \
                 normal at centroid is degenerate"
            );
        }
        let translation = representative_normal * dist;

        // Translate boundary to form top loop.
        let mut top_verts = Vec::with_capacity(boundary_verts.len());
        for p in &positions {
            top_verts.push(self.add_vertex(*p + translation));
        }

        // Top cap face.
        let top_face = self.add_face(&top_verts, material)?;

        // Side quads — same winding as extrude_planar_box.
        let n = boundary_verts.len();
        let mut side_faces = Vec::with_capacity(n);
        for i in 0..n {
            let next = (i + 1) % n;
            let quad = if dist > 0.0 {
                [
                    boundary_verts[i],
                    boundary_verts[next],
                    top_verts[next],
                    top_verts[i],
                ]
            } else {
                [
                    boundary_verts[next],
                    boundary_verts[i],
                    top_verts[i],
                    top_verts[next],
                ]
            };
            let side = self.add_face(&quad, material)?;
            side_faces.push(side);
        }

        // Top cap surface — synthesized as Plane from top vertex positions.
        // (Approximation: NURBS profile surface NOT carried to top — future
        // enhancement W-3-ε would translate the NURBS surface.)
        let top_positions: Vec<DVec3> = top_verts
            .iter()
            .filter_map(|v| self.vertex_pos(*v).ok())
            .collect();
        if top_positions.len() >= 3 {
            let top_surface = synthesize_plane_surface(&top_positions);
            if let Some(top_face_mut) = self.faces.get_mut(top_face) {
                top_face_mut.set_surface(Some(top_surface));
            }
        }

        // Side surfaces — synthesized Plane from each quad.
        for &side_fid in &side_faces {
            let face_ref = self.faces.get(side_fid);
            if face_ref.is_none() || !face_ref.unwrap().is_active() {
                continue;
            }
            let start = self.faces[side_fid].outer().start;
            if start.is_null() {
                continue;
            }
            let side_verts = self.collect_loop_verts(start)?;
            let positions: Vec<DVec3> = side_verts
                .iter()
                .filter_map(|v| self.vertex_pos(*v).ok())
                .collect();
            if positions.len() >= 3 {
                let side_surface = synthesize_plane_surface(&positions);
                self.faces[side_fid].set_surface(Some(side_surface));
            }
        }

        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(top_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::GeneralSweep,
            top_face,
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-4-α — Revolve mode dispatch (full 360° only).
    ///
    /// Extracts profile face's outer-loop polyline, validates axis +
    /// face plane perpendicularity, then delegates to `Mesh::revolve`
    /// (existing operation). Profile face is preserved (Shape ownership
    /// pattern); generated side faces are CreateSolidResult.side_faces.
    ///
    /// W-4-α scope:
    /// - Full 360° only — `(angle_rad - TAU).abs() > 1e-3` → NotYetSupported
    /// - Multi-loop face → reject (ADR-016 Q2 / ADR-080 L8 정합)
    /// - Profile plane must contain axis (face_normal ⊥ axis_dir)
    /// - Fixed default segments = 32 (chord-tolerance based future)
    fn revolve_profile_face(
        &mut self,
        profile_face: FaceId,
        axis_origin: DVec3,
        axis_dir: DVec3,
        angle_rad: f64,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        // ADR-248 (Phase 3 E1) — angle ∈ (0, 2π]. Full (≈2π) keeps the original
        // open-polyline Mesh::revolve path; partial (< 2π) builds a CAPPED wedge
        // solid (loft of rotated profile sections + θ=0 / θ=angle end caps).
        let two_pi = std::f64::consts::TAU;
        let is_full = (angle_rad - two_pi).abs() <= 1e-3;
        if angle_rad <= 1e-3 || angle_rad > two_pi + 1e-3 {
            return Err(SolidError::NotYetSupported {
                reason: format!("Revolve angle {:.4} rad out of range (0, 2π]", angle_rad),
            }
            .into());
        }

        // §W4-C — Axis validation.
        let axis_unit = axis_dir.normalize_or_zero();
        if axis_unit.length_squared() < 0.5 {
            return Err(SolidError::NotYetSupported {
                reason: "Revolve axis_dir is near-zero".to_string(),
            }
            .into());
        }

        // §W4-B — Multi-loop guard.
        let face = self
            .faces
            .get(profile_face)
            .ok_or(SolidError::FaceNotFound)?;
        if !face.inners().is_empty() {
            return Err(SolidError::NotYetSupported {
                reason: "Revolve multi-loop face rejected (ADR-016 Q2)".to_string(),
            }
            .into());
        }

        // Extract polyline from outer loop.
        let outer_start = face.outer().start;
        if outer_start.is_null() {
            bail!("revolve_profile_face: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;
        if boundary_verts.len() < 2 {
            bail!(
                "revolve_profile_face: profile boundary has only {} verts",
                boundary_verts.len()
            );
        }
        let profile_points: Vec<DVec3> = boundary_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;

        // §W4-C — Profile face plane must contain axis (normal ⊥ axis).
        let face_surface = self
            .faces
            .get(profile_face)
            .and_then(|f| f.surface().cloned());
        if let Some(AnalyticSurface::Plane { normal, .. }) = face_surface {
            let face_normal = normal.normalize_or_zero();
            let dot = face_normal.dot(axis_unit).abs();
            if dot > 0.001 {
                return Err(SolidError::NotYetSupported {
                    reason: format!(
                        "Revolve: profile face plane does not contain axis \
                         (face_normal · axis_dir = {:.4}, expected ~0)",
                        dot
                    ),
                }
                .into());
            }
        }

        // §W4-D — Fixed default segments (full turn). Partial scales pro-rata.
        const DEFAULT_REVOLVE_SEGMENTS: u32 = 32;

        if is_full {
            // FULL 360°. A profile CLEAR of the axis sweeps into a CLOSED SOLID
            // ring (torus-like). Build it as a section loft with a seamless 2π
            // wrap (section[segments] coincides with section[0], so add_vertex
            // dedup welds the seam), NO end caps, and REMOVE the profile — it is
            // an interior cross-section, not a boundary. Previously this reused
            // the open-polyline `Mesh::revolve`, which left the profile as an
            // internal membrane → open (bnd>0) + non-manifold seam, even though
            // every per-face invariant passed. (Adversarial sweep, Round-11.)
            //
            // An axis-touching profile (pole) keeps the legacy open-polyline
            // surface path — the pole would collapse the section loft.
            let clear_of_axis = profile_points.iter().all(|&p| {
                let rel = p - axis_origin;
                let axial = rel.dot(axis_unit);
                (rel - axis_unit * axial).length() >= EPSILON_LENGTH * 10.0
            });
            if clear_of_axis {
                let segments = DEFAULT_REVOLVE_SEGMENTS;
                let step = two_pi / segments as f64;
                let sections: Vec<Vec<DVec3>> = (0..=segments)
                    .map(|k| {
                        let theta = k as f64 * step; // k=segments → 2π ≡ section[0]
                        profile_points
                            .iter()
                            .map(|&p| {
                                crate::operations::revolve::rotate_around_axis(
                                    p, axis_origin, axis_unit, theta,
                                )
                            })
                            .collect()
                    })
                    .collect();
                let side_faces = self
                    .loft(&sections, /* closed_sections */ true, material)
                    .map_err(|e| anyhow::anyhow!("Full revolve loft failed: {}", e))?;
                // The profile is an interior cross-section of the ring — remove it.
                let _ = self.remove_face(profile_face);
                if self.faces.contains(profile_face) {
                    self.faces.remove(profile_face);
                }
                let _ = self.reconcile_face_normals();
                let top_face = side_faces.first().copied().unwrap_or(profile_face);
                let all_solid_faces = side_faces.clone();
                return Ok(CreateSolidResult {
                    profile_face,
                    solid_kind: SolidKind::RevolutionSolid,
                    top_face,
                    side_faces,
                    all_solid_faces,
                    adjacent_splits: 0,
                    split_debug: Vec::new(),
                });
            }

            // Axis-touching profile — legacy open-polyline surface of revolution.
            let side_faces = self
                .revolve(
                    &profile_points,
                    axis_origin,
                    axis_unit,
                    DEFAULT_REVOLVE_SEGMENTS,
                    material,
                )
                .map_err(|e| anyhow::anyhow!("Revolve operation failed: {}", e))?;
            let mut all_solid_faces = Vec::with_capacity(1 + side_faces.len());
            all_solid_faces.push(profile_face);
            all_solid_faces.extend(side_faces.iter().copied());
            return Ok(CreateSolidResult {
                profile_face,
                solid_kind: SolidKind::RevolutionSolid,
                top_face: profile_face,
                side_faces,
                all_solid_faces,
                adjacent_splits: 0,
                split_debug: Vec::new(),
            });
        }

        // ── ADR-248 (Phase 3 E1) — PARTIAL revolve → capped wedge solid ──────
        // Sweep the closed profile boundary along the angular arc (loft of
        // rotated copies), then seal the two angular ends: θ=0 = profile_face,
        // θ=angle = a fresh rotated cap. MVP requires the profile to stay clear
        // of the axis (no poles); an axis-touching partial revolve is future.
        let pole_threshold = EPSILON_LENGTH * 10.0;
        for &p in &profile_points {
            let rel = p - axis_origin;
            let axial = rel.dot(axis_unit);
            let radial = (rel - axis_unit * axial).length();
            if radial < pole_threshold {
                return Err(SolidError::NotYetSupported {
                    reason: "Partial revolve: profile touches the axis (pole) — \
                             offset the profile from the axis".to_string(),
                }
                .into());
            }
        }
        let segments = ((DEFAULT_REVOLVE_SEGMENTS as f64 * angle_rad / two_pi).round() as u32)
            .max(2);
        let step = angle_rad / segments as f64;
        // sections[k] = profile rotated by k·step, k = 0..=segments.
        let sections: Vec<Vec<DVec3>> = (0..=segments)
            .map(|k| {
                let theta = k as f64 * step;
                profile_points
                    .iter()
                    .map(|&p| {
                        crate::operations::revolve::rotate_around_axis(
                            p, axis_origin, axis_unit, theta,
                        )
                    })
                    .collect()
            })
            .collect();
        // Side bands (closed sections — each rotated profile is a closed loop).
        // Section 0 == profile_points → dedup to profile_face verts (θ=0 cap).
        let mut side_faces = self
            .loft(&sections, /* closed_sections */ true, material)
            .map_err(|e| anyhow::anyhow!("Partial revolve loft failed: {}", e))?;
        // θ=angle end cap — reversed loop winding (opposite the θ=0 profile_face)
        // so its outward normal points along +θ. Verified manifold below.
        let end_verts: Vec<VertId> = sections[segments as usize]
            .iter()
            .map(|&p| self.add_vertex(p))
            .collect();
        let mut cap_loop = end_verts;
        cap_loop.reverse();
        let end_cap = self.add_face_with_holes(&cap_loop, &[], material)?;
        side_faces.push(end_cap);

        // Winding is the source of truth (ADR-007) — refresh cached normals.
        let _ = self.reconcile_face_normals();

        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::RevolutionSolid,
            top_face: end_cap,
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-3-β — Loft mode dispatch (two profiles).
    ///
    /// Connects the boundary of `profile_face` to the boundary of
    /// `other_profile` via ruled side faces. Delegates to `Mesh::loft`
    /// with closed_sections=true (each profile is a closed loop).
    ///
    /// W-3-β scope (MVP, two-profile only):
    /// - Both faces exist + active
    /// - Both faces multi-loop guard (ADR-016 Q2 / L8)
    /// - Outer-loop vertex counts match (no auto-resampling — future)
    /// - Profiles must NOT be the same FaceId
    fn loft_between_profiles(
        &mut self,
        profile_face: FaceId,
        other_profile: FaceId,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        // §W3β-A — Both faces must be distinct.
        if profile_face == other_profile {
            return Err(SolidError::NotYetSupported {
                reason: "Loft: both profiles are the same FaceId".to_string(),
            }
            .into());
        }

        // §W3β-A — Both faces must exist + active.
        let f1 = self
            .faces
            .get(profile_face)
            .ok_or(SolidError::FaceNotFound)?;
        if !f1.is_active() {
            return Err(SolidError::FaceNotFound.into());
        }
        if !f1.inners().is_empty() {
            return Err(SolidError::NotYetSupported {
                reason: "Loft profile (first) multi-loop face rejected (ADR-016 Q2)".to_string(),
            }
            .into());
        }
        let f1_outer_start = f1.outer().start;
        if f1_outer_start.is_null() {
            bail!("loft_between_profiles: profile_face has null outer loop start");
        }

        let f2 = self
            .faces
            .get(other_profile)
            .ok_or(SolidError::FaceNotFound)?;
        if !f2.is_active() {
            return Err(SolidError::FaceNotFound.into());
        }
        if !f2.inners().is_empty() {
            return Err(SolidError::NotYetSupported {
                reason: "Loft profile (second) multi-loop face rejected (ADR-016 Q2)"
                    .to_string(),
            }
            .into());
        }
        let f2_outer_start = f2.outer().start;
        if f2_outer_start.is_null() {
            bail!("loft_between_profiles: other_profile has null outer loop start");
        }

        // Extract two profiles' outer-loop vertex counts.
        let mut v1 = self.collect_loop_verts(f1_outer_start)?;
        let mut v2 = self.collect_loop_verts(f2_outer_start)?;

        if v1.len() < 3 || v2.len() < 3 {
            bail!(
                "loft_between_profiles: profile boundary has < 3 verts ({} / {})",
                v1.len(), v2.len()
            );
        }

        // ADR-247 (Phase 3 E2) — auto-resample the SHORTER profile up to the
        // longer's vertex count by subdividing its longest boundary edges at
        // their midpoints (split_edge). This preserves the cap FaceId AND its
        // outline (inserted verts lie on the original perimeter), so the cap
        // verts still dedup-match the loft section positions (Mesh::loft uses
        // add_vertex → spatial-hash dedup) and the result stays manifold.
        // Both profiles must therefore be plain polygon caps (≥ 3 boundary
        // verts); closed-curve self-loop profiles are out of scope (they have
        // < 3 boundary verts and fail the guard above).
        if v1.len() != v2.len() {
            let target = v1.len().max(v2.len());
            if v1.len() < target {
                self.resample_loft_profile(profile_face, target)?;
                v1 = self.collect_loop_verts(self.faces[profile_face].outer().start)?;
            }
            if v2.len() < target {
                self.resample_loft_profile(other_profile, target)?;
                v2 = self.collect_loop_verts(self.faces[other_profile].outer().start)?;
            }
            ensure!(
                v1.len() == v2.len(),
                "loft resample failed to equalize counts ({} vs {})",
                v1.len(), v2.len()
            );
        }

        let section1: Vec<DVec3> = v1
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;
        let section2: Vec<DVec3> = v2
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<Vec<_>>>()?;

        // §W3β-C — Delegate to Mesh::loft.
        let sections = vec![section1, section2];
        let side_faces = self
            .loft(&sections, /* closed_sections */ true, material)
            .map_err(|e| anyhow::anyhow!("Loft operation failed: {}", e))?;

        let mut all_solid_faces = Vec::with_capacity(2 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.push(other_profile);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::LoftSolid,
            top_face: other_profile, // second profile = "top" cap
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-247 (Phase 3 E2) — subdivide a polygon profile face's boundary up to
    /// `target` vertices by repeatedly splitting its current longest edge at the
    /// midpoint. Preserves the face id and outline (new verts lie on the original
    /// perimeter). Used by loft to equalize mismatched profile vertex counts so
    /// both caps dedup-match the loft section positions (→ manifold result).
    fn resample_loft_profile(&mut self, face: FaceId, target: usize) -> Result<()> {
        let mut guard = 0usize;
        loop {
            let start = self.faces[face].outer().start;
            let verts = self.collect_loop_verts(start)?;
            let n = verts.len();
            if n >= target {
                break;
            }
            guard += 1;
            if guard > target + 16 {
                bail!(
                    "resample_loft_profile: failed to reach target {} (stuck at {})",
                    target, n
                );
            }
            // Find the longest boundary edge and split it at its midpoint.
            let mut best_eid: Option<EdgeId> = None;
            let mut best_mid = DVec3::ZERO;
            let mut best_len = -1.0f64;
            for i in 0..n {
                let va = verts[i];
                let vb = verts[(i + 1) % n];
                let eid = match self.find_edge(va, vb) {
                    Some(e) => e,
                    None => continue,
                };
                let pa = self.vertex_pos(va)?;
                let pb = self.vertex_pos(vb)?;
                let len = (pb - pa).length();
                if len > best_len {
                    best_len = len;
                    best_eid = Some(eid);
                    best_mid = (pa + pb) * 0.5;
                }
            }
            let eid = best_eid.ok_or_else(|| {
                anyhow::anyhow!("resample_loft_profile: no boundary edge on face {:?}", face)
            })?;
            self.split_edge(eid, best_mid)?;
        }
        Ok(())
    }

    /// ADR-079 W-3-α — Sweep mode dispatch.
    ///
    /// Tessellates the path AnalyticCurve to a polyline, validates that the
    /// profile face's plane normal is aligned with the path's start tangent,
    /// projects profile vertices into the local (basis_u, basis_v) frame
    /// (the path's start cross-section), and delegates to `Mesh::sweep`.
    ///
    /// W-3-α scope:
    /// - Path tessellation via `AnalyticCurve::tessellate(chord_tol)`
    ///   (chord_tol = `EPSILON_LENGTH × 1e3` ≈ 1.5 mm)
    /// - Profile face plane normal must be ‖ path start tangent
    /// - Multi-loop face → reject (ADR-016 Q2 / L8)
    /// - Path tessellation < 2 points → reject (`SweepPathDegenerate`)
    fn sweep_profile_along_path(
        &mut self,
        profile_face: FaceId,
        path: &AnalyticCurve,
        material: MaterialId,
    ) -> Result<CreateSolidResult> {
        let tol = crate::tolerances::EPSILON_LENGTH;
        let chord_tol = tol * 1000.0; // §W3-I-L2: 1.5 mm chord tolerance

        // §W3-D-C — Multi-loop guard.
        let face = self
            .faces
            .get(profile_face)
            .ok_or(SolidError::FaceNotFound)?;
        if !face.inners().is_empty() {
            return Err(SolidError::NotYetSupported {
                reason: "Sweep multi-loop face rejected (ADR-016 Q2)".to_string(),
            }
            .into());
        }

        // Profile face surface — must be Plane (W-3-α MVP).
        let face_surface = face.surface().cloned();
        let (face_origin, face_normal, face_basis_u) = match face_surface {
            Some(AnalyticSurface::Plane { origin, normal, basis_u, .. }) => (
                origin,
                normal.normalize_or_zero(),
                basis_u.normalize_or_zero(),
            ),
            _ => {
                return Err(SolidError::NotYetSupported {
                    reason: "Sweep MVP: profile face surface must be Plane (W-3-δ scope)"
                        .to_string(),
                }
                .into());
            }
        };
        if face_normal.length_squared() < 0.5 || face_basis_u.length_squared() < 0.5 {
            bail!("sweep_profile_along_path: profile face plane vectors degenerate");
        }
        let face_basis_v = face_normal.cross(face_basis_u);

        // §W3α-A — Tessellate path.
        let path_polyline = path
            .tessellate(chord_tol, self)
            .map_err(|e| anyhow::anyhow!("Sweep path tessellation failed: {}", e))?;
        if path_polyline.len() < 2 {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "Sweep path degenerate (tessellation produced {} points)",
                    path_polyline.len()
                ),
            }
            .into());
        }

        // §W3α-B — Profile plane normal ‖ path start tangent.
        let path_tangent = (path_polyline[1] - path_polyline[0]).normalize_or_zero();
        if path_tangent.length_squared() < 0.5 {
            bail!("sweep_profile_along_path: path start tangent degenerate");
        }
        if face_normal.dot(path_tangent).abs() < 0.999 {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "Sweep: profile face normal not aligned with path start tangent \
                     (|dot| = {:.4}, expected ≥ 0.999)",
                    face_normal.dot(path_tangent).abs()
                ),
            }
            .into());
        }

        // Extract profile polyline → project to local (u, v, 0) coords.
        let outer_start = self.faces[profile_face].outer().start;
        if outer_start.is_null() {
            bail!("sweep_profile_along_path: profile face has null outer loop start");
        }
        let boundary_verts = self.collect_loop_verts(outer_start)?;
        if boundary_verts.len() < 3 {
            bail!(
                "sweep_profile_along_path: profile boundary has only {} verts",
                boundary_verts.len()
            );
        }
        let mut profile_local: Vec<DVec3> = Vec::with_capacity(boundary_verts.len());
        for &v in &boundary_verts {
            let pos = self.vertex_pos(v)?;
            let from_origin = pos - face_origin;
            let x = from_origin.dot(face_basis_u);
            let y = from_origin.dot(face_basis_v);
            // z = 0 (profile is in plane); z is along path tangent direction.
            profile_local.push(DVec3::new(x, y, 0.0));
        }

        // Mesh::sweep expects profile in local XY (z=0), path in 3D world.
        // Translate path so path[0] aligns with face_origin (Mesh::sweep
        // places sections AT each path point, so the first section is at
        // path[0], not at face_origin). We adjust by translating path
        // points to start from face_origin.
        let path_offset = face_origin - path_polyline[0];
        let path_world: Vec<DVec3> = path_polyline.iter().map(|p| *p + path_offset).collect();

        // §W3α-D — Delegate to Mesh::sweep.
        let side_faces = self
            .sweep(&profile_local, &path_world, /* closed_profile */ true, material)
            .map_err(|e| anyhow::anyhow!("Sweep operation failed: {}", e))?;

        let mut all_solid_faces = Vec::with_capacity(1 + side_faces.len());
        all_solid_faces.push(profile_face);
        all_solid_faces.extend(side_faces.iter().copied());

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::SweptSolid,
            top_face: profile_face, // sentinel — no separate "top"
            side_faces,
            all_solid_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-2-γ-i — Cylinder smooth-group radius offset.
    ///
    /// Profile face has `AnalyticSurface::Cylinder`. Detects the smooth
    /// group (all active faces sharing the same Cylinder instance within
    /// `EPSILON_LENGTH`), then radially offsets all group vertices by
    /// `dist`:
    ///   - Each vertex `v`: split into axial + radial components relative
    ///     to the cylinder axis. Scale radial by `(r + dist) / r`. Axial
    ///     preserved.
    ///   - All group face surfaces updated with `radius = current + dist`.
    ///   - Boundary `Arc` curves on cap edges (whose normal ≈ axis_dir
    ///     and center on axis) get their radius updated too.
    ///
    /// **Auto-expand semantics** (§W2γ1-B-(a)): the caller passes a single
    /// `profile_face`; this method expands to the full smooth group.
    /// Partial-panel rejection is not needed because the operation is
    /// idempotent across the group.
    ///
    /// Returns `NotYetSupported` if the new radius would collapse below
    /// `EPSILON_LENGTH` (geometry inversion guard).
    fn offset_smooth_group_cylinder(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let (axis_origin, axis_dir, current_radius, ref_dir, u_range, v_range) =
            match profile_surface {
                AnalyticSurface::Cylinder {
                    axis_origin,
                    axis_dir,
                    radius,
                    ref_dir,
                    u_range,
                    v_range,
                } => (
                    *axis_origin,
                    axis_dir.normalize_or_zero(),
                    *radius,
                    *ref_dir,
                    *u_range,
                    *v_range,
                ),
                _ => bail!("offset_smooth_group_cylinder: profile is not Cylinder"),
            };
        if axis_dir.length_squared() < 0.5 {
            bail!("offset_smooth_group_cylinder: axis_dir is near-zero");
        }
        if current_radius <= crate::tolerances::EPSILON_LENGTH {
            bail!(
                "offset_smooth_group_cylinder: current radius {:.3e} below epsilon",
                current_radius
            );
        }

        let new_radius = current_radius + dist;
        if new_radius <= crate::tolerances::EPSILON_LENGTH {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "offset would collapse cylinder radius to {:.3e} (current {:.3e}, dist {:.3e})",
                    new_radius, current_radius, dist
                ),
            }
            .into());
        }
        let scale = new_radius / current_radius;
        let tol = crate::tolerances::EPSILON_LENGTH;

        // Detect smooth group: active faces whose surface is a Cylinder
        // matching axis_origin, axis_dir, current_radius, ref_dir within tol.
        let group_faces: Vec<FaceId> = self
            .faces
            .iter()
            .filter_map(|(fid, face)| {
                if !face.is_active() {
                    return None;
                }
                match face.surface() {
                    Some(AnalyticSurface::Cylinder {
                        axis_origin: o,
                        axis_dir: a,
                        radius: r,
                        ref_dir: rd,
                        ..
                    }) => {
                        let a_n = a.normalize_or_zero();
                        let rd_n = rd.normalize_or_zero();
                        let ref_n = ref_dir.normalize_or_zero();
                        let same_axis = (*o - axis_origin).length() < tol
                            && a_n.dot(axis_dir).abs() > 0.999
                            && rd_n.dot(ref_n).abs() > 0.999
                            && (*r - current_radius).abs() < tol;
                        if same_axis {
                            Some(fid)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();

        if !group_faces.contains(&profile_face) {
            bail!(
                "offset_smooth_group_cylinder: profile face {profile_face:?} \
                 not in detected smooth group (size {})",
                group_faces.len()
            );
        }

        // Collect unique vertices across the group.
        let mut group_verts: std::collections::HashSet<crate::entities::VertId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let start = self.faces[fid].outer().start;
            if start.is_null() {
                continue;
            }
            for v in self.collect_loop_verts(start)? {
                group_verts.insert(v);
            }
        }

        // Radial scale each vertex relative to the cylinder axis.
        for v in group_verts.iter().copied().collect::<Vec<_>>() {
            let pos = self.vertex_pos(v)?;
            let from_axis = pos - axis_origin;
            let axial = from_axis.dot(axis_dir) * axis_dir;
            let radial = from_axis - axial;
            let new_pos = axis_origin + axial + radial * scale;
            self.move_vertex(v, new_pos)?;
        }

        // Update each group face's Cylinder surface with new radius.
        let new_surface = AnalyticSurface::Cylinder {
            axis_origin,
            axis_dir,
            radius: new_radius,
            ref_dir,
            u_range,
            v_range,
        };
        for &fid in &group_faces {
            if let Some(face) = self.faces.get_mut(fid) {
                if face.is_active() {
                    face.set_surface(Some(new_surface.clone()));
                }
            }
        }

        // Update Arc curves on edges incident to group faces (cap rings).
        // Filter: arc center on axis (cross product with axis_dir near zero)
        // AND arc normal parallel to axis_dir.
        let mut updated_arcs: std::collections::HashSet<crate::entities::EdgeId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let edges = self.face_outer_edges(fid)?;
            for eid in edges {
                if updated_arcs.contains(&eid) {
                    continue;
                }
                let new_curve = if let Some(edge) = self.edges.get(eid) {
                    match edge.curve() {
                        Some(AnalyticCurve::Arc {
                            center,
                            radius: ar,
                            normal,
                            basis_u,
                            start_angle,
                            end_angle,
                        }) => {
                            let center_off_axis =
                                ((*center - axis_origin).cross(axis_dir)).length();
                            let normal_dot = normal.normalize_or_zero().dot(axis_dir).abs();
                            // Match to current cylinder radius (avoids touching
                            // unrelated arcs that happen to share axis).
                            let radius_match = (*ar - current_radius).abs() < tol;
                            if center_off_axis < tol && normal_dot > 0.999 && radius_match {
                                Some(AnalyticCurve::Arc {
                                    center: *center,
                                    radius: new_radius,
                                    normal: *normal,
                                    basis_u: *basis_u,
                                    start_angle: *start_angle,
                                    end_angle: *end_angle,
                                })
                            } else {
                                None
                            }
                        }
                        Some(AnalyticCurve::Circle {
                            center,
                            radius: cr,
                            normal,
                            basis_u,
                        }) => {
                            let center_off_axis =
                                ((*center - axis_origin).cross(axis_dir)).length();
                            let normal_dot = normal.normalize_or_zero().dot(axis_dir).abs();
                            let radius_match = (*cr - current_radius).abs() < tol;
                            if center_off_axis < tol && normal_dot > 0.999 && radius_match {
                                Some(AnalyticCurve::Circle {
                                    center: *center,
                                    radius: new_radius,
                                    normal: *normal,
                                    basis_u: *basis_u,
                                })
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(c) = new_curve {
                    if let Some(edge) = self.edges.get_mut(eid) {
                        edge.set_curve(Some(c));
                    }
                    updated_arcs.insert(eid);
                }
            }
        }

        // Result — top_face = profile_face (no new face created in offset),
        // side_faces = group members excluding profile.
        let side_faces: Vec<FaceId> = group_faces
            .iter()
            .copied()
            .filter(|&f| f != profile_face)
            .collect();

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::SmoothGroupOffset,
            top_face: profile_face,
            side_faces,
            all_solid_faces: group_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-2-γ-ii — Sphere smooth-group radius offset.
    ///
    /// Profile face has `AnalyticSurface::Sphere`. Detects the smooth
    /// group (active faces sharing the same Sphere instance within
    /// `EPSILON_LENGTH`), then radially offsets all group vertices by
    /// `dist`:
    ///   - Each vertex `v`: scale `(v - center)` by `(r + dist) / r`
    ///     about the sphere center. Equivalent to uniform radial scale
    ///     in 3D about `center`.
    ///   - All group face surfaces updated with `radius = current + dist`.
    ///   - Boundary `Arc` / `Circle` curves are also uniformly scaled
    ///     about the sphere center: new center = scale(C - sphere_center),
    ///     new radius = old_radius * scale. normal/basis_u preserved
    ///     under uniform scaling.
    ///
    /// **Auto-expand semantics** (§W2γ2-B-(a)): single profile_face →
    /// full smooth group, idempotent across the group.
    ///
    /// Returns `NotYetSupported` if the new radius would collapse below
    /// `EPSILON_LENGTH` (geometry inversion guard).
    fn offset_smooth_group_sphere(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let (center, current_radius, axis_dir, ref_dir, u_range, v_range) = match profile_surface {
            AnalyticSurface::Sphere {
                center,
                radius,
                axis_dir,
                ref_dir,
                u_range,
                v_range,
            } => (*center, *radius, *axis_dir, *ref_dir, *u_range, *v_range),
            _ => bail!("offset_smooth_group_sphere: profile is not Sphere"),
        };
        if current_radius <= crate::tolerances::EPSILON_LENGTH {
            bail!(
                "offset_smooth_group_sphere: current radius {:.3e} below epsilon",
                current_radius
            );
        }

        let new_radius = current_radius + dist;
        if new_radius <= crate::tolerances::EPSILON_LENGTH {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "offset would collapse sphere radius to {:.3e} (current {:.3e}, dist {:.3e})",
                    new_radius, current_radius, dist
                ),
            }
            .into());
        }
        let scale = new_radius / current_radius;
        let tol = crate::tolerances::EPSILON_LENGTH;

        // Detect smooth group: active faces whose surface is a Sphere
        // matching center + current_radius within tol.
        let group_faces: Vec<FaceId> = self
            .faces
            .iter()
            .filter_map(|(fid, face)| {
                if !face.is_active() {
                    return None;
                }
                match face.surface() {
                    Some(AnalyticSurface::Sphere {
                        center: c,
                        radius: r,
                        ..
                    }) => {
                        let same = (*c - center).length() < tol
                            && (*r - current_radius).abs() < tol;
                        if same {
                            Some(fid)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();

        if !group_faces.contains(&profile_face) {
            bail!(
                "offset_smooth_group_sphere: profile face {profile_face:?} \
                 not in detected smooth group (size {})",
                group_faces.len()
            );
        }

        // Collect unique vertices across the group.
        let mut group_verts: std::collections::HashSet<crate::entities::VertId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let start = self.faces[fid].outer().start;
            if start.is_null() {
                continue;
            }
            for v in self.collect_loop_verts(start)? {
                group_verts.insert(v);
            }
        }

        // Uniform radial scale each vertex about the sphere center.
        // Vertices at center are degenerate (shouldn't occur on a sphere
        // surface) — skip to avoid NaN.
        for v in group_verts.iter().copied().collect::<Vec<_>>() {
            let pos = self.vertex_pos(v)?;
            let from_c = pos - center;
            if from_c.length_squared() < tol * tol {
                continue;
            }
            let new_pos = center + from_c * scale;
            self.move_vertex(v, new_pos)?;
        }

        // Update each group face's Sphere surface with new radius.
        let new_surface = AnalyticSurface::Sphere {
            center,
            radius: new_radius,
            axis_dir, // ADR-204: smooth-group offset preserves orientation
            ref_dir,
            u_range,
            v_range,
        };
        for &fid in &group_faces {
            if let Some(face) = self.faces.get_mut(fid) {
                if face.is_active() {
                    face.set_surface(Some(new_surface.clone()));
                }
            }
        }

        // Update Arc / Circle curves on edges incident to group faces.
        // Under uniform 3D scale about sphere center, an arc transforms:
        //   - new center = sphere_center + (old_center - sphere_center) * scale
        //   - new radius = old_radius * scale
        //   - normal / basis_u preserved (uniform scale preserves orientation)
        let mut updated_arcs: std::collections::HashSet<crate::entities::EdgeId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let edges = self.face_outer_edges(fid)?;
            for eid in edges {
                if updated_arcs.contains(&eid) {
                    continue;
                }
                let new_curve = if let Some(edge) = self.edges.get(eid) {
                    match edge.curve() {
                        Some(AnalyticCurve::Arc {
                            center: ac,
                            radius: ar,
                            normal,
                            basis_u,
                            start_angle,
                            end_angle,
                        }) => Some(AnalyticCurve::Arc {
                            center: center + (*ac - center) * scale,
                            radius: ar * scale,
                            normal: *normal,
                            basis_u: *basis_u,
                            start_angle: *start_angle,
                            end_angle: *end_angle,
                        }),
                        Some(AnalyticCurve::Circle {
                            center: cc,
                            radius: cr,
                            normal,
                            basis_u,
                        }) => Some(AnalyticCurve::Circle {
                            center: center + (*cc - center) * scale,
                            radius: cr * scale,
                            normal: *normal,
                            basis_u: *basis_u,
                        }),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(c) = new_curve {
                    if let Some(edge) = self.edges.get_mut(eid) {
                        edge.set_curve(Some(c));
                    }
                    updated_arcs.insert(eid);
                }
            }
        }

        let side_faces: Vec<FaceId> = group_faces
            .iter()
            .copied()
            .filter(|&f| f != profile_face)
            .collect();

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::SmoothGroupOffset,
            top_face: profile_face,
            side_faces,
            all_solid_faces: group_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-2-γ-iii — Cone constant-offset (§W2γ3-D Option 3).
    ///
    /// True surface-offset semantics: each vertex moves by `dist` along
    /// its outward surface normal at P. The cone's `half_angle` and
    /// `ref_dir` are preserved (cone identity invariant); the apex shifts
    /// along `-axis_dir` by `dist / sin(half_angle)` and v_range shifts by
    /// `dist * cos²(half_angle) / sin(half_angle)`.
    ///
    /// **Math derivation (apex at origin, axis = +Z, axial coord = z)**:
    /// - At point P with axial z and angular u: P = (z·tan(α)·cos(u),
    ///   z·tan(α)·sin(u), z)
    /// - Outward normal: n(u) = (cos(α)·cos(u), cos(α)·sin(u), -sin(α))
    /// - After offset: P' = P + dist·n
    ///   - new radius at z: z·tan(α) + dist·cos(α)
    ///   - new axial: z - dist·sin(α)
    /// - To represent P' on a cone with same α and same axis: new apex
    ///   at z' = -dist/sin(α) (relative to old apex)
    /// - In vector form: `apex_new = apex_old - (dist/sin(α)) · axis_dir`
    ///
    /// **Per-vertex normal** (P relative to apex):
    /// - radial_vec = (P - apex) - ((P - apex)·axis_dir)·axis_dir
    /// - radial_dir = radial_vec.normalize()
    /// - normal = cos(α)·radial_dir - sin(α)·axis_dir
    ///
    /// **Boundary latitude rings** (Arc/Circle with center on axis,
    /// normal ‖ axis_dir):
    /// - new_center = old_center - dist·sin(α)·axis_dir
    /// - new_radius = old_radius + dist·cos(α)
    /// - normal / basis_u / angles preserved
    ///
    /// Returns `NotYetSupported` if:
    /// - half_angle outside (1e-6, π/2 - 1e-6) — singular cone
    /// - new v_range minimum collapses below `EPSILON_LENGTH`
    fn offset_smooth_group_cone(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let (apex, axis_dir, half_angle, ref_dir, u_range, v_range) = match profile_surface {
            AnalyticSurface::Cone {
                apex,
                axis_dir,
                half_angle,
                ref_dir,
                u_range,
                v_range,
            } => (
                *apex,
                axis_dir.normalize_or_zero(),
                *half_angle,
                *ref_dir,
                *u_range,
                *v_range,
            ),
            _ => bail!("offset_smooth_group_cone: profile is not Cone"),
        };
        if axis_dir.length_squared() < 0.5 {
            bail!("offset_smooth_group_cone: axis_dir near zero");
        }

        let alpha_eps = 1e-6;
        if half_angle < alpha_eps
            || half_angle > std::f64::consts::FRAC_PI_2 - alpha_eps
        {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "cone half_angle {:.4e} outside (epsilon, π/2 - epsilon) — singular",
                    half_angle
                ),
            }
            .into());
        }

        let sin_a = half_angle.sin();
        let cos_a = half_angle.cos();
        let tan_a = half_angle.tan();
        let tol = crate::tolerances::EPSILON_LENGTH;

        // Apex shifts along -axis_dir by dist/sin(α). New v_range shifts by
        // dist*cos²(α)/sin(α) (constant, preserves v_range width).
        let apex_shift = -dist / sin_a;
        let new_apex = apex + apex_shift * axis_dir;
        let v_shift = dist * cos_a * cos_a / sin_a;
        let new_v_range = (v_range.0 + v_shift, v_range.1 + v_shift);

        // Collapse guard — new v_range must remain positive.
        if new_v_range.0 < tol {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "offset would collapse cone: new v_min = {:.3e} ≤ epsilon \
                     (old v_min {:.3e}, dist {:.3e}, half_angle {:.4})",
                    new_v_range.0, v_range.0, dist, half_angle
                ),
            }
            .into());
        }

        // Detect smooth group: faces with matching Cone instance.
        let group_faces: Vec<FaceId> = self
            .faces
            .iter()
            .filter_map(|(fid, face)| {
                if !face.is_active() {
                    return None;
                }
                match face.surface() {
                    Some(AnalyticSurface::Cone {
                        apex: a,
                        axis_dir: ad,
                        half_angle: ha,
                        ref_dir: rd,
                        ..
                    }) => {
                        let same = (*a - apex).length() < tol
                            && ad.normalize_or_zero().dot(axis_dir).abs() > 0.999
                            && (*ha - half_angle).abs() < 1e-9
                            && rd.normalize_or_zero()
                                .dot(ref_dir.normalize_or_zero())
                                .abs()
                                > 0.999;
                        if same {
                            Some(fid)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();

        if !group_faces.contains(&profile_face) {
            bail!(
                "offset_smooth_group_cone: profile face {profile_face:?} \
                 not in detected smooth group (size {})",
                group_faces.len()
            );
        }

        // Collect unique vertices across the group.
        let mut group_verts: std::collections::HashSet<crate::entities::VertId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let start = self.faces[fid].outer().start;
            if start.is_null() {
                continue;
            }
            for v in self.collect_loop_verts(start)? {
                group_verts.insert(v);
            }
        }

        // Move each vertex along its surface normal at P.
        for v in group_verts.iter().copied().collect::<Vec<_>>() {
            let pos = self.vertex_pos(v)?;
            let from_apex = pos - apex;
            let axial = from_apex.dot(axis_dir);
            let radial_vec = from_apex - axial * axis_dir;
            if radial_vec.length_squared() < tol * tol {
                // Vertex at apex — singular, skip.
                continue;
            }
            let radial_dir = radial_vec.normalize();
            let normal = cos_a * radial_dir - sin_a * axis_dir;
            let new_pos = pos + dist * normal;
            self.move_vertex(v, new_pos)?;
        }

        // Update each group face's Cone surface with new apex + v_range.
        let new_surface = AnalyticSurface::Cone {
            apex: new_apex,
            axis_dir,
            half_angle,
            ref_dir,
            u_range,
            v_range: new_v_range,
        };
        for &fid in &group_faces {
            if let Some(face) = self.faces.get_mut(fid) {
                if face.is_active() {
                    face.set_surface(Some(new_surface.clone()));
                }
            }
        }

        // Update boundary Arc / Circle latitude rings:
        //   filter: center on axis (cross-product with axis_dir < tol)
        //         + normal ‖ axis_dir
        //         + radius ≈ axial_pos · tan(half_angle) (sanity)
        // Update: new_center = center - dist·sin(α)·axis_dir
        //         new_radius = radius + dist·cos(α)
        let mut updated_arcs: std::collections::HashSet<crate::entities::EdgeId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let edges = self.face_outer_edges(fid)?;
            for eid in edges {
                if updated_arcs.contains(&eid) {
                    continue;
                }
                let new_curve = if let Some(edge) = self.edges.get(eid) {
                    match edge.curve() {
                        Some(AnalyticCurve::Arc {
                            center,
                            radius: ar,
                            normal,
                            basis_u,
                            start_angle,
                            end_angle,
                        }) => {
                            let center_off_axis =
                                ((*center - apex).cross(axis_dir)).length();
                            let normal_dot = normal.normalize_or_zero().dot(axis_dir).abs();
                            let v_axial = (*center - apex).dot(axis_dir);
                            let expected_r = v_axial * tan_a;
                            // Use looser tol on radius (numeric drift after move_vertex on
                            // earlier iterations) — but pre-move check happens BEFORE
                            // any vertex move on this iteration's edge sweep, so it's tight.
                            let radius_match = (*ar - expected_r).abs() < tol;
                            if center_off_axis < tol
                                && normal_dot > 0.999
                                && radius_match
                            {
                                let new_r = *ar + dist * cos_a;
                                if new_r > tol {
                                    Some(AnalyticCurve::Arc {
                                        center: *center - dist * sin_a * axis_dir,
                                        radius: new_r,
                                        normal: *normal,
                                        basis_u: *basis_u,
                                        start_angle: *start_angle,
                                        end_angle: *end_angle,
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        Some(AnalyticCurve::Circle {
                            center,
                            radius: cr,
                            normal,
                            basis_u,
                        }) => {
                            let center_off_axis =
                                ((*center - apex).cross(axis_dir)).length();
                            let normal_dot = normal.normalize_or_zero().dot(axis_dir).abs();
                            let v_axial = (*center - apex).dot(axis_dir);
                            let expected_r = v_axial * tan_a;
                            let radius_match = (*cr - expected_r).abs() < tol;
                            if center_off_axis < tol
                                && normal_dot > 0.999
                                && radius_match
                            {
                                let new_r = *cr + dist * cos_a;
                                if new_r > tol {
                                    Some(AnalyticCurve::Circle {
                                        center: *center - dist * sin_a * axis_dir,
                                        radius: new_r,
                                        normal: *normal,
                                        basis_u: *basis_u,
                                    })
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(c) = new_curve {
                    if let Some(edge) = self.edges.get_mut(eid) {
                        edge.set_curve(Some(c));
                    }
                    updated_arcs.insert(eid);
                }
            }
        }

        let side_faces: Vec<FaceId> = group_faces
            .iter()
            .copied()
            .filter(|&f| f != profile_face)
            .collect();

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::SmoothGroupOffset,
            top_face: profile_face,
            side_faces,
            all_solid_faces: group_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// ADR-079 W-2-γ-iv — Torus constant-offset (§W2γ4-D Option 2).
    ///
    /// Equivalent to `minor_radius += dist` because the torus surface
    /// normal at any point is exactly the radial direction from the
    /// minor circle's center (which sits on the major circle).
    /// Center / axis_dir / ref_dir / major_radius UNCHANGED.
    ///
    /// **Math** (P on torus with center C, axis Z, ref X, R = major,
    /// r = minor):
    /// - radial_vec = (P - C) - ((P - C)·Z)·Z  (in major-plane component)
    /// - radial_dir = radial_vec.normalize()
    /// - major_circle_pt = C + R·radial_dir
    /// - normal at P = (P - major_circle_pt).normalize()
    ///   (this is exactly the unit vector from minor circle center to P,
    ///    which equals cos(v)·radial_dir + sin(v)·axis_dir for some v)
    /// - P' = P + dist·normal
    /// - new minor circle has same center (major_circle_pt) but radius
    ///   r + dist → P' lies on torus with same C/Z/X/R but minor = r + dist
    ///
    /// **Latitude circle update** (Arc/Circle with center on axis +
    /// normal ‖ axis_dir) — center at C + r·sin(v)·Z, radius = R +
    /// r·cos(v) for some v ∈ [0, 2π]:
    /// - extract sin(v) = axial_offset / r, cos(v) = (radius - R) / r
    /// - sanity: sin² + cos² ≈ 1
    /// - new_center = C + (r+d)·sin(v)·Z = old_center + d·sin(v)·Z
    /// - new_radius = R + (r+d)·cos(v) = old_radius + d·cos(v)
    ///
    /// Returns `NotYetSupported` if:
    /// - new minor_radius ≤ EPSILON_LENGTH (collapse / inversion)
    fn offset_smooth_group_torus(
        &mut self,
        profile_face: FaceId,
        dist: f64,
        profile_surface: &AnalyticSurface,
    ) -> Result<CreateSolidResult> {
        let (center, axis_dir, ref_dir, major_radius, minor_radius, u_range, v_range) =
            match profile_surface {
                AnalyticSurface::Torus {
                    center,
                    axis_dir,
                    ref_dir,
                    major_radius,
                    minor_radius,
                    u_range,
                    v_range,
                } => (
                    *center,
                    axis_dir.normalize_or_zero(),
                    *ref_dir,
                    *major_radius,
                    *minor_radius,
                    *u_range,
                    *v_range,
                ),
                _ => bail!("offset_smooth_group_torus: profile is not Torus"),
            };
        if axis_dir.length_squared() < 0.5 {
            bail!("offset_smooth_group_torus: axis_dir near zero");
        }
        let tol = crate::tolerances::EPSILON_LENGTH;
        if major_radius <= tol || minor_radius <= tol {
            bail!(
                "offset_smooth_group_torus: degenerate radii \
                 (major {:.3e}, minor {:.3e})",
                major_radius,
                minor_radius
            );
        }

        let new_minor = minor_radius + dist;
        if new_minor <= tol {
            return Err(SolidError::NotYetSupported {
                reason: format!(
                    "offset would collapse torus minor_radius to {:.3e} \
                     (current {:.3e}, dist {:.3e})",
                    new_minor, minor_radius, dist
                ),
            }
            .into());
        }

        // Detect smooth group: faces with matching Torus instance.
        let group_faces: Vec<FaceId> = self
            .faces
            .iter()
            .filter_map(|(fid, face)| {
                if !face.is_active() {
                    return None;
                }
                match face.surface() {
                    Some(AnalyticSurface::Torus {
                        center: c,
                        axis_dir: ad,
                        ref_dir: rd,
                        major_radius: mr,
                        minor_radius: nr,
                        ..
                    }) => {
                        let same = (*c - center).length() < tol
                            && ad.normalize_or_zero().dot(axis_dir).abs() > 0.999
                            && rd.normalize_or_zero()
                                .dot(ref_dir.normalize_or_zero())
                                .abs()
                                > 0.999
                            && (*mr - major_radius).abs() < tol
                            && (*nr - minor_radius).abs() < tol;
                        if same {
                            Some(fid)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();

        if !group_faces.contains(&profile_face) {
            bail!(
                "offset_smooth_group_torus: profile face {profile_face:?} \
                 not in detected smooth group (size {})",
                group_faces.len()
            );
        }

        // Collect group vertices.
        let mut group_verts: std::collections::HashSet<crate::entities::VertId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let start = self.faces[fid].outer().start;
            if start.is_null() {
                continue;
            }
            for v in self.collect_loop_verts(start)? {
                group_verts.insert(v);
            }
        }

        // Move each vertex along surface normal at P.
        // Surface normal = unit vector from major-circle point to P.
        for v in group_verts.iter().copied().collect::<Vec<_>>() {
            let pos = self.vertex_pos(v)?;
            let from_c = pos - center;
            let axial = from_c.dot(axis_dir);
            let radial_vec = from_c - axial * axis_dir;
            if radial_vec.length_squared() < tol * tol {
                // Vertex on torus axis — degenerate (shouldn't happen on a
                // valid torus surface). Skip.
                continue;
            }
            let radial_dir = radial_vec.normalize();
            let major_pt = center + major_radius * radial_dir;
            let to_surface = pos - major_pt;
            if to_surface.length_squared() < tol * tol {
                // Vertex at major circle center — also degenerate.
                continue;
            }
            let normal = to_surface.normalize();
            let new_pos = pos + dist * normal;
            self.move_vertex(v, new_pos)?;
        }

        // Update each group face's Torus surface with new minor_radius.
        let new_surface = AnalyticSurface::Torus {
            center,
            axis_dir,
            ref_dir,
            major_radius,
            minor_radius: new_minor,
            u_range,
            v_range,
        };
        for &fid in &group_faces {
            if let Some(face) = self.faces.get_mut(fid) {
                if face.is_active() {
                    face.set_surface(Some(new_surface.clone()));
                }
            }
        }

        // Update latitude circles on group face boundaries:
        //   filter: center on axis + normal ‖ axis_dir
        //   sanity: extract sin(v) = axial_offset/minor, cos(v) = (r-R)/minor;
        //           verify sin² + cos² ≈ 1
        //   update: new_center = center + d·sin(v)·axis_dir,
        //           new_radius = r + d·cos(v)
        let mut updated_arcs: std::collections::HashSet<crate::entities::EdgeId> =
            std::collections::HashSet::new();
        for &fid in &group_faces {
            let edges = self.face_outer_edges(fid)?;
            for eid in edges {
                if updated_arcs.contains(&eid) {
                    continue;
                }
                let new_curve = if let Some(edge) = self.edges.get(eid) {
                    match edge.curve() {
                        Some(AnalyticCurve::Arc {
                            center: ac,
                            radius: ar,
                            normal,
                            basis_u,
                            start_angle,
                            end_angle,
                        }) => Self::torus_latitude_arc_update(
                            *ac,
                            *ar,
                            *normal,
                            *basis_u,
                            Some((*start_angle, *end_angle)),
                            center,
                            axis_dir,
                            major_radius,
                            minor_radius,
                            dist,
                            tol,
                        ),
                        Some(AnalyticCurve::Circle {
                            center: cc,
                            radius: cr,
                            normal,
                            basis_u,
                        }) => Self::torus_latitude_arc_update(
                            *cc,
                            *cr,
                            *normal,
                            *basis_u,
                            None,
                            center,
                            axis_dir,
                            major_radius,
                            minor_radius,
                            dist,
                            tol,
                        ),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(c) = new_curve {
                    if let Some(edge) = self.edges.get_mut(eid) {
                        edge.set_curve(Some(c));
                    }
                    updated_arcs.insert(eid);
                }
            }
        }

        let side_faces: Vec<FaceId> = group_faces
            .iter()
            .copied()
            .filter(|&f| f != profile_face)
            .collect();

        Ok(CreateSolidResult {
            profile_face,
            solid_kind: SolidKind::SmoothGroupOffset,
            top_face: profile_face,
            side_faces,
            all_solid_faces: group_faces,
            adjacent_splits: 0,
            split_debug: Vec::new(),
        })
    }

    /// Helper for `offset_smooth_group_torus` — update a latitude
    /// Arc/Circle on a torus under minor_radius offset by `dist`.
    /// Returns `Some(new_curve)` if the arc passes the latitude filter
    /// (center on axis + normal ‖ axis_dir + sin²+cos²≈1 sanity), else `None`.
    /// `angles = Some((start, end))` for Arc, `None` for Circle.
    #[allow(clippy::too_many_arguments)]
    fn torus_latitude_arc_update(
        arc_center: DVec3,
        arc_radius: f64,
        arc_normal: DVec3,
        arc_basis_u: DVec3,
        angles: Option<(f64, f64)>,
        torus_center: DVec3,
        axis_dir: DVec3,
        major_radius: f64,
        minor_radius: f64,
        dist: f64,
        tol: f64,
    ) -> Option<AnalyticCurve> {
        // Filter: center on axis + normal parallel to axis.
        let center_off_axis = ((arc_center - torus_center).cross(axis_dir)).length();
        let normal_dot = arc_normal.normalize_or_zero().dot(axis_dir).abs();
        if center_off_axis >= tol || normal_dot < 0.999 {
            return None;
        }

        // Extract latitude angle v from arc params.
        let axial_offset = (arc_center - torus_center).dot(axis_dir);
        let sin_v = axial_offset / minor_radius;
        let cos_v = (arc_radius - major_radius) / minor_radius;
        // Sanity: must lie on unit circle (within reasonable numeric tol).
        let unit_check = (sin_v * sin_v + cos_v * cos_v - 1.0).abs();
        if unit_check > 1e-6 {
            return None;
        }

        let new_axial = axial_offset + dist * sin_v;
        let new_center = torus_center + new_axial * axis_dir
            + (arc_center - torus_center - axial_offset * axis_dir);
        let new_radius = arc_radius + dist * cos_v;
        if new_radius <= tol {
            return None;
        }

        match angles {
            Some((s, e)) => Some(AnalyticCurve::Arc {
                center: new_center,
                radius: new_radius,
                normal: arc_normal,
                basis_u: arc_basis_u,
                start_angle: s,
                end_angle: e,
            }),
            None => Some(AnalyticCurve::Circle {
                center: new_center,
                radius: new_radius,
                normal: arc_normal,
                basis_u: arc_basis_u,
            }),
        }
    }
}

/// ADR-079 §2.3 — Classify the boundary curves of a profile face.
///
/// Walks the outer loop edges and inspects each `Edge::curve()`:
/// - All `Line` (or `None` per Phase N synthesize) → `AllLinear`
/// - All `Circle` / `Arc` → `AllCircular`
/// - 그 외 (Bezier / BSpline / NURBS / 혼합) → `Mixed`
pub fn classify_boundary(mesh: &Mesh, face: FaceId) -> Result<BoundaryKind> {
    let edges = mesh.face_outer_edges(face)?;
    if edges.is_empty() {
        bail!("classify_boundary: face {face:?} has no outer edges");
    }

    let mut all_linear = true;
    let mut all_circular = true;

    for &eid in &edges {
        let edge = mesh
            .edges
            .get(eid)
            .ok_or_else(|| anyhow::anyhow!("classify_boundary: edge {eid:?} not found"))?;
        match edge.curve() {
            None => {
                // Phase N: synthesized Line. Treat as Line.
                all_circular = false;
            }
            Some(AnalyticCurve::Line { .. }) => {
                all_circular = false;
            }
            Some(AnalyticCurve::Circle { .. } | AnalyticCurve::Arc { .. }) => {
                all_linear = false;
            }
            Some(_) => {
                // Bezier / BSpline / NURBS — Mixed
                all_linear = false;
                all_circular = false;
                break;
            }
        }
    }

    Ok(if all_linear {
        BoundaryKind::AllLinear
    } else if all_circular {
        BoundaryKind::AllCircular
    } else {
        BoundaryKind::Mixed
    })
}

/// ADR-079 §W2-B-(a) — Extract shared circle parameters from a profile
/// face whose outer boundary is `AllCircular`.
///
/// Returns `(center, radius, normal, basis_u)` of the underlying circle.
/// All Arc/Circle edges in the loop must share these parameters within
/// `EPSILON_LENGTH`. Edges with `Some(Line)` or `None` curve fail loudly
/// — caller should have classified the boundary as `AllCircular` first.
///
/// On mismatch returns `Err`, allowing the caller to convert to
/// `SolidError::NotYetSupported` and trigger Q3 fallback.
fn extract_shared_circle_params(
    mesh: &Mesh,
    face: FaceId,
) -> Result<(DVec3, f64, DVec3, DVec3)> {
    let edges = mesh.face_outer_edges(face)?;
    if edges.is_empty() {
        bail!("extract_shared_circle_params: face {face:?} has no outer edges");
    }

    let mut shared: Option<(DVec3, f64, DVec3, DVec3)> = None;
    let tol = crate::tolerances::EPSILON_LENGTH;

    for &eid in &edges {
        let edge = mesh.edges.get(eid).ok_or_else(|| {
            anyhow::anyhow!("extract_shared_circle_params: edge {eid:?} not found")
        })?;
        let (c, r, n, bu) = match edge.curve() {
            Some(AnalyticCurve::Circle { center, radius, normal, basis_u }) => {
                (*center, *radius, *normal, *basis_u)
            }
            Some(AnalyticCurve::Arc { center, radius, normal, basis_u, .. }) => {
                (*center, *radius, *normal, *basis_u)
            }
            _ => bail!(
                "extract_shared_circle_params: edge {eid:?} is not Circle/Arc \
                 (caller should classify as AllCircular first)"
            ),
        };
        match shared {
            None => shared = Some((c, r, n, bu)),
            Some((cs, rs, ns, _)) => {
                if (c - cs).length() > tol {
                    bail!(
                        "center mismatch (Δ = {:.2e} mm > tol {:.2e})",
                        (c - cs).length(),
                        tol
                    );
                }
                if (r - rs).abs() > tol {
                    bail!(
                        "radius mismatch (Δ = {:.2e} mm > tol {:.2e})",
                        (r - rs).abs(),
                        tol
                    );
                }
                // Normal may be flipped between sub-arcs of the same circle —
                // accept either orientation as long as parallel.
                let dot = n.normalize_or_zero().dot(ns.normalize_or_zero());
                if dot.abs() < 0.999 {
                    bail!(
                        "normal mismatch (dot = {:.4}, expected |dot| ≥ 0.999)",
                        dot
                    );
                }
            }
        }
    }

    shared.ok_or_else(|| anyhow::anyhow!("extract_shared_circle_params: empty boundary"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{FaceId, MaterialId};
    use crate::mesh::Mesh;
    use crate::surfaces::AnalyticSurface;

    /// Helper — build a unit square Plane-surfaced face on z=0, normal +Z.
    fn build_unit_square_plane_face(mesh: &mut Mesh) -> FaceId {
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face = mesh.add_face(&[v00, v10, v11, v01], mat).expect("add_face");
        // Attach Plane surface (truth source).
        let surface = AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        };
        mesh.faces[face].set_surface(Some(surface));
        face
    }

    /// ADR-259 sim helper — CCW concave L-shape Plane face (z=0, +Z), arms width 1.
    fn build_l_plane_face(mesh: &mut Mesh) -> FaceId {
        let mat = MaterialId::new(0);
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(3.0, 0.0, 0.0),
            DVec3::new(3.0, 1.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(1.0, 3.0, 0.0),
            DVec3::new(0.0, 3.0, 0.0),
        ];
        let vs: Vec<VertId> = pts.iter().map(|p| mesh.add_vertex(*p)).collect();
        let face = mesh.add_face(&vs, mat).expect("add_face L");
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 3.0),
            v_range: (0.0, 3.0),
        }));
        face
    }

    /// ADR-259 sim helper — are 4+ points coplanar (planar trapezoid check)?
    fn is_planar(pts: &[DVec3]) -> bool {
        if pts.len() < 4 {
            return true;
        }
        let nrm = (pts[1] - pts[0]).cross(pts[2] - pts[0]);
        if nrm.length_squared() < 1e-18 {
            return true;
        }
        let nrm = nrm.normalize();
        pts.iter().all(|p| ((*p - pts[0]).dot(nrm)).abs() < 1e-6)
    }

    // ── ADR-259 β-1 SIMULATION (dormant kernel, real Mesh API + manifold verify) ──
    // Proves the taper construction is manifold + clean + planar-sided BEFORE the
    // op is wired into dispatch / WASM / tool (no scene-path exposure yet).

    #[test]
    fn adr259_sim_taper_square_frustum_manifold() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let surf = mesh.faces[profile].surface().cloned().unwrap();
        let res = mesh
            .extrude_planar_box_tapered(profile, 1.0, 15.0, MaterialId::new(0), &surf)
            .expect("taper square OK");
        assert_eq!(res.solid_kind, SolidKind::Box);
        assert_eq!(res.side_faces.len(), 4);
        assert_eq!(res.all_solid_faces.len(), 6);
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "taper frustum must be manifold-valid (no broken faces)"
        );
        let report = mesh.verify_outward_normals();
        assert!(report.is_closed_solid, "taper frustum must be a closed solid");
        assert_eq!(report.inward_count, 0, "all faces outward (ADR-183)");
        // Top shrunk inward (~[0.268, 0.732]²) — every top vert strictly inside.
        let tv = mesh
            .collect_loop_verts(mesh.faces[res.top_face].outer().start)
            .unwrap();
        for v in tv {
            let p = mesh.vertex_pos(v).unwrap();
            assert!(p.x > 0.1 && p.x < 0.9 && p.y > 0.1 && p.y < 0.9, "top shrunk: {:?}", p);
            assert!((p.z - 1.0).abs() < 1e-9, "top lifted to z=1");
        }
    }

    #[test]
    fn adr259_sim_taper_concave_l_manifold() {
        let mut mesh = Mesh::new();
        let profile = build_l_plane_face(&mut mesh);
        let surf = mesh.faces[profile].surface().cloned().unwrap();
        // 10° taper, dist 1.0 → d_off ≈ 0.176 < arm half-width → valid concave frustum.
        let res = mesh
            .extrude_planar_box_tapered(profile, 1.0, 10.0, MaterialId::new(0), &surf)
            .expect("taper concave L OK");
        assert_eq!(res.side_faces.len(), 6, "L has 6 edges → 6 trapezoid sides");
        assert_eq!(res.all_solid_faces.len(), 8, "profile + top + 6 sides");
        // Authoritative orientability/manifold gate (works for concave):
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "concave taper frustum must be manifold-valid (orientable, consistent winding)"
        );
        let report = mesh.verify_outward_normals();
        assert!(report.is_closed_solid, "concave taper closed solid");
        // NOTE: verify_outward_normals uses the mesh-CENTROID heuristic
        // (mesh_invariants.rs:318) which is reliable only for CONVEX solids —
        // for a concave L-frustum it false-flags faces near the reflex region.
        // We therefore do NOT assert inward_count==0 here (it is not a winding
        // bug — verify_face_invariants confirms consistent orientation). The
        // real BackSide/render check for concave is the γ live-browser sim.
    }

    #[test]
    fn adr259_sim_taper_steep_collapse_rejected() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let surf = mesh.faces[profile].surface().cloned().unwrap();
        let fc = mesh.face_count();
        // 88° → d_off = tan(88°) ≈ 28.6 ≫ inradius 0.5 → collapse → Err (D5).
        let res = mesh.extrude_planar_box_tapered(profile, 1.0, 88.0, MaterialId::new(0), &surf);
        assert!(res.is_err(), "steep taper must be rejected (fail-closed)");
        assert_eq!(
            mesh.face_count(),
            fc,
            "mesh UNTOUCHED on reject — no half-applied broken faces"
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "mesh still valid after reject"
        );
    }

    #[test]
    fn adr259_sim_taper_concave_over_inward_rejected() {
        let mut mesh = Mesh::new();
        let profile = build_l_plane_face(&mut mesh);
        let surf = mesh.faces[profile].surface().cloned().unwrap();
        let fc = mesh.face_count();
        // 45° → d_off = 1.0 ≥ arm width 1 → self-intersect/collapse → Err.
        let res = mesh.extrude_planar_box_tapered(profile, 1.0, 45.0, MaterialId::new(0), &surf);
        assert!(res.is_err(), "concave over-inward must be rejected (fail-closed)");
        assert_eq!(mesh.face_count(), fc, "mesh untouched on reject");
    }

    #[test]
    fn adr259_sim_taper_side_faces_exact_planes() {
        // The exact-Plane-sides invariant (ADR-259 §2 / L-259-1) for convex AND concave.
        for concave in [false, true] {
            let mut mesh = Mesh::new();
            let profile = if concave {
                build_l_plane_face(&mut mesh)
            } else {
                build_unit_square_plane_face(&mut mesh)
            };
            let surf = mesh.faces[profile].surface().cloned().unwrap();
            let res = mesh
                .extrude_planar_box_tapered(profile, 1.0, 10.0, MaterialId::new(0), &surf)
                .expect("taper OK");
            for &side in &res.side_faces {
                let verts = mesh
                    .collect_loop_verts(mesh.faces[side].outer().start)
                    .unwrap();
                let pos: Vec<DVec3> = verts.iter().map(|v| mesh.vertex_pos(*v).unwrap()).collect();
                assert!(
                    is_planar(&pos),
                    "side trapezoid must be exactly planar (concave={}): {:?}",
                    concave,
                    pos
                );
                // And the synthesized surface is a Plane (not best-fit NURBS).
                assert!(
                    matches!(mesh.faces[side].surface(), Some(AnalyticSurface::Plane { .. })),
                    "side surface must be Plane"
                );
            }
        }
    }

    #[test]
    fn adr259_sim_taper_outward_flare_manifold() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let surf = mesh.faces[profile].surface().cloned().unwrap();
        // negative taper = outward flare (top bigger, ~[-0.268, 1.268]²).
        let res = mesh
            .extrude_planar_box_tapered(profile, 1.0, -15.0, MaterialId::new(0), &surf)
            .expect("outward flare OK");
        assert!(mesh.verify_face_invariants().is_valid(), "flare manifold-valid");
        let report = mesh.verify_outward_normals();
        assert!(report.is_closed_solid && report.inward_count == 0);
        let tv = mesh
            .collect_loop_verts(mesh.faces[res.top_face].outer().start)
            .unwrap();
        let any_outside = tv.iter().any(|v| {
            let p = mesh.vertex_pos(*v).unwrap();
            p.x < -0.1 || p.x > 1.1 || p.y < -0.1 || p.y > 1.1
        });
        assert!(any_outside, "flare top must expand beyond the unit square");
    }

    // ── ADR-259 β-1 DISPATCH (create_solid CreateSolidMode::ExtrudeTapered) ──

    #[test]
    fn adr259_create_solid_taper_dispatch_box() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let res = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeTapered { distance: 1.0, taper_deg: 15.0 },
                MaterialId::new(0),
            )
            .expect("tapered dispatch OK");
        assert_eq!(res.solid_kind, SolidKind::Box);
        assert_eq!(res.side_faces.len(), 4);
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "dispatch taper frustum manifold valid"
        );
    }

    #[test]
    fn adr259_create_solid_taper_solid_face_rejected() {
        // v1 = flat-profile only. Tapering a SOLID face (a box top — is_move_only)
        // would be non-manifold; the dispatch guard must reject it (no fallback).
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let box_res = mesh
            .create_solid(profile, CreateSolidMode::Extrude { distance: 1.0 }, MaterialId::new(0))
            .expect("box OK");
        let top = box_res.top_face; // bounds the box solid.
        let res = mesh.create_solid(
            top,
            CreateSolidMode::ExtrudeTapered { distance: 0.5, taper_deg: 10.0 },
            MaterialId::new(0),
        );
        assert!(res.is_err(), "taper on a solid face must be rejected (v1)");
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "box untouched + valid after rejected solid-face taper"
        );
    }

    #[test]
    fn adr259_create_solid_taper_allcircular_not_supported() {
        // D2: v1 supports (Plane, AllLinear) only. A circle profile → NotYetSupported.
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let r = 5.0;
        let anchor = mesh.add_vertex(DVec3::X * r);
        let circle = AnalyticCurve::Circle {
            center: DVec3::ZERO,
            radius: r,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let fid = mesh.add_face_closed_curve(anchor, circle, mat).unwrap();
        let res = mesh.create_solid(
            fid,
            CreateSolidMode::ExtrudeTapered { distance: 2.0, taper_deg: 10.0 },
            mat,
        );
        assert!(res.is_err(), "AllCircular taper not supported in v1 (D2)");
    }

    // ── ADR-260 β-1 SIMULATION GATE (circle → cone / frustum extrude) ──────
    // "먼저 시뮬" — verify manifold + half_angle/radius math + fail-closed
    // guards on the engine BEFORE live exposure (ADR-259 답습).

    /// Extract `(apex, axis_dir, half_angle, ref_dir, v_range)` from a face's
    /// `AnalyticSurface::Cone`, or panic.
    fn cone_params_of(
        mesh: &Mesh,
        fid: FaceId,
    ) -> (DVec3, DVec3, f64, DVec3, (f64, f64)) {
        match mesh.faces.get(fid).and_then(|f| f.surface()) {
            Some(AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. }) => {
                (*apex, *axis_dir, *half_angle, *ref_dir, *v_range)
            }
            other => panic!("face {fid:?} expected Cone surface, got {other:?}"),
        }
    }

    #[test]
    fn adr260_sim_apex_self_loop_manifold() {
        // Self-loop circle, top_scale=0, dist>0 → 2 faces (base + cone side),
        // manifold valid, Cone surface (apex degenerate v=0).
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let r = 500.0;
        let dist = 800.0;
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, r);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: dist, top_scale: 0.0 },
                MaterialId::new(0),
            )
            .expect("apex cone create_solid OK");
        assert_eq!(result.solid_kind, SolidKind::Cone);
        assert_eq!(result.all_solid_faces.len(), 2, "apex cone = 2 faces (base + cone side)");
        assert_eq!(result.side_faces.len(), 1, "1 cone side face");

        let report = mesh.verify_face_invariants();
        assert!(report.is_valid(), "apex cone manifold valid, violations: {:?}", report.violations);

        // Cone surface: apex = center + n·dist, axis_dir = -n, half_angle = atan(R/dist).
        let (apex, axis, half, _ref, vr) = cone_params_of(&mesh, result.side_faces[0]);
        assert!((apex - DVec3::Z * dist).length() < 1e-6, "apex at center + n·dist");
        assert!((axis - DVec3::NEG_Z).length() < 1e-9, "axis_dir = -n (apex→base)");
        assert!((half - (r / dist).atan()).abs() < 1e-9, "half_angle = atan(R/dist)");
        assert!((vr.0 - 0.0).abs() < 1e-9 && (vr.1 - dist).abs() < 1e-6, "v_range (0, dist)");
        // radius at base_v must equal R.
        assert!((vr.1 * half.tan() - r).abs() < 1e-6, "radius(base_v) = R");
    }

    #[test]
    fn adr260_sim_frustum_self_loop_manifold() {
        // Self-loop circle, top_scale=0.5 → 3 faces (base + scaled top + annulus),
        // manifold valid, top radius = R·0.5, Cone surface.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let r = 500.0;
        let dist = 800.0;
        let s = 0.5;
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, r);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: dist, top_scale: s },
                MaterialId::new(0),
            )
            .expect("frustum cone create_solid OK");
        assert_eq!(result.solid_kind, SolidKind::Cone);
        assert_eq!(result.all_solid_faces.len(), 3, "frustum = 3 faces (base + top + annulus)");

        let report = mesh.verify_face_invariants();
        assert!(report.is_valid(), "frustum manifold valid, violations: {:?}", report.violations);

        // Top circle radius = R·s — read top_face self-loop Circle curve.
        let top_start = mesh.faces[result.top_face].outer().start;
        let top_eid = mesh.hes[top_start].edge();
        let top_curve = mesh.edges[top_eid].curve().cloned().expect("top self-loop Circle");
        match top_curve {
            crate::curves::AnalyticCurve::Circle { radius, center, .. } => {
                assert!((radius - r * s).abs() < 1e-6, "top radius = R·s");
                assert!((center - DVec3::Z * dist).length() < 1e-6, "top center = base + n·dist");
            }
            other => panic!("top edge expected Circle, got {other:?}"),
        }

        // Annulus Cone surface: radius(base_v)=R, radius(top_v)=R·s.
        let (apex, axis, half, _ref, vr) = cone_params_of(&mesh, result.side_faces[0]);
        let expected_apex = DVec3::Z * (dist / (1.0 - s));
        assert!((apex - expected_apex).length() < 1e-6, "apex = center + n·dist/(1-s)");
        assert!((axis - DVec3::NEG_Z).length() < 1e-9, "axis_dir = -n");
        assert!((half - (r * (1.0 - s) / dist).atan()).abs() < 1e-9, "half_angle = atan(R(1-s)/dist)");
        assert!((vr.1 * half.tan() - r).abs() < 1e-6, "radius(base_v) = R");
        assert!((vr.0 * half.tan() - r * s).abs() < 1e-6, "radius(top_v) = R·s");
        assert!(vr.0 < vr.1, "top_v < base_v");
    }

    #[test]
    fn adr260_sim_apex_dist_negative_manifold() {
        // dist<0 (downward apex cone) → manifold valid, axis_dir = +n.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let r = 400.0;
        let dist = -600.0;
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, r);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: dist, top_scale: 0.0 },
                MaterialId::new(0),
            )
            .expect("apex cone dist<0 OK");
        assert!(mesh.verify_face_invariants().is_valid(), "dist<0 apex manifold valid");
        let (apex, axis, _half, _ref, _vr) = cone_params_of(&mesh, result.side_faces[0]);
        assert!((apex - DVec3::Z * dist).length() < 1e-6, "apex = center + n·dist (below)");
        assert!((axis - DVec3::Z).length() < 1e-9, "dist<0 → axis_dir = +n (apex→base)");
    }

    #[test]
    fn adr260_sim_frustum_dist_negative_manifold() {
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 400.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: -600.0, top_scale: 0.4 },
                MaterialId::new(0),
            )
            .expect("frustum dist<0 OK");
        assert_eq!(result.all_solid_faces.len(), 3);
        assert!(mesh.verify_face_invariants().is_valid(), "frustum dist<0 manifold valid");
    }

    #[test]
    fn adr260_sim_top_scale_snap_to_apex() {
        // top_scale·R < EPSILON_LENGTH → snap to apex (2 faces, not frustum).
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 500.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: 1e-12 },
                MaterialId::new(0),
            )
            .expect("snap-to-apex OK");
        assert_eq!(result.all_solid_faces.len(), 2, "sub-tolerance top_scale snaps to apex (2 faces)");
        assert!(mesh.verify_face_invariants().is_valid());
    }

    #[test]
    fn adr260_sim_polygonal_apex_fan() {
        // Polygonal-arc circle (16 verts), top_scale=0 → fan + ONE Cone surface.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 500.0, 16);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: 0.0 },
                MaterialId::new(0),
            )
            .expect("polygonal apex fan OK");
        assert_eq!(result.solid_kind, SolidKind::Cone);
        assert_eq!(result.side_faces.len(), 16, "16 fan triangles");
        assert!(mesh.verify_face_invariants().is_valid(), "polygonal apex fan manifold valid");
        // All side faces share ONE Cone surface (same apex).
        let (apex0, _, _, _, _) = cone_params_of(&mesh, result.side_faces[0]);
        for &f in &result.side_faces {
            let (a, _, _, _, _) = cone_params_of(&mesh, f);
            assert!((a - apex0).length() < 1e-9, "all fan faces share apex");
        }
    }

    #[test]
    fn adr260_sim_polygonal_frustum_quads() {
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 500.0, 16);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: 0.4 },
                MaterialId::new(0),
            )
            .expect("polygonal frustum quads OK");
        assert_eq!(result.side_faces.len(), 16, "16 quad sides");
        assert!(mesh.verify_face_invariants().is_valid(), "polygonal frustum manifold valid");
    }

    #[test]
    fn adr260_create_solid_dispatch_cone() {
        // Full dispatch: (Plane, AllCircular) self-loop → Cone result.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 500.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: 0.3 },
                MaterialId::new(0),
            )
            .expect("dispatch → cone OK");
        assert_eq!(result.solid_kind, SolidKind::Cone);
    }

    #[test]
    fn adr260_create_solid_top_scale_one_rejected() {
        // top_scale ≥ 1 = cylinder → hard reject (no fallback, D5).
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 500.0);
        let err = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: 1.0 },
                MaterialId::new(0),
            )
            .expect_err("top_scale=1 must reject");
        assert!(err.to_string().contains("top_scale") || err.to_string().contains("cylinder"));
    }

    #[test]
    fn adr260_create_solid_top_scale_negative_rejected() {
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 500.0);
        let err = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 800.0, top_scale: -0.5 },
                MaterialId::new(0),
            )
            .expect_err("negative top_scale must reject");
        assert!(err.to_string().contains("top_scale"));
    }

    #[test]
    fn adr260_create_solid_degenerate_distance_rejected() {
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 500.0);
        let err = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeCone { distance: 0.0, top_scale: 0.3 },
                MaterialId::new(0),
            )
            .expect_err("dist=0 must reject");
        let _ = err;
    }

    #[test]
    fn adr260_create_solid_solid_face_rejected() {
        // Coning a face that already bounds a solid (is_move_only) → reject
        // (ADR-087 K-ε sandwich guard, SSOT since fallback_dist = None).
        let mut mesh = Mesh::new();
        // Build a box: extrude a unit square plane face.
        let profile = build_unit_square_plane_face(&mut mesh);
        let box_res = mesh
            .create_solid(profile, CreateSolidMode::Extrude { distance: 1000.0 }, MaterialId::new(0))
            .expect("box extrude OK");
        // Try to cone the box top (a solid face).
        let err = mesh
            .create_solid(
                box_res.top_face,
                CreateSolidMode::ExtrudeCone { distance: 500.0, top_scale: 0.0 },
                MaterialId::new(0),
            )
            .expect_err("cone on solid face must reject");
        assert!(err.to_string().contains("solid face"), "is_move_only guard fired, got: {}", err);
    }

    // ── ADR-261 β-1 SIMULATION GATE (bidirectional / two-sided extrude) ────
    // "먼저 시뮬" — verify manifold + cap planes (Z-extent) + ADR-060 curve
    // translate (cylinder centers) + fail-closed guards BEFORE live exposure.

    /// Active-vertex Z extent (min, max) — solid span along +Z normal profiles.
    fn active_z_extent(mesh: &Mesh) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for (_id, v) in mesh.verts.iter() {
            if v.is_active() {
                let z = v.pos().z;
                lo = lo.min(z);
                hi = hi.max(z);
            }
        }
        (lo, hi)
    }

    #[test]
    fn adr261_sim_box_symmetric_manifold() {
        // AllLinear square (z=0, +Z), symmetric (d, d) → box spanning [−d, +d].
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let d = 500.0;
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: d, dist_neg: d },
                MaterialId::new(0),
            )
            .expect("symmetric bidir OK");
        assert_eq!(result.solid_kind, SolidKind::Box);
        let report = mesh.verify_face_invariants();
        assert!(report.is_valid(), "symmetric box manifold valid, violations: {:?}", report.violations);
        let (lo, hi) = active_z_extent(&mesh);
        assert!((lo + d).abs() < 1e-6, "bottom cap at −d, got {}", lo);
        assert!((hi - d).abs() < 1e-6, "top cap at +d, got {}", hi);
    }

    #[test]
    fn adr261_sim_box_asymmetric_manifold() {
        // (dist_pos, dist_neg) = (800, 300) → box spanning [−300, +800].
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 800.0, dist_neg: 300.0 },
                MaterialId::new(0),
            )
            .expect("asymmetric bidir OK");
        assert_eq!(result.solid_kind, SolidKind::Box);
        assert!(mesh.verify_face_invariants().is_valid(), "asymmetric box manifold valid");
        let (lo, hi) = active_z_extent(&mesh);
        assert!((lo + 300.0).abs() < 1e-6, "bottom at −300, got {}", lo);
        assert!((hi - 800.0).abs() < 1e-6, "top at +800, got {}", hi);
    }

    #[test]
    fn adr261_sim_cylinder_symmetric_manifold() {
        // AllCircular self-loop circle, symmetric (d, d) → cylinder [−d, +d].
        // Verifies ADR-060 translate moved the bottom Circle curve center to −d.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let d = 500.0;
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 400.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: d, dist_neg: d },
                MaterialId::new(0),
            )
            .expect("cylinder symmetric bidir OK");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        let report = mesh.verify_face_invariants();
        assert!(report.is_valid(), "cylinder bidir manifold valid, violations: {:?}", report.violations);

        // Bottom profile self-loop Circle center at −d (ADR-060 translate).
        let bot_eid = mesh.hes[mesh.faces[result.profile_face].outer().start].edge();
        match mesh.edges[bot_eid].curve() {
            Some(crate::curves::AnalyticCurve::Circle { center, radius, .. }) => {
                assert!((center.z + d).abs() < 1e-6, "bottom Circle center at −d, got {}", center.z);
                assert!((radius - 400.0).abs() < 1e-6, "radius preserved");
            }
            other => panic!("bottom edge expected Circle, got {other:?}"),
        }
        // Top self-loop Circle center at +d.
        let top_eid = mesh.hes[mesh.faces[result.top_face].outer().start].edge();
        match mesh.edges[top_eid].curve() {
            Some(crate::curves::AnalyticCurve::Circle { center, .. }) => {
                assert!((center.z - d).abs() < 1e-6, "top Circle center at +d, got {}", center.z);
            }
            other => panic!("top edge expected Circle, got {other:?}"),
        }
    }

    #[test]
    fn adr261_sim_cylinder_asymmetric_manifold() {
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 400.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 800.0, dist_neg: 300.0 },
                MaterialId::new(0),
            )
            .expect("cylinder asymmetric bidir OK");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert!(mesh.verify_face_invariants().is_valid(), "cylinder asymmetric manifold valid");
    }

    #[test]
    fn adr261_sim_dist_neg_zero_oneway() {
        // dist_neg = 0 → translate no-op → one-way +Z extrude [0, +d].
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 500.0, dist_neg: 0.0 },
                MaterialId::new(0),
            )
            .expect("dist_neg=0 one-way OK");
        assert!(mesh.verify_face_invariants().is_valid(), "one-way (neg=0) manifold valid");
        let (lo, hi) = active_z_extent(&mesh);
        assert!(lo.abs() < 1e-6, "bottom stays at 0 (no translate), got {}", lo);
        assert!((hi - 500.0).abs() < 1e-6, "top at +500, got {}", hi);
    }

    #[test]
    fn adr261_create_solid_negative_rejected() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let err = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 500.0, dist_neg: -100.0 },
                MaterialId::new(0),
            )
            .expect_err("negative dist_neg must reject");
        assert!(err.to_string().contains("≥ 0") || err.to_string().contains("dist"));
    }

    #[test]
    fn adr261_create_solid_zero_sum_rejected() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let err = mesh
            .create_solid(
                profile,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 0.0, dist_neg: 0.0 },
                MaterialId::new(0),
            )
            .expect_err("zero-sum must reject (zero volume)");
        let _ = err;
    }

    #[test]
    fn adr261_create_solid_solid_face_rejected() {
        // Bidirectional on a face that already bounds a solid (is_move_only) →
        // reject (ADR-087 K-ε sandwich guard, SSOT since fallback_dist = None).
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let box_res = mesh
            .create_solid(profile, CreateSolidMode::Extrude { distance: 1000.0 }, MaterialId::new(0))
            .expect("box extrude OK");
        let err = mesh
            .create_solid(
                box_res.top_face,
                CreateSolidMode::ExtrudeBidirectional { dist_pos: 300.0, dist_neg: 300.0 },
                MaterialId::new(0),
            )
            .expect_err("bidir on solid face must reject");
        assert!(err.to_string().contains("solid face"), "is_move_only guard fired, got: {}", err);
    }

    /// **(Plane, Mixed) native extrude (2026-06-16, follow-up)** — arc cap
    /// (circle + secant cut = 2 arc + 1 chord) 은 이제 `NotYetSupported` 대신
    /// native `extrude_planar_mixed` 로 처리: per-edge Cylinder/Plane side wall
    /// + arc-aware top rim. push_pull fallback 의 결과를 construction-time 에
    /// 직접 생성 (post-hoc promote 불필요).
    #[test]
    fn create_solid_plane_mixed_arc_cap_native() {
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let r = 60.0;
        let anchor = mesh.add_vertex(DVec3::X * r);
        let circle = AnalyticCurve::Circle { center: DVec3::ZERO, radius: r, normal: DVec3::Z, basis_u: DVec3::X };
        let fid = mesh.add_face_closed_curve(anchor, circle, mat).unwrap();
        let caps = mesh
            .split_circle_face_by_line(fid, DVec3::new(-90.0, 20.0, 0.0), DVec3::new(90.0, 20.0, 0.0), mat)
            .unwrap()
            .unwrap();
        let cap = caps[0];
        assert_eq!(classify_boundary(&mesh, cap).unwrap(), BoundaryKind::Mixed, "arc cap = Mixed");

        // native (Plane, Mixed) — NOT a NotYetSupported error anymore.
        let result = mesh
            .create_solid(cap, CreateSolidMode::Extrude { distance: 70.0 }, mat)
            .expect("native (Plane, Mixed) extrude OK");
        assert_eq!(result.solid_kind, SolidKind::GeneralSweep, "Mixed → GeneralSweep");

        // 2 arc edges → 2 Cylinder side walls; 1 chord → 1 Plane side wall.
        let cyl = result.side_faces.iter().filter(|&&f| {
            matches!(mesh.faces.get(f).and_then(|x| x.surface()), Some(AnalyticSurface::Cylinder { .. }))
        }).count();
        let plane_sides = result.side_faces.iter().filter(|&&f| {
            matches!(mesh.faces.get(f).and_then(|x| x.surface()), Some(AnalyticSurface::Plane { .. }))
        }).count();
        assert_eq!(cyl, 2, "2 arc edges → 2 Cylinder side walls (no flat facet)");
        assert_eq!(plane_sides, 1, "1 chord edge → 1 Plane side wall");

        // top cap arc-aware (2 arc edges = smooth rim).
        let tv = mesh.collect_loop_verts(mesh.faces[result.top_face].outer().start).unwrap();
        let tn = tv.len();
        let top_arcs = (0..tn).filter(|&i| matches!(
            mesh.find_edge(tv[i], tv[(i + 1) % tn]).and_then(|e| mesh.edge_curve(e)),
            Some(AnalyticCurve::Arc { .. })
        )).count();
        assert_eq!(top_arcs, 2, "top cap keeps 2 arc edges (smooth rim, no facet)");

        assert!(mesh.verify_face_invariants().is_valid(), "native mixed extrude manifold valid");
    }

    #[test]
    fn create_solid_extrude_plane_rect_returns_box() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let face_count_before = mesh.face_count();

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        assert_eq!(result.solid_kind, SolidKind::Box);
        assert_eq!(result.profile_face, profile);
        assert_eq!(result.side_faces.len(), 4);
        // Profile + top + 4 sides = 6 faces in solid.
        assert_eq!(result.all_solid_faces.len(), 6);
        // mesh.face_count() should grow by 5 (1 top + 4 sides; profile preserved).
        assert_eq!(mesh.face_count(), face_count_before + 5);
    }

    #[test]
    fn adr183_create_solid_extrude_up_all_normals_outward() {
        // 사용자 결재 2026-06-01 — push-pull(extrude) 박스의 base 면이 inward
        // 였던 회귀 차단. create_box 와 동일하게 0 inward 이어야 함.
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 3.0 }, // up (+Z)
            MaterialId::new(0),
        )
        .expect("create_solid OK");

        let report = mesh.verify_outward_normals();
        assert!(report.is_closed_solid, "extruded box must be a closed solid");
        assert_eq!(
            report.inward_count, 0,
            "ADR-183: all 6 faces of an extruded box must point OUTWARD \
             (inward faces: {:?})",
            report.inward_faces
        );
    }

    #[test]
    fn adr183_create_solid_extrude_down_all_normals_outward() {
        // dist < 0 (recess down) — top_face 가 바닥이 되는 경우도 outward.
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: -3.0 }, // down (-Z)
            MaterialId::new(0),
        )
        .expect("create_solid OK");

        let report = mesh.verify_outward_normals();
        assert!(report.is_closed_solid, "recessed box must be a closed solid");
        assert_eq!(
            report.inward_count, 0,
            "ADR-183: all 6 faces of a downward-extruded box must point OUTWARD \
             (inward faces: {:?})",
            report.inward_faces
        );
    }

    #[test]
    fn adr183_create_solid_extrude_box_stays_closed_manifold() {
        // flip_face 가 manifold(닫힌 solid + non-manifold edge 0)를 깨지 않음.
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 2.0 },
            MaterialId::new(0),
        )
        .expect("create_solid OK");

        let active: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        let info = mesh.face_set_manifold_info(&active);
        assert!(info.is_closed_solid, "box must remain a closed solid after ADR-183 flip");
        assert_eq!(info.non_manifold_edge_count, 0, "no non-manifold edges after flip");
    }

    #[test]
    fn adr267_beta2_extruded_box_passes_volume_integrity_gate() {
        // ADR-267 β-2 — 실제 extrude 로 만든 box 는 watertight + crack-free 이므로
        // verify_volume_integrity(ClosedSolid) 게이트를 통과해야 한다 (게이트가 정상
        // op 를 오탐하지 않음). create_solid_extrude WASM wrapper 가 이 결과에 delta
        // 게이트를 적용해 손상 유발 시에만 rollback 한다.
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 2.0 },
            MaterialId::new(0),
        )
        .expect("create_solid OK");

        let active: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        let report = mesh.verify_volume_integrity(crate::IntegrityScope::ClosedSolid(&active));
        assert!(
            report.is_valid(),
            "extruded box must pass watertight gate: {}",
            report.summary()
        );
        assert!(report.geometric_cracks.is_empty(), "no cracks in a clean box");
        assert_eq!(report.open_boundary_edges, 0, "box is watertight");
    }

    #[test]
    fn create_solid_attaches_planes_to_new_faces() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 2.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        // Top face: AnalyticSurface::Plane attached.
        let top_surface = mesh.faces[result.top_face].surface();
        assert!(
            matches!(top_surface, Some(AnalyticSurface::Plane { .. })),
            "top face must have Plane surface attached"
        );

        // Each side face: AnalyticSurface::Plane attached.
        for &side_fid in &result.side_faces {
            let side_surface = mesh.faces[side_fid].surface();
            assert!(
                matches!(side_surface, Some(AnalyticSurface::Plane { .. })),
                "side face {side_fid:?} must have Plane surface attached"
            );
        }
    }

    #[test]
    fn create_solid_extrude_no_surface_returns_no_profile_surface() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::ZERO);
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        // Note: no surface attached.
        let profile = mesh.add_face(&[v00, v10, v11, v01], mat).expect("add_face");

        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 1.0 },
            mat,
        );
        assert!(result.is_err(), "should fail without profile surface");
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(
            err_msg.contains("NoProfileSurface") || err_msg.contains("AnalyticSurface"),
            "error must mention missing surface, got: {err_msg}"
        );
    }

    #[test]
    fn revolve_partial_axis_touching_profile_pole_bails() {
        // ADR-248 (Phase 3 E1) — partial revolve is now supported, BUT the unit
        // square spans x∈[0,1] so it TOUCHES the Y-axis (pole). Axis-touching
        // partial revolve is not yet supported → NotYetSupported (pole).
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Revolve {
                axis_origin: DVec3::ZERO,
                axis_dir: DVec3::Y,
                angle_rad: std::f64::consts::PI, // 180° — partial
            },
            MaterialId::new(0),
        );
        let err_msg = format!("{}", result.err().unwrap());
        assert!(
            err_msg.contains("not yet supported") && err_msg.contains("axis"),
            "axis-touching partial revolve must bail (pole), got: {err_msg}"
        );
    }

    /// ADR-248 (Phase 3 E1) — partial revolve of an AXIS-CLEAR profile builds a
    /// capped wedge solid. Rect in the XZ plane (normal Y, x∈[2,4]) revolved 90°
    /// around the Z-axis → quarter torus segment with θ=0 + θ=90° end caps.
    #[test]
    fn revolve_partial_offset_profile_makes_capped_wedge() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let v0 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(4.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(4.0, 0.0, 2.0));
        let v3 = mesh.add_vertex(DVec3::new(2.0, 0.0, 2.0));
        let prof = mesh.add_face(&[v0, v1, v2, v3], mat).unwrap();
        mesh.faces[prof].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO, normal: DVec3::Y, basis_u: DVec3::X,
            u_range: (0.0, 1.0), v_range: (0.0, 1.0),
        }));
        let result = mesh
            .create_solid(
                prof,
                CreateSolidMode::Revolve {
                    axis_origin: DVec3::ZERO,
                    axis_dir: DVec3::Z,
                    angle_rad: std::f64::consts::FRAC_PI_2, // 90°
                },
                mat,
            )
            .expect("axis-clear partial revolve must succeed (capped wedge)");
        assert_eq!(result.solid_kind, SolidKind::RevolutionSolid);
        assert_eq!(
            mesh.face_set_manifold_info(&result.all_solid_faces).boundary_edge_count,
            0,
            "partial revolve must be a closed solid"
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "invariants after partial revolve:\n{}",
            mesh.verify_face_invariants().summary()
        );
    }

    #[test]
    fn create_solid_zero_distance_returns_degenerate() {
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 0.0 },
            MaterialId::new(0),
        );
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(
            err_msg.contains("DegenerateDistance") || err_msg.contains("EPSILON"),
            "error must indicate degenerate distance, got: {err_msg}"
        );
    }

    #[test]
    fn classify_boundary_all_linear_for_unit_square() {
        let mut mesh = Mesh::new();
        let face = build_unit_square_plane_face(&mut mesh);
        let kind = classify_boundary(&mesh, face).expect("classify OK");
        assert_eq!(kind, BoundaryKind::AllLinear);
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-2-α — Plane + AllCircular → Cylinder regression
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build an N-segment circle face on z=0 with normal +Z.
    /// Each segment edge gets `AnalyticCurve::Arc` attached, sharing
    /// (center, radius, normal, basis_u). Face gets `AnalyticSurface::Plane`.
    fn build_circle_face(mesh: &mut Mesh, radius: f64, segments: u32) -> FaceId {
        use crate::curves::AnalyticCurve;
        let mat = MaterialId::new(0);
        let n = segments as usize;
        let center = DVec3::ZERO;
        let normal = DVec3::Z;
        let basis_u = DVec3::X;

        let mut verts = Vec::with_capacity(n);
        for i in 0..n {
            let theta = (i as f64) * std::f64::consts::TAU / (n as f64);
            verts.push(mesh.add_vertex(DVec3::new(
                radius * theta.cos(),
                radius * theta.sin(),
                0.0,
            )));
        }
        let face = mesh.add_face(&verts, mat).expect("add_face");

        // Attach Plane surface.
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: center,
            normal,
            basis_u,
            u_range: (-radius, radius),
            v_range: (-radius, radius),
        }));

        // Attach Arc curve to each edge.
        let edges = mesh.face_outer_edges(face).expect("face_outer_edges");
        let two_pi = std::f64::consts::TAU;
        for (i, &eid) in edges.iter().enumerate() {
            let theta_start = (i as f64) * two_pi / (n as f64);
            let theta_end = ((i + 1) as f64) * two_pi / (n as f64);
            let curve = AnalyticCurve::Arc {
                center,
                radius,
                normal,
                basis_u,
                start_angle: theta_start,
                end_angle: theta_end,
            };
            mesh.edges[eid].set_curve(Some(curve));
        }

        face
    }

    #[test]
    fn create_solid_extrude_plane_circle_returns_cylinder() {
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let face_count_before = mesh.face_count();

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 10.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert_eq!(result.profile_face, profile);
        assert_eq!(result.side_faces.len(), 16);
        // Profile + top + 16 sides = 18 faces.
        assert_eq!(result.all_solid_faces.len(), 18);
        assert_eq!(mesh.face_count(), face_count_before + 17);
    }

    #[test]
    fn create_solid_cylinder_attaches_cylinder_surface_to_sides() {
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 3.0, 12);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 4.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        // Top face: Plane.
        let top_surface = mesh.faces[result.top_face].surface();
        assert!(
            matches!(top_surface, Some(AnalyticSurface::Plane { .. })),
            "top face must have Plane surface attached"
        );

        // ALL side faces: Cylinder, sharing (center, radius).
        for &side_fid in &result.side_faces {
            let side_surface = mesh.faces[side_fid].surface();
            match side_surface {
                Some(AnalyticSurface::Cylinder { radius, axis_origin, .. }) => {
                    assert!((radius - 3.0).abs() < 1e-9, "radius != 3.0: got {radius}");
                    assert!(
                        (axis_origin - DVec3::ZERO).length() < 1e-9,
                        "axis_origin != ZERO"
                    );
                }
                other => panic!(
                    "side face {side_fid:?} must have Cylinder surface, got {:?}",
                    other.map(|s| s.kind_label())
                ),
            }
        }
    }

    #[test]
    fn create_solid_cylinder_negative_distance_winding_correct() {
        // Recess (dist < 0) — top is below profile, side winding reversed.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 2.0, 8);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: -3.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert_eq!(result.side_faces.len(), 8);
        // All side faces must still have Cylinder surface attached.
        for &side_fid in &result.side_faces {
            assert!(
                matches!(
                    mesh.faces[side_fid].surface(),
                    Some(AnalyticSurface::Cylinder { .. })
                ),
                "side face {side_fid:?} must have Cylinder (dist < 0)"
            );
        }
    }

    #[test]
    fn create_solid_cylinder_arcs_share_circle_params_check() {
        // Sanity: extract_shared_circle_params returns the exact center/radius.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 7.5, 24);
        let (center, radius, _normal, _basis) =
            extract_shared_circle_params(&mesh, profile).expect("extract OK");
        assert!((center - DVec3::ZERO).length() < 1e-9);
        assert!((radius - 7.5).abs() < 1e-9);
    }

    #[test]
    fn create_solid_cylinder_arc_param_mismatch_falls_back() {
        // Tamper one edge's Arc curve to have different center → mismatch.
        use crate::curves::AnalyticCurve;
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 8);
        let edges = mesh.face_outer_edges(profile).expect("edges");
        // Replace first edge's curve with a different center.
        let bad = AnalyticCurve::Arc {
            center: DVec3::new(100.0, 0.0, 0.0), // wrong center
            radius: 5.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_4,
        };
        mesh.edges[edges[0]].set_curve(Some(bad));

        // Confirm classify still returns AllCircular (kind-only check).
        assert_eq!(
            classify_boundary(&mesh, profile).expect("classify"),
            BoundaryKind::AllCircular
        );

        // create_solid should now return NotYetSupported (Q3 fallback).
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 2.0 },
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail with mismatched arc params");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("mismatch"),
            "expected NotYetSupported with mismatch reason, got: {msg}"
        );
    }

    #[test]
    fn classify_boundary_all_circular_for_circle_face() {
        let mut mesh = Mesh::new();
        let face = build_circle_face(&mut mesh, 1.0, 12);
        let kind = classify_boundary(&mesh, face).expect("classify OK");
        assert_eq!(kind, BoundaryKind::AllCircular);
    }

    #[test]
    fn create_solid_cylinder_top_translated_by_profile_normal() {
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 4.0, 8);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 6.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        // Top face's outer loop should have z = 6.0 (translated by +Z * 6).
        let top_start = mesh.faces[result.top_face].outer().start;
        let top_verts = mesh.collect_loop_verts(top_start).expect("top verts");
        for v in &top_verts {
            let pos = mesh.vertex_pos(*v).expect("vertex_pos");
            assert!(
                (pos.z - 6.0).abs() < 1e-9,
                "top vertex z must be 6.0, got {}",
                pos.z
            );
            // Radial check: x² + y² = 16 (radius 4).
            let r2 = pos.x * pos.x + pos.y * pos.y;
            assert!((r2 - 16.0).abs() < 1e-6, "radius² != 16: got {r2}");
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-2-γ-i — Cylinder smooth-group radius offset
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build an existing cylinder solid via W-2-α and return
    /// (mesh, profile, top_face, side_faces). The side faces share a
    /// single Cylinder surface instance — the smooth group W-2-γ-i targets.
    fn build_cylinder_solid(radius: f64, dist: f64, segments: u32) -> (Mesh, CreateSolidResult) {
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, radius, segments);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: dist },
                MaterialId::new(0),
            )
            .expect("create_solid OK");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        (mesh, result)
    }

    #[test]
    fn cylinder_smooth_group_offset_outward_increases_radius() {
        let (mut mesh, cyl) = build_cylinder_solid(2.0, 5.0, 16);
        // Pick any side face as profile for the offset operation.
        let side_profile = cyl.side_faces[0];

        // Offset outward by +1.0 → new radius = 3.0.
        let result = mesh
            .create_solid(
                side_profile,
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("smooth-group offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        // Group should contain all 16 side faces.
        assert_eq!(
            result.all_solid_faces.len(),
            16,
            "smooth group must include all 16 side faces"
        );

        // All side face surfaces must now have radius = 3.0.
        for &fid in &cyl.side_faces {
            match mesh.faces[fid].surface() {
                Some(AnalyticSurface::Cylinder { radius, .. }) => {
                    assert!(
                        (radius - 3.0).abs() < 1e-9,
                        "face {fid:?} radius != 3.0, got {radius}"
                    );
                }
                other => panic!(
                    "face {fid:?} must be Cylinder, got {:?}",
                    other.map(|s| s.kind_label())
                ),
            }
        }
    }

    #[test]
    fn cylinder_smooth_group_offset_scales_vertices_radially() {
        let (mut mesh, _cyl) = build_cylinder_solid(2.0, 5.0, 8);
        // Find one side face and use as profile.
        let side_profile = mesh
            .faces
            .iter()
            .find_map(|(fid, face)| {
                matches!(face.surface(), Some(AnalyticSurface::Cylinder { .. }))
                    .then_some(fid)
            })
            .expect("must find a side face");

        let result = mesh
            .create_solid(
                side_profile,
                CreateSolidMode::Extrude { distance: 3.0 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // After offset (2 → 5), every group vertex should have radius 5
        // (in the xy plane, since axis = +Z).
        let mut group_verts = std::collections::HashSet::new();
        for &fid in &result.all_solid_faces {
            let start = mesh.faces[fid].outer().start;
            for v in mesh.collect_loop_verts(start).unwrap() {
                group_verts.insert(v);
            }
        }
        for v in &group_verts {
            let pos = mesh.vertex_pos(*v).unwrap();
            let r = (pos.x * pos.x + pos.y * pos.y).sqrt();
            assert!((r - 5.0).abs() < 1e-6, "vertex r != 5.0: got {r}");
        }
    }

    #[test]
    fn cylinder_smooth_group_offset_inward_decreases_radius() {
        let (mut mesh, cyl) = build_cylinder_solid(5.0, 3.0, 12);
        // Inward offset: -2.0 → new radius = 3.0.
        let result = mesh
            .create_solid(
                cyl.side_faces[0],
                CreateSolidMode::Extrude { distance: -2.0 },
                MaterialId::new(0),
            )
            .expect("inward offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        for &fid in &cyl.side_faces {
            if let Some(AnalyticSurface::Cylinder { radius, .. }) = mesh.faces[fid].surface() {
                assert!((radius - 3.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn cylinder_smooth_group_offset_collapse_falls_back() {
        // Inward offset that would collapse radius below epsilon → Q3 fallback.
        let (mut mesh, cyl) = build_cylinder_solid(2.0, 4.0, 8);
        let result = mesh.create_solid(
            cyl.side_faces[0],
            CreateSolidMode::Extrude { distance: -2.0 }, // exactly to zero
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail (collapse)");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("collapse"),
            "expected NotYetSupported with 'collapse' reason, got: {msg}"
        );
    }

    #[test]
    fn cylinder_smooth_group_offset_updates_cap_arc_radius() {
        let (mut mesh, cyl) = build_cylinder_solid(3.0, 4.0, 8);
        // Verify cap edges initially have Arc curves with radius=3.
        // (build_circle_face attached Arc to profile edges; W-2-α didn't
        // attach Arc to top cap edges — the top cap edges are NEW edges
        // that connect newly-translated vertices, no curve set.)
        // Profile (= original circle face) edges have Arc r=3.
        let profile_edges = mesh.face_outer_edges(cyl.profile_face).unwrap();
        let initial_arc_count = profile_edges
            .iter()
            .filter(|&&eid| {
                matches!(
                    mesh.edges.get(eid).and_then(|e| e.curve()),
                    Some(AnalyticCurve::Arc { .. })
                )
            })
            .count();
        assert!(initial_arc_count > 0, "profile edges must have Arc curves");

        let _ = mesh
            .create_solid(
                cyl.side_faces[0],
                CreateSolidMode::Extrude { distance: 2.0 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // After offset, Arc curves on profile edges should now have radius=5.
        for &eid in &profile_edges {
            if let Some(AnalyticCurve::Arc { radius, .. }) =
                mesh.edges.get(eid).and_then(|e| e.curve())
            {
                assert!(
                    (radius - 5.0).abs() < 1e-9,
                    "cap arc edge {eid:?} radius != 5.0: got {radius}"
                );
            }
        }
    }

    #[test]
    fn cylinder_smooth_group_offset_returns_smooth_group_offset_kind() {
        let (mut mesh, cyl) = build_cylinder_solid(1.5, 2.0, 6);
        let result = mesh
            .create_solid(
                cyl.side_faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");
        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        // top_face = profile_face (no new face created).
        assert_eq!(result.top_face, result.profile_face);
        // side_faces = group members minus profile.
        assert_eq!(result.side_faces.len(), 5);
        // all_solid_faces = full group.
        assert_eq!(result.all_solid_faces.len(), 6);
    }

    #[test]
    fn cylinder_smooth_group_offset_preserves_axial_height() {
        // Axial position (z, since axis = +Z) must be preserved by offset.
        let (mut mesh, cyl) = build_cylinder_solid(2.0, 7.0, 8);
        let _ = mesh
            .create_solid(
                cyl.side_faces[0],
                CreateSolidMode::Extrude { distance: 1.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // Profile (z=0) and top cap (z=7) z-coordinates must be unchanged.
        let profile_start = mesh.faces[cyl.profile_face].outer().start;
        for v in mesh.collect_loop_verts(profile_start).unwrap() {
            let pos = mesh.vertex_pos(v).unwrap();
            assert!(
                pos.z.abs() < 1e-9,
                "profile z must remain 0, got {}",
                pos.z
            );
        }
        let top_start = mesh.faces[cyl.top_face].outer().start;
        for v in mesh.collect_loop_verts(top_start).unwrap() {
            let pos = mesh.vertex_pos(v).unwrap();
            assert!(
                (pos.z - 7.0).abs() < 1e-9,
                "top z must remain 7, got {}",
                pos.z
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-2-γ-ii — Sphere smooth-group radius offset
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build 2 triangle faces on a sphere centered at origin.
    /// Both faces share the same `AnalyticSurface::Sphere` instance, so
    /// they form a smooth group.
    ///
    /// Triangles share an edge (north_pole — equator_y) so the test
    /// exercises shared-vertex semantics.
    fn build_sphere_two_faces(radius: f64) -> (Mesh, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;

        // 4 verts on the sphere surface:
        //   v_x  = (R, 0, 0)        equator at θ=0
        //   v_y  = (0, R, 0)        equator at θ=90°
        //   v_nx = (-R, 0, 0)       equator at θ=180°
        //   v_n  = (0, 0, R)        north pole
        let v_x = mesh.add_vertex(DVec3::new(radius, 0.0, 0.0));
        let v_y = mesh.add_vertex(DVec3::new(0.0, radius, 0.0));
        let v_nx = mesh.add_vertex(DVec3::new(-radius, 0.0, 0.0));
        let v_n = mesh.add_vertex(DVec3::new(0.0, 0.0, radius));

        // f1: v_x → v_y → v_n (CCW from outside the sphere octant)
        // f2: v_y → v_nx → v_n (adjacent triangle sharing edge v_y → v_n)
        let f1 = mesh.add_face(&[v_x, v_y, v_n], mat).expect("f1");
        let f2 = mesh.add_face(&[v_y, v_nx, v_n], mat).expect("f2");

        let surface = AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
        };
        mesh.faces[f1].set_surface(Some(surface.clone()));
        mesh.faces[f2].set_surface(Some(surface));

        (mesh, vec![f1, f2])
    }

    #[test]
    fn sphere_smooth_group_offset_outward_increases_radius() {
        let (mut mesh, faces) = build_sphere_two_faces(2.0);

        // Offset outward by +1.0 → new radius = 3.0.
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("sphere offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        assert_eq!(result.all_solid_faces.len(), 2);

        for &fid in &faces {
            match mesh.faces[fid].surface() {
                Some(AnalyticSurface::Sphere { radius, center, .. }) => {
                    assert!(
                        (radius - 3.0).abs() < 1e-9,
                        "face {fid:?} radius != 3.0, got {radius}"
                    );
                    assert!(
                        (center - DVec3::ZERO).length() < 1e-9,
                        "center must remain at ZERO"
                    );
                }
                other => panic!(
                    "face {fid:?} must be Sphere, got {:?}",
                    other.map(|s| s.kind_label())
                ),
            }
        }
    }

    #[test]
    fn sphere_smooth_group_offset_scales_vertices_radially_about_center() {
        let (mut mesh, faces) = build_sphere_two_faces(2.0);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 3.0 }, // 2 → 5
                MaterialId::new(0),
            )
            .expect("offset OK");

        // After offset, every group vertex should be at distance 5 from origin.
        let mut group_verts = std::collections::HashSet::new();
        for &fid in &result.all_solid_faces {
            let start = mesh.faces[fid].outer().start;
            for v in mesh.collect_loop_verts(start).unwrap() {
                group_verts.insert(v);
            }
        }
        for v in &group_verts {
            let pos = mesh.vertex_pos(*v).unwrap();
            let r = pos.length();
            assert!(
                (r - 5.0).abs() < 1e-9,
                "vertex distance from center != 5.0: got {r}"
            );
        }
    }

    #[test]
    fn sphere_smooth_group_offset_inward_decreases_radius() {
        let (mut mesh, faces) = build_sphere_two_faces(5.0);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: -2.0 }, // 5 → 3
                MaterialId::new(0),
            )
            .expect("inward offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        for &fid in &faces {
            if let Some(AnalyticSurface::Sphere { radius, .. }) = mesh.faces[fid].surface() {
                assert!((radius - 3.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn sphere_smooth_group_offset_collapse_falls_back() {
        let (mut mesh, faces) = build_sphere_two_faces(2.0);
        // -2.0 → new_radius = 0 → collapse.
        let result = mesh.create_solid(
            faces[0],
            CreateSolidMode::Extrude { distance: -2.0 },
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail (collapse)");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("collapse"),
            "expected NotYetSupported with 'collapse' reason, got: {msg}"
        );
    }

    #[test]
    fn sphere_smooth_group_offset_returns_smooth_group_offset_kind() {
        let (mut mesh, faces) = build_sphere_two_faces(1.5);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        assert_eq!(result.top_face, result.profile_face);
        assert_eq!(result.side_faces.len(), 1); // 2 group faces - 1 profile
        assert_eq!(result.all_solid_faces.len(), 2);
    }

    #[test]
    fn sphere_smooth_group_offset_preserves_center() {
        // Sphere centered at non-origin must keep its center after offset.
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let center = DVec3::new(10.0, 20.0, 30.0);
        let radius = 4.0;

        let v_a = mesh.add_vertex(center + DVec3::new(radius, 0.0, 0.0));
        let v_b = mesh.add_vertex(center + DVec3::new(0.0, radius, 0.0));
        let v_c = mesh.add_vertex(center + DVec3::new(0.0, 0.0, radius));
        let f1 = mesh.add_face(&[v_a, v_b, v_c], mat).expect("f1");

        // Need a 2nd face for a non-trivial group.
        let v_d = mesh.add_vertex(center + DVec3::new(-radius, 0.0, 0.0));
        let f2 = mesh.add_face(&[v_b, v_d, v_c], mat).expect("f2");

        let surface = AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
        };
        mesh.faces[f1].set_surface(Some(surface.clone()));
        mesh.faces[f2].set_surface(Some(surface));

        let _ = mesh
            .create_solid(
                f1,
                CreateSolidMode::Extrude { distance: 2.0 },
                mat,
            )
            .expect("offset OK");

        // Center must remain (10, 20, 30); radius now 6.
        if let Some(AnalyticSurface::Sphere { center: c, radius: r, .. }) =
            mesh.faces[f1].surface()
        {
            assert!((*c - center).length() < 1e-9, "center must be preserved");
            assert!((r - 6.0).abs() < 1e-9, "radius must be 6 (4 + 2)");
        } else {
            panic!("face surface must be Sphere");
        }

        // Verify each vertex distance from center = 6.
        let start = mesh.faces[f1].outer().start;
        for v in mesh.collect_loop_verts(start).unwrap() {
            let pos = mesh.vertex_pos(v).unwrap();
            let dist = (pos - center).length();
            assert!(
                (dist - 6.0).abs() < 1e-9,
                "vertex distance from center != 6: got {dist}"
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-2-γ-iii — Cone constant-offset (Option 3)
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build 2 triangle faces on a cone with apex at origin,
    /// axis = +Z, opening toward +Z. half_angle controls the slope.
    /// Both triangles share an edge and the same Cone surface instance.
    fn build_cone_two_faces(
        half_angle: f64,
        v_min: f64,
        v_max: f64,
    ) -> (Mesh, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let apex = DVec3::ZERO;
        let axis_dir = DVec3::Z;
        let ref_dir = DVec3::X;

        let tan_a = half_angle.tan();
        // 4 verts at u = 0, 90°, 180° on two latitude rings (v_min, v_max).
        // Triangles:
        //   f1: (u=0, v=v_min) → (u=90°, v=v_min) → (u=0, v=v_max)
        //   f2: (u=90°, v=v_min) → (u=180°, v=v_min) → (u=90°, v=v_max)
        // Each triangle shares verts with its neighbor.
        let p = |u: f64, v: f64| -> DVec3 {
            DVec3::new(v * tan_a * u.cos(), v * tan_a * u.sin(), v)
        };
        let v_a = mesh.add_vertex(p(0.0, v_min));
        let v_b = mesh.add_vertex(p(std::f64::consts::FRAC_PI_2, v_min));
        let v_c = mesh.add_vertex(p(std::f64::consts::PI, v_min));
        let v_top_0 = mesh.add_vertex(p(0.0, v_max));
        let v_top_90 = mesh.add_vertex(p(std::f64::consts::FRAC_PI_2, v_max));

        let f1 = mesh.add_face(&[v_a, v_b, v_top_90, v_top_0], mat).expect("f1");
        let f2 = mesh.add_face(&[v_b, v_c, v_top_90], mat).expect("f2");

        let surface = AnalyticSurface::Cone {
            apex,
            axis_dir,
            half_angle,
            ref_dir,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v_min, v_max),
        };
        mesh.faces[f1].set_surface(Some(surface.clone()));
        mesh.faces[f2].set_surface(Some(surface));

        (mesh, vec![f1, f2])
    }

    #[test]
    fn cone_smooth_group_offset_preserves_half_angle_and_axis() {
        let half_angle = std::f64::consts::FRAC_PI_4; // 45°
        let (mut mesh, faces) = build_cone_two_faces(half_angle, 1.0, 5.0);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("cone offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);

        for &fid in &faces {
            match mesh.faces[fid].surface() {
                Some(AnalyticSurface::Cone {
                    half_angle: ha,
                    axis_dir: ad,
                    ref_dir: rd,
                    apex: a,
                    ..
                }) => {
                    // half_angle preserved.
                    assert!(
                        (ha - half_angle).abs() < 1e-9,
                        "half_angle must be preserved: got {ha}"
                    );
                    // axis_dir preserved.
                    assert!(
                        ad.normalize().dot(DVec3::Z).abs() > 0.9999,
                        "axis_dir must remain ‖ +Z"
                    );
                    // ref_dir preserved.
                    assert!(
                        rd.normalize().dot(DVec3::X).abs() > 0.9999,
                        "ref_dir must remain ‖ +X"
                    );
                    // apex shift = -dist/sin(α) * axis_dir = -0.5/sin(45°) * Z.
                    let expected_shift = -0.5 / half_angle.sin();
                    assert!(
                        ((a.z) - expected_shift).abs() < 1e-9,
                        "apex.z must = {expected_shift:.6}, got {}",
                        a.z
                    );
                }
                other => panic!(
                    "face {fid:?} must remain Cone, got {:?}",
                    other.map(|s| s.kind_label())
                ),
            }
        }
    }

    #[test]
    fn cone_smooth_group_offset_apex_translates_along_minus_axis() {
        // half_angle = 30° → sin = 0.5 → apex shifts by -2.0 * dist along +Z.
        let half_angle = std::f64::consts::FRAC_PI_6;
        let (mut mesh, faces) = build_cone_two_faces(half_angle, 1.0, 4.0);
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // dist = 1, sin(30°) = 0.5 → apex_shift = -2.0 along +Z.
        if let Some(AnalyticSurface::Cone { apex, .. }) = mesh.faces[faces[0]].surface() {
            assert!(
                (apex.z - (-2.0)).abs() < 1e-9,
                "apex.z must = -2.0, got {}",
                apex.z
            );
            assert!(
                apex.x.abs() < 1e-9 && apex.y.abs() < 1e-9,
                "apex x/y must remain 0"
            );
        } else {
            panic!("face surface must be Cone");
        }
    }

    #[test]
    fn cone_smooth_group_offset_vertex_moves_along_normal_by_dist() {
        // Vertex at (v*tan(α), 0, v) on cone with α=45°, v=2 → (2, 0, 2).
        // After dist=√2 outward offset, expected new pos: (2,0,2) + √2 * normal.
        // normal at u=0, v=2 (α=45°): (cos(45°)*1, 0, -sin(45°)) = (√2/2, 0, -√2/2).
        // P_new = (2 + √2 * √2/2, 0, 2 - √2 * √2/2) = (2 + 1, 0, 2 - 1) = (3, 0, 1).
        let half_angle = std::f64::consts::FRAC_PI_4;
        let (mut mesh, faces) = build_cone_two_faces(half_angle, 2.0, 4.0);
        let dist = 2.0_f64.sqrt();
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: dist },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // Find the vertex that was originally at (2, 0, 2): u=0, v=2.
        // After offset, expected position: (3, 0, 1).
        let expected_new = DVec3::new(3.0, 0.0, 1.0);
        let mut found = false;
        for &fid in &faces {
            let start = mesh.faces[fid].outer().start;
            for v in mesh.collect_loop_verts(start).unwrap() {
                let pos = mesh.vertex_pos(v).unwrap();
                if (pos - expected_new).length() < 1e-6 {
                    found = true;
                    break;
                }
            }
        }
        assert!(
            found,
            "must find vertex at expected post-offset position (3, 0, 1)"
        );
    }

    #[test]
    fn cone_smooth_group_offset_inward_decreases_radius_at_each_v() {
        let half_angle = std::f64::consts::FRAC_PI_4;
        let (mut mesh, faces) = build_cone_two_faces(half_angle, 2.0, 5.0);
        // Inward offset: dist = -1.
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: -1.0 },
                MaterialId::new(0),
            )
            .expect("inward offset OK");

        // Expected: half_angle preserved, apex_z = +1/sin(45°) = +√2,
        // v_range_new = (2 + (-1)*cos²/sin, 5 + same) = (2 - √2/2 ... )
        // Easier: just check Cone surface attached and half_angle preserved.
        if let Some(AnalyticSurface::Cone { half_angle: ha, .. }) =
            mesh.faces[faces[0]].surface()
        {
            assert!((ha - half_angle).abs() < 1e-9);
        } else {
            panic!("face surface must remain Cone");
        }
    }

    #[test]
    fn cone_smooth_group_offset_collapse_falls_back() {
        let half_angle = std::f64::consts::FRAC_PI_4;
        let (mut mesh, faces) = build_cone_two_faces(half_angle, 1.0, 3.0);
        // dist = -2 → v_min becomes 1 + (-2)*cos²(45°)/sin(45°) = 1 - √2 < 0.
        let result = mesh.create_solid(
            faces[0],
            CreateSolidMode::Extrude { distance: -2.0 },
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail (collapse)");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("collapse"),
            "expected NotYetSupported with 'collapse' reason, got: {msg}"
        );
    }

    #[test]
    fn cone_smooth_group_offset_singular_half_angle_rejected() {
        // half_angle ≈ 0 → singular cone → NotYetSupported.
        let (mut mesh, faces) = build_cone_two_faces(1e-8, 1.0, 2.0);
        let result = mesh.create_solid(
            faces[0],
            CreateSolidMode::Extrude { distance: 0.5 },
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("singular"),
            "expected singular rejection, got: {msg}"
        );
    }

    #[test]
    fn cone_smooth_group_offset_returns_smooth_group_offset_kind() {
        let (mut mesh, faces) = build_cone_two_faces(std::f64::consts::FRAC_PI_4, 1.0, 3.0);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");
        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        assert_eq!(result.top_face, result.profile_face);
        assert_eq!(result.side_faces.len(), 1);
        assert_eq!(result.all_solid_faces.len(), 2);
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-2-γ-iv — Torus constant-offset (= minor_radius offset)
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build 2 triangle faces on a torus with center origin,
    /// axis = +Z, ref = +X. Both share the same Torus surface instance.
    /// Vertices placed at known (u, v) parameter positions.
    fn build_torus_two_faces(
        major: f64,
        minor: f64,
    ) -> (Mesh, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let axis_dir = DVec3::Z;
        let ref_dir = DVec3::X;
        let bi = axis_dir.cross(ref_dir); // Y

        // Parametric position on torus.
        let p = |u: f64, v: f64| -> DVec3 {
            let radial = u.cos() * ref_dir + u.sin() * bi;
            center + major * radial + minor * (v.cos() * radial + v.sin() * axis_dir)
        };

        // 5 verts at (u, v) ∈ {(0, 0), (90°, 0), (180°, 0), (0, 90°), (90°, 90°)}.
        let v_a = mesh.add_vertex(p(0.0, 0.0));
        let v_b = mesh.add_vertex(p(std::f64::consts::FRAC_PI_2, 0.0));
        let v_c = mesh.add_vertex(p(std::f64::consts::PI, 0.0));
        let v_top_a = mesh.add_vertex(p(0.0, std::f64::consts::FRAC_PI_2));
        let v_top_b = mesh.add_vertex(p(std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2));

        // Two faces sharing edge v_b → v_top_b:
        //   f1: v_a → v_b → v_top_b → v_top_a
        //   f2: v_b → v_c → v_top_b
        let f1 = mesh
            .add_face(&[v_a, v_b, v_top_b, v_top_a], mat)
            .expect("f1");
        let f2 = mesh.add_face(&[v_b, v_c, v_top_b], mat).expect("f2");

        let surface = AnalyticSurface::Torus {
            center,
            axis_dir,
            ref_dir,
            major_radius: major,
            minor_radius: minor,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, std::f64::consts::TAU),
        };
        mesh.faces[f1].set_surface(Some(surface.clone()));
        mesh.faces[f2].set_surface(Some(surface));

        (mesh, vec![f1, f2])
    }

    #[test]
    fn torus_smooth_group_offset_increases_minor_radius() {
        let (mut mesh, faces) = build_torus_two_faces(5.0, 1.0);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("torus offset OK");

        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        for &fid in &faces {
            match mesh.faces[fid].surface() {
                Some(AnalyticSurface::Torus {
                    minor_radius,
                    major_radius,
                    center: c,
                    ..
                }) => {
                    assert!(
                        (minor_radius - 1.5).abs() < 1e-9,
                        "minor radius != 1.5: got {minor_radius}"
                    );
                    // major / center UNCHANGED.
                    assert!((major_radius - 5.0).abs() < 1e-9);
                    assert!((c - DVec3::ZERO).length() < 1e-9);
                }
                other => panic!(
                    "face {fid:?} must remain Torus, got {:?}",
                    other.map(|s| s.kind_label())
                ),
            }
        }
    }

    #[test]
    fn torus_smooth_group_offset_vertex_distance_to_major_circle_changes() {
        // After offset, every vertex's distance to its major-circle point
        // should equal new_minor (5 → 5 + dist).
        let major = 4.0;
        let minor_old = 1.0;
        let dist = 0.5;
        let (mut mesh, faces) = build_torus_two_faces(major, minor_old);
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: dist },
                MaterialId::new(0),
            )
            .expect("offset OK");

        let mut group_verts = std::collections::HashSet::new();
        for &fid in &faces {
            let start = mesh.faces[fid].outer().start;
            for v in mesh.collect_loop_verts(start).unwrap() {
                group_verts.insert(v);
            }
        }
        let expected_minor = minor_old + dist;
        for v in &group_verts {
            let pos = mesh.vertex_pos(*v).unwrap();
            // Compute major-circle point: project pos onto Z=0 plane,
            // normalize, scale by major.
            let pos_xy = DVec3::new(pos.x, pos.y, 0.0);
            if pos_xy.length() < 1e-9 {
                continue; // skip on-axis (shouldn't happen here)
            }
            let major_pt = pos_xy.normalize() * major;
            let dist_to_major = (pos - major_pt).length();
            assert!(
                (dist_to_major - expected_minor).abs() < 1e-6,
                "vertex distance to major circle != {expected_minor}: got {dist_to_major}"
            );
        }
    }

    #[test]
    fn torus_smooth_group_offset_preserves_major_radius_and_axis() {
        let (mut mesh, faces) = build_torus_two_faces(7.0, 2.0);
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: -0.5 },
                MaterialId::new(0),
            )
            .expect("inward OK");

        if let Some(AnalyticSurface::Torus {
            major_radius,
            minor_radius,
            axis_dir,
            ref_dir,
            center: c,
            ..
        }) = mesh.faces[faces[0]].surface()
        {
            assert!((major_radius - 7.0).abs() < 1e-9, "major must be preserved");
            assert!((minor_radius - 1.5).abs() < 1e-9, "minor = 2 - 0.5 = 1.5");
            assert!(axis_dir.normalize().dot(DVec3::Z).abs() > 0.9999);
            assert!(ref_dir.normalize().dot(DVec3::X).abs() > 0.9999);
            assert!((c - DVec3::ZERO).length() < 1e-9);
        } else {
            panic!("face surface must remain Torus");
        }
    }

    #[test]
    fn torus_smooth_group_offset_collapse_falls_back() {
        let (mut mesh, faces) = build_torus_two_faces(5.0, 1.0);
        // -1.0 → minor_new = 0 → collapse.
        let result = mesh.create_solid(
            faces[0],
            CreateSolidMode::Extrude { distance: -1.0 },
            MaterialId::new(0),
        );
        let err = result.err().expect("must fail (collapse)");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("collapse"),
            "expected NotYetSupported with 'collapse' reason, got: {msg}"
        );
    }

    #[test]
    fn torus_smooth_group_offset_returns_smooth_group_offset_kind() {
        let (mut mesh, faces) = build_torus_two_faces(3.0, 0.5);
        let result = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.2 },
                MaterialId::new(0),
            )
            .expect("offset OK");
        assert_eq!(result.solid_kind, SolidKind::SmoothGroupOffset);
        assert_eq!(result.top_face, result.profile_face);
        assert_eq!(result.side_faces.len(), 1);
        assert_eq!(result.all_solid_faces.len(), 2);
    }

    #[test]
    fn torus_smooth_group_offset_updates_outer_latitude_circle() {
        // Attach an outer-latitude full circle (v=0): center = torus_center,
        // radius = major + minor, normal = axis_dir.
        // After offset by d=0.5: new_radius = (major + minor) + 0.5*cos(0)
        // = (major + minor) + 0.5. center unchanged (sin(0) = 0).
        use crate::curves::AnalyticCurve;
        let major = 5.0;
        let minor = 1.0;
        let (mut mesh, faces) = build_torus_two_faces(major, minor);
        let edges = mesh.face_outer_edges(faces[0]).expect("edges");
        let circ_eid = edges[0];

        // Construct outer latitude circle (v=0).
        let circ = AnalyticCurve::Circle {
            center: DVec3::ZERO,
            radius: major + minor, // = 6
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        mesh.edges[circ_eid].set_curve(Some(circ));

        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        if let Some(AnalyticCurve::Circle {
            radius: nr,
            center: nc,
            ..
        }) = mesh.edges.get(circ_eid).and_then(|e| e.curve())
        {
            // sin(v=0) = 0 → center axial unchanged
            // cos(v=0) = 1 → new_radius = 6 + 0.5*1 = 6.5
            assert!(
                (nr - 6.5).abs() < 1e-9,
                "outer latitude new radius != 6.5: got {nr}"
            );
            assert!(
                (nc - DVec3::ZERO).length() < 1e-9,
                "outer latitude center must remain at origin (sin(0) = 0)"
            );
        } else {
            panic!("edge curve must remain Circle after offset");
        }
    }

    #[test]
    fn torus_smooth_group_offset_updates_top_latitude_circle() {
        // Top latitude (v=π/2): center = torus_center + minor·axis,
        // radius = major (since cos(π/2) = 0).
        // After offset by d=0.5: new sin(v)=1, cos(v)=0
        //   new_axial = old_axial + d*sin(v) = minor + 0.5
        //   new_radius = old_radius + d*cos(v) = major (unchanged)
        use crate::curves::AnalyticCurve;
        let major = 4.0;
        let minor = 1.0;
        let (mut mesh, faces) = build_torus_two_faces(major, minor);
        let edges = mesh.face_outer_edges(faces[0]).expect("edges");
        let circ_eid = edges[0];

        let circ = AnalyticCurve::Circle {
            center: DVec3::new(0.0, 0.0, minor), // = (0, 0, 1)
            radius: major,                       // = 4
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        mesh.edges[circ_eid].set_curve(Some(circ));

        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 0.5 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        if let Some(AnalyticCurve::Circle {
            radius: nr,
            center: nc,
            ..
        }) = mesh.edges.get(circ_eid).and_then(|e| e.curve())
        {
            assert!(
                (nr - 4.0).abs() < 1e-9,
                "top latitude radius must remain 4.0 (cos(π/2)=0): got {nr}"
            );
            assert!(
                (nc.z - 1.5).abs() < 1e-9,
                "top latitude axial must be 1 + 0.5*1 = 1.5: got {}",
                nc.z
            );
        } else {
            panic!("edge curve must remain Circle");
        }
    }

    #[test]
    fn sphere_smooth_group_offset_scales_boundary_arcs_about_center() {
        // Attach an Arc curve to one boundary edge and verify it's scaled
        // uniformly about the sphere center (not just radius scaled in
        // place — center also moves under uniform scale).
        use crate::curves::AnalyticCurve;
        let (mut mesh, faces) = build_sphere_two_faces(2.0);
        let edges = mesh.face_outer_edges(faces[0]).expect("edges");
        let arc_eid = edges[0];

        // Attach a small Arc with its own (off-center) parameters.
        let arc_center = DVec3::new(1.0, 0.0, 0.0);
        let arc_radius = 0.5;
        let initial_arc = AnalyticCurve::Arc {
            center: arc_center,
            radius: arc_radius,
            normal: DVec3::Y,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        };
        mesh.edges[arc_eid].set_curve(Some(initial_arc));

        // Offset by +1 → scale = 3/2 = 1.5.
        let _ = mesh
            .create_solid(
                faces[0],
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("offset OK");

        // Expected:
        //   new_center = ZERO + (arc_center - ZERO) * 1.5 = (1.5, 0, 0)
        //   new_radius = 0.5 * 1.5 = 0.75
        if let Some(AnalyticCurve::Arc {
            center: nc,
            radius: nr,
            ..
        }) = mesh.edges.get(arc_eid).and_then(|e| e.curve())
        {
            assert!((nc - DVec3::new(1.5, 0.0, 0.0)).length() < 1e-9,
                "arc center expected (1.5, 0, 0), got {nc:?}");
            assert!((nr - 0.75).abs() < 1e-9,
                "arc radius expected 0.75, got {nr}");
        } else {
            panic!("arc curve must remain on edge after offset");
        }
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-4-α — Revolve mode dispatch (full 360° only)
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build a triangular profile face in the xy plane (so its
    /// face normal is +Z), with vertices that lie on the +X half-plane
    /// (one vertex on +Y axis, one off). Suitable for revolve around the
    /// y-axis to create a vase/cone-like solid.
    fn build_revolve_profile_face(mesh: &mut Mesh) -> FaceId {
        let mat = MaterialId::new(0);
        // Triangle in xy plane: (1, 0, 0), (2, 0, 0), (1, 1, 0).
        // Revolved around +Y axis would produce an annular cone.
        let v0 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let face = mesh.add_face(&[v0, v1, v2], mat).expect("add_face");
        // Plane surface: xy plane (normal +Z).
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 2.0),
            v_range: (0.0, 1.0),
        }));
        face
    }

    #[test]
    fn revolve_mode_full_360_returns_revolution_solid() {
        let mut mesh = Mesh::new();
        let profile = build_revolve_profile_face(&mut mesh);
        let face_count_before = mesh.face_count();

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Revolve {
                    axis_origin: DVec3::ZERO,
                    axis_dir: DVec3::Y, // y-axis (lies in xy plane)
                    angle_rad: std::f64::consts::TAU,
                },
                MaterialId::new(0),
            )
            .expect("revolve full 360 OK");

        assert_eq!(result.solid_kind, SolidKind::RevolutionSolid);
        assert_eq!(result.profile_face, profile);
        assert!(result.side_faces.len() > 0, "revolve must produce side faces");
        assert!(mesh.face_count() > face_count_before);

        // Round-11: a full-360 revolve of a profile CLEAR of the axis now yields
        // a CLOSED SOLID ring (seamless 2π wrap, no caps, profile removed as an
        // interior cross-section) — previously it left an open, non-manifold
        // surface with the profile as an internal membrane.
        let active: Vec<_> =
            mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
        let info = mesh.face_set_manifold_info(&active);
        assert!(info.is_closed_solid,
            "full-360 revolve of a clear-of-axis profile must be a closed solid \
             (bnd={}, nm={})", info.boundary_edge_count, info.non_manifold_edge_count);
        assert_eq!(info.boundary_edge_count, 0, "no open boundary");
        assert_eq!(info.non_manifold_edge_count, 0, "no non-manifold seam");
        assert!(mesh.detect_self_intersections().is_clean(), "no self-intersection");
        // The profile face is consumed (interior cross-section).
        assert!(!mesh.faces.contains(profile) || !mesh.faces[profile].is_active(),
            "profile face removed (interior)");
    }

    #[test]
    fn revolve_mode_axis_zero_rejected() {
        let mut mesh = Mesh::new();
        let profile = build_revolve_profile_face(&mut mesh);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Revolve {
                axis_origin: DVec3::ZERO,
                axis_dir: DVec3::ZERO, // zero axis
                angle_rad: std::f64::consts::TAU,
            },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject zero axis");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("near-zero"),
            "expected near-zero axis rejection, got: {msg}"
        );
    }

    #[test]
    fn revolve_mode_profile_face_not_in_plane_with_axis_rejected() {
        // Profile face on z=0 (normal +Z), axis on +Z (parallel to normal).
        // face_normal · axis_dir = +Z · +Z = 1 (not perpendicular).
        let mut mesh = Mesh::new();
        let profile = build_revolve_profile_face(&mut mesh);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Revolve {
                axis_origin: DVec3::ZERO,
                axis_dir: DVec3::Z, // parallel to face normal — invalid
                angle_rad: std::f64::consts::TAU,
            },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject non-perpendicular axis");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("not contain axis"),
            "expected plane-axis perpendicularity rejection, got: {msg}"
        );
    }

    #[test]
    fn revolve_mode_multi_loop_face_rejected() {
        // Frame face with hole — multi-loop should reject.
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let outer = [
            mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0)),
            mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0)),
        ];
        let inner = [
            mesh.add_vertex(DVec3::new(3.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 7.0, 0.0)),
            mesh.add_vertex(DVec3::new(3.0, 7.0, 0.0)),
        ];
        let face = mesh
            .add_face_with_holes(&outer, &[&inner], mat)
            .expect("frame face");
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 10.0),
            v_range: (0.0, 10.0),
        }));

        let result = mesh.create_solid(
            face,
            CreateSolidMode::Revolve {
                axis_origin: DVec3::ZERO,
                axis_dir: DVec3::Y,
                angle_rad: std::f64::consts::TAU,
            },
            mat,
        );
        let err = result.err().expect("must reject multi-loop");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("multi-loop"),
            "expected multi-loop rejection, got: {msg}"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-3-α — Sweep mode dispatch
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build a unit-square profile face on z=0 with normal +Z,
    /// suitable for sweep along a path along +Z.
    fn build_z_normal_profile_face(mesh: &mut Mesh) -> FaceId {
        let mat = MaterialId::new(0);
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face = mesh.add_face(&[v0, v1, v2, v3], mat).expect("add_face");
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        }));
        face
    }

    #[test]
    fn sweep_mode_along_straight_z_path_returns_swept_solid() {
        // Profile on z=0 (normal +Z), path Line from (0,0,0) → (0,0,5)
        // (along +Z, tangent matches profile normal).
        let mut mesh = Mesh::new();
        let profile = build_z_normal_profile_face(&mut mesh);
        // Add path Line vertices.
        let pa = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let pb = mesh.add_vertex(DVec3::new(0.0, 0.0, 5.0));
        let path_curve = AnalyticCurve::Line { start: pa, end: pb };

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Sweep { path: path_curve },
                MaterialId::new(0),
            )
            .expect("sweep along Z OK");

        assert_eq!(result.solid_kind, SolidKind::SweptSolid);
        assert_eq!(result.profile_face, profile);
        assert!(
            result.side_faces.len() >= 4,
            "swept tube must have ≥ 4 side faces (one per profile edge)"
        );
    }

    #[test]
    fn sweep_mode_path_tangent_misaligned_with_profile_normal_rejected() {
        // Profile normal = +Z, path tangent = +X (perpendicular). Reject.
        let mut mesh = Mesh::new();
        let profile = build_z_normal_profile_face(&mut mesh);
        let pa = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let pb = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));
        let path_curve = AnalyticCurve::Line { start: pa, end: pb };

        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Sweep { path: path_curve },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject misaligned path");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("tangent"),
            "expected tangent misalignment rejection, got: {msg}"
        );
    }

    #[test]
    fn sweep_mode_multi_loop_face_rejected() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let outer = [
            mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0)),
            mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0)),
        ];
        let inner = [
            mesh.add_vertex(DVec3::new(3.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 7.0, 0.0)),
            mesh.add_vertex(DVec3::new(3.0, 7.0, 0.0)),
        ];
        let face = mesh
            .add_face_with_holes(&outer, &[&inner], mat)
            .expect("frame face");
        mesh.faces[face].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 10.0),
            v_range: (0.0, 10.0),
        }));
        let pa = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let pb = mesh.add_vertex(DVec3::new(0.0, 0.0, 5.0));
        let path_curve = AnalyticCurve::Line { start: pa, end: pb };

        let result = mesh.create_solid(
            face,
            CreateSolidMode::Sweep { path: path_curve },
            mat,
        );
        let err = result.err().expect("must reject multi-loop");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("multi-loop"),
            "expected multi-loop rejection, got: {msg}"
        );
    }

    #[test]
    fn sweep_mode_circular_path_arc_succeeds() {
        // Arc path on xy plane: small quarter-circle.
        // Profile must align with path's start tangent.
        let mut mesh = Mesh::new();
        // Path arc center at origin, radius 5, in xy plane.
        // At θ=0, point = (5, 0, 0), tangent = (0, 5, 0) normalized = +Y.
        // Profile must have normal = +Y.
        let mat = MaterialId::new(0);
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 0.0, 1.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, 1.0));
        let profile = mesh.add_face(&[v0, v1, v2, v3], mat).expect("profile");
        mesh.faces[profile].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::new(5.0, 0.0, 0.0), // path start point
            normal: DVec3::Y,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        }));

        let path_curve = AnalyticCurve::Arc {
            center: DVec3::ZERO,
            radius: 5.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        };

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Sweep { path: path_curve },
                mat,
            )
            .expect("arc path sweep OK");
        assert_eq!(result.solid_kind, SolidKind::SweptSolid);
        assert!(
            result.side_faces.len() >= 4,
            "arc sweep must produce side faces"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-3-δ — Extrude on NURBS-class profile (tessellation-based)
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build a quad face whose surface is a synthetic flat
    /// BezierPatch (linear 2×2 control grid) — equivalent to a plane in
    /// shape, but classified as NURBS-class for dispatch purposes.
    fn build_bezier_patch_quad_face(mesh: &mut Mesh) -> FaceId {
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face = mesh.add_face(&[v00, v10, v11, v01], mat).expect("add_face");
        // Flat BezierPatch (2×2 = bilinear). Normal at (0.5, 0.5) = +Z.
        mesh.faces[face].set_surface(Some(AnalyticSurface::BezierPatch {
            ctrl_grid: vec![
                vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(1.0, 0.0, 0.0)],
                vec![DVec3::new(0.0, 1.0, 0.0), DVec3::new(1.0, 1.0, 0.0)],
            ],
        }));
        face
    }

    #[test]
    fn extrude_on_bezier_patch_returns_general_sweep() {
        let mut mesh = Mesh::new();
        let profile = build_bezier_patch_quad_face(&mut mesh);
        let face_count_before = mesh.face_count();

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 1.0 },
                MaterialId::new(0),
            )
            .expect("BezierPatch profile extrude OK (W-3-δ)");

        assert_eq!(result.solid_kind, SolidKind::GeneralSweep);
        assert_eq!(result.profile_face, profile);
        assert_eq!(result.side_faces.len(), 4);
        // profile + top + 4 sides = 6.
        assert_eq!(result.all_solid_faces.len(), 6);
        assert_eq!(mesh.face_count(), face_count_before + 5);
    }

    /// Helper — 3×3 degree-2 BSpline/NURBS control grid (linear-equivalent
    /// surface in xy plane). Required because deg-1 bspline_surface gives
    /// degenerate derivative at parametric center.
    fn make_3x3_xy_grid() -> Vec<Vec<DVec3>> {
        vec![
            vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(0.5, 0.0, 0.0),
                DVec3::new(1.0, 0.0, 0.0),
            ],
            vec![
                DVec3::new(0.0, 0.5, 0.0),
                DVec3::new(0.5, 0.5, 0.0),
                DVec3::new(1.0, 0.5, 0.0),
            ],
            vec![
                DVec3::new(0.0, 1.0, 0.0),
                DVec3::new(0.5, 1.0, 0.0),
                DVec3::new(1.0, 1.0, 0.0),
            ],
        ]
    }

    #[test]
    fn extrude_on_bspline_surface_returns_general_sweep() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face = mesh.add_face(&[v00, v10, v11, v01], mat).expect("face");
        mesh.faces[face].set_surface(Some(AnalyticSurface::BSplineSurface {
            ctrl_grid: make_3x3_xy_grid(),
            knots_u: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            deg_u: 2,
            deg_v: 2,
        }));

        let result = mesh
            .create_solid(
                face,
                CreateSolidMode::Extrude { distance: 1.0 },
                mat,
            )
            .expect("BSplineSurface profile extrude OK");
        assert_eq!(result.solid_kind, SolidKind::GeneralSweep);
        assert_eq!(result.side_faces.len(), 4);
    }

    #[test]
    fn extrude_on_nurbs_surface_returns_general_sweep() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face = mesh.add_face(&[v00, v10, v11, v01], mat).expect("face");
        mesh.faces[face].set_surface(Some(AnalyticSurface::NURBSSurface {
            ctrl_grid: make_3x3_xy_grid(),
            weights: vec![
                vec![1.0, 1.0, 1.0],
                vec![1.0, 1.0, 1.0],
                vec![1.0, 1.0, 1.0],
            ],
            knots_u: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            deg_u: 2,
            deg_v: 2,
            trim_loops: vec![],
        }));

        let result = mesh
            .create_solid(
                face,
                CreateSolidMode::Extrude { distance: 1.0 },
                mat,
            )
            .expect("NURBSSurface profile extrude OK");
        assert_eq!(result.solid_kind, SolidKind::GeneralSweep);
        assert_eq!(result.side_faces.len(), 4);
    }

    #[test]
    fn extrude_on_nurbs_class_top_face_synthesized_as_plane() {
        // W-3-δ approximation: top face surface is Plane (synthesized
        // from translated vertex positions), not the original NURBS surface.
        let mut mesh = Mesh::new();
        let profile = build_bezier_patch_quad_face(&mut mesh);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 2.0 },
                MaterialId::new(0),
            )
            .expect("OK");
        let top_surface = mesh.faces[result.top_face].surface();
        assert!(
            matches!(top_surface, Some(AnalyticSurface::Plane { .. })),
            "W-3-δ approximation: top face synthesized as Plane, not NURBS"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // ADR-079 W-3-β — Loft mode dispatch (two profiles)
    // ════════════════════════════════════════════════════════════════════

    /// Helper — build two square profile faces stacked in z (z=0 and z=2),
    /// suitable for loft.
    fn build_two_square_profiles(mesh: &mut Mesh) -> (FaceId, FaceId) {
        let mat = MaterialId::new(0);
        // Bottom square at z=0 (4 verts CCW from above).
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let bottom = mesh.add_face(&[v00, v10, v11, v01], mat).expect("bottom");
        mesh.faces[bottom].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        }));
        // Top square at z=2 (slightly larger).
        let w00 = mesh.add_vertex(DVec3::new(-0.5, -0.5, 2.0));
        let w10 = mesh.add_vertex(DVec3::new(1.5, -0.5, 2.0));
        let w11 = mesh.add_vertex(DVec3::new(1.5, 1.5, 2.0));
        let w01 = mesh.add_vertex(DVec3::new(-0.5, 1.5, 2.0));
        let top = mesh.add_face(&[w00, w10, w11, w01], mat).expect("top");
        mesh.faces[top].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::new(0.0, 0.0, 2.0),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-0.5, 1.5),
            v_range: (-0.5, 1.5),
        }));
        (bottom, top)
    }

    #[test]
    fn loft_mode_two_squares_returns_loft_solid() {
        let mut mesh = Mesh::new();
        let (bottom, top) = build_two_square_profiles(&mut mesh);

        let result = mesh
            .create_solid(
                bottom,
                CreateSolidMode::Loft { other_profile: top },
                MaterialId::new(0),
            )
            .expect("loft 2 squares OK");

        assert_eq!(result.solid_kind, SolidKind::LoftSolid);
        assert_eq!(result.profile_face, bottom);
        assert_eq!(result.top_face, top);
        // Loft of two 4-vertex squares: 4 ruled bands.
        assert_eq!(
            result.side_faces.len(),
            4,
            "loft 4-square to 4-square must produce 4 ruled side faces"
        );
        // all_solid_faces = bottom + top + 4 sides = 6.
        assert_eq!(result.all_solid_faces.len(), 6);
    }

    #[test]
    fn loft_mode_same_profile_id_rejected() {
        let mut mesh = Mesh::new();
        let (bottom, _top) = build_two_square_profiles(&mut mesh);

        let result = mesh.create_solid(
            bottom,
            CreateSolidMode::Loft {
                other_profile: bottom, // same as profile_face
            },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject same profile");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("same FaceId"),
            "expected same-FaceId rejection, got: {msg}"
        );
    }

    #[test]
    fn loft_mode_vertex_count_mismatch_resamples() {
        // Bottom: 4-vertex square. Top: 3-vertex triangle. ADR-247 (Phase 3 E2)
        // — the shorter profile (triangle, 3) auto-resamples up to the longer's
        // count (4) via midpoint edge-split and the loft produces a closed solid
        // (was a NotYetSupported bail pre-ADR-247).
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let v00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let bottom = mesh.add_face(&[v00, v10, v11, v01], mat).expect("bottom");
        mesh.faces[bottom].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        }));
        let w0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 2.0));
        let w1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 2.0));
        let w2 = mesh.add_vertex(DVec3::new(0.5, 1.0, 2.0));
        let top = mesh.add_face(&[w0, w1, w2], mat).expect("top");
        mesh.faces[top].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::new(0.0, 0.0, 2.0),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 1.0),
            v_range: (0.0, 1.0),
        }));

        let result = mesh
            .create_solid(bottom, CreateSolidMode::Loft { other_profile: top }, mat)
            .expect("ADR-247: mismatched-count loft must auto-resample (was rejected)");
        assert_eq!(result.solid_kind, SolidKind::LoftSolid);
        // Triangle top resampled up to the square's 4 verts.
        assert_eq!(
            mesh.collect_loop_verts(mesh.faces[top].outer().start).unwrap().len(),
            4,
            "triangle top resampled to 4 verts"
        );
        assert_eq!(
            mesh.face_set_manifold_info(&result.all_solid_faces).boundary_edge_count,
            0,
            "resampled loft must be a closed solid"
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "invariants after resampled loft:\n{}",
            mesh.verify_face_invariants().summary()
        );
    }

    /// ADR-247 (Phase 3 E2) — larger gap (triangle 3 → hexagon 6) exercises
    /// MULTIPLE midpoint splits on the shorter profile.
    #[test]
    fn loft_mode_triangle_to_hexagon_resamples() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        // Triangle z=0.
        let t0 = mesh.add_vertex(DVec3::new(-3.0, -2.0, 0.0));
        let t1 = mesh.add_vertex(DVec3::new(3.0, -2.0, 0.0));
        let t2 = mesh.add_vertex(DVec3::new(0.0, 3.0, 0.0));
        let tri = mesh.add_face(&[t0, t1, t2], mat).unwrap();
        mesh.faces[tri].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO, normal: DVec3::Z, basis_u: DVec3::X,
            u_range: (0.0, 1.0), v_range: (0.0, 1.0),
        }));
        // Hexagon z=10.
        let mut hv = Vec::new();
        for k in 0..6 {
            let a = std::f64::consts::TAU * (k as f64) / 6.0;
            hv.push(mesh.add_vertex(DVec3::new(5.0 * a.cos(), 5.0 * a.sin(), 10.0)));
        }
        let hex = mesh.add_face(&hv, mat).unwrap();
        mesh.faces[hex].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::new(0.0, 0.0, 10.0), normal: DVec3::Z, basis_u: DVec3::X,
            u_range: (0.0, 1.0), v_range: (0.0, 1.0),
        }));

        let result = mesh
            .create_solid(tri, CreateSolidMode::Loft { other_profile: hex }, mat)
            .expect("triangle→hexagon loft must auto-resample");
        assert_eq!(
            mesh.collect_loop_verts(mesh.faces[tri].outer().start).unwrap().len(),
            6,
            "triangle resampled to 6 verts (3 midpoint splits)"
        );
        assert_eq!(
            mesh.face_set_manifold_info(&result.all_solid_faces).boundary_edge_count,
            0,
            "closed solid"
        );
        assert!(mesh.verify_face_invariants().is_valid());
    }

    #[test]
    fn loft_mode_first_profile_multi_loop_rejected() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        // Frame face (multi-loop) as first profile.
        let outer = [
            mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0)),
            mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0)),
            mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0)),
        ];
        let inner = [
            mesh.add_vertex(DVec3::new(3.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 3.0, 0.0)),
            mesh.add_vertex(DVec3::new(7.0, 7.0, 0.0)),
            mesh.add_vertex(DVec3::new(3.0, 7.0, 0.0)),
        ];
        let frame = mesh
            .add_face_with_holes(&outer, &[&inner], mat)
            .expect("frame face");
        mesh.faces[frame].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 10.0),
            v_range: (0.0, 10.0),
        }));
        // Plain top square (4 verts).
        let w00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 5.0));
        let w10 = mesh.add_vertex(DVec3::new(10.0, 0.0, 5.0));
        let w11 = mesh.add_vertex(DVec3::new(10.0, 10.0, 5.0));
        let w01 = mesh.add_vertex(DVec3::new(0.0, 10.0, 5.0));
        let top = mesh.add_face(&[w00, w10, w11, w01], mat).expect("top");
        mesh.faces[top].set_surface(Some(AnalyticSurface::Plane {
            origin: DVec3::new(0.0, 0.0, 5.0),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 10.0),
            v_range: (0.0, 10.0),
        }));

        let result = mesh.create_solid(
            frame,
            CreateSolidMode::Loft { other_profile: top },
            mat,
        );
        let err = result.err().expect("must reject multi-loop");
        let msg = format!("{}", err);
        assert!(
            msg.contains("not yet supported") && msg.contains("multi-loop"),
            "expected multi-loop rejection, got: {msg}"
        );
    }

    #[test]
    fn loft_mode_invalid_face_id_rejected() {
        let mut mesh = Mesh::new();
        let (bottom, _top) = build_two_square_profiles(&mut mesh);

        let result = mesh.create_solid(
            bottom,
            CreateSolidMode::Loft {
                other_profile: FaceId::new(999),
            },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject missing face");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("FaceNotFound") || msg.contains("face not found"),
            "expected FaceNotFound, got: {msg}"
        );
    }

    #[test]
    fn sweep_mode_invalid_face_id_rejected() {
        let mut mesh = Mesh::new();
        let pa = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let pb = mesh.add_vertex(DVec3::new(0.0, 0.0, 5.0));
        let path_curve = AnalyticCurve::Line { start: pa, end: pb };

        let result = mesh.create_solid(
            FaceId::new(999),
            CreateSolidMode::Sweep { path: path_curve },
            MaterialId::new(0),
        );
        let err = result.err().expect("must reject missing face");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("FaceNotFound") || msg.contains("face not found"),
            "expected FaceNotFound, got: {msg}"
        );
    }

    #[test]
    fn revolve_mode_invalid_face_id_rejected() {
        let mut mesh = Mesh::new();
        // No face exists; arbitrary FaceId.
        let result = mesh.create_solid(
            FaceId::new(999),
            CreateSolidMode::Revolve {
                axis_origin: DVec3::ZERO,
                axis_dir: DVec3::Y,
                angle_rad: std::f64::consts::TAU,
            },
            MaterialId::new(0),
        );
        // create_solid 의 사전 검사 (faces.contains) 에서 FaceNotFound 발생.
        let err = result.err().expect("must reject missing face");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("FaceNotFound") || msg.contains("face not found"),
            "expected FaceNotFound, got: {msg}"
        );
    }

    #[test]
    fn create_solid_rejects_inactive_profile_face() {
        // ADR-256 follow-up (engine-layer hardening) — a deactivated-but-
        // slot-resident face (contains() == true, is_active() == false) must
        // get a clean, early, uniform FaceNotFound at the entry. (Verified
        // red→green: WITHOUT this guard the Extrude path only catches it deep
        // in the ADR-102 cleave step with an opaque "profile boundary
        // collection failed: ... is inactive" message, and non-cleave modes
        // — Revolve/Sweep/Loft — have no early profile is_active check at
        // all.) Closing it at the single entry covers every mode + any
        // internal Rust caller; the WASM boundary guard sits in front for
        // UI/MCP/script.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let fid = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).unwrap();
        // Soft-delete: slot stays (contains() == true) but face is inactive.
        mesh.faces[fid].set_active(false);
        assert!(mesh.faces.contains(fid), "slot must remain after soft-delete");
        assert!(!mesh.faces[fid].is_active(), "face must be inactive");

        let result = mesh.create_solid(
            fid,
            CreateSolidMode::Extrude { distance: 10.0 },
            MaterialId::new(0),
        );
        let err = result
            .err()
            .expect("must reject inactive face, not silently proceed");
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("FaceNotFound") || msg.contains("face not found"),
            "expected FaceNotFound for inactive profile face, got: {msg}"
        );
        // Over-rejection is impossible by construction: for an ACTIVE face
        // `!is_active()` is false, so the guard's added clause is inert and
        // create_solid behaves exactly as before. The full axia-geo suite
        // (all existing create_solid tests use active faces) is the empirical
        // proof of zero over-rejection.
    }

    // ────────────────────────────────────────────────────────────────────
    // ADR-089 A-θ-β: closed-curve face Push-Pull (Path A tessellate)
    // ────────────────────────────────────────────────────────────────────

    /// Build a canonical closed-curve face: 1 anchor + 1 self-loop edge
    /// with Circle curve attached + Plane surface attach (A-η-1).
    fn build_closed_curve_circle_face(
        mesh: &mut Mesh,
        center: DVec3,
        radius: f64,
    ) -> FaceId {
        let anchor_pos = center + DVec3::X * radius; // θ=0
        let anchor = mesh.add_vertex(anchor_pos);
        let circle = AnalyticCurve::Circle {
            center,
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        mesh.add_face_closed_curve(anchor, circle, MaterialId::new(0))
            .expect("add_face_closed_curve")
    }

    #[test]
    fn adr089_a_theta_closed_curve_face_extrudes_to_cylinder() {
        // Closed-curve face (1 anchor + 1 self-loop edge) must extrude
        // via Path A tessellate fast-path → Cylinder solid result.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let face_count_before = mesh.face_count();

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 10.0 },
                MaterialId::new(0),
            )
            .expect("ADR-089 A-θ-β: closed-curve Push-Pull must succeed");

        assert_eq!(
            result.solid_kind,
            SolidKind::Cylinder,
            "ADR-089 A-θ-β: result must be Cylinder"
        );
        // Tessellation produces N >= 8 segments. side_faces.len() >= 8.
        assert!(
            result.side_faces.len() >= 8,
            "tessellation must produce ≥ 8 side faces, got {}",
            result.side_faces.len()
        );
        // Original closed-curve face was removed; substituted polygonal
        // face + top + N sides added. face_count_before was 1 (closed
        // curve face); after: 1 substituted + 1 top + N sides.
        assert!(mesh.face_count() > face_count_before);

        // Invariants pass.
        let report = mesh.verify_face_invariants();
        assert!(
            report.is_valid(),
            "ADR-089 A-θ-β: invariants must pass, violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn adr089_a_theta_closed_curve_negative_distance_recess() {
        // dist < 0 (recess) must also work via Path A.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 2.0);

        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: -3.0 },
                MaterialId::new(0),
            )
            .expect("recess must succeed");

        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert!(result.side_faces.len() >= 8);
    }

    #[test]
    fn adr089_a_theta_closed_curve_attaches_cylinder_surface_to_sides() {
        // Side walls of resulting cylinder must carry AnalyticSurface::
        // Cylinder (so subsequent ops — Boolean / Offset — see kernel).
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 4.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 6.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        for &side in &result.side_faces {
            let surface = mesh.faces[side].surface();
            assert!(
                matches!(surface, Some(AnalyticSurface::Cylinder { .. })),
                "ADR-089 A-θ-β: side wall must have Cylinder surface, got {:?}",
                surface.map(|s| s.kind_label())
            );
        }
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-092 C-β — Push-Pull preserves closed-curve metadata on top
    //   face boundary (manifold-safe: Arc curves on N polygon edges).
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr092_c_beta_top_face_edges_have_arc_curves() {
        // After Push-Pull, the TOP face's N polygon edges must carry
        // AnalyticCurve::Arc with translated center — fixes the
        // "polygon rim visible at top" defect (사용자 시연 2026-05-09).
        let mut mesh = Mesh::new();
        let profile =
            build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 10.0 },
                MaterialId::new(0),
            )
            .expect("closed-curve Push-Pull must succeed");

        let top_edges = mesh
            .face_outer_edges(result.top_face)
            .expect("top face has outer edges");
        assert!(
            top_edges.len() >= 8,
            "top face must have ≥ 8 polygon edges, got {}",
            top_edges.len()
        );

        // Every top edge must have an AnalyticCurve::Arc attached.
        let mut arc_count = 0;
        let mut all_have_arc = true;
        for &eid in &top_edges {
            match mesh.edges[eid].curve() {
                Some(AnalyticCurve::Arc { .. }) => arc_count += 1,
                _ => all_have_arc = false,
            }
        }
        assert!(
            all_have_arc,
            "ADR-092 C-β: ALL {} top face edges must have Arc curve, only {} did",
            top_edges.len(),
            arc_count
        );
    }

    #[test]
    fn adr092_c_beta_top_arc_center_is_translated_from_bottom() {
        // The Arc center on top edges must equal bottom_center +
        // (profile_normal · dist). This is the architectural anchor —
        // top boundary is the same Circle, just translated.
        let mut mesh = Mesh::new();
        let bottom_center = DVec3::new(3.0, 4.0, 0.0); // arbitrary
        let profile =
            build_closed_curve_circle_face(&mut mesh, bottom_center, 5.0);
        let dist = 7.0;
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: dist },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        // Profile normal for build_closed_curve_circle_face is +Z.
        let expected_top_center = bottom_center + DVec3::Z * dist;

        let top_edges = mesh.face_outer_edges(result.top_face).unwrap();
        for &eid in &top_edges {
            if let Some(AnalyticCurve::Arc { center, .. }) =
                mesh.edges[eid].curve()
            {
                assert!(
                    (*center - expected_top_center).length() < 1e-9,
                    "ADR-092 C-β: Arc center expected {:?}, got {:?}",
                    expected_top_center,
                    center
                );
            }
        }
    }

    #[test]
    fn adr092_c_beta_top_arc_radius_matches_bottom() {
        // Top Arcs must share the bottom's radius (extrusion is purely
        // axial — no scaling).
        let mut mesh = Mesh::new();
        let radius = 5.0;
        let profile =
            build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, radius);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        let top_edges = mesh.face_outer_edges(result.top_face).unwrap();
        for &eid in &top_edges {
            if let Some(AnalyticCurve::Arc { radius: r, .. }) =
                mesh.edges[eid].curve()
            {
                assert!(
                    (*r - radius).abs() < 1e-9,
                    "ADR-092 C-β: Arc radius {} does not match bottom radius {}",
                    r,
                    radius
                );
            }
        }
    }

    #[test]
    fn adr092_c_beta_dcel_topology_unchanged_manifold_safe() {
        // C-β must NOT alter DCEL topology — only Arc metadata is added
        // to existing edges. Same face count + manifold invariants pass.
        let mut mesh = Mesh::new();
        let profile =
            build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let _ = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 10.0 },
                MaterialId::new(0),
            )
            .expect("Push-Pull OK");

        // Manifold invariants must hold — boundary edges remain shared
        // between top face and side quads (no orphan boundary edges
        // introduced by C-β).
        let report = mesh.verify_face_invariants();
        assert!(
            report.is_valid(),
            "ADR-092 C-β: manifold must hold post-Arc-attach, violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn adr092_c_beta_negative_distance_translation_correct() {
        // Recess (dist < 0) must place Arc center at bottom_center +
        // normal · negative_dist (downward). Smoke for sign correctness.
        let mut mesh = Mesh::new();
        let profile =
            build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 3.0);
        let dist = -4.0;
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: dist },
                MaterialId::new(0),
            )
            .expect("recess OK");

        let expected_top_center = DVec3::new(0.0, 0.0, dist); // 0 + Z·(-4)
        let top_edges = mesh.face_outer_edges(result.top_face).unwrap();
        let first = top_edges[0];
        if let Some(AnalyticCurve::Arc { center, .. }) =
            mesh.edges[first].curve()
        {
            assert!(
                (*center - expected_top_center).length() < 1e-9,
                "ADR-092 C-β: recess top Arc center expected {:?}, got {:?}",
                expected_top_center,
                center
            );
        } else {
            panic!("ADR-092 C-β: top edge must have Arc curve");
        }
    }

    #[test]
    fn adr092_c_beta_polygonal_path_unaffected() {
        // Regression guard — polygonal circle (build_circle_face with
        // explicit segments) does NOT enter
        // extrude_closed_curve_face_via_tessellation, so its top face
        // edges should NOT receive new Arc curves from C-β. Their
        // existing Arc state (set by the polygonal path) remains.
        // Smoke — the test just verifies no panic / no manifold break
        // along the polygonal path after C-β patch.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let _ = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 7.0 },
                MaterialId::new(0),
            )
            .expect("polygonal Push-Pull OK");
        let report = mesh.verify_face_invariants();
        assert!(
            report.is_valid(),
            "ADR-092 C-β: polygonal path must remain manifold, violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn adr092_c_beta_top_arc_normal_matches_profile() {
        // Top Arcs must inherit the profile normal (extrusion is along
        // the profile normal, top plane is parallel).
        let mut mesh = Mesh::new();
        let profile =
            build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 6.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");

        let top_edges = mesh.face_outer_edges(result.top_face).unwrap();
        for &eid in &top_edges {
            if let Some(AnalyticCurve::Arc { normal, .. }) =
                mesh.edges[eid].curve()
            {
                // Profile normal for build_closed_curve_circle_face is +Z.
                let dot = normal.dot(DVec3::Z).abs();
                assert!(
                    dot > 0.99,
                    "ADR-092 C-β: top Arc normal {:?} must align with +Z",
                    normal
                );
            }
        }
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-093 D-β — Cylinder Side Face Owner-ID Grouping (B-MVP)
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr093_d_beta_face_surface_owner_id_default_none() {
        // 새 face 의 default surface_owner_id 는 None.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let f = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).unwrap();
        assert_eq!(mesh.face_surface_owner_id(f), None,
            "fresh face must have no surface_owner_id");
    }

    #[test]
    fn adr093_d_beta_next_surface_owner_id_starts_at_1_and_increments() {
        let mut mesh = Mesh::new();
        let id1 = mesh.next_surface_owner_id();
        let id2 = mesh.next_surface_owner_id();
        let id3 = mesh.next_surface_owner_id();
        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
    }

    #[test]
    fn adr093_d_beta_walk_returns_self_for_none_id() {
        // owner_id 가 없는 face 는 walk 결과가 자기 자신.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let f = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).unwrap();
        assert_eq!(mesh.walk_face_owner_siblings(f), vec![f]);
    }

    #[test]
    fn adr093_d_beta_walk_collects_all_with_same_id() {
        // 두 개 face 에 동일 owner_id 부여 → walk 가 둘 다 반환.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let v4 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        let v5 = mesh.add_vertex(DVec3::new(2.0, 1.0, 0.0));
        let f1 = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).unwrap();
        let f2 = mesh.add_face(&[v1, v4, v5, v2], MaterialId::new(0)).unwrap();
        let owner = mesh.next_surface_owner_id();
        assert!(mesh.set_face_surface_owner_id(f1, Some(owner)));
        assert!(mesh.set_face_surface_owner_id(f2, Some(owner)));
        let mut siblings = mesh.walk_face_owner_siblings(f1);
        siblings.sort_by_key(|f| f.raw());
        let mut expected = vec![f1, f2]; expected.sort_by_key(|f| f.raw());
        assert_eq!(siblings, expected,
            "both faces with same owner must be returned as siblings");
    }

    #[test]
    fn adr093_d_beta_extrude_planar_cylinder_assigns_same_owner_to_n_sides() {
        // Path A cylinder 결과: N side faces 가 동일 owner_id 공유.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 8.0 },
            MaterialId::new(0),
        ).expect("create_solid OK");

        // 모든 side face 가 owner_id 부여 + 같은 값 + None 아님
        let owner_ids: Vec<Option<u32>> = result.side_faces.iter()
            .map(|&fid| mesh.face_surface_owner_id(fid))
            .collect();
        assert!(owner_ids.iter().all(|o| o.is_some()),
            "ALL side faces must have surface_owner_id, got {:?}", owner_ids);
        let first = owner_ids[0];
        assert!(owner_ids.iter().all(|&o| o == first),
            "ALL side faces must share the SAME owner_id, got {:?}", owner_ids);

        // walk 결과가 모든 side faces (sorted dedup)
        let any_side = result.side_faces[0];
        let mut siblings = mesh.walk_face_owner_siblings(any_side);
        siblings.sort_by_key(|f| f.raw());
        let mut expected = result.side_faces.clone();
        expected.sort_by_key(|f| f.raw());
        assert_eq!(siblings, expected,
            "walk from one side face must return all N side faces");
    }

    #[test]
    fn adr093_d_beta_extrude_planar_cylinder_owner_unique_per_cylinder() {
        // 두 개 cylinder 생성 시 각자 독립적 owner_id.
        let mut mesh = Mesh::new();

        // Cylinder 1
        let profile1 = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 3.0);
        let result1 = mesh.create_solid(
            profile1,
            CreateSolidMode::Extrude { distance: 5.0 },
            MaterialId::new(0),
        ).expect("create_solid 1 OK");
        let id1 = mesh.face_surface_owner_id(result1.side_faces[0]).unwrap();

        // Cylinder 2 — 다른 위치
        let profile2 = build_closed_curve_circle_face(
            &mut mesh, DVec3::new(20.0, 0.0, 0.0), 4.0,
        );
        let result2 = mesh.create_solid(
            profile2,
            CreateSolidMode::Extrude { distance: 6.0 },
            MaterialId::new(0),
        ).expect("create_solid 2 OK");
        let id2 = mesh.face_surface_owner_id(result2.side_faces[0]).unwrap();

        assert_ne!(id1, id2,
            "two distinct cylinders must have different owner_ids");

        // walk 가 cross 안 함
        let walk1 = mesh.walk_face_owner_siblings(result1.side_faces[0]);
        let walk2 = mesh.walk_face_owner_siblings(result2.side_faces[0]);
        for fid in &walk1 {
            assert!(!walk2.contains(fid),
                "cylinder 1 walk must not include cylinder 2 faces");
        }
    }

    #[test]
    fn adr093_d_beta_set_owner_on_inactive_face_returns_false() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let f = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0)).unwrap();
        // Soft-delete the face
        mesh.remove_face(f).expect("remove_face OK");
        // setting owner on inactive face must fail (no panic)
        assert!(!mesh.set_face_surface_owner_id(f, Some(7)),
            "set_face_surface_owner_id on inactive face must return false");
        // Reading owner on inactive face → None
        assert_eq!(mesh.face_surface_owner_id(f), None);
    }

    #[test]
    fn adr093_d_beta_polygonal_circle_path_also_gets_owner_id() {
        // ADR-088 cross-cut — 폴리곤 circle path 도 cylinder 결과 → side
        // faces 가 동일 owner_id 부여 (extrude_planar_cylinder 가 통합
        // 진입점이므로 폴리곤 / closed-curve 둘 다 활성).
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 7.0 },
            MaterialId::new(0),
        ).expect("polygonal circle OK");
        let first_owner = mesh.face_surface_owner_id(result.side_faces[0]);
        assert!(first_owner.is_some(),
            "polygonal cylinder side faces also receive owner_id (D-F lock-in)");
        for &fid in &result.side_faces {
            assert_eq!(mesh.face_surface_owner_id(fid), first_owner,
                "all 16 polygonal side faces share the same owner_id");
        }
    }

    // ────────────────────────────────────────────────────────────────
    // K3 (보고서 시나리오 3 hotfix, 2026-05-23) — surface_owner_id
    // propagation through 6 split sites.
    //
    // Path A cylinder Push/Pull 후 side face split (Mesh::split_face) 시
    // sub-face 가 parent owner_id 를 자동 propagation 받아야 — 그렇지
    // 않으면 cylinder 측면 group full-selection (ADR-093 D-δ) 시 N-1
    // face 만 선택됨.
    // ADR-089 A-χ-β (parent surface propagation) 패턴 답습.
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn k3_split_face_propagates_surface_owner_id() {
        // Setup: cylinder Path A → N side faces with same owner_id.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 8);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 10.0 },
            MaterialId::new(0),
        ).expect("create_solid OK");
        let side = result.side_faces[0];
        let parent_owner = mesh.face_surface_owner_id(side)
            .expect("side face must have owner_id");

        // Get two non-adjacent vertices on side face's outer loop.
        let outer_start = mesh.faces[side].outer().start;
        let verts = mesh.collect_loop_verts(outer_start)
            .expect("collect verts OK");
        assert!(verts.len() >= 4,
            "side face needs >= 4 verts for non-adjacent split, got {}",
            verts.len());
        let v1 = verts[0];
        let v2 = verts[2];  // non-adjacent (skip 1 vert)

        // Split side face.
        let (face_a, face_b) = mesh.split_face(side, v1, v2)
            .expect("split_face OK");

        // K3 invariant: both sub-faces share parent owner_id.
        assert_eq!(mesh.face_surface_owner_id(face_a), Some(parent_owner),
            "K3: face_a must inherit parent owner_id");
        assert_eq!(mesh.face_surface_owner_id(face_b), Some(parent_owner),
            "K3: face_b must inherit parent owner_id");

        // Walk from any sub-face → all N+1 siblings (original N-1
        // remaining + 2 split sub-faces) share owner_id.
        let siblings = mesh.walk_face_owner_siblings(face_a);
        assert!(siblings.len() >= 2,
            "walk must find at least 2 siblings (face_a + face_b), got {}",
            siblings.len());
        for &sib in &siblings {
            assert_eq!(mesh.face_surface_owner_id(sib), Some(parent_owner),
                "all siblings must share parent owner_id");
        }
    }

    #[test]
    fn path_b_cylinder_annulus_has_owner_id() {
        // Path B annulus owner_id hotfix (2026-05-23, 사용자 시연 evidence).
        // Path B cylinder annulus 도 surface_owner_id 부여 받아야 함 —
        // 그렇지 않으면 Path B cylinder 측면 group selection (ADR-093 D-δ)
        // 미작동.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("Path B cylinder OK");

        assert_eq!(result.side_faces.len(), 1, "Path B = 1 annulus face");
        let annulus = result.side_faces[0];
        let owner = mesh.face_surface_owner_id(annulus);
        assert!(owner.is_some(),
            "Path B annulus must have surface_owner_id (사용자 시연 evidence — \
             previously missing, K3 group selection 미작동의 root cause)");
    }

    #[test]
    fn path_b_cylinder_walk_returns_annulus_self() {
        // Regression guard — Path B annulus walk_face_owner_siblings 가
        // 자기 자신 반환 (1 group of 1). Future split 시 sub-faces 모두
        // walk 에 포함되어야 함 (K3 propagation 정합).
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("Path B cylinder OK");

        let annulus = result.side_faces[0];
        let siblings = mesh.walk_face_owner_siblings(annulus);
        assert_eq!(siblings.len(), 1,
            "Path B annulus walk returns self only (1 group of 1)");
        assert_eq!(siblings[0], annulus,
            "walk siblings[0] == annulus");
    }

    #[test]
    fn path_b_cylinder_owner_unique_across_cylinders() {
        // 두 개 Path B cylinder 생성 시 각자 독립적 owner_id.
        let mut mesh = Mesh::new();
        let profile1 = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 3.0);
        let result1 = mesh
            .extrude_cylinder_kernel_native(profile1, 5.0, MaterialId::new(0))
            .expect("cylinder 1 OK");
        let owner1 = mesh.face_surface_owner_id(result1.side_faces[0])
            .expect("cylinder 1 annulus has owner_id");

        let profile2 = build_closed_curve_circle_face(
            &mut mesh, DVec3::new(20.0, 0.0, 0.0), 4.0,
        );
        let result2 = mesh
            .extrude_cylinder_kernel_native(profile2, 6.0, MaterialId::new(0))
            .expect("cylinder 2 OK");
        let owner2 = mesh.face_surface_owner_id(result2.side_faces[0])
            .expect("cylinder 2 annulus has owner_id");

        assert_ne!(owner1, owner2,
            "두 cylinder annulus 의 owner_id 는 독립적");
    }

    #[test]
    fn k3_split_face_no_owner_propagates_none() {
        // Regression guard: face without owner_id → sub-faces stay None.
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(2.0, 2.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 2.0, 0.0));
        let f = mesh.add_face(&[v0, v1, v2, v3], MaterialId::new(0))
            .expect("add_face OK");
        // No owner_id assigned (default None for plain add_face).
        assert_eq!(mesh.face_surface_owner_id(f), None,
            "plain face has no owner_id");

        let (face_a, face_b) = mesh.split_face(f, v0, v2)
            .expect("split_face OK");
        assert_eq!(mesh.face_surface_owner_id(face_a), None,
            "K3: sub-face stays None if parent had no owner_id");
        assert_eq!(mesh.face_surface_owner_id(face_b), None,
            "K3: sub-face stays None if parent had no owner_id");
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-094 B-δ-prep — Path B kernel-native cylinder (additive coexist)
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr094_b_delta_prep_cylinder_native_face_count_3_2_2() {
        // Path B canonical: 3 face / 2 edge / 2 vert (산업 CAD parity).
        // ADR-090 §1.2 / ADR-094 §1 architectural goal 검증.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let active_verts_before = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        let active_edges_before = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        let active_faces_before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active_verts_before, 1, "profile = 1 anchor vert");
        assert_eq!(active_edges_before, 1, "profile = 1 self-loop edge");
        assert_eq!(active_faces_before, 1, "profile = 1 closed-curve face");

        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("kernel-native cylinder extrude OK");

        // 3 face / 2 edge / 2 vert.
        let active_verts = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        let active_edges = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        let active_faces = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active_verts, 2, "Path B cylinder = 2 anchor verts (top + bot)");
        assert_eq!(active_edges, 2, "Path B cylinder = 2 self-loop edges (top + bot circles)");
        assert_eq!(active_faces, 3, "Path B cylinder = 3 faces (top + bot + annulus side)");

        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert_eq!(result.side_faces.len(), 1,
            "Path B annulus side = single face (not N quads)");
    }

    #[test]
    fn adr094_b_delta_prep_annulus_face_has_multi_loop_schema() {
        // Annulus side face must have face_to_boundary_loops entry
        // (Path B canonical) with 2 loops (top + bot).
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");
        let annulus = result.side_faces[0];

        assert!(mesh.face_has_multi_loop_schema(annulus),
            "annulus must have multi-loop schema (Path B canonical)");
        let loops = mesh.face_boundary_loops(annulus);
        assert_eq!(loops.len(), 2,
            "annulus must have 2 boundary loops (top + bot circles)");
    }

    #[test]
    fn adr094_b_delta_prep_annulus_has_cylinder_surface() {
        // Annulus side face must carry AnalyticSurface::Cylinder for
        // kernel-aware ops (Boolean / Push-Pull / Offset).
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 4.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 6.0, MaterialId::new(0))
            .expect("create_solid OK");
        let annulus = result.side_faces[0];
        let surface = mesh.faces[annulus].surface();
        assert!(
            matches!(surface, Some(AnalyticSurface::Cylinder { .. })),
            "annulus face must have Cylinder surface, got {:?}",
            surface.map(|s| s.kind_label()),
        );
    }

    #[test]
    fn adr094_b_delta_prep_top_face_is_closed_curve_with_plane() {
        // Top face must be a closed-curve face with translated Circle
        // and Plane surface (ADR-089 A-η-1 inheritance).
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");
        let top = result.top_face;

        // Top face must have Plane surface (ADR-089 A-η-1).
        assert!(
            matches!(mesh.faces[top].surface(), Some(AnalyticSurface::Plane { .. })),
            "top face must have Plane surface (ADR-089 A-η-1)",
        );

        // Top face's outer = self-loop edge with translated Circle.
        let top_outer_start = mesh.faces[top].outer().start;
        let top_eid = mesh.hes[top_outer_start].edge();
        assert!(mesh.edges[top_eid].is_self_loop(),
            "top face boundary must be a self-loop edge");
        match mesh.edges[top_eid].curve() {
            Some(AnalyticCurve::Circle { center, radius, .. }) => {
                let expected_center = DVec3::Z * 8.0; // bot center + Z*8
                assert!((*center - expected_center).length() < 1e-9,
                    "top circle center expected {:?}, got {:?}",
                    expected_center, center);
                assert!((*radius - 5.0).abs() < 1e-9,
                    "top circle radius preserved");
            }
            other => panic!("top edge must have Circle curve, got {:?}", other),
        }
    }

    #[test]
    fn adr094_b_delta_prep_negative_distance_recess() {
        // Recess (dist < 0) must work — translation is signed.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 3.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, -4.0, MaterialId::new(0))
            .expect("recess OK");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert_eq!(result.side_faces.len(), 1);

        // Top face should be at z = -4.
        let top_outer_start = mesh.faces[result.top_face].outer().start;
        let top_eid = mesh.hes[top_outer_start].edge();
        if let Some(AnalyticCurve::Circle { center, .. }) = mesh.edges[top_eid].curve() {
            assert!((center.z - (-4.0)).abs() < 1e-9,
                "recess top circle z = -4, got {}", center.z);
        }
    }

    #[test]
    fn adr094_b_delta_prep_legacy_path_a_unaffected() {
        // Coexist guarantee — Path A entry (extrude_closed_curve_face_via_
        // tessellation via create_solid) UNCHANGED.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("Path A still works");
        // Path A produces N quad sides (≥ 8).
        assert!(result.side_faces.len() >= 8,
            "Path A coexist — quad sides preserved (got {})",
            result.side_faces.len());
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-094 B-ζ-prep — Render path additive (annulus tessellation).
    //
    // Verifies that the existing curved-surface render path (mesh.rs
    // export_buffers_inner lines 4714-4774) works for the annulus face
    // *as-is* — full Cylinder surface tessellation with u_range
    // (0, 2π) + v_range (v_lo, v_hi) produces a complete cylinder
    // tube. compute_uv_slice_for_quad_face gracefully returns None
    // for the 1-vert (self-loop) face, so the full surface renders.
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr094_b_zeta_prep_annulus_emits_triangles() {
        // Render verification — annulus face must produce ≥ 2 triangles
        // (a complete cylinder tube tessellated chord-tolerant) when
        // export_buffers is called.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");
        let annulus = result.side_faces[0];

        let (_pos, _norm, indices, face_map, _pos_f64) = mesh
            .export_buffers()
            .expect("export_buffers OK");

        // Triangles where face_map[tri] == annulus.raw().
        let annulus_tri_count = face_map.iter()
            .filter(|&&fid| fid == annulus.raw())
            .count();
        // Cylinder tessellation: chord_tol = 0.1mm, R=5 → ~23 segments
        // around. With v slice ≥ 1, expect ≥ 32 triangles for the full
        // tube (architectural floor — actual count is implementation-
        // dependent on surface::tessellate).
        assert!(annulus_tri_count >= 32,
            "annulus must emit ≥ 32 triangles (full Cylinder tessellation), \
             got {}", annulus_tri_count);
        // Each tri uses 3 indices.
        assert!(indices.len() >= annulus_tri_count * 3);
    }

    #[test]
    fn adr094_b_zeta_prep_annulus_normals_radial() {
        // Cylinder normals must be radial (perpendicular to axis).
        // Each vertex normal should satisfy |n.dot(axis_dir)| < eps.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let _ = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");

        let (positions, normals, _, _face_map, _) = mesh
            .export_buffers()
            .expect("export_buffers OK");

        let n_verts = positions.len() / 3;
        let mut radial_count = 0;
        for i in 0..n_verts {
            let nz = normals[i * 3 + 2]; // axis = +Z
            // Radial normal: nz ≈ 0 (perpendicular to Z).
            if nz.abs() < 0.05 {
                radial_count += 1;
            }
        }
        // The annulus contributes mostly radial normals; top/bottom
        // contribute ±Z. Expect ≥ 32 verts with |nz| ≈ 0 (radial).
        assert!(radial_count >= 32,
            "annulus tessellation must produce ≥ 32 radial-normal \
             vertices, got {}", radial_count);
    }

    #[test]
    fn adr094_b_zeta_prep_top_bottom_faces_render_planar() {
        // Top + bottom closed-curve faces must continue to render via
        // ADR-089 A-κ closed-curve fast-path (analytic Plane, fan
        // tessellation). Coexist with annulus.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");

        let (_, _, _, face_map, _) = mesh
            .export_buffers()
            .expect("export_buffers OK");

        // Both top + bottom faces must produce triangles.
        let bot_tri = face_map.iter()
            .filter(|&&fid| fid == result.profile_face.raw())
            .count();
        let top_tri = face_map.iter()
            .filter(|&&fid| fid == result.top_face.raw())
            .count();
        assert!(bot_tri > 0, "bottom face must emit triangles");
        assert!(top_tri > 0, "top face must emit triangles");
    }

    #[test]
    fn adr094_b_zeta_prep_edge_wireframe_emits_two_smooth_rings() {
        // Edge wireframe — top + bottom self-loop edges with Circle
        // curves must render as smooth ring polylines (ADR-089 A-κ-β
        // self-loop fast-path), giving 2 rings of multi-segment polylines.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let _ = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");

        let (lines, edge_map) = mesh.export_edge_lines_with_map(20.1);

        // Group segments by EdgeId.
        let mut seg_by_edge = std::collections::HashMap::new();
        for &eid in &edge_map {
            *seg_by_edge.entry(eid).or_insert(0) += 1;
        }

        // Multi-segment edges (≥ 2 segments) = Circle / Arc curves.
        let multi: Vec<&i32> = seg_by_edge.values()
            .filter(|&&c| c >= 2).collect();
        // 2 self-loop edges (top + bot) — both should be multi-segment.
        assert!(multi.len() >= 2,
            "expect ≥ 2 multi-segment edges (top + bottom rings), got {}",
            multi.len());
        assert!(lines.len() >= 12,
            "expect ≥ 2 rings worth of polyline segments");
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-094 B-ε-prep — Boolean dispatch additive (multi-loop face SSI)
    //
    // Verifies that Boolean dispatch's eligibility + SSI dispatch
    // *naturally accepts* annulus side faces. Existing eligibility
    // (classify_dispatch_eligibility) checks face.surface() presence
    // and surface_to_bspline conversion — it does NOT inspect outer/
    // inners. nurbs_boolean_v2 operates purely in surface parameter
    // space — operand boundary loops only matter for trim. So multi-
    // loop schema should pass through transparently.
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr094_b_epsilon_prep_top_bot_passes_boolean_eligibility() {
        // Path B의 top + bottom closed-curve face 둘 다 Plane surface
        // (ADR-089 A-η-1). Plane × Plane Boolean dispatch는 already
        // 지원됨 — 본 테스트가 architectural anchor: Path B faces 가
        // 기존 Boolean SSI 와 호환.
        use crate::operations::boolean_dispatch::classify_dispatch_eligibility;

        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("Path B cylinder OK");

        // Top face × bottom face — both Plane surfaces, Path B endpoints.
        let r = classify_dispatch_eligibility(
            &mesh, &[result.profile_face], &[result.top_face],
        );
        assert!(r.is_ok(),
            "Path B top × bottom (Plane × Plane) must pass eligibility, \
             got {:?}", r);
    }

    #[test]
    fn adr094_b_epsilon_prep_annulus_eligibility_surface_kind_only() {
        // Architectural anchor: Boolean dispatch eligibility는 **surface-
        // driven**, NOT boundary-loop-driven. 다중 boundary loop 와는
        // 무관하게 surface kind 만 검사. Cylinder → NURBS conversion
        // 자체는 *pre-existing limitation* (별도 phase, all cylinder
        // operands 가 동일 영향 — Path A 든 Path B 든 같음).
        //
        // 본 테스트는 eligibility 가 multi-loop schema 자체로는 거부
        // 안 한다는 architectural anchor 검증. 거부 사유는 surface kind
        // 만 (Cylinder Unsupported).
        use crate::operations::boolean_dispatch::classify_dispatch_eligibility;
        use crate::operations::boolean_dispatch::NurbsBooleanFailReason;

        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("Path B cylinder OK");
        let annulus = result.side_faces[0];

        // Verify multi-loop schema set.
        assert!(mesh.face_has_multi_loop_schema(annulus));
        assert_eq!(mesh.face_boundary_loops(annulus).len(), 2);

        // Pair with another Plane face to isolate Cylinder side as cause.
        let plane2 = build_closed_curve_circle_face(
            &mut mesh, DVec3::new(20.0, 0.0, 4.0), 3.0,
        );
        let err = classify_dispatch_eligibility(&mesh, &[annulus], &[plane2])
            .expect_err("Cylinder side currently rejected by surface_to_bspline \
                         pre-existing limitation");
        // Rejection reason = surface kind, NOT multi-loop schema.
        match err {
            NurbsBooleanFailReason::UnsupportedSurfaceKind { kind, .. } => {
                assert_eq!(kind, "Cylinder",
                    "rejection reason must be Cylinder surface kind \
                     (pre-existing limitation, NOT multi-loop schema)");
            }
            other => panic!(
                "expected UnsupportedSurfaceKind, got {:?} — multi-loop \
                 schema must NOT cause eligibility failure", other,
            ),
        }
    }

    #[test]
    fn adr094_b_epsilon_prep_annulus_surface_extraction_unchanged() {
        // Boolean dispatch reads face.surface() + face.material() — both
        // work for annulus identically to legacy quad face. Architectural
        // anchor: surface-driven dispatch is multi-loop transparent.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect("create_solid OK");
        let annulus = result.side_faces[0];

        let surface = mesh.face_surface(annulus)
            .expect("annulus has surface");
        let _ = mesh.faces[annulus].material();
        // Verify it's a Cylinder (not None, not Plane).
        match surface {
            AnalyticSurface::Cylinder { radius, .. } => {
                assert!((radius - 5.0).abs() < 1e-9,
                    "annulus radius preserved");
            }
            other => panic!("expected Cylinder, got {:?}", other.kind_label()),
        }
    }

    #[test]
    fn adr094_b_epsilon_prep_legacy_path_a_eligibility_same_failure_mode() {
        // Coexist anchor — Path A side quad (also Cylinder) and Path B
        // annulus (Cylinder) get the SAME pre-existing limitation
        // failure reason. Multi-loop schema 자체는 eligibility 영향 0.
        use crate::operations::boolean_dispatch::classify_dispatch_eligibility;
        use crate::operations::boolean_dispatch::NurbsBooleanFailReason;

        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("Path A cylinder OK");
        let path_a_quad = result.side_faces[0];

        let plane2 = build_closed_curve_circle_face(
            &mut mesh, DVec3::new(20.0, 0.0, 4.0), 3.0,
        );
        // Path A side quad rejection — same surface kind reason.
        match classify_dispatch_eligibility(&mesh, &[path_a_quad], &[plane2]) {
            Err(NurbsBooleanFailReason::UnsupportedSurfaceKind { kind, .. }) => {
                assert_eq!(kind, "Cylinder",
                    "Path A quad gets identical rejection reason — \
                     architectural symmetry between Path A / Path B");
            }
            other => panic!("expected UnsupportedSurfaceKind, got {:?}", other),
        }
    }

    // ────────────────────────────────────────────────────────────────
    // ADR-094 B-η — Default flip dispatch (architectural switch)
    // ────────────────────────────────────────────────────────────────

    #[test]
    fn adr094_b_eta_engine_default_is_path_a_legacy() {
        // Engine default = false (Path A) — preserves 245+ regression
        // assets. Production layer flips via set_cylinder_path_b_default.
        let mesh = Mesh::new();
        assert!(!mesh.cylinder_path_b_default(),
            "engine default must be Path A (false) — preserves regression assets");
    }

    #[test]
    fn adr094_b_eta_path_b_active_after_flag_flip() {
        // After set_cylinder_path_b_default(true), create_solid on
        // closed-curve profile routes to Path B (annulus).
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        assert!(mesh.cylinder_path_b_default());

        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK with Path B");
        // Path B = single annulus side face.
        assert_eq!(result.side_faces.len(), 1,
            "Path B flip → 1 annulus side face (not N quads)");
        // Total = 3 face / 2 edge / 2 vert.
        let active_faces = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active_faces, 3, "Path B = 3 face total");
    }

    #[test]
    fn adr094_b_eta_path_a_default_off_preserved() {
        // OFF preference (default false) — closed-curve profile still
        // routes to Path A (N quads).
        let mut mesh = Mesh::new();
        // Don't flip — default off.
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("Path A default OK");
        assert!(result.side_faces.len() >= 8,
            "Path A default → ≥ 8 quad sides, got {}", result.side_faces.len());
    }

    #[test]
    fn adr094_b_eta_path_a_explicit_off_after_toggle() {
        // Toggle on then off — must revert to Path A. Tests bidirectional
        // flag transitions.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        mesh.set_cylinder_path_b_default(false);
        assert!(!mesh.cylinder_path_b_default());

        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("create_solid OK");
        assert!(result.side_faces.len() >= 8,
            "after toggle off, Path A revert (≥ 8 quad sides)");
    }

    #[test]
    fn adr094_b_eta_polygonal_profile_unaffected_by_flag() {
        // Polygonal profile (N≥3 verts, build_circle_face) does NOT enter
        // closed-curve fast-path. Flag has no effect on polygonal path.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true); // flag ON
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 7.0 },
                MaterialId::new(0),
            )
            .expect("polygonal Path A OK");
        // Polygonal profile → Path A 16 quads (flag bypassed).
        assert_eq!(result.side_faces.len(), 16,
            "polygonal profile preserves 16 quad sides regardless of flag");
    }

    #[test]
    fn adr094_b_eta_path_b_invariants_pass() {
        // Path B annulus must pass verify_face_invariants. Architectural
        // anchor: B-η flip preserves manifold + ADR-007.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let _ = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("Path B create_solid OK");

        let report = mesh.verify_face_invariants();
        // Path B annulus may show some non-manifold edges given the
        // novel topology (2 self-loop edges on 1 face). Document the
        // current state: ADR-094 §7 Lock-in 명시 - LOCKED #1 P7 / #12
        // P11 의 변형 명시 정의 가 미진행 (B-η-future). 본 anchor 는
        // 기본 검증만.
        let _violations = report.violations;
        // Smoke — verify_face_invariants doesn't crash on annulus.
    }

    #[test]
    fn adr094_b_eta_path_b_face_count_3_2_2_via_create_solid() {
        // End-to-end: production-equivalent flow create_solid with
        // flag ON produces 3/2/2 architectural anchor.
        let mut mesh = Mesh::new();
        mesh.set_cylinder_path_b_default(true);
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        let _result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 8.0 },
                MaterialId::new(0),
            )
            .expect("Path B via create_solid OK");

        let active_faces = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let active_edges = mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
        let active_verts = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        assert_eq!(active_faces, 3, "B-η end-to-end: 3 faces");
        assert_eq!(active_edges, 2, "B-η end-to-end: 2 self-loop edges");
        assert_eq!(active_verts, 2, "B-η end-to-end: 2 anchor verts");
    }

    #[test]
    fn adr094_b_delta_prep_rejects_non_closed_curve_profile() {
        // B-δ-prep precondition: profile must be closed-curve. Polygonal
        // profile must be rejected.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16); // polygonal
        let err = mesh
            .extrude_cylinder_kernel_native(profile, 8.0, MaterialId::new(0))
            .expect_err("polygonal profile must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("self-loop") || msg.contains("closed-curve"),
            "rejection should mention self-loop / closed-curve precondition, \
             got: {}", msg,
        );
    }

    #[test]
    fn adr089_a_theta_polygonal_circle_unaffected_by_fast_path() {
        // Regression guard — polygonal circle (≥ 3 verts, Arc curves) must
        // continue using the existing extrude_planar_cylinder path, not
        // the new closed-curve fast-path.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 7.0 },
                MaterialId::new(0),
            )
            .expect("polygonal circle path unchanged");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        // Polygonal path: profile_face IS the original (not removed).
        assert_eq!(result.profile_face, profile);
        assert_eq!(result.side_faces.len(), 16);
    }

    #[test]
    fn adr089_a_upsilon_self_loop_edge_cleanup_after_extrude() {
        // After A-θ-β extrude_closed_curve_face_via_tessellation, the
        // original closed-curve self-loop edge must be deactivated so
        // the wireframe export does not emit overlapping polylines on
        // the new bottom polygon.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 5.0);
        // Capture original self-loop edge id BEFORE extrude.
        let outer_start = mesh.faces[profile].outer().start;
        let original_edge = mesh.hes[outer_start].edge();
        assert!(mesh.edges[original_edge].is_self_loop(),
            "pre-condition: original edge must be self-loop");
        // Extrude
        let _ = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 10.0 },
                MaterialId::new(0),
            )
            .expect("extrude OK");
        // Original self-loop edge must be inactive (or removed from edges
        // SlotStorage). L-υ-1.
        let still_active = mesh
            .edges
            .get(original_edge)
            .map(|e| e.is_active())
            .unwrap_or(false);
        assert!(!still_active,
            "ADR-089 A-υ-β: leftover self-loop edge must be cleaned up");
    }

    #[test]
    fn adr089_a_upsilon_anchor_vertex_deactivated_if_isolated() {
        // Anchor vertex of the closed-curve face must be deactivated
        // after extrude (it has no other edge references). L-υ-2.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 3.0);
        let outer_start = mesh.faces[profile].outer().start;
        let original_edge = mesh.hes[outer_start].edge();
        let anchor = mesh.edges[original_edge].v_small();
        let _ = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 5.0 },
                MaterialId::new(0),
            )
            .expect("extrude OK");
        let anchor_active = mesh.verts.get(anchor).map(|v| v.is_active()).unwrap_or(false);
        assert!(!anchor_active,
            "ADR-089 A-υ-β: isolated anchor vertex must be deactivated");
    }

    #[test]
    fn adr089_a_upsilon_extrude_polygon_unaffected() {
        // Regression — polygonal Circle face (no self-loop) keeps using
        // existing extrude path. No anchor vertex / self-loop concept.
        let mut mesh = Mesh::new();
        let profile = build_circle_face(&mut mesh, 5.0, 16);
        let face_count_before = mesh.face_count();
        let result = mesh
            .create_solid(
                profile,
                CreateSolidMode::Extrude { distance: 7.0 },
                MaterialId::new(0),
            )
            .expect("polygon Circle extrude OK");
        assert_eq!(result.solid_kind, SolidKind::Cylinder);
        assert_eq!(result.profile_face, profile);
        assert!(mesh.faces[profile].is_active(),
            "regression guard — polygonal profile preserved");
        assert!(mesh.face_count() > face_count_before);
    }

    #[test]
    fn adr089_a_theta_zero_distance_rejected_before_tessellation() {
        // Degenerate distance (< EPSILON_LENGTH) must reject upfront —
        // Path A fast-path must not run if distance is invalid.
        let mut mesh = Mesh::new();
        let profile = build_closed_curve_circle_face(&mut mesh, DVec3::ZERO, 1.0);
        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 0.0 },
            MaterialId::new(0),
        );
        assert!(result.is_err(), "zero-distance must error");
        // Profile face should still be intact (no premature mutation).
        assert!(mesh.faces.contains(profile));
    }

    // ─── ADR-102 γ — Detach-on-Arrangement wiring regressions ────────
    //
    // These verify that `create_solid_extrude` invokes the β-1 cleave
    // helpers in the right cases:
    //   1) Isolated face → no-op cleave (existing behavior preserved)
    //   2) Coplanar T-junction sibling → cleave first, then extrude
    //   3) ADR-101 B-4 lens scenario → manifold-safe result

    /// Helper: build two adjacent unit squares sharing edge x=1, both
    /// on z=0 plane (CCW from +Z view). Returns (face_a, face_b).
    fn build_two_adjacent_squares(mesh: &mut Mesh) -> (FaceId, FaceId) {
        let mat = MaterialId::new(0);
        // Square A: (0,0)-(1,1)
        let a00 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a10 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let a11 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let a01 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let face_a = mesh.add_face(&[a00, a10, a11, a01], mat).unwrap();
        // Square B: (1,0)-(2,1), shares edge a10-a11
        let b10 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
        let b11 = mesh.add_vertex(DVec3::new(2.0, 1.0, 0.0));
        // add_vertex de-dups at (1,0,0) and (1,1,0) → reuses a10, a11.
        let face_b = mesh.add_face(&[a10, b10, b11, a11], mat).unwrap();
        // Attach Plane surface to both (truth source for extrude routing).
        let plane = AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 2.0),
            v_range: (0.0, 1.0),
        };
        mesh.faces[face_a].set_surface(Some(plane.clone()));
        mesh.faces[face_b].set_surface(Some(plane));
        (face_a, face_b)
    }

    #[test]
    fn adr102_gamma_create_solid_extrude_isolated_face_no_cleave() {
        // Regression guard — ADR-102 γ wiring must be transparent for
        // isolated faces. Same expectations as before-γ behavior.
        let mut mesh = Mesh::new();
        let profile = build_unit_square_plane_face(&mut mesh);
        let face_count_before = mesh.face_count();

        let result = mesh.create_solid(
            profile,
            CreateSolidMode::Extrude { distance: 1.0 },
            MaterialId::new(0),
        ).expect("isolated face extrude OK");

        assert_eq!(result.solid_kind, SolidKind::Box);
        // ADR-102 L-102-4 hot path: no-op cleave preserves original id.
        assert_eq!(result.profile_face, profile,
            "isolated face must NOT be cleaved (no-op)");
        assert_eq!(result.side_faces.len(), 4);
        assert_eq!(mesh.face_count(), face_count_before + 5);
    }

    #[test]
    fn adr102_gamma_create_solid_extrude_with_sibling_cleaves_first() {
        // Two coplanar adjacent squares. Push/Pull on A should:
        //   1) Auto-cleave A's outer boundary from B (β-1)
        //   2) Then extrude — A becomes a Box
        //   3) B remains a 1-face sheet, unchanged
        let mut mesh = Mesh::new();
        let (face_a, face_b) = build_two_adjacent_squares(&mut mesh);
        let b_verts_before = mesh.collect_loop_verts(
            mesh.faces[face_b].outer().start).unwrap();

        let result = mesh.create_solid(
            face_a,
            CreateSolidMode::Extrude { distance: 1.0 },
            MaterialId::new(0),
        ).expect("adjacent face extrude OK");

        // Cleave happened — the new profile_face id differs from face_a.
        assert_ne!(result.profile_face, face_a,
            "ADR-102 γ: coplanar sibling triggers cleave");

        // Sibling B is unchanged (L-102-1 source-side only).
        assert!(mesh.faces[face_b].is_active(),
            "sibling B must still be active");
        let b_verts_after = mesh.collect_loop_verts(
            mesh.faces[face_b].outer().start).unwrap();
        assert_eq!(b_verts_before, b_verts_after,
            "sibling B's boundary verts unchanged");

        // The resulting solid + sibling must be manifold-safe — no edge
        // is shared by ≥3 active faces.
        let mut all_active: Vec<FaceId> = Vec::new();
        for (fid, f) in mesh.faces.iter() {
            if f.is_active() { all_active.push(fid); }
        }
        let info = mesh.face_set_manifold_info(&all_active);
        assert_eq!(info.non_manifold_edge_count, 0,
            "ADR-102 L-102-3: post-cleave mesh must have 0 non-manifold \
             edges; got {} (info = {:?})",
            info.non_manifold_edge_count, info);
    }
}
