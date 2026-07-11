//! ADR-194 β-1 — Push/Pull Phase 2 carve intent detection (read-only).
//!
//! Pure geometric query: given a face and a signed extrude distance, classify
//! whether the push would carve INTO existing solid material (penetration) and
//! whether it stops inside (`Pocket`) or exits the opposite wall (`Through`).
//!
//! Convention (ADR-007 outward normals): `dist >= 0` extrudes outward (adds
//! material) → `None`. `dist < 0` pushes inward; a ray from the face centroid
//! along `-normal` finds the nearest non-coplanar opposite face —
//!   - hit within `|dist|`            ⇒ `Through`
//!   - hit beyond `|dist|`            ⇒ `Pocket { depth: |dist| }`
//!   - no opposite wall at all        ⇒ `None` (no closed solid to carve;
//!                                       conservative — 메타-원칙 #16)
//!
//! **NO mutation, NO auto-trigger.** This is a pure query. The carve *dispatch*
//! (carve vs add vs MoveOnly) and its default ON/OFF is ADR-194 β-4 (separate
//! 사용자 결재) — 메타-원칙 #16 (자동화는 사용자 의도를 미리 알 수 없다).
//!
//! Scope (Stage B MVP, ADR-194 §5.2): straight perpendicular push. The opposite
//! wall is found by a single axis ray from the face centroid. Oblique push /
//! curved walls / multi-solid are Stage A (별도 ADR).
//!
//! Cross-link: ADR-194 (Phase 2 α spec), ADR-007 (outward normals), ADR-191
//! (ring→tube, β-2 will consume the intent), LOCKED #5 (coplanar offset tol).

use anyhow::{bail, Result};
use glam::{DVec2, DVec3};

use crate::surfaces::AnalyticSurface;
use crate::{mesh::Mesh, EdgeId, FaceId, MaterialId, VertId};

/// ADR-273 UX — user-facing reason a straight-tube through-drill cannot proceed:
/// the exit wall is not a parallel straight exit (a tapered / angled / non-convex
/// far wall). Surfaced when the exit punch finds no coplanar host, or the bridge's
/// anti-parallel guard trips — instead of a technical "no coplanar face contains
/// the polygon hole centroid …". 메타-원칙 #5 (사용자 편의).
const STRAIGHT_THROUGH_EXIT_MSG: &str =
    "경사진(비평행) 벽은 곧은 관통을 지원하지 않습니다 — 반대편 벽이 입구와 평행한 \
     곧은 벽에서 관통하거나 Boolean 빼기를 사용하세요 (a tapered / angled / non-convex \
     exit wall is not supported by the straight-tube drill)";

/// ADR-194 β-1 — what an inward extrude would mean w.r.t. existing material.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CarveIntent {
    /// Outward push, or no opposite solid wall → normal extrude-add (not a carve).
    None,
    /// Inward push that stops inside the solid → pocket/recess of `depth` mm.
    Pocket { depth: f64 },
    /// Inward push that reaches/exceeds the opposite wall → through-hole.
    Through,
}

/// ADR-194 β-2 — result of [`Mesh::drill_circular_through_hole`].
#[derive(Debug, Clone)]
pub struct DrillThroughResult {
    /// Near (entry) face, now a ring-with-hole.
    pub entry_face: FaceId,
    /// Far (exit) face, now a ring-with-hole.
    pub exit_face: FaceId,
    /// The N cylindrical tube-wall quads bridging the two hole loops.
    pub tube_faces: Vec<FaceId>,
    /// Through length (entry plane → exit plane along the drill normal).
    pub depth: f64,
}

/// ADR-252 — result of a blind pocket carve ([`Mesh::carve_pocket_from_source_face`]).
#[derive(Debug, Clone)]
pub struct PocketResult {
    /// Host wall face, now a ring-with-hole (the pocket opening).
    pub ring_face: FaceId,
    /// The recessed floor face (faces the opening).
    pub floor_face: FaceId,
    /// The N side-wall quads bridging the opening loop to the floor loop.
    pub wall_faces: Vec<FaceId>,
    /// Pocket depth (inward along −host_normal).
    pub depth: f64,
}

/// Ray/plane numeric epsilon.
const CARVE_EPS: f64 = 1e-6;
/// Two planes are the SAME plane (skip the source's coplanar siblings, e.g. the
/// ring around a punched hole): `|n·n'| > DOT` AND offset within `OFFSET`.
///
/// ADR-274 — reference the canonical plane tolerances (`plane.rs` SSOT) instead
/// of ad-hoc magic numbers. `1.0 - EPS_PLANE_NORMAL = 0.9999` (0.81°), matching
/// every other coplanarity gate. Behavior-preserving here: the coplanar ring is
/// exactly parallel (dot ≈ 1) and opposite walls are ~90° (dot ≈ 0), so the
/// tight `COPLANAR_OFFSET` gate — not the normal threshold — selects the ring.
const COPLANAR_DOT: f64 = 1.0 - crate::plane::EPS_PLANE_NORMAL;
/// Canonical plane-offset tolerance (`plane.rs` SSOT) — 1.5μm. (Distinct from
/// the 0.15μm vertex-dedup tolerance; this is "point lies on the plane".)
const COPLANAR_OFFSET: f64 = crate::plane::EPS_PLANE_OFFSET;
/// Through-vs-pocket boundary slack (a push that exactly reaches the far wall
/// counts as Through).
const THROUGH_SLACK: f64 = 1e-4;
/// ADR-262 β-3 — door-vs-window auto-detection (Q3 floor-snap): an opening whose
/// bottom sits in the lower `DOOR_FLOOR_FRACTION` of the wall height is a DOOR
/// (bottom snapped to the wall floor); higher = a WINDOW. Doors start at ≈0%,
/// windows' sills are usually ≥ 25% — 15% cleanly separates them.
const DOOR_FLOOR_FRACTION: f64 = 0.15;

/// ADR-262 β-1 — result of cutting a DOOR opening (floor-reaching notch) through
/// a wall. Unlike [`DrillThroughResult`] (a closed window ring), the door is a
/// U-notch: the front + back faces become Π, the bottom face loses the door
/// strip, and the opening is bridged by 3 jambs (left / header / right; the
/// bottom is open — the doorway threshold).
#[derive(Clone, Debug)]
pub struct DoorOpeningResult {
    /// Front host face, now a Π notch.
    pub front_face: FaceId,
    /// Back face (opposite wall), now a Π notch.
    pub back_face: FaceId,
    /// The 3 jamb faces [left, header, right].
    pub jamb_faces: Vec<FaceId>,
}

impl Mesh {
    /// ADR-194 β-1 (read-only) — classify the carve intent of extruding
    /// `face_id` by signed `dist` along its (outward) normal. Pure query; no
    /// mutation, no trigger. See module docs for the convention.
    pub fn detect_carve_intent(&self, face_id: FaceId, dist: f64) -> CarveIntent {
        // Outward / zero / non-finite → not a carve (normal extrude-add).
        if !dist.is_finite() || dist >= -CARVE_EPS {
            return CarveIntent::None;
        }
        let (src_n, src_pt, _poly) = match self.carve_face_plane(face_id) {
            Some(t) => t,
            None => return CarveIntent::None,
        };
        let dir = -src_n; // inward (opposite the outward normal)
        let len = -dist; // |dist|

        match self.carve_ray_nearest_face(src_pt, dir, face_id, src_n, src_pt) {
            Some(t) if len + THROUGH_SLACK >= t => CarveIntent::Through,
            Some(_) => CarveIntent::Pocket { depth: len },
            None => CarveIntent::None,
        }
    }

    /// `(outward_normal, centroid, world_polygon)` of an active face's outer
    /// loop, or `None` if missing / degenerate.
    fn carve_face_plane(&self, face_id: FaceId) -> Option<(DVec3, DVec3, Vec<DVec3>)> {
        let face = self.faces.get(face_id).filter(|f| f.is_active())?;
        let verts = self.collect_loop_verts(face.outer().start).ok()?;
        if verts.len() < 3 {
            return None;
        }
        let pts: Vec<DVec3> = verts
            .iter()
            .filter_map(|&v| self.vertex_pos(v).ok())
            .collect();
        if pts.len() < 3 {
            return None;
        }
        // Prefer the analytic Plane normal (exact); fall back to the cached
        // DCEL face normal.
        let n = match self.face_surface(face_id) {
            Some(AnalyticSurface::Plane { normal, .. }) => normal.normalize_or_zero(),
            _ => face.normal().normalize_or_zero(),
        };
        if n.length_squared() < 0.5 {
            return None;
        }
        let centroid = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        Some((n, centroid, pts))
    }

    /// Nearest active face hit by the ray `(origin, dir)`, excluding `skip` and
    /// any face coplanar with the source plane (normal `src_n` through
    /// `src_pt`). Returns the hit distance `t`, or `None` if nothing is struck.
    fn carve_ray_nearest_face(
        &self,
        origin: DVec3,
        dir: DVec3,
        skip: FaceId,
        src_n: DVec3,
        src_pt: DVec3,
    ) -> Option<f64> {
        let mut best: Option<f64> = None;
        for (fid, f) in self.faces.iter() {
            if fid == skip || !f.is_active() {
                continue;
            }
            let (fn_, fpt, poly) = match self.carve_face_plane(fid) {
                Some(t) => t,
                None => continue,
            };
            // Skip faces lying in the SAME plane as the source (the coplanar
            // ring around the push region — not an opposite wall). Anti-parallel
            // walls at a different offset (e.g. the box bottom) are NOT skipped
            // because their offset differs.
            if fn_.dot(src_n).abs() > COPLANAR_DOT
                && (fpt - src_pt).dot(src_n).abs() < COPLANAR_OFFSET
            {
                continue;
            }
            let denom = dir.dot(fn_);
            if denom.abs() < CARVE_EPS {
                continue; // ray parallel to the face plane
            }
            let t = (fpt - origin).dot(fn_) / denom;
            if t < CARVE_EPS {
                continue; // behind the origin / at the source
            }
            let hit = origin + dir * t;
            if point_in_face(hit, &poly, fn_) {
                best = Some(best.map_or(t, |b| b.min(t)));
            }
        }
        best
    }

    /// ADR-269 — would drilling `profile_pts` straight through be a CROSS-DRILL
    /// through a PRE-EXISTING hole/tube (which the straight-tube MVP cannot build)?
    ///
    /// The exit is punched on the nearest opposite face's plane. If that nearest
    /// face is an INTERIOR tube wall (a pre-existing hole the axis passes through),
    /// it is tiny and cannot contain the projected profile → cross-drill. A clean
    /// outer wall — or the far wall of another stacked solid (legitimate
    /// multi-solid drill) — DOES contain the projected profile. So: find the
    /// nearest opposite face + its through-distance, project the profile onto that
    /// plane, and if the face cannot host every projected point → cross-drill.
    ///
    /// Same filters + nearest pick as [`Self::carve_ray_nearest_face`]. Returns
    /// `true` only when a valid opposite face exists but cannot host the profile.
    fn carve_drill_is_cross_drill(&self, profile_pts: &[DVec3], n: DVec3) -> bool {
        if profile_pts.is_empty() {
            return false;
        }
        let center = profile_pts.iter().copied().sum::<DVec3>() / profile_pts.len() as f64;
        let dir = -n;
        let mut best: Option<(f64, DVec3, Vec<DVec3>)> = None; // (t, fn, poly)
        for (fid, f) in self.faces.iter() {
            if fid == FaceId::new(u32::MAX) || !f.is_active() {
                continue;
            }
            let (fn_, fpt, poly) = match self.carve_face_plane(fid) {
                Some(t) => t,
                None => continue,
            };
            if fn_.dot(n).abs() > COPLANAR_DOT && (fpt - center).dot(n).abs() < COPLANAR_OFFSET {
                continue;
            }
            let denom = dir.dot(fn_);
            if denom.abs() < CARVE_EPS {
                continue;
            }
            let t = (fpt - center).dot(fn_) / denom;
            if t < CARVE_EPS {
                continue;
            }
            if point_in_face(center + dir * t, &poly, fn_)
                && best.as_ref().map_or(true, |(bt, ..)| t < *bt)
            {
                best = Some((t, fn_, poly));
            }
        }
        let Some((t, fn_, poly)) = best else { return false };
        // Project every profile point onto the exit plane along the drill axis and
        // require the nearest opposite face to contain them all. A tiny interior
        // tube wall fails this; a full outer / stacked-solid wall passes.
        profile_pts.iter().any(|&p| !point_in_face(p + dir * t, &poly, fn_))
    }

    /// ADR-194 β-2 — drill a circular **through-hole** (A "dedicated bridge").
    ///
    /// Explicit op (NOT auto-triggered — 메타-원칙 #16; the push-driven dispatch
    /// is β-4). Reuses `punch_circular_hole` on the near + far faces, then
    /// bridges the two hole loops with N tube-wall quads using the existing
    /// punch verts (no new verts on the loops → manifold-safe weld).
    ///
    /// `center` + `normal` define the drill axis (axis = `normal`); the near
    /// (entry) face is found by `punch_circular_hole`'s host search, the far
    /// (exit) face by a ray along `-normal`.
    ///
    /// Errors: degenerate inputs, no opposite wall (not a through-solid), or a
    /// punch / bridge failure. The mesh is NOT auto-rolled-back here — the
    /// Scene layer wraps this in a snapshot (ADR-190 P0.2 pattern).
    pub fn drill_circular_through_hole(
        &mut self,
        center: DVec3,
        normal: DVec3,
        radius: f64,
        segments: u32,
    ) -> Result<DrillThroughResult> {
        if !(radius > 0.0) {
            bail!("drill: radius must be positive, got {radius}");
        }
        if segments < 3 {
            bail!("drill: segments must be >= 3, got {segments}");
        }
        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("drill: degenerate normal");
        }

        // 1) Through length = nearest opposite wall along -n. The coplanar-skip
        //    (src_n = n, src_pt = center) excludes the entry plane; side faces
        //    are ray-parallel → skipped. Done BEFORE any punch (original solid).
        let depth = self
            .carve_ray_nearest_face(center, -n, FaceId::new(u32::MAX), n, center)
            .ok_or_else(|| {
                anyhow::anyhow!("drill: no opposite wall along -normal (not a through-solid)")
            })?;
        if !(depth > 1e-6) {
            bail!("drill: opposite wall too close ({depth})");
        }
        // ADR-269 — reject cross-drilling through a pre-existing void (see
        // drill_polygon_through_hole). Representative profile = 4 rim points.
        let (bu, bv) = {
            let mut t = DVec3::X;
            if t.cross(n).length_squared() < 1e-6 { t = DVec3::Y; }
            let u = (t - n * t.dot(n)).normalize_or_zero();
            (u, n.cross(u).normalize_or_zero())
        };
        let rim = [
            center + bu * radius, center - bu * radius,
            center + bv * radius, center - bv * radius,
        ];
        if self.carve_drill_is_cross_drill(&rim, n) {
            bail!(
                "관통 축이 기존 구멍과 교차합니다 — 구멍 위치를 옮겨 주세요 \
                 (cross-drilling through an existing hole is not supported)"
            );
        }

        // 2) Punch the entry hole (near face) + grab its hole loop (HE order).
        let entry_face = self.punch_circular_hole(center, n, radius, segments)?;
        let e_loop = self
            .drill_extract_hole_loop(entry_face, center, radius)
            .ok_or_else(|| anyhow::anyhow!("drill: entry hole loop not found"))?;

        // 3) Punch the exit hole on the opposite plane (projected center).
        let exit_center = center - n * depth;
        let exit_face = self.punch_circular_hole(exit_center, n, radius, segments)?;
        let b_loop = self
            .drill_extract_hole_loop(exit_face, exit_center, radius)
            .ok_or_else(|| anyhow::anyhow!("drill: exit hole loop not found"))?;

        // 4-5) Reverse + rotationally align the exit loop, bridge one quad per
        //      segment, verify manifold — shared with the rectangular drill
        //      (ADR-249; mesh.rs:10358 "common helper" follow-up realized).
        self.bridge_through_loops(entry_face, exit_face, e_loop, b_loop, n, depth)
    }

    /// ADR-249 (P1) — drill a **rectangular** through-hole. The rect analog of
    /// [`Mesh::drill_circular_through_hole`]: punches `punch_rect_hole` on the
    /// near + far faces (axis = `normal`) and bridges the two 4-corner loops with
    /// the shared [`Mesh::bridge_through_loops`] tube builder. Convex straight-
    /// through MVP. Explicit op (NOT auto-triggered — 메타-원칙 #16).
    ///
    /// `corner_a`/`corner_b` define the rect (their bbox in the entry face's
    /// in-plane basis, per `punch_rect_hole`); `normal` is the drill axis. The far
    /// corners are the entry corners projected `depth` along `-normal`. Errors on
    /// degenerate inputs, no opposite wall, or a punch / bridge failure (the Scene
    /// layer wraps this in a snapshot — ADR-190 P0.2).
    pub fn drill_rect_through_hole(
        &mut self,
        corner_a: DVec3,
        corner_b: DVec3,
        normal: DVec3,
    ) -> Result<DrillThroughResult> {
        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("drill rect: degenerate normal");
        }
        let center = (corner_a + corner_b) * 0.5;

        // 1) Through length = nearest opposite wall along -n (original solid),
        //    measured BEFORE any punch (mirrors the circular drill).
        let depth = self
            .carve_ray_nearest_face(center, -n, FaceId::new(u32::MAX), n, center)
            .ok_or_else(|| {
                anyhow::anyhow!("drill rect: no opposite wall along -normal (not a through-solid)")
            })?;
        if !(depth > 1e-6) {
            bail!("drill rect: opposite wall too close ({depth})");
        }
        // ADR-269 — reject cross-drilling through a pre-existing void (see
        // drill_polygon_through_hole). Diagonal corners span the rect extent.
        if self.carve_drill_is_cross_drill(&[corner_a, corner_b], n) {
            bail!(
                "관통 축이 기존 구멍과 교차합니다 — 구멍 위치를 옮겨 주세요 \
                 (cross-drilling through an existing hole is not supported)"
            );
        }

        // 2) Punch the entry rect + grab its hole loop (match by POSITION — the
        //    host may already have sibling holes; see drill_extract_hole_loop_near).
        let entry_center = (corner_a + corner_b) * 0.5;
        let entry_face = self.punch_rect_hole(corner_a, corner_b, n)?;
        let e_loop = self
            .drill_extract_hole_loop_near(entry_face, entry_center)
            .ok_or_else(|| anyhow::anyhow!("drill rect: entry hole loop not found"))?;

        // 3) Punch the exit rect on the opposite plane (projected corners). A
        //    tapered / angled far wall lands the straight projection on no
        //    coplanar host → surface a clear reason (ADR-273 UX).
        let exit_a = corner_a - n * depth;
        let exit_b = corner_b - n * depth;
        let exit_face = self
            .punch_rect_hole(exit_a, exit_b, n)
            .map_err(|_| anyhow::anyhow!("{STRAIGHT_THROUGH_EXIT_MSG}"))?;
        let b_loop = self
            .drill_extract_hole_loop_near(exit_face, entry_center - n * depth)
            .ok_or_else(|| anyhow::anyhow!("drill rect: exit hole loop not found"))?;

        // 4-5) Shared tube bridge + anti-parallel/manifold guards.
        self.bridge_through_loops(entry_face, exit_face, e_loop, b_loop, n, depth)
    }

    /// ADR-249 (P5) — drill an **arbitrary-profile** through-hole. The polygon
    /// analog of [`Mesh::drill_rect_through_hole`]: punches `punch_polygon_hole`
    /// on the near + far faces (axis = `normal`) and bridges the two loops with
    /// the shared [`Mesh::bridge_through_loops`] tube builder. Convex straight-
    /// through MVP. Explicit op (NOT auto-triggered — 메타-원칙 #16).
    ///
    /// `loop_pts` = the profile loop (a simple polygon on/near the entry plane,
    /// CCW around the host normal); the far loop is the entry loop projected
    /// `depth` along `-normal`. Errors on degenerate inputs, no opposite wall, or
    /// a punch / bridge failure (Scene layer wraps in a snapshot — ADR-190 P0.2).
    pub fn drill_polygon_through_hole(
        &mut self,
        loop_pts: &[DVec3],
        normal: DVec3,
    ) -> Result<DrillThroughResult> {
        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("drill polygon: degenerate normal");
        }
        if loop_pts.len() < 3 {
            bail!(
                "drill polygon: needs at least 3 profile points, got {}",
                loop_pts.len()
            );
        }
        let center = loop_pts.iter().copied().sum::<DVec3>() / loop_pts.len() as f64;

        // 1) Through length to the nearest opposite wall (original solid).
        let depth = self
            .carve_ray_nearest_face(center, -n, FaceId::new(u32::MAX), n, center)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "drill polygon: no opposite wall along -normal (not a through-solid)"
                )
            })?;
        if !(depth > 1e-6) {
            bail!("drill polygon: opposite wall too close ({depth})");
        }
        // ADR-269 — reject cross-drilling: if the axis passes through a pre-existing
        // void (existing hole/tube), the nearest opposite face is the tiny interior
        // tube wall (which cannot host the profile), not the outer opposite wall. A
        // legitimate multi-solid / thick drill hosts the profile fine. The
        // straight-tube MVP cannot build the intersection; guide to reposition/Boolean.
        if self.carve_drill_is_cross_drill(loop_pts, n) {
            bail!(
                "관통 축이 기존 구멍과 교차합니다 — 구멍 위치를 옮겨 주세요 \
                 (cross-drilling through an existing hole is not supported by the straight-tube drill)"
            );
        }

        // 2) Punch the entry profile + grab its hole loop (match by POSITION — the
        //    host may already have sibling holes; see drill_extract_hole_loop_near).
        let entry_face = self.punch_polygon_hole(loop_pts, n)?;
        let e_loop = self
            .drill_extract_hole_loop_near(entry_face, center)
            .ok_or_else(|| anyhow::anyhow!("drill polygon: entry hole loop not found"))?;

        // 3) Punch the exit profile on the opposite plane (projected loop). If
        //    the far wall is not a parallel straight exit (tapered / angled), the
        //    straight-projected loop lands on no coplanar host → surface a clear
        //    reason instead of the technical "no coplanar face …" (ADR-273 UX).
        let exit_pts: Vec<DVec3> = loop_pts.iter().map(|&p| p - n * depth).collect();
        let exit_face = self
            .punch_polygon_hole(&exit_pts, n)
            .map_err(|_| anyhow::anyhow!("{STRAIGHT_THROUGH_EXIT_MSG}"))?;
        let b_loop = self
            .drill_extract_hole_loop_near(exit_face, center - n * depth)
            .ok_or_else(|| anyhow::anyhow!("drill polygon: exit hole loop not found"))?;

        // 4-5) Shared tube bridge + anti-parallel/manifold guards.
        self.bridge_through_loops(entry_face, exit_face, e_loop, b_loop, n, depth)
    }

    /// ADR-252 — carve a blind POCKET into a solid from a coplanar profile face
    /// drawn on one of its walls (the "Push/Pull a rect drawn on a face → pocket"
    /// SketchUp flow). `source_face` = the drawn profile sheet (coplanar with +
    /// contained in a solid wall); `depth` (> 0) = the inward recess depth.
    ///
    /// The profile sheet is consumed; the host wall becomes a ring-with-hole (the
    /// opening), a recessed floor is created `depth` inward, and N side walls
    /// bridge the opening to the floor → a watertight blind pocket. The drill_*
    /// analog with the EXIT punch replaced by a floor cap (ADR-249 bridge pattern,
    /// de-risk-simulated manifold). Convex straight pocket MVP; errors if `depth`
    /// reaches the opposite wall (use a through-hole / drill instead).
    pub fn carve_pocket_from_source_face(
        &mut self,
        source_face: FaceId,
        depth: f64,
    ) -> Result<PocketResult> {
        if !(depth > 1e-6) {
            bail!("pocket: depth must be positive, got {depth}");
        }

        // 1) Read the profile outline (polygon OR closed-curve) + normal + edges.
        // ADR-267 follow-up — a drawn circle is a closed-curve face (1 anchor +
        // self-loop edge); face_outline_points tessellates it to a polygon so the
        // rest of the pocket build (punch_polygon_hole + walls) is unchanged.
        if !self.faces.get(source_face).map(|f| f.is_active()).unwrap_or(false) {
            bail!("pocket: source face inactive/missing");
        }
        let outline = self.face_outline_points(source_face).ok_or_else(|| {
            anyhow::anyhow!("pocket: source face has no usable outline (need a polygon ≥3 verts or a closed curve)")
        })?;
        if outline.len() < 3 {
            bail!("pocket: source outline too small");
        }
        let n_s = self.faces[source_face].normal().normalize_or_zero();
        if n_s.length_squared() < 0.5 {
            bail!("pocket: degenerate source normal");
        }
        let centroid = outline.iter().copied().sum::<DVec3>() / outline.len() as f64;
        let src_edges: Vec<EdgeId> = self
            .collect_loop_hes(self.faces[source_face].outer().start)
            .map(|hes| hes.iter().map(|&h| self.hes[h].edge()).collect())
            .unwrap_or_default();

        // 2) Remove the profile sheet (face + its boundary edges) so the punch
        //    host-search hits the WALL — the sheet is the smallest coplanar
        //    containing face and would otherwise be picked as the host.
        self.remove_face(source_face);
        for e in src_edges {
            let _ = self.remove_edge_and_halfedges(e);
        }

        // 3) Punch the profile into the host wall → ring-with-hole + opening loop.
        //    Match the opening by POSITION (the host may already have sibling
        //    holes from other sub-faces — see drill_extract_hole_loop_near).
        let outline_center = outline.iter().copied().sum::<DVec3>() / outline.len() as f64;
        let ring = self.punch_polygon_hole(&outline, n_s)?;
        let opening = self
            .drill_extract_hole_loop_near(ring, outline_center)
            .ok_or_else(|| anyhow::anyhow!("pocket: opening loop not found"))?;
        let n_host = self.faces[ring].normal().normalize_or_zero();
        let inward = -n_host;

        // 4) Depth guard — the floor must stay inside the solid (not reach the
        //    opposite wall). carve ray from the centroid along `inward`.
        if let Some(t) =
            self.carve_ray_nearest_face(centroid, inward, ring, n_host, centroid)
        {
            if depth >= t - 1e-6 {
                bail!(
                    "pocket: depth {depth} reaches the opposite wall ({t}) — use a through-hole"
                );
            }
        }

        // 5) Floor loop = opening verts pushed `depth` inward (new verts).
        let floor: Vec<VertId> = opening
            .iter()
            .map(|&v| {
                let p = self.vertex_pos(v).unwrap();
                self.add_vertex(p + inward * depth)
            })
            .collect();

        // 6) Side walls — bridge opening → floor. ADR-268 — per-vertex nearest
        // pairing (no twist) + a UNIFORM winding so walls face INTO the void
        // (toward the pocket axis), mirroring the drill (bridge_through_loops) fix.
        // The old code aligned only vertex 0 (twist on circle/polygon) and used a
        // fixed order that left the walls facing into the MATERIAL (backside from
        // inside the pocket).
        let seed = if inward.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (seed - inward * seed.dot(inward)).normalize_or_zero();
        let vv = inward.cross(u);
        let proj = |p: DVec3| (p.dot(u), p.dot(vv));
        let cnt = opening.len();
        let mut b_rev = floor.clone();
        b_rev.reverse();
        // Nearest pairing: paired[i] = floor vertex nearest opening[i] in the
        // push-perpendicular plane.
        let o_proj: Vec<(f64, f64)> = opening
            .iter()
            .map(|&vtx| self.vertex_pos(vtx).map(proj))
            .collect::<Result<_>>()?;
        let f_proj: Vec<(f64, f64)> = b_rev
            .iter()
            .map(|&vtx| self.vertex_pos(vtx).map(proj))
            .collect::<Result<_>>()?;
        let mut paired: Vec<VertId> = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let (ox, oy) = o_proj[i];
            let mut best = f64::INFINITY;
            let mut bj = 0usize;
            for (j, &(fx, fy)) in f_proj.iter().enumerate() {
                let d = (fx - ox).powi(2) + (fy - oy).powi(2);
                if d < best {
                    best = d;
                    bj = j;
                }
            }
            paired.push(b_rev[bj]);
        }
        // Pocket axis (opening centroid) → walls must face toward it (into void).
        let mut axis_c = DVec3::ZERO;
        for &vtx in &opening {
            axis_c += self.vertex_pos(vtx)?;
        }
        axis_c /= cnt as f64;
        let flip = {
            let a = self.vertex_pos(opening[0])?;
            let a2 = self.vertex_pos(opening[1 % cnt])?;
            let b2 = self.vertex_pos(paired[1 % cnt])?;
            let b = self.vertex_pos(paired[0])?;
            let nrm0 = (a2 - a).cross(b2 - a).normalize_or_zero();
            let mid = (a + a2 + b2 + b) / 4.0;
            let r = mid - axis_c;
            let radial = (r - inward * r.dot(inward)).normalize_or_zero();
            nrm0.dot(radial) > 0.0
        };
        let material = self.faces[ring].material();
        let mut wall_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = opening[i];
            let a2 = opening[(i + 1) % cnt];
            let b = paired[i];
            let b2 = paired[(i + 1) % cnt];
            let quad = if flip { [b, b2, a2, a] } else { [a, a2, b2, b] };
            wall_faces.push(self.add_face(&quad, material)?);
        }

        // 7) Floor cap (closes the bottom; faces the opening = -inward, into the
        // void). ADR-268 — flip the loop if it would face +inward (into material,
        // backside from inside the pocket).
        let floor_face = {
            let mut fl = b_rev.clone();
            if fl.len() >= 3 {
                let p0 = self.vertex_pos(fl[0])?;
                let p1 = self.vertex_pos(fl[1])?;
                let p2 = self.vertex_pos(fl[2])?;
                let fnrm = (p1 - p0).cross(p2 - p0).normalize_or_zero();
                if fnrm.dot(inward) > 0.0 {
                    fl.reverse();
                }
            }
            self.add_face(&fl, material)?
        };

        // 8) Manifold guard (ADR-190 P0.2 — the caller's snapshot rolls back).
        let report = self.verify_face_invariants();
        if !report.is_valid() {
            bail!(
                "pocket: result not manifold ({} violations) — unsupported geometry",
                report.violations.len()
            );
        }
        Ok(PocketResult {
            ring_face: ring,
            floor_face,
            wall_faces,
            depth,
        })
    }

    /// ADR-271 β — carve a blind POCKET into a CURVED wall from a sketched cap
    /// (ADR-263 `draw_circle_on_cylinder` → cap). **MVP: Cylinder host.** The cap
    /// (a curved patch on the cylinder side) is recessed radially inward by
    /// `depth`: its boundary stays as the opening, a smaller cap (radius r−depth)
    /// becomes the floor, and radial side walls bridge the two → a watertight
    /// dimple.
    ///
    /// Unlike the planar pocket ([`Mesh::carve_pocket_from_source_face`]),
    /// `inward` is **per-vertex radial** (toward the cylinder axis), not a single
    /// plane normal (ADR-271 L2). The cap's boundary edges are SHARED with the
    /// remainder (annulus), so the walls weld to the freed cap-side half-edges in
    /// cap-loop order (no free re-punch — ADR-271 §3). `depth` must stay inside
    /// the solid (`< radius`).
    /// ADR-287 β-1 — shared core for a curved POCKET (inward) / BOSS (outward)
    /// on any analytic surface. The cap boundary loop is offset per-vertex by
    /// `offset_fn` (world pos → offset world pos); the cap is removed, N side
    /// walls weld to the freed cap-side half-edges in cap-loop order, and the
    /// offset loop is capped with a floor/roof face carrying `floor_surface`
    /// (ADR-089 A-χ inheritance). Topology is **surface-agnostic** — the wall
    /// winding `[a, a2, b2, b]` + forward floor order are forced by the
    /// remainder hole-loop welding → manifold by construction (ADR-286 β-1
    /// finding). Callers validate surface-specific depth bounds BEFORE calling.
    /// Returns `PocketResult { depth: 0.0 }` — callers set `depth`.
    fn curved_carve_core(
        &mut self,
        cap_face: FaceId,
        op_name: &str,
        offset_fn: impl Fn(DVec3) -> DVec3,
        floor_surface: crate::surfaces::AnalyticSurface,
    ) -> Result<PocketResult> {
        if !self.faces.get(cap_face).map(|f| f.is_active()).unwrap_or(false) {
            bail!("{op_name}: cap face inactive/missing");
        }
        // Opening = cap boundary loop (verts on the host surface).
        let opening = self.collect_loop_verts(self.faces[cap_face].outer().start)?;
        let cnt = opening.len();
        if cnt < 3 {
            bail!("{op_name}: cap boundary too small ({cnt} verts)");
        }
        let material = self.faces[cap_face].material();

        // Host (remainder) on the twin side of the cap boundary — captured
        // BEFORE removing the cap (returned for XIA reconcile).
        let first_he = self.faces[cap_face].outer().start;
        let host = {
            let tw = self.hes[first_he].next_rad();
            if tw.is_null() || tw == first_he {
                FaceId::new(u32::MAX)
            } else {
                self.hes[tw].face()
            }
        };

        // Offset loop verts (floor for pocket / roof for boss) — per-vertex
        // surface-normal offset supplied by the caller.
        let floor: Vec<VertId> = opening
            .iter()
            .map(|&v| {
                let p = self.vertex_pos(v).unwrap();
                self.add_vertex(offset_fn(p))
            })
            .collect();

        // Remove the cap — its boundary becomes the recess/boss opening; the
        // walls weld to the freed cap-side half-edges.
        self.remove_face(cap_face);

        // Side walls — bridge opening[i] → floor[i] in cap-loop order (reuses the
        // freed cap-side half-edge, welds to the remainder). floor[i] ↔ opening[i]
        // 1:1 → no twist. The floor edge (b2→b) is a NEW edge twinned by the cap.
        let mut wall_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = opening[i];
            let a2 = opening[(i + 1) % cnt];
            let b = floor[i];
            let b2 = floor[(i + 1) % cnt];
            wall_faces.push(self.add_face(&[a, a2, b2, b], material)?);
        }

        // Floor/roof cap — forward order twins the walls' (floor[i+1]→floor[i]).
        let floor_face = self.add_face(&floor, material)?;
        let _ = self.set_face_surface(floor_face, Some(floor_surface));

        // Manifold guard (ADR-190 P0.2 — the caller's snapshot rolls back).
        let report = self.verify_face_invariants();
        if !report.is_valid() {
            bail!(
                "{op_name}: result not manifold ({} violations)",
                report.violations.len()
            );
        }
        Ok(PocketResult {
            ring_face: host,
            floor_face,
            wall_faces,
            depth: 0.0,
        })
    }

    /// ADR-271/287 — carve a blind curved POCKET (inward recess) from a sketched
    /// cap (ADR-263 split). The opening is offset **per-vertex along the inward
    /// surface normal** by `depth`; the floor is the same analytic surface at the
    /// recessed parameter (ADR-089 A-χ). Cylinder (ADR-271) + Sphere (ADR-287 β-1);
    /// Cone/Torus are ADR-287 β-2/β-3.
    pub fn carve_curved_pocket(
        &mut self,
        cap_face: FaceId,
        depth: f64,
    ) -> Result<PocketResult> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if !(depth > 1e-6) {
            bail!("curved pocket: depth must be positive, got {depth}");
        }
        let surf = self.faces.get(cap_face).and_then(|f| f.surface().cloned());
        match surf {
            Some(S::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. }) => {
                if depth >= radius - 1e-6 {
                    bail!("curved pocket: depth {depth} reaches the axis (radius {radius})");
                }
                let axis_d = axis_dir.normalize_or_zero();
                let offset = move |p: DVec3| {
                    let rel = p - axis_origin;
                    let radial_out = rel - axis_d * rel.dot(axis_d);
                    p - radial_out.normalize_or_zero() * depth
                };
                let floor_surf = S::Cylinder {
                    axis_origin, axis_dir, radius: radius - depth, ref_dir,
                    u_range: (0.0, TAU), v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved pocket", offset, floor_surf)?;
                res.depth = depth;
                Ok(res)
            }
            Some(S::Cone { apex, axis_dir, half_angle, ref_dir, u_range, v_range }) => {
                let axis_d = axis_dir.normalize_or_zero();
                let sin_a = half_angle.sin();
                let cos_a = half_angle.cos();
                if !(sin_a > 1e-6) {
                    bail!("curved pocket: degenerate cone half-angle");
                }
                let offset = move |p: DVec3| {
                    // Cone normal at p = cos α · radial − sin α · axis (outward);
                    // inward pocket → −normal.
                    let d = p - apex;
                    let v = d.dot(axis_d);
                    let foot = apex + axis_d * v;
                    let radial = (p - foot).normalize_or_zero();
                    let n = radial * cos_a - axis_d * sin_a;
                    p - n * depth
                };
                // A constant normal-offset of a cone is a PARALLEL cone: same
                // half-angle, apex shifted along the axis by depth/sin α (ADR-287 §3).
                let floor_surf = S::Cone {
                    apex: apex + axis_d * (depth / sin_a),
                    axis_dir, half_angle, ref_dir, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved pocket", offset, floor_surf)?;
                res.depth = depth;
                Ok(res)
            }
            Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, u_range, v_range }) => {
                if depth >= minor_radius - 1e-6 {
                    bail!("curved pocket: depth {depth} reaches the tube center (minor {minor_radius})");
                }
                let offset = move |p: DVec3| {
                    // Torus normal = from the tube center circle outward; inward →
                    // toward the tube center (minor_radius − depth).
                    match crate::surfaces::torus::project_to_torus(
                        center, axis_dir, ref_dir, major_radius, minor_radius, p,
                    ) {
                        Some((_pt, u, v)) => {
                            let n = crate::surfaces::torus::normal(
                                center, axis_dir, ref_dir, major_radius, minor_radius, u, v,
                            );
                            p - n * depth
                        }
                        None => p, // degenerate — core manifold guard catches
                    }
                };
                let floor_surf = S::Torus {
                    center, axis_dir, ref_dir, major_radius,
                    minor_radius: minor_radius - depth, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved pocket", offset, floor_surf)?;
                res.depth = depth;
                Ok(res)
            }
            Some(S::Sphere { center, radius, axis_dir, ref_dir, u_range, v_range }) => {
                // Sphere normal = radial from center; inward → toward center. Works
                // for an N-vert (polyline-split) cap; a self-loop cap (analytic
                // circle-split, production drawCircleOnSphere) has a 1-vert boundary
                // and the core bails gracefully ("too small") — see ADR-287 §7.
                if depth >= radius - 1e-6 {
                    bail!("curved pocket: depth {depth} reaches the center (radius {radius})");
                }
                let offset = move |p: DVec3| {
                    let n = (p - center).normalize_or_zero();
                    p - n * depth
                };
                let floor_surf = S::Sphere {
                    center, radius: radius - depth, axis_dir, ref_dir, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved pocket", offset, floor_surf)?;
                res.depth = depth;
                Ok(res)
            }
            _ => bail!("curved pocket: cap must be a Cylinder/Sphere/Cone/Torus-surface face"),
        }
    }

    /// ADR-286/287 — raise a curved BOSS (outward protrusion) from a sketched cap
    /// (ADR-263 split): the mirror of [`Mesh::carve_curved_pocket`]. The opening
    /// is offset **per-vertex along the outward surface normal** by `height`; the
    /// roof is the same analytic surface at the raised parameter (ADR-089 A-χ).
    /// Topology is identical to the pocket (manifold by construction, ADR-286 β-1);
    /// unlike the pocket there is no inner bound — a boss may rise arbitrarily far.
    /// Cylinder (ADR-286) + Sphere (ADR-287 β-1); Cone/Torus are ADR-287 β-2/β-3.
    pub fn add_curved_boss(
        &mut self,
        cap_face: FaceId,
        height: f64,
    ) -> Result<PocketResult> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if !(height > 1e-6) {
            bail!("curved boss: height must be positive, got {height}");
        }
        let surf = self.faces.get(cap_face).and_then(|f| f.surface().cloned());
        match surf {
            Some(S::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. }) => {
                let axis_d = axis_dir.normalize_or_zero();
                let offset = move |p: DVec3| {
                    let rel = p - axis_origin;
                    let radial_out = rel - axis_d * rel.dot(axis_d);
                    p + radial_out.normalize_or_zero() * height
                };
                let roof_surf = S::Cylinder {
                    axis_origin, axis_dir, radius: radius + height, ref_dir,
                    u_range: (0.0, TAU), v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved boss", offset, roof_surf)?;
                res.depth = height;
                Ok(res)
            }
            Some(S::Cone { apex, axis_dir, half_angle, ref_dir, u_range, v_range }) => {
                let axis_d = axis_dir.normalize_or_zero();
                let sin_a = half_angle.sin();
                let cos_a = half_angle.cos();
                if !(sin_a > 1e-6) {
                    bail!("curved boss: degenerate cone half-angle");
                }
                let offset = move |p: DVec3| {
                    // Outward boss → +normal (cos α · radial − sin α · axis).
                    let d = p - apex;
                    let v = d.dot(axis_d);
                    let foot = apex + axis_d * v;
                    let radial = (p - foot).normalize_or_zero();
                    let n = radial * cos_a - axis_d * sin_a;
                    p + n * height
                };
                // Outward parallel cone: apex shifted the OTHER way (−height/sin α).
                let roof_surf = S::Cone {
                    apex: apex - axis_d * (height / sin_a),
                    axis_dir, half_angle, ref_dir, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved boss", offset, roof_surf)?;
                res.depth = height;
                Ok(res)
            }
            Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, u_range, v_range }) => {
                let offset = move |p: DVec3| {
                    match crate::surfaces::torus::project_to_torus(
                        center, axis_dir, ref_dir, major_radius, minor_radius, p,
                    ) {
                        Some((_pt, u, v)) => {
                            let n = crate::surfaces::torus::normal(
                                center, axis_dir, ref_dir, major_radius, minor_radius, u, v,
                            );
                            p + n * height
                        }
                        None => p,
                    }
                };
                let roof_surf = S::Torus {
                    center, axis_dir, ref_dir, major_radius,
                    minor_radius: minor_radius + height, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved boss", offset, roof_surf)?;
                res.depth = height;
                Ok(res)
            }
            Some(S::Sphere { center, radius, axis_dir, ref_dir, u_range, v_range }) => {
                // Outward boss → +radial normal. N-vert (polyline) cap only;
                // self-loop cap bails gracefully in the core (ADR-287 §7).
                let offset = move |p: DVec3| {
                    let n = (p - center).normalize_or_zero();
                    p + n * height
                };
                let roof_surf = S::Sphere {
                    center, radius: radius + height, axis_dir, ref_dir, u_range, v_range,
                };
                let mut res = self.curved_carve_core(cap_face, "curved boss", offset, roof_surf)?;
                res.depth = height;
                Ok(res)
            }
            _ => bail!("curved boss: cap must be a Cylinder/Sphere/Cone/Torus-surface face"),
        }
    }

    /// ADR-287 — the cap centroid's perpendicular distance from the surface axis
    /// (the "reach-the-axis" threshold that routes a deep inward push to a
    /// diametric THROUGH-drill instead of a blind pocket). For a Cylinder this is
    /// the (constant) radius; for a Cone it is the cap's local radius (v·tan α at
    /// the cap height). `None` for surfaces without an axis-through (Torus's
    /// natural through is a tube-bore, not diametric — deferred; Sphere/Plane).
    pub fn curved_cap_axis_radial(&self, cap_face: FaceId) -> Option<f64> {
        use crate::surfaces::AnalyticSurface as S;
        let face = self.faces.get(cap_face).filter(|f| f.is_active())?;
        let (axis_o, axis_d) = match face.surface()? {
            S::Cylinder { axis_origin, axis_dir, .. } => (*axis_origin, axis_dir.normalize_or_zero()),
            S::Cone { apex, axis_dir, .. } => (*apex, axis_dir.normalize_or_zero()),
            _ => return None,
        };
        let verts = self.collect_loop_verts(face.outer().start).ok()?;
        if verts.is_empty() {
            return None;
        }
        let centroid: DVec3 = verts.iter().filter_map(|&v| self.vertex_pos(v).ok()).sum::<DVec3>()
            / verts.len() as f64;
        let rel = centroid - axis_o;
        Some((rel - axis_d * rel.dot(axis_d)).length())
    }

    /// ADR-271 δ — drill a diametric THROUGH-hole from a sketched Cylinder cap: a
    /// straight bore along the cap-center radial, entering at the cap and exiting
    /// the opposite side of the cylinder. The cap + a mirrored exit patch are
    /// consumed; N tube walls bridge the two holes → a watertight genus-1 tunnel.
    ///
    /// Exit point (closed form): with `rout` = radial-outward at the cap center and
    /// `a_i` = the radial vector of entry vert `i`, the far intersection along
    /// `-rout` is `exit_i = entry_i − 2(a_i·rout)·rout`. Cross-drill (ADR-269) is
    /// honoured — if the bore crosses a pre-existing void the exit points miss the
    /// cylinder and the exit split fails (rejected).
    pub fn carve_curved_through(&mut self, cap_face: FaceId) -> Result<DrillThroughResult> {
        use crate::surfaces::AnalyticSurface as S;
        if !self.faces.get(cap_face).map(|f| f.is_active()).unwrap_or(false) {
            bail!("curved through: cap face inactive/missing");
        }
        // ADR-287 — generalize the diametric bore to Cone + Torus. The bore
        // reflects each entry vert across the axis-plane ⊥ rout; that preserves the
        // axial component AND the in-plane radius, so the exit lands on the same
        // analytic surface (cylinder const radius / cone radius=v·tanα at same v /
        // torus at mirrored u, same v). Only the exit SPLIT is per-surface.
        let surf = self.faces.get(cap_face).and_then(|f| f.surface().cloned());
        let (axis_o, axis_d) = match &surf {
            Some(S::Cylinder { axis_origin, axis_dir, .. }) => (*axis_origin, axis_dir.normalize_or_zero()),
            Some(S::Cone { apex, axis_dir, .. }) => (*apex, axis_dir.normalize_or_zero()),
            Some(S::Torus { center, axis_dir, .. }) => (*center, axis_dir.normalize_or_zero()),
            _ => bail!("curved through: cap must be a Cylinder/Cone/Torus-surface face"),
        };
        let entry = self.collect_loop_verts(self.faces[cap_face].outer().start)?;
        let cnt = entry.len();
        if cnt < 3 {
            bail!("curved through: cap boundary too small ({cnt} verts)");
        }

        // Radial vector (perpendicular to axis) of a point, + radial-outward unit
        // at the cap center.
        let radial_vec = |p: DVec3| -> DVec3 {
            let rel = p - axis_o;
            rel - axis_d * rel.dot(axis_d)
        };
        let center: DVec3 = entry.iter().filter_map(|&v| self.vertex_pos(v).ok()).sum::<DVec3>()
            / cnt as f64;
        let rout = radial_vec(center).normalize_or_zero();
        if rout.length_squared() < 0.5 {
            bail!("curved through: cap center on the axis (degenerate radial)");
        }

        // Exit points on the opposite surface (straight bore along −rout).
        let exit_pts: Vec<DVec3> = entry
            .iter()
            .map(|&v| {
                let p = self.vertex_pos(v).unwrap();
                p - rout * (2.0 * radial_vec(p).dot(rout))
            })
            .collect();

        // Host (remainder/annulus) = the twin-side face of the cap boundary.
        let host = {
            let tw = self.hes[self.faces[cap_face].outer().start].next_rad();
            if tw.is_null() { bail!("curved through: cap boundary is a free edge"); }
            self.hes[tw].face()
        };

        // Split the annulus at the exit ring — per-surface (ADR-269 — a bore
        // crossing a void → exit points off-surface → split fails → rejected).
        let split_res = match &surf {
            Some(S::Cylinder { .. }) => self.split_cylinder_face_by_circle(host, &exit_pts),
            Some(S::Cone { .. }) => self.split_cone_face_by_circle(host, &exit_pts),
            Some(S::Torus { .. }) => self.split_torus_face_by_circle(host, &exit_pts),
            _ => None,
        };
        let (exit_cap, _rem) = split_res
            .ok_or_else(|| anyhow::anyhow!(
                "curved through: exit split failed — 관통 축이 기존 구멍/특징과 교차하거나 반대면에 닿지 않습니다"
            ))?;
        let exit = self.collect_loop_verts(self.faces[exit_cap].outer().start)?;
        if exit.len() != cnt {
            // Re-pair handles minor count drift; a large mismatch is unsupported.
            if exit.is_empty() {
                bail!("curved through: exit ring empty");
            }
        }

        let material = self.faces[cap_face].material();
        // Remove BOTH caps → two aligned holes in the cylinder wall.
        self.remove_face(cap_face);
        self.remove_face(exit_cap);

        // Pair entry[i] → the exit vert nearest its straight projection exit_pts[i].
        let paired: Vec<VertId> = (0..cnt)
            .map(|i| {
                let target = exit_pts[i];
                exit.iter()
                    .copied()
                    .min_by(|&x, &y| {
                        let dx = self.vertex_pos(x).unwrap().distance_squared(target);
                        let dy = self.vertex_pos(y).unwrap().distance_squared(target);
                        dx.partial_cmp(&dy).unwrap()
                    })
                    .unwrap()
            })
            .collect();

        // Tube walls — bridge entry[i] → paired[i] in entry-loop order (welds the
        // freed cap-side half-edges). Winding faces into the tunnel (void).
        let mut tube_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = entry[i];
            let a2 = entry[(i + 1) % cnt];
            let b = paired[i];
            let b2 = paired[(i + 1) % cnt];
            tube_faces.push(self.add_face(&[a, a2, b2, b], material)?);
        }

        let report = self.verify_face_invariants();
        if !report.is_valid() {
            bail!(
                "curved through: result not manifold ({} violations)",
                report.violations.len()
            );
        }
        let depth = 2.0 * radial_vec(center).dot(rout); // diametric bore length
        Ok(DrillThroughResult {
            entry_face: host,
            exit_face: host,
            tube_faces,
            depth,
        })
    }

    /// ADR-252 Amendment 2 — drill a THROUGH-hole from a coplanar profile sheet
    /// drawn on a solid wall (the pocket's "push all the way through" sibling).
    /// The sheet is consumed (face + edges) so the punch host-search hits the
    /// wall; the profile is then drilled through both walls via
    /// [`Mesh::drill_polygon_through_hole`] (entry + exit rings + tube). Used by
    /// the Push/Pull tool when the inward push reaches the opposite wall.
    pub fn carve_through_from_source_face(
        &mut self,
        source_face: FaceId,
    ) -> Result<DrillThroughResult> {
        // ADR-267 follow-up — polygon OR closed-curve (circle) profile.
        if !self.faces.get(source_face).map(|f| f.is_active()).unwrap_or(false) {
            bail!("through: source face inactive/missing");
        }
        let outline = self.face_outline_points(source_face).ok_or_else(|| {
            anyhow::anyhow!("through: source face has no usable outline (need a polygon ≥3 verts or a closed curve)")
        })?;
        if outline.len() < 3 {
            bail!("through: source outline too small");
        }
        let n_s = self.faces[source_face].normal().normalize_or_zero();
        if n_s.length_squared() < 0.5 {
            bail!("through: degenerate source normal");
        }
        let src_edges: Vec<EdgeId> = self
            .collect_loop_hes(self.faces[source_face].outer().start)
            .map(|hes| hes.iter().map(|&h| self.hes[h].edge()).collect())
            .unwrap_or_default();
        // Consume the profile sheet so the drill's punch host-search hits the wall.
        self.remove_face(source_face);
        for e in src_edges {
            let _ = self.remove_edge_and_halfedges(e);
        }
        self.drill_polygon_through_hole(&outline, n_s)
    }

    /// ADR-249 — shared tube-wall bridge for the circular + rectangular drills.
    /// The exit wall must be anti-parallel to the drill `axis` (convex straight-
    /// through MVP); the two hole loops must have equal size. Reverses +
    /// rotationally aligns the exit loop, bridges one quad per segment
    /// (`[a2, a, b, b2]` welds its top edge as the twin of the entry inner HE,
    /// its bottom edge as the twin of the exit inner HE, verticals as twins with
    /// the adjacent quads → manifold tube wall, ADR-007), and verifies manifold
    /// (ADR-190 P0.2 — the caller's snapshot rolls back on Err; 메타-원칙 #6).
    fn bridge_through_loops(
        &mut self,
        entry_face: FaceId,
        exit_face: FaceId,
        e_loop: Vec<VertId>,
        b_loop: Vec<VertId>,
        axis: DVec3,
        depth: f64,
    ) -> Result<DrillThroughResult> {
        let n = axis;
        // The exit wall must face OPPOSITE the drill axis (anti-parallel). A
        // same-sign exit normal means the -n ray hit a non-convex internal
        // up-facing ledge, not the back wall — the `b_loop` reversal assumption
        // breaks and the tube would be silently non-manifold/inverted. Reject
        // explicitly (MVP = straight through convex solids; 메타-원칙 #5).
        let exit_n = self.faces[exit_face].normal().normalize_or_zero();
        if exit_n.dot(n) > -0.5 {
            bail!("{STRAIGHT_THROUGH_EXIT_MSG}");
        }
        let cnt = e_loop.len();
        if b_loop.len() != cnt {
            bail!(
                "drill: entry/exit hole loop size mismatch ({} vs {})",
                cnt,
                b_loop.len()
            );
        }

        // ADR-267 follow-up (drill wall winding + twist fix) — the old code aligned
        // only vertex 0 (rest assumed matching order → twisted quads on circle/hex)
        // and used a fixed quad order that left EVERY tube wall facing into the
        // MATERIAL (backside showed from inside the hole → "깨진/캡" 렌더). Replace
        // with PER-VERTEX nearest pairing (untwisted planar quads) + a UNIFORM
        // winding derived so all walls face INTO the void (toward the hole axis),
        // matching the solid's outward convention. Winding is applied uniformly, so
        // adjacent walls keep opposite HE directions on shared edges (manifold), and
        // the final verify_face_invariants guards the result (caller rolls back).
        let mut b_rev = b_loop.clone();
        b_rev.reverse();
        let seed = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (seed - n * seed.dot(n)).normalize_or_zero();
        let vv = n.cross(u);
        let proj = |p: DVec3| (p.dot(u), p.dot(vv));

        // Per-vertex nearest pairing: paired[i] = exit vertex geometrically nearest
        // e_loop[i] in the axis-perpendicular plane (straight-through convex ⇒
        // congruent loops ⇒ a clean bijective rotation, no twist).
        let e_proj: Vec<(f64, f64)> = e_loop
            .iter()
            .map(|&vtx| self.vertex_pos(vtx).map(proj))
            .collect::<Result<_>>()?;
        let b_proj: Vec<(f64, f64)> = b_rev
            .iter()
            .map(|&vtx| self.vertex_pos(vtx).map(proj))
            .collect::<Result<_>>()?;
        let mut paired: Vec<VertId> = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let (ex, ey) = e_proj[i];
            let mut best = f64::INFINITY;
            let mut bj = 0usize;
            for (j, &(bx, by)) in b_proj.iter().enumerate() {
                let d = (bx - ex).powi(2) + (by - ey).powi(2);
                if d < best {
                    best = d;
                    bj = j;
                }
            }
            paired.push(b_rev[bj]);
        }

        // Hole-axis centroid (entry plane) → outward-direction target.
        let mut e_centroid = DVec3::ZERO;
        for &vtx in &e_loop {
            e_centroid += self.vertex_pos(vtx)?;
        }
        e_centroid /= cnt as f64;

        // Base quad order a→a2→b2→b. Flip the whole tube uniformly if wall 0's
        // normal points AWAY from the axis (into material) instead of into the void.
        let flip = {
            let a = self.vertex_pos(e_loop[0])?;
            let a2 = self.vertex_pos(e_loop[1 % cnt])?;
            let b2 = self.vertex_pos(paired[1 % cnt])?;
            let b = self.vertex_pos(paired[0])?;
            let nrm0 = (a2 - a).cross(b2 - a).normalize_or_zero();
            let mid = (a + a2 + b2 + b) / 4.0;
            let r = mid - e_centroid;
            let radial = (r - n * r.dot(n)).normalize_or_zero();
            nrm0.dot(radial) > 0.0
        };

        // Bridge: one quad per segment, uniform winding.
        let material = self.faces[entry_face].material();
        let mut tube_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = e_loop[i];
            let a2 = e_loop[(i + 1) % cnt];
            let b = paired[i];
            let b2 = paired[(i + 1) % cnt];
            let quad = if flip { [b, b2, a2, a] } else { [a, a2, b2, b] };
            let f = self.add_face(&quad, material)?;
            tube_faces.push(f);
        }

        let report = self.verify_face_invariants();
        if !report.is_valid() {
            bail!(
                "drill: result is not manifold ({} invariant violations) — unsupported geometry",
                report.violations.len()
            );
        }
        Ok(DrillThroughResult {
            entry_face,
            exit_face,
            tube_faces,
            depth,
        })
    }

    /// ADR-252 — `true` if `face` is a coplanar profile **contained in a LARGER
    /// face** on the same plane (the "rect drawn on a wall" signal). Used by the
    /// Push/Pull tool to route an inward push to a pocket carve (vs a plain
    /// extrude). Read-only; the larger container is the host wall.
    /// ADR-267 follow-up (circle cut) — a face's outline as polygon points.
    ///
    /// Polygon face (≥3 boundary verts) → those vertex positions. **Closed-curve
    /// face** (ADR-089 kernel-native: 1 anchor vert + 1 self-loop edge carrying an
    /// `AnalyticCurve`, e.g. a drawn circle) → the curve tessellated to a polygon
    /// outline. This lets the coplanar-container detection + `carve_pocket_from_
    /// source_face` consume a circle (or freeform closed curve) profile the same
    /// way they consume a rect — matching `punch_circular_hole`'s faceted behavior.
    pub fn face_outline_points(&self, face: FaceId) -> Option<Vec<DVec3>> {
        let f = self.faces.get(face).filter(|x| x.is_active())?;
        let verts = self.collect_loop_verts(f.outer().start).ok()?;
        if verts.len() >= 3 {
            let pts: Vec<DVec3> = verts.iter().filter_map(|&v| self.vertex_pos(v).ok()).collect();
            return if pts.len() >= 3 { Some(pts) } else { None };
        }
        // Closed-curve profile: 1-vert self-loop with an analytic curve.
        if verts.len() == 1 {
            let hes = self.collect_loop_hes(f.outer().start).ok()?;
            if hes.len() == 1 {
                let eid = self.hes[hes[0]].edge();
                let curve = self.edges.get(eid).filter(|e| e.is_active())?.curve().cloned()?;
                return Self::curve_closed_outline(&curve);
            }
        }
        None
    }

    /// Tessellate a CLOSED analytic curve to a polygon outline (closing dup
    /// dropped). Line/Arc are not closed profiles → `None`.
    fn curve_closed_outline(curve: &crate::curves::AnalyticCurve) -> Option<Vec<DVec3>> {
        use crate::curves::AnalyticCurve;
        let tol = 0.1_f64;
        let finish = |pts: Vec<DVec3>| -> Option<Vec<DVec3>> {
            let mut pts = pts;
            if pts.len() >= 4
                && (pts[0] - pts[pts.len() - 1]).length() < crate::tolerances::EPSILON_LENGTH
            {
                pts.pop();
            }
            if pts.len() >= 3 { Some(pts) } else { None }
        };
        match curve {
            AnalyticCurve::Circle { center, radius, normal, basis_u } => {
                let ct = tol.min(radius * 0.02).max(1e-4);
                finish(crate::curves::circle::tessellate_full(*center, *radius, *normal, *basis_u, ct))
            }
            AnalyticCurve::Bezier { control_pts } => {
                crate::curves::bezier::tessellate(control_pts, tol).ok().and_then(finish)
            }
            AnalyticCurve::BSpline { control_pts, knots, degree } => {
                crate::curves::bspline::tessellate(control_pts, knots, *degree as usize, tol).ok().and_then(finish)
            }
            AnalyticCurve::NURBS { control_pts, weights, knots, degree } => {
                crate::curves::nurbs::tessellate(control_pts, weights, knots, *degree as usize, tol).ok().and_then(finish)
            }
            _ => None,
        }
    }

    pub fn face_has_larger_coplanar_container(&self, face: FaceId) -> bool {
        self.find_larger_coplanar_container_face(face).is_some()
    }

    /// ADR-252 — the LARGER coplanar face on the same plane that contains `face`
    /// (the host wall the profile was drawn on), or `None`. Smallest such larger
    /// container is returned (the most specific host). Read-only.
    pub fn find_larger_coplanar_container_face(&self, face: FaceId) -> Option<FaceId> {
        let f = self.faces.get(face).filter(|x| x.is_active())?;
        let n = f.normal().normalize_or_zero();
        if n.length_squared() < 0.5 {
            return None;
        }
        // ADR-267 follow-up — polygon OR closed-curve (circle) outline.
        let pts = self.face_outline_points(face)?;
        if pts.len() < 3 {
            return None;
        }
        let centroid = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let my_area = self.face_area(face);
        let mut best: Option<(FaceId, f64)> = None;
        for (hid, h) in self.faces.iter() {
            if hid == face || !h.is_active() {
                continue;
            }
            let hn = h.normal().normalize_or_zero();
            if hn.dot(n).abs() < 0.999 {
                continue; // not coplanar (parallel) with `face`
            }
            let h_area = self.face_area(hid);
            if h_area <= my_area + 1e-6 {
                continue; // only a strictly LARGER container counts
            }
            let hverts = match self.collect_loop_verts(h.outer().start) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            let hpts: Vec<DVec3> = hverts.iter().filter_map(|&v| self.vertex_pos(v).ok()).collect();
            if hpts.len() < 3 {
                continue;
            }
            // Same plane (my centroid within the spatial-hash tol of h's plane).
            if (centroid - hpts[0]).dot(hn).abs() > COPLANAR_OFFSET {
                continue;
            }
            if point_in_face(centroid, &hpts, hn) {
                // Smallest larger container = most specific host.
                if best.map(|(_, a)| h_area < a).unwrap_or(true) {
                    best = Some((hid, h_area));
                }
            }
        }
        best.map(|(f, _)| f)
    }

    /// ADR-252 Amendment 2 — distance from `source_face`'s plane to the opposite
    /// wall along the inward direction (the host solid's thickness under the
    /// profile), or `None` if there is no host wall / opposite wall. Used to
    /// decide pocket (depth < thickness) vs through-hole (depth ≥ thickness).
    pub fn wall_thickness_from_source_face(&self, source_face: FaceId) -> Option<f64> {
        let host = self.find_larger_coplanar_container_face(source_face)?;
        let n_host = self.faces.get(host)?.normal().normalize_or_zero();
        if n_host.length_squared() < 0.5 {
            return None;
        }
        // ADR-267 follow-up — polygon OR closed-curve (circle) outline, so the
        // through-vs-blind decision fires for a drawn circle too. Without this a
        // deep circle push always read as blind → carve bailed ("reaches opposite
        // wall") → the tool fell back to a capped extrude instead of a through-cut.
        let pts = self.face_outline_points(source_face)?;
        if pts.len() < 3 {
            return None;
        }
        let centroid = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        // Inward = into the solid (opposite the host wall's outward normal). The
        // source sheet (coplanar with the host) is skipped by the coplanar guard.
        self.carve_ray_nearest_face(centroid, -n_host, host, n_host, centroid)
    }

    /// Extract the inner loop of `face` whose centroid is geometrically NEAREST
    /// `target`. Robust variant of [`Mesh::drill_extract_new_hole_loop`] for a
    /// host wall that ALREADY has other holes (sibling sub-faces drawn on the
    /// same wall — the user's multi-rect panel). The "newest by VertId" heuristic
    /// breaks there: `punch_polygon_hole` may REUSE existing dedup'd verts
    /// (LOCKED #5) so the just-punched loop is NOT the highest-id one, and the
    /// tube bridge then welds mismatched entry/exit loops → non-manifold. Matching
    /// by position picks the correct loop regardless of id order.
    fn drill_extract_hole_loop_near(&self, face: FaceId, target: DVec3) -> Option<Vec<VertId>> {
        let f = self.faces.get(face)?;
        let mut best: Option<(f64, Vec<VertId>)> = None;
        for inner in f.inners() {
            if inner.start.is_null() {
                continue;
            }
            let verts = match self.collect_loop_verts(inner.start) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            let mut c = DVec3::ZERO;
            let mut ok = true;
            for &v in &verts {
                match self.vertex_pos(v) {
                    Ok(p) => c += p,
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            c /= verts.len() as f64;
            let d = (c - target).length_squared();
            if best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
                best = Some((d, verts));
            }
        }
        best.map(|(_, v)| v)
    }

    /// Find the hole loop just punched into `face` — the inner loop whose verts
    /// all lie ~`radius` from `center`. `punch_circular_hole` places the new
    /// circle verts at EXACTLY `radius` and pushes the loop FIRST (it is
    /// `inners()[0]`), so the first radial match is the just-punched hole even
    /// when the face already had holes — the tight band rejects a different-
    /// radius pre-existing hole. (A pre-existing *near-concentric near-equal-
    /// radius* hole is rejected upstream by punch's overlap check, and a wrong
    /// pick would anyway be caught by the post-bridge manifold guard.)
    fn drill_extract_hole_loop(
        &self,
        face: FaceId,
        center: DVec3,
        radius: f64,
    ) -> Option<Vec<VertId>> {
        let f = self.faces.get(face)?;
        // Tight: punch verts sit at exactly `radius` (drift ≤ 0.15μm); a 1% +
        // 1μm band accepts the punched loop but rejects a different-radius hole.
        let tol = radius * 0.01 + 1e-3;
        for inner in f.inners() {
            if inner.start.is_null() {
                continue;
            }
            let verts = match self.collect_loop_verts(inner.start) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            let matches = verts.iter().all(|&v| {
                self.vertex_pos(v)
                    .map(|p| ((p - center).length() - radius).abs() < tol)
                    .unwrap_or(false)
            });
            if matches {
                return Some(verts);
            }
        }
        None
    }

    /// ADR-262 β-1 — find the coplanar host wall face: active, normal ∥ `n`
    /// (|dot| > 0.999), `center` on its plane (≤ 1μm) and inside its outer loop.
    /// Mirrors `punch_rect_hole`'s host search.
    fn find_door_host(&self, center: DVec3, n: DVec3) -> Option<FaceId> {
        let plane_tol = 1e-3;
        for (fid, f) in self.faces.iter() {
            if !f.is_active() {
                continue;
            }
            let fnn = f.normal().normalize_or_zero();
            if fnn.length_squared() < 1e-10 || fnn.dot(n).abs() < 0.999 {
                continue;
            }
            let os = f.outer().start;
            if os.is_null() {
                continue;
            }
            let verts = match self.collect_loop_verts(os) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            let p0 = match self.vertex_pos(verts[0]) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if (center - p0).dot(fnn).abs() > plane_tol {
                continue;
            }
            let mut t = DVec3::X;
            if t.cross(fnn).length_squared() < 1e-6 {
                t = DVec3::Y;
            }
            let e1 = (t - fnn * t.dot(fnn)).normalize_or_zero();
            let e2 = fnn.cross(e1).normalize_or_zero();
            let proj = |p: DVec3| {
                let v = p - p0;
                (v.dot(e1), v.dot(e2))
            };
            let poly: Vec<(f64, f64)> = verts
                .iter()
                .filter_map(|v| self.vertex_pos(*v).ok())
                .map(proj)
                .collect();
            let (cx, cy) = proj(center);
            let m = poly.len();
            let mut inside = false;
            let mut j = m - 1;
            for i in 0..m {
                let (xi, yi) = poly[i];
                let (xj, yj) = poly[j];
                if ((yi > cy) != (yj > cy)) && (cx < (xj - xi) * (cy - yi) / (yj - yi + 1e-12) + xi) {
                    inside = !inside;
                }
                j = i;
            }
            if inside {
                return Some(fid);
            }
        }
        None
    }

    /// ADR-262 β-1 — notch a wall face by the door U-chain `BL→TL→TR→BR` and
    /// remove the door rect, leaving a Π face. `bl_pos`/`br_pos` land on the
    /// face's lowest horizontal outer edge; `tl_pos`/`tr_pos` at the header.
    /// Returns `(BL, BR, TL, TR)`. (Validated by `adr262_sim_door_notch_full_
    /// manifold`.)
    fn notch_wall_face_for_door(
        &mut self,
        face: FaceId,
        bl_pos: DVec3,
        br_pos: DVec3,
        tl_pos: DVec3,
        tr_pos: DVec3,
        mat: MaterialId,
    ) -> Result<(VertId, VertId, VertId, VertId)> {
        let edges = self.face_outer_edges(face)?;
        let bottom = edges
            .iter()
            .copied()
            .filter(|&e| {
                let a = self.vertex_pos(self.edges[e].v_small()).unwrap_or(DVec3::ZERO);
                let b = self.vertex_pos(self.edges[e].v_large()).unwrap_or(DVec3::ZERO);
                (a.z - b.z).abs() < 1e-3
            })
            .min_by(|&e1, &e2| {
                let z1 = self.vertex_pos(self.edges[e1].v_small()).map(|p| p.z).unwrap_or(f64::MAX);
                let z2 = self.vertex_pos(self.edges[e2].v_small()).map(|p| p.z).unwrap_or(f64::MAX);
                z1.partial_cmp(&z2).unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| anyhow::anyhow!("door notch: face has no horizontal bottom edge"))?;
        let (bl, el, er) = self.split_edge(bottom, bl_pos)?;
        // pick the sub-edge whose span contains br_pos (param t ∈ (0,1)).
        let right = [el, er]
            .into_iter()
            .find(|&e| {
                let a = self.vertex_pos(self.edges[e].v_small()).unwrap_or(DVec3::ZERO);
                let b = self.vertex_pos(self.edges[e].v_large()).unwrap_or(DVec3::ZERO);
                let d = b - a;
                let t = (br_pos - a).dot(d) / d.length_squared().max(1e-12);
                t > 0.001 && t < 0.999
            })
            .ok_or_else(|| anyhow::anyhow!("door notch: right sub-edge for BR not found"))?;
        let (br, _, _) = self.split_edge(right, br_pos)?;
        let tl = self.add_vertex(tl_pos);
        let tr = self.add_vertex(tr_pos);
        self.add_edge(bl, tl)?;
        self.add_edge(tl, tr)?;
        self.add_edge(tr, br)?;
        let split = crate::operations::face_split::split_face_by_chain(self, face, &[bl, tl, tr, br], mat)?;
        let door = split
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = self
                    .collect_loop_verts(self.faces[nf].outer().start)
                    .unwrap_or_default();
                vs.len() == 4 && vs.contains(&tl) && vs.contains(&tr)
            })
            .ok_or_else(|| anyhow::anyhow!("door notch: door-rect sub-face not found"))?;
        self.remove_face(door);
        Ok((bl, br, tl, tr))
    }

    /// ADR-262 β-1 — cut a DOOR opening (floor-reaching notch) through a box
    /// wall. A door reaches the wall's bottom edge → a U-notch (open bottom),
    /// not a closed ring (a window = [`Mesh::drill_rect_through_hole`]).
    ///
    /// `corner_a` / `corner_b` = two opposite corners of the door rect on the
    /// host wall face (one at the wall bottom edge, one at the header).
    /// `normal` = the host face's outward normal. MVP: an axis-aligned door on
    /// a Z-up vertical box wall, straight through to the anti-parallel opposite
    /// wall. A door whose bottom does NOT reach the wall bottom edge →
    /// `NotYetSupported`-style error (caller uses the window path). Errors leave
    /// a partially-mutated mesh → the Scene wrapper rolls back via its snapshot
    /// (ADR-190 P0.2). 메타-원칙 #16 — explicit op, no auto-trigger.
    ///
    /// Construction (Q2, validated by `adr262_sim_door_notch_full_manifold`):
    /// split FRONT + BACK by the door U-chain, notch the BOTTOM (remove the door
    /// strip), bridge 3 jambs → watertight closed manifold.
    pub fn cut_wall_door_opening(
        &mut self,
        corner_a: DVec3,
        corner_b: DVec3,
        normal: DVec3,
    ) -> Result<DoorOpeningResult> {
        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("door: degenerate normal");
        }
        if n.z.abs() > 0.1 {
            bail!("door: MVP requires a vertical wall face (n.z ≈ 0)");
        }
        let center = (corner_a + corner_b) * 0.5;
        let door_bottom_z = corner_a.z.min(corner_b.z);
        let header_z = corner_a.z.max(corner_b.z);
        if header_z - door_bottom_z < 1e-6 {
            bail!("door: degenerate height (corners share a Z)");
        }

        // 1) Front host face F.
        let f_front = self
            .find_door_host(center, n)
            .ok_or_else(|| anyhow::anyhow!("door: no host wall face at corners"))?;
        let mat = self.faces[f_front].material();

        // 2) F's bottom edge (lowest horizontal outer edge) + the Bot face on it.
        let f_edges = self.face_outer_edges(f_front)?;
        let f_bottom = f_edges
            .iter()
            .copied()
            .filter(|&e| {
                let a = self.vertex_pos(self.edges[e].v_small()).unwrap_or(DVec3::ZERO);
                let b = self.vertex_pos(self.edges[e].v_large()).unwrap_or(DVec3::ZERO);
                (a.z - b.z).abs() < 1e-3
            })
            .min_by(|&e1, &e2| {
                let z1 = self.vertex_pos(self.edges[e1].v_small()).map(|p| p.z).unwrap_or(f64::MAX);
                let z2 = self.vertex_pos(self.edges[e2].v_small()).map(|p| p.z).unwrap_or(f64::MAX);
                z1.partial_cmp(&z2).unwrap_or(std::cmp::Ordering::Equal)
            })
            .ok_or_else(|| anyhow::anyhow!("door: F has no horizontal bottom edge"))?;
        let be_a = self.vertex_pos(self.edges[f_bottom].v_small())?;
        let be_b = self.vertex_pos(self.edges[f_bottom].v_large())?;

        // Door detection (auto floor-snap, ADR-262 Q3): the opening is a DOOR if
        // its bottom is in the lower fraction of the wall height — the door
        // bottom is then snapped to the wall bottom edge (BL/BR placed there).
        // A higher bottom (a raised sill) is a WINDOW → reject (caller routes to
        // drill_rect_through_hole). Relative threshold = unit-agnostic + cleanly
        // separates doors (bottom ≈ 0%) from windows (sill usually ≥ 25%).
        let f_top_z = self
            .collect_loop_verts(self.faces[f_front].outer().start)?
            .iter()
            .filter_map(|&v| self.vertex_pos(v).ok())
            .map(|p| p.z)
            .fold(f64::MIN, f64::max);
        let wall_height = (f_top_z - be_a.z).max(1e-6);
        if door_bottom_z - be_a.z > DOOR_FLOOR_FRACTION * wall_height {
            bail!(
                "door: opening bottom is not near the wall floor \
                 (this is a window — use drill_rect_through_hole)"
            );
        }

        // Bot = the other face sharing F's bottom edge.
        let bot = {
            let he = self.edges[f_bottom].any_he();
            let f1 = self.hes[he].face();
            let he2 = self.hes[he].next_rad();
            let f2 = self.hes[he2].face();
            if f1 == f_front {
                f2
            } else {
                f1
            }
        };
        if bot.is_null() || !self.faces.contains(bot) {
            bail!("door: no bottom face adjacent to F (not a solid wall)");
        }

        // 3) Door corner positions on F (BL/BR on the bottom edge, TL/TR at header).
        let d = be_b - be_a;
        let dlen2 = d.length_squared().max(1e-12);
        let xa = (corner_a - be_a).dot(d) / dlen2;
        let xb = (corner_b - be_a).dot(d) / dlen2;
        let (x0, x1) = (xa.min(xb), xa.max(xb));
        let bl_pos = be_a + d * x0;
        let br_pos = be_a + d * x1;
        let up = DVec3::Z * (header_z - door_bottom_z);
        let tl_pos = bl_pos + up;
        let tr_pos = br_pos + up;

        // 4) Opposite wall depth (−n ray) BEFORE any mutation.
        let depth = self
            .carve_ray_nearest_face(center, -n, FaceId::new(u32::MAX), n, center)
            .ok_or_else(|| anyhow::anyhow!("door: no opposite wall along −normal (not a through-solid)"))?;
        if depth <= 1e-6 {
            bail!("door: opposite wall too close ({depth})");
        }

        // 5) FRONT notch.
        let (blf, brf, tlf, trf) =
            self.notch_wall_face_for_door(f_front, bl_pos, br_pos, tl_pos, tr_pos, mat)?;

        // 6) BACK notch (door corners projected −n·depth onto the exit plane).
        let off = n * depth;
        let center_b = center - off;
        let f_back = self
            .find_door_host(center_b, -n)
            .ok_or_else(|| anyhow::anyhow!("door: no back wall face on the exit plane"))?;
        let (blb, brb, tlb, trb) = self.notch_wall_face_for_door(
            f_back,
            bl_pos - off,
            br_pos - off,
            tl_pos - off,
            tr_pos - off,
            mat,
        )?;

        // 7) BOTTOM notch — remove the door strip {blf, brf, brb, blb}.
        self.add_edge(blf, blb)?;
        let bs1 = crate::operations::face_split::split_face_by_chain(self, bot, &[blf, blb], mat)?;
        let bot_right = bs1
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = self
                    .collect_loop_verts(self.faces[nf].outer().start)
                    .unwrap_or_default();
                vs.contains(&brf) && vs.contains(&brb)
            })
            .ok_or_else(|| anyhow::anyhow!("door: Bot right part (BR side) not found"))?;
        self.add_edge(brf, brb)?;
        let bs2 =
            crate::operations::face_split::split_face_by_chain(self, bot_right, &[brf, brb], mat)?;
        let door_strip = bs2
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = self
                    .collect_loop_verts(self.faces[nf].outer().start)
                    .unwrap_or_default();
                vs.len() == 4
                    && vs.contains(&blf)
                    && vs.contains(&brf)
                    && vs.contains(&blb)
                    && vs.contains(&brb)
            })
            .ok_or_else(|| anyhow::anyhow!("door: Bot door strip not found"))?;
        self.remove_face(door_strip);

        // 8) 3-jamb bridge (left / header / right; bottom open = doorway threshold).
        //    Winding [front_bottom, back_bottom, back_top, front_top] — validated
        //    manifold (jamb bottoms twin the Bot strips' inner edges).
        let left = self.add_face(&[blf, blb, tlb, tlf], mat)?;
        let header = self.add_face(&[tlf, tlb, trb, trf], mat)?;
        let right = self.add_face(&[trf, trb, brb, brf], mat)?;

        Ok(DoorOpeningResult {
            front_face: f_front,
            back_face: f_back,
            jamb_faces: vec![left, header, right],
        })
    }
}

/// Even-odd point-in-polygon test for `p` against a planar `poly` (normal `n`),
/// after projecting both to the face's 2D basis. Self-contained MVP test — the
/// carve ray strikes opposite walls near their interior, well off any edge.
fn point_in_face(p: DVec3, poly: &[DVec3], n: DVec3) -> bool {
    // In-plane orthonormal basis (u, v).
    let seed = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u = (seed - n * seed.dot(n)).normalize_or_zero();
    if u.length_squared() < 0.5 {
        return false;
    }
    let v = n.cross(u);
    let to2 = |q: DVec3| DVec2::new(q.dot(u), q.dot(v));
    let poly2: Vec<DVec2> = poly.iter().map(|&q| to2(q)).collect();
    point_in_poly_2d(to2(p), &poly2)
}

/// Standard even-odd ray-crossing point-in-polygon (2D).
fn point_in_poly_2d(p: DVec2, poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let pi = poly[i];
        let pj = poly[j];
        if (pi.y > p.y) != (pj.y > p.y) {
            let dy = pj.y - pi.y;
            if dy.abs() > f64::EPSILON {
                let x_cross = (pj.x - pi.x) * (p.y - pi.y) / dy + pi.x;
                if p.x < x_cross {
                    inside = !inside;
                }
            }
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MaterialId;
    use glam::DVec3;

    // Box centered at origin, 200×200×200 → Z ∈ [-100, 100].
    // create_box(center, width=X, height=Z, depth=Y, mat) → [Bottom, Top, Front, Back, Right, Left].
    fn box200() -> (Mesh, FaceId, FaceId) {
        let mut mesh = Mesh::default();
        let faces = mesh
            .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .expect("box");
        (mesh, faces[0], faces[1]) // (bottom -Z, top +Z)
    }

    /// ADR-273 UX — a straight through-drill whose far wall is NOT a parallel
    /// straight exit (here: a triangular prism, whose walls meet at an angle)
    /// must fail with the CLEAR reason ("경사진 … 곧은 관통을 지원하지 않습니다"),
    /// not the technical "no coplanar face contains the polygon hole centroid …".
    /// A box (parallel opposite wall) still drills through — the regression guard.
    #[test]
    fn adr273_non_parallel_exit_drill_clear_reason() {
        let mat = MaterialId::new(0);
        // Triangular prism: a triangle base pushed up. Its 3 vertical side walls
        // meet at 60°/120° angles — no wall has a parallel opposite.
        let mut mesh = Mesh::default();
        let v0 = mesh.add_vertex(DVec3::new(-150.0, -80.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(150.0, -80.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(0.0, 170.0, 0.0));
        let base = mesh.add_face(&[v0, v1, v2], mat).expect("triangle base"); // +Z
        let _ = mesh.push_pull(base, 200.0, mat).expect("prism");

        // Drill a rect through the FRONT wall (the v0→v1 side, outward normal -Y).
        // The +Y ray into the solid exits near the apex v2 — between the two
        // slanted side walls, so NO coplanar exit face exists.
        let ca = DVec3::new(-40.0, -80.0, 60.0);
        let cb = DVec3::new(40.0, -80.0, 140.0);
        let r = mesh.drill_rect_through_hole(ca, cb, DVec3::new(0.0, -1.0, 0.0));
        match r {
            Err(e) => {
                let m = e.to_string();
                // Must be a CLEAR user-facing reason (tapered/angled exit OR
                // cross-drill), NOT the technical "no coplanar face contains the
                // polygon hole centroid …" that leaked before ADR-273.
                assert!(
                    !m.contains("no coplanar face contains the polygon hole"),
                    "must not surface the technical punch error, got: {m}"
                );
                assert!(
                    m.contains("경사진") || m.contains("관통 축") || m.contains("tapered"),
                    "must give a clear Korean reason, got: {m}"
                );
            }
            Ok(_) => panic!("a non-parallel-exit through-drill should be rejected"),
        }

        // Regression: a box (parallel opposite wall) still drills through cleanly.
        let (mut bx, _bot, _top) = box200();
        let ok = bx.drill_rect_through_hole(
            DVec3::new(-40.0, -100.0, -40.0),
            DVec3::new(40.0, -100.0, 40.0),
            DVec3::new(0.0, -1.0, 0.0),
        );
        assert!(ok.is_ok(), "box straight through-drill must still succeed: {ok:?}");
    }

    /// ADR-262 β-1 DETAILED SIM (먼저 시뮬) — full door notch on a box wall:
    /// FRONT + BACK U-chain split (split_face_by_chain) + Bot notch (2 strips) +
    /// 3-jamb bridge (left/header/right, bottom open). Validates the Q2
    /// split-face + cut construction is a **watertight closed manifold** (0
    /// violations) BEFORE the polished kernel + WASM + tool. Run with
    /// `--nocapture` for the step-by-step topology dump. This is the β-1 sim
    /// gate — the door-vs-window geometric gap (floor-reaching notch) realized.
    #[test]
    fn adr262_sim_door_notch_full_manifold() {
        use crate::operations::face_split::split_face_by_chain;
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        // create_box(w,h,d): empirically w→X, h→Z, d→Y. For a WALL (length X,
        // height Z, thickness Y): create_box(length, height, thickness).
        // Wall X=2000 (length), Z=2500 (height), Y=200 (thickness).
        // Spans X[-1000,1000] Z[-1250,1250] Y[-100,100].
        mesh.create_box(DVec3::ZERO, 2000.0, 2500.0, 200.0, mat)
            .expect("box wall");
        let active0 = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        eprintln!("[SIM] box wall: {} active faces", active0);

        // 1) Front face F (normal ≈ (0,-1,0)).
        let f_front = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .find(|(_, f)| {
                (f.normal().normalize_or_zero() - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-3
            })
            .map(|(id, _)| id)
            .expect("front face (−Y)");
        eprintln!("[SIM] F = {:?} normal={:?}", f_front, mesh.faces[f_front].normal());

        // 2) F's bottom edge: both endpoints at z ≈ -1250.
        let f_edges = mesh.face_outer_edges(f_front).expect("F edges");
        eprintln!("[SIM] F has {} outer edges", f_edges.len());
        for &e in &f_edges {
            let a = mesh.vertex_pos(mesh.edges[e].v_small()).unwrap();
            let b = mesh.vertex_pos(mesh.edges[e].v_large()).unwrap();
            eprintln!("[SIM]   F edge {:?}: {:?} → {:?}", e, a, b);
        }
        // bottom edge = horizontal edge (both endpoints same min-z, |Δz|≈0).
        let bottom_edge = f_edges
            .iter()
            .copied()
            .filter(|&e| {
                let a = mesh.vertex_pos(mesh.edges[e].v_small()).unwrap();
                let b = mesh.vertex_pos(mesh.edges[e].v_large()).unwrap();
                (a.z - b.z).abs() < 1e-3 // horizontal
            })
            .min_by(|&e1, &e2| {
                let z1 = mesh.vertex_pos(mesh.edges[e1].v_small()).unwrap().z;
                let z2 = mesh.vertex_pos(mesh.edges[e2].v_small()).unwrap().z;
                z1.partial_cmp(&z2).unwrap()
            })
            .expect("F bottom edge (lowest horizontal)");
        let be_a = mesh.vertex_pos(mesh.edges[bottom_edge].v_small()).unwrap();
        let be_b = mesh.vertex_pos(mesh.edges[bottom_edge].v_large()).unwrap();
        eprintln!("[SIM] F bottom edge = {:?}: {:?} → {:?}", bottom_edge, be_a, be_b);
        // door bottom z = the bottom edge's z; door x range [-300,300]; header
        // = bottom_z + 2100. y = the bottom edge's y.
        let door_z = be_a.z;
        let door_y = be_a.y;
        let header_z = door_z + 2100.0;
        eprintln!("[SIM] door_z={} door_y={} header_z={}", door_z, door_y, header_z);

        // 3) split bottom edge at x=-300 (BL) and x=+300 (BR).
        let (bl, el, er) = mesh
            .split_edge(bottom_edge, DVec3::new(-300.0, door_y, door_z))
            .expect("split BL");
        let right_edge = [el, er]
            .into_iter()
            .find(|&e| {
                let a = mesh.vertex_pos(mesh.edges[e].v_small()).unwrap().x;
                let b = mesh.vertex_pos(mesh.edges[e].v_large()).unwrap().x;
                a.min(b) <= 300.0 && 300.0 <= a.max(b)
            })
            .expect("right sub-edge spanning x=300");
        let (br, _, _) = mesh
            .split_edge(right_edge, DVec3::new(300.0, door_y, door_z))
            .expect("split BR");
        eprintln!("[SIM] BL={:?} BR={:?}", bl, br);

        // 4) interior verts TL, TR (header) + chain edges.
        let tl = mesh.add_vertex(DVec3::new(-300.0, door_y, header_z));
        let tr = mesh.add_vertex(DVec3::new(300.0, door_y, header_z));
        mesh.add_edge(bl, tl).expect("e BL-TL");
        mesh.add_edge(tl, tr).expect("e TL-TR");
        mesh.add_edge(tr, br).expect("e TR-BR");

        // 5) split F by the U-chain [BL, TL, TR, BR].
        let split = split_face_by_chain(&mut mesh, f_front, &[bl, tl, tr, br], mat)
            .expect("split F by U-chain");
        eprintln!("[SIM] F split → {} new faces", split.new_faces.len());
        for &nf in &split.new_faces {
            let vn = mesh
                .collect_loop_verts(mesh.faces[nf].outer().start)
                .map(|v| v.len())
                .unwrap_or(0);
            eprintln!(
                "[SIM]   face {:?}: {} boundary verts, normal={:?}",
                nf,
                vn,
                mesh.faces[nf].normal()
            );
        }

        // door rect = the 4-vert sub-face containing TL & TR.
        let door = split
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = mesh
                    .collect_loop_verts(mesh.faces[nf].outer().start)
                    .unwrap_or_default();
                vs.len() == 4 && vs.contains(&tl) && vs.contains(&tr)
            })
            .expect("door rect sub-face");
        mesh.remove_face(door);
        let (blf, tlf, trf, brf) = (bl, tl, tr, br);
        eprintln!("[SIM] F notch done (removed door rect {:?})", door);
        eprintln!("[SIM] after F-split: valid={}", mesh.verify_face_invariants().is_valid());

        // ── B (back, +Y) split — symmetric ────────────────────────────────
        let f_back = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .find(|(_, f)| {
                (f.normal().normalize_or_zero() - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-3
            })
            .map(|(id, _)| id)
            .expect("back face (+Y)");
        let b_edges = mesh.face_outer_edges(f_back).expect("B edges");
        let b_bottom = b_edges
            .iter()
            .copied()
            .filter(|&e| {
                let a = mesh.vertex_pos(mesh.edges[e].v_small()).unwrap();
                let b = mesh.vertex_pos(mesh.edges[e].v_large()).unwrap();
                (a.z - b.z).abs() < 1e-3
            })
            .min_by(|&e1, &e2| {
                let z1 = mesh.vertex_pos(mesh.edges[e1].v_small()).unwrap().z;
                let z2 = mesh.vertex_pos(mesh.edges[e2].v_small()).unwrap().z;
                z1.partial_cmp(&z2).unwrap()
            })
            .expect("B bottom edge");
        let back_y = mesh.vertex_pos(mesh.edges[b_bottom].v_small()).unwrap().y;
        let (blb, elb, erb) = mesh
            .split_edge(b_bottom, DVec3::new(-300.0, back_y, door_z))
            .expect("split BL_b");
        let rb = [elb, erb]
            .into_iter()
            .find(|&e| {
                let a = mesh.vertex_pos(mesh.edges[e].v_small()).unwrap().x;
                let b = mesh.vertex_pos(mesh.edges[e].v_large()).unwrap().x;
                a.min(b) <= 300.0 && 300.0 <= a.max(b)
            })
            .expect("B right sub-edge");
        let (brb, _, _) = mesh
            .split_edge(rb, DVec3::new(300.0, back_y, door_z))
            .expect("split BR_b");
        let tlb = mesh.add_vertex(DVec3::new(-300.0, back_y, header_z));
        let trb = mesh.add_vertex(DVec3::new(300.0, back_y, header_z));
        mesh.add_edge(blb, tlb).expect("e");
        mesh.add_edge(tlb, trb).expect("e");
        mesh.add_edge(trb, brb).expect("e");
        let split_b = split_face_by_chain(&mut mesh, f_back, &[blb, tlb, trb, brb], mat)
            .expect("split B by U-chain");
        let door_b = split_b
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = mesh.collect_loop_verts(mesh.faces[nf].outer().start).unwrap_or_default();
                vs.len() == 4 && vs.contains(&tlb) && vs.contains(&trb)
            })
            .expect("door rect B");
        mesh.remove_face(door_b);
        eprintln!("[SIM] B notch done. valid={}", mesh.verify_face_invariants().is_valid());

        // ── Bot (−Z) notch — remove the door strip [blf,brf,brb,blb] ──────
        let f_bot = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .find(|(_, f)| {
                (f.normal().normalize_or_zero() - DVec3::new(0.0, 0.0, -1.0)).length() < 1e-3
            })
            .map(|(id, _)| id)
            .expect("bottom face (−Z)");
        let bot_vn = mesh.collect_loop_verts(mesh.faces[f_bot].outer().start).map(|v| v.len()).unwrap_or(0);
        eprintln!("[SIM] Bot {:?} has {} boundary verts (expect 8: 4 corners + blf,brf,blb,brb)", f_bot, bot_vn);
        // split Bot left of door: chain [blf, blb]; then right: [brf, brb].
        mesh.add_edge(blf, blb).expect("e blf-blb");
        let bot_split1 = split_face_by_chain(&mut mesh, f_bot, &[blf, blb], mat).expect("Bot split L");
        // the sub-face containing brf/brb (the door+right part).
        let bot_right = bot_split1
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = mesh.collect_loop_verts(mesh.faces[nf].outer().start).unwrap_or_default();
                vs.contains(&brf) && vs.contains(&brb)
            })
            .expect("Bot right part");
        mesh.add_edge(brf, brb).expect("e brf-brb");
        let bot_split2 = split_face_by_chain(&mut mesh, bot_right, &[brf, brb], mat).expect("Bot split R");
        // door strip = the sub-face whose verts are exactly {blf,brf,brb,blb}.
        let door_strip = bot_split2
            .new_faces
            .iter()
            .copied()
            .find(|&nf| {
                let vs = mesh.collect_loop_verts(mesh.faces[nf].outer().start).unwrap_or_default();
                vs.len() == 4 && vs.contains(&blf) && vs.contains(&brf) && vs.contains(&blb) && vs.contains(&brb)
            })
            .expect("Bot door strip");
        mesh.remove_face(door_strip);
        eprintln!("[SIM] Bot notch done. valid={}", mesh.verify_face_invariants().is_valid());

        // ── 3-jamb bridge (left / header / right; bottom open) ────────────
        // Try winding [front_bottom, back_bottom, back_top, front_top] etc.;
        // verify_face_invariants reveals if a flip is needed.
        let left = mesh.add_face(&[blf, blb, tlb, tlf], mat).expect("left jamb");
        let header = mesh.add_face(&[tlf, tlb, trb, trf], mat).expect("header");
        let right = mesh.add_face(&[trf, trb, brb, brf], mat).expect("right jamb");
        eprintln!("[SIM] jambs: left={:?} header={:?} right={:?}", left, header, right);

        let report = mesh.verify_face_invariants();
        let active_final = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        eprintln!(
            "[SIM] FINAL: valid={} violations={} active_faces={}",
            report.is_valid(),
            report.violations.len(),
            active_final
        );
        for v in report.violations.iter().take(10) {
            eprintln!("[SIM]   violation: {:?}", v);
        }
        // SIM GATE — the full door notch (F-Π + B-Π + Bot 2 strips + 3 jambs)
        // is a watertight closed manifold. 6 box faces → F/B each split (net +0,
        // door removed) → Bot split into 2 strips (net +1) → +3 jambs = 10.
        assert!(
            report.is_valid(),
            "door notch must be manifold-valid, violations: {:?}",
            report.violations
        );
        assert!(report.violations.is_empty(), "door notch: 0 violations");
        assert_eq!(active_final, 10, "6 box − 1 (Bot door strip) + ... = 10 faces");
        // door dimensions: header at door_z + 2100, door width = 600 (x ∈ [-300,300]).
        assert!((header_z - (door_z + 2100.0)).abs() < 1e-9);
    }

    /// Helper: box WALL (length X=2000, height Z=2500, thickness Y=200). Spans
    /// X[-1000,1000] Z[-1250,1250] Y[-100,100]. Bottom at z=-1250.
    fn box_wall() -> Mesh {
        let mut mesh = Mesh::default();
        mesh.create_box(DVec3::ZERO, 2000.0, 2500.0, 200.0, MaterialId::new(0))
            .expect("box wall");
        mesh
    }

    /// ADR-262 β-1 — generic `cut_wall_door_opening` on a box wall → watertight
    /// closed manifold door notch (10 faces, 3 jambs).
    #[test]
    fn adr262_door_box_wall_manifold() {
        let mut mesh = box_wall();
        // Door on the −Y front face: BL at the wall bottom (z=-1250), TR at the
        // header (z=850). x ∈ [-300, 300].
        let res = mesh
            .cut_wall_door_opening(
                DVec3::new(-300.0, -100.0, -1250.0),
                DVec3::new(300.0, -100.0, 850.0),
                DVec3::new(0.0, -1.0, 0.0),
            )
            .expect("door cut OK");
        assert_eq!(res.jamb_faces.len(), 3, "left + header + right jambs");
        assert_ne!(res.front_face, res.back_face);
        let report = mesh.verify_face_invariants();
        assert!(
            report.is_valid() && report.violations.is_empty(),
            "door notch manifold-valid, violations: {:?}",
            report.violations
        );
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active, 10, "6 box − Bot door strip + 3 jambs = 10");
    }

    /// A door whose bottom does NOT reach the wall bottom edge is a WINDOW →
    /// rejected (the caller routes to `drill_rect_through_hole`).
    #[test]
    fn adr262_door_window_bottom_rejected() {
        let mut mesh = box_wall();
        let r = mesh.cut_wall_door_opening(
            DVec3::new(-300.0, -100.0, -500.0), // bottom z=-500 > wall bottom -1250
            DVec3::new(300.0, -100.0, 850.0),
            DVec3::new(0.0, -1.0, 0.0),
        );
        assert!(r.is_err(), "non-floor-reaching opening = window, must reject");
        assert!(
            r.unwrap_err().to_string().contains("window"),
            "error should point to the window path"
        );
    }

    /// Degenerate normal → reject.
    #[test]
    fn adr262_door_degenerate_normal_rejected() {
        let mut mesh = box_wall();
        let r = mesh.cut_wall_door_opening(
            DVec3::new(-300.0, -100.0, -1250.0),
            DVec3::new(300.0, -100.0, 850.0),
            DVec3::ZERO,
        );
        assert!(r.is_err(), "degenerate normal must reject");
    }

    /// A horizontal (non-vertical) host face is out of MVP scope → reject.
    #[test]
    fn adr262_door_horizontal_face_rejected() {
        let mut mesh = box_wall();
        // Top face normal +Z (n.z = 1) → not a wall.
        let r = mesh.cut_wall_door_opening(
            DVec3::new(-300.0, -50.0, 1250.0),
            DVec3::new(300.0, 50.0, 1250.0),
            DVec3::new(0.0, 0.0, 1.0),
        );
        assert!(r.is_err(), "horizontal face (n.z≈1) must reject (MVP vertical only)");
    }

    /// Outward push (dist > 0 along +normal) is never a carve.
    #[test]
    fn adr194_b1_outward_push_is_none() {
        let (mesh, _bottom, top) = box200();
        assert_eq!(mesh.detect_carve_intent(top, 50.0), CarveIntent::None);
        assert_eq!(mesh.detect_carve_intent(top, 500.0), CarveIntent::None);
    }

    /// Zero / non-finite distance is None (no push).
    #[test]
    fn adr194_b1_zero_and_nonfinite_is_none() {
        let (mesh, _b, top) = box200();
        assert_eq!(mesh.detect_carve_intent(top, 0.0), CarveIntent::None);
        assert_eq!(mesh.detect_carve_intent(top, f64::NAN), CarveIntent::None);
        assert_eq!(mesh.detect_carve_intent(top, f64::INFINITY), CarveIntent::None);
    }

    /// Inward push that stops inside the 200-tall box → Pocket with that depth.
    #[test]
    fn adr194_b1_inward_pocket() {
        let (mesh, _b, top) = box200();
        assert_eq!(
            mesh.detect_carve_intent(top, -100.0),
            CarveIntent::Pocket { depth: 100.0 }
        );
        // depth carries the magnitude exactly.
        match mesh.detect_carve_intent(top, -37.5) {
            CarveIntent::Pocket { depth } => assert!((depth - 37.5).abs() < 1e-9),
            other => panic!("expected Pocket, got {:?}", other),
        }
    }

    /// Inward push that reaches/exceeds the opposite (bottom) wall → Through.
    #[test]
    fn adr194_b1_inward_through() {
        let (mesh, _b, top) = box200();
        assert_eq!(mesh.detect_carve_intent(top, -250.0), CarveIntent::Through);
    }

    /// A push that exactly reaches the far wall (200 in a 200-tall box) is
    /// Through (THROUGH_SLACK boundary).
    #[test]
    fn adr194_b1_exact_to_wall_is_through() {
        let (mesh, _b, top) = box200();
        assert_eq!(mesh.detect_carve_intent(top, -200.0), CarveIntent::Through);
        // Just short of the wall is still a Pocket.
        assert_eq!(
            mesh.detect_carve_intent(top, -199.9),
            CarveIntent::Pocket { depth: 199.9 }
        );
    }

    /// Symmetry — the bottom face pushed inward (ray +Z hits the top) carves too.
    #[test]
    fn adr194_b1_bottom_face_inward_through() {
        let (mesh, bottom, _t) = box200();
        assert_eq!(mesh.detect_carve_intent(bottom, -250.0), CarveIntent::Through);
        assert_eq!(
            mesh.detect_carve_intent(bottom, -80.0),
            CarveIntent::Pocket { depth: 80.0 }
        );
    }

    /// A standalone face (no solid behind it) → None even when pushed inward:
    /// there is no opposite wall to carve, so the op is a normal extrude-add.
    #[test]
    fn adr194_b1_standalone_face_no_material_is_none() {
        let mut mesh = Mesh::default();
        let a = mesh.add_vertex(DVec3::new(-50.0, -50.0, 0.0));
        let b = mesh.add_vertex(DVec3::new(50.0, -50.0, 0.0));
        let c = mesh.add_vertex(DVec3::new(50.0, 50.0, 0.0));
        let d = mesh.add_vertex(DVec3::new(-50.0, 50.0, 0.0));
        let f = mesh.add_face(&[a, b, c, d], MaterialId::new(0)).expect("quad");
        // Inward push (either sign of the quad normal) has no opposite face.
        assert_eq!(mesh.detect_carve_intent(f, -100.0), CarveIntent::None);
        assert_eq!(mesh.detect_carve_intent(f, 100.0), CarveIntent::None);
    }

    /// Inactive / unknown face → None (no panic).
    #[test]
    fn adr194_b1_inactive_or_unknown_face_is_none() {
        let (mesh, _b, _t) = box200();
        assert_eq!(
            mesh.detect_carve_intent(FaceId::new(9999), -100.0),
            CarveIntent::None
        );
    }

    // ── β-2: drill_circular_through_hole ─────────────────────────────────

    /// Drill a circular through-hole in a 200³ box: 2 ring-with-hole caps +
    /// N tube quads + 4 sides, manifold-valid, depth = box height.
    #[test]
    fn adr194_b2_drill_through_box_manifold() {
        let (mut mesh, _bottom, _top) = box200();
        let res = mesh
            .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 30.0, 16)
            .expect("drill through");
        assert_eq!(res.tube_faces.len(), 16, "16 tube quads");
        assert!(
            (res.depth - 200.0).abs() < 1e-6,
            "depth = box height 200, got {}",
            res.depth
        );
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        // 6 box faces (2 caps now ring-with-hole + 4 sides) + 16 tube quads.
        assert_eq!(active, 22, "expected 22 active faces, got {}", active);
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "drilled box must be manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
    }

    /// ADR-269 — cross-drilling: a through-drill whose axis passes through the
    /// void of an EXISTING through-hole is rejected cleanly (the straight-tube MVP
    /// cannot build the intersection). Profile-drill variant originates from a real
    /// wall (like the carve/scene path). A parallel profile that MISSES the void
    /// still succeeds. Guards against the opposite-wall ray latching onto the
    /// interior tube wall (user: side hole aligned with a top hole → cryptic
    /// "extends outside" + non-drill).
    #[test]
    fn adr269_cross_drilling_through_existing_hole_rejected() {
        let (mut mesh, _b, _t) = box200(); // x,y,z ∈ [-100,100] (origin-centered)
        // Existing vertical (Z) through-hole at (x=0,y=0), r=30 (entry on +Z top).
        mesh.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 30.0, 24)
            .expect("first vertical drill");

        // Triangular profile on the -X wall (x=-100) centered at (·,0,0): its +X
        // drill axis crosses the vertical tube void (x²+y²<30² at y=0) →
        // cross-drilling → rejected with a clear message.
        let tri = [
            DVec3::new(-100.0, -15.0, -15.0),
            DVec3::new(-100.0, 15.0, -15.0),
            DVec3::new(-100.0, 0.0, 15.0),
        ];
        let crossed = mesh.drill_polygon_through_hole(&tri, DVec3::new(-1.0, 0.0, 0.0));
        assert!(crossed.is_err(), "cross-drilling must be rejected, got Ok");
        let msg = crossed.unwrap_err().to_string();
        assert!(msg.contains("cross-drilling") || msg.contains("교차"),
            "clear cross-drill message, got: {msg}");
        assert!(mesh.verify_face_invariants().is_valid(), "mesh intact after rejected cross-drill");

        // Control: the same profile offset in Y (y≈60) misses the r=30 vertical
        // void → succeeds (a clean second through-hole).
        let tri2 = [
            DVec3::new(-100.0, 45.0, -15.0),
            DVec3::new(-100.0, 75.0, -15.0),
            DVec3::new(-100.0, 60.0, 15.0),
        ];
        let missed = mesh.drill_polygon_through_hole(&tri2, DVec3::new(-1.0, 0.0, 0.0));
        assert!(missed.is_ok(), "parallel drill that misses the void must succeed: {:?}", missed.err());
    }

    /// ADR-271 β — carve a blind radial pocket into a Cylinder side from a sketched
    /// cap (ADR-263 split). The recess must stay a watertight manifold solid, add
    /// N side walls + 1 floor, and the floor must sit at radius (r − depth).
    #[test]
    fn adr271_carve_curved_pocket_cylinder_blind() {
        use crate::surfaces::{cylinder, AnalyticSurface};
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        mesh.set_cylinder_path_b_default(true);
        let faces = mesh.create_cylinder(DVec3::ZERO, 10.0, 20.0, 24, mat).expect("cylinder");
        let annulus = faces[2];
        let (ax_o, ax_d, rad, refd, vlo, vhi) = match mesh.face_surface(annulus).cloned().expect("surf") {
            AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. } =>
                (axis_origin, axis_dir, radius, ref_dir, v_range.0, v_range.1),
            other => panic!("Cylinder, got {other:?}"),
        };
        let vmid = 0.5 * (vlo + vhi);
        let cp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.0, vmid);
        let rp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.4, vmid);
        let samples = cylinder::circle_on_cylinder(ax_o, ax_d, rad, refd, cp, rp, 0.05).expect("circle");
        let (cap, _host) = mesh.split_cylinder_face_by_circle(annulus, &samples).expect("split");

        let n_wall_expect = mesh.collect_loop_verts(mesh.faces[cap].outer().start).unwrap().len();
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };
        let closed_before = mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid;

        let depth = 3.0;
        let res = mesh.carve_curved_pocket(cap, depth).expect("curved pocket must carve");

        // N walls + 1 floor.
        assert_eq!(res.wall_faces.len(), n_wall_expect, "one wall per cap boundary edge");
        assert!((res.depth - depth).abs() < 1e-9);

        // Manifold (carve_curved_pocket bails if not) + still a closed watertight solid.
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after curved pocket");
        assert!(closed_before, "cylinder was a closed solid before");
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid,
            "recessed cylinder stays a watertight closed solid");

        // Floor sits at the recessed radius (r − depth): every floor vertex is
        // `radius − depth` from the axis.
        let floor_vs = mesh.collect_loop_verts(mesh.faces[res.floor_face].outer().start).unwrap();
        for &v in &floor_vs {
            let p = mesh.vertex_pos(v).unwrap();
            let rel = p - ax_o;
            let r = (rel - ax_d * rel.dot(ax_d)).length();
            assert!((r - (rad - depth)).abs() < 1e-6, "floor vert at radius {r}, want {}", rad - depth);
        }

        // depth ≥ radius is rejected (would reach/cross the axis).
        let mut m2 = Mesh::new();
        m2.set_cylinder_path_b_default(true);
        let f2 = m2.create_cylinder(DVec3::ZERO, 10.0, 20.0, 24, mat).expect("cyl2");
        let (a2o, a2d, r2, rd2, vl2, vh2) = match m2.face_surface(f2[2]).cloned().unwrap() {
            AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. } =>
                (axis_origin, axis_dir, radius, ref_dir, v_range.0, v_range.1),
            _ => unreachable!(),
        };
        let vm2 = 0.5 * (vl2 + vh2);
        let s2 = cylinder::circle_on_cylinder(a2o, a2d, r2, rd2,
            cylinder::evaluate(a2o, a2d, r2, rd2, 0.0, vm2),
            cylinder::evaluate(a2o, a2d, r2, rd2, 0.4, vm2), 0.05).unwrap();
        let (cap2, _) = m2.split_cylinder_face_by_circle(f2[2], &s2).unwrap();
        assert!(m2.carve_curved_pocket(cap2, 10.0).is_err(), "depth reaching the axis must be rejected");
    }

    /// ADR-286 β — raise a curved BOSS from a sketched Cylinder cap (the pocket
    /// mirror). The result must be a watertight manifold solid, add N side walls
    /// + 1 roof, the roof must sit at radius + height, and the roof must face
    /// radially OUTWARD (correct exterior orientation, ADR-268 "topology ≠
    /// orientation" lesson — check the normal, not just closed-solid).
    #[test]
    fn adr286_add_curved_boss_cylinder() {
        use crate::surfaces::{cylinder, AnalyticSurface};
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        mesh.set_cylinder_path_b_default(true);
        let faces = mesh.create_cylinder(DVec3::ZERO, 10.0, 20.0, 24, mat).expect("cylinder");
        let annulus = faces[2];
        let (ax_o, ax_d, rad, refd, vlo, vhi) = match mesh.face_surface(annulus).cloned().expect("surf") {
            AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. } =>
                (axis_origin, axis_dir, radius, ref_dir, v_range.0, v_range.1),
            other => panic!("Cylinder, got {other:?}"),
        };
        let vmid = 0.5 * (vlo + vhi);
        let cp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.0, vmid);
        let rp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.4, vmid);
        let samples = cylinder::circle_on_cylinder(ax_o, ax_d, rad, refd, cp, rp, 0.05).expect("circle");
        let (cap, _host) = mesh.split_cylinder_face_by_circle(annulus, &samples).expect("split");

        let n_wall_expect = mesh.collect_loop_verts(mesh.faces[cap].outer().start).unwrap().len();
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid, "closed before");

        let height = 5.0;
        let res = mesh.add_curved_boss(cap, height).expect("curved boss must raise");

        // N walls + 1 roof.
        assert_eq!(res.wall_faces.len(), n_wall_expect, "one wall per cap boundary edge");
        assert!((res.depth - height).abs() < 1e-9);

        // Manifold (add_curved_boss bails if not) + still a closed watertight solid.
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after curved boss");
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid,
            "bossed cylinder stays a watertight closed solid");

        // Roof sits at the raised radius (r + height): every roof vertex is
        // `radius + height` from the axis.
        let roof_vs = mesh.collect_loop_verts(mesh.faces[res.floor_face].outer().start).unwrap();
        for &v in &roof_vs {
            let p = mesh.vertex_pos(v).unwrap();
            let rel = p - ax_o;
            let r = (rel - ax_d * rel.dot(ax_d)).length();
            assert!((r - (rad + height)).abs() < 1e-6, "roof vert at radius {r}, want {}", rad + height);
        }

        // Orientation (ADR-268 lesson): the roof's geometric normal must point
        // radially OUTWARD (exterior of the boss), not inward. Compute the face
        // normal from its loop and compare with the radial-out direction at the
        // roof centroid.
        let centroid = {
            let mut c = DVec3::ZERO;
            for &v in &roof_vs { c += mesh.vertex_pos(v).unwrap(); }
            c / roof_vs.len() as f64
        };
        let rel_c = centroid - ax_o;
        let radial_out = (rel_c - ax_d * rel_c.dot(ax_d)).normalize();
        let n = mesh.compute_normal(&roof_vs).expect("roof normal");
        assert!(n.dot(radial_out) > 0.5,
            "roof faces radially outward (n·out={}, n={n:?}, out={radial_out:?})", n.dot(radial_out));

        // height must be positive.
        assert!(mesh.add_curved_boss(res.floor_face, -1.0).is_err(), "negative height rejected");
    }

    /// ADR-287 β-1/β-2 — curved POCKET + BOSS on a CONE cap. The offset is along
    /// the cone surface normal; the floor/roof is a PARALLEL cone (same half-angle,
    /// apex shifted ∓depth/sin α). The KEY de-risk: every floor vert must lie on the
    /// single parallel cone (validates the §3 apex-shift derivation).
    #[test]
    fn adr287_curved_pocket_boss_cone() {
        use crate::surfaces::{cone, AnalyticSurface};
        let mat = MaterialId::new(0);
        let (apex_p, ad_p, ha_p, rd_p);
        let make_cap = |mesh: &mut Mesh| -> (FaceId, DVec3, DVec3, f64, DVec3) {
            let faces = mesh.create_cone_kernel_native(DVec3::ZERO, 5.0, 20.0, mat).expect("cone");
            let side = faces[1]; // faces[0]=base (Plane), faces[1]=side (Cone)
            let (apex, ad, ha, rd) = match mesh.face_surface(side).cloned().unwrap() {
                AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, .. } =>
                    (apex, axis_dir, half_angle, ref_dir),
                other => panic!("Cone, got {other:?}"),
            };
            let vmid = 10.0;
            let cp = cone::evaluate(apex, ad, ha, rd, 0.0, vmid);
            let rp = cone::evaluate(apex, ad, ha, rd, 0.4, vmid);
            let samples = cone::circle_on_cone(apex, ad, ha, rd, cp, rp, 0.05).expect("geodesic circle");
            let (cap, _host) = mesh.split_cone_face_by_circle(side, &samples).expect("split");
            (cap, apex, ad, ha, rd)
        };
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };

        // ── POCKET (inward) → parallel cone, apex + ad·(depth/sin α) ──
        let mut mesh = Mesh::new();
        let (cap, apex, ad, ha, rd) = make_cap(&mut mesh);
        (apex_p, ad_p, ha_p, rd_p) = (apex, ad, ha, rd);
        let depth = 1.0;
        let res = mesh.carve_curved_pocket(cap, depth).expect("cone pocket");
        assert!(res.wall_faces.len() >= 3);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after cone pocket");
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid, "cone pocket watertight");
        // floor verts lie EXACTLY on the parallel cone (apex shifted by depth/sin α).
        let apex_floor = apex_p + ad_p.normalize() * (depth / ha_p.sin());
        let floor_vs = mesh.collect_loop_verts(mesh.faces[res.floor_face].outer().start).unwrap();
        for &v in &floor_vs {
            let p = mesh.vertex_pos(v).unwrap();
            let (sp, _, _) = cone::project_to_cone(apex_floor, ad_p, ha_p, rd_p, p)
                .expect("floor vert projects onto parallel cone");
            assert!((sp - p).length() < 1e-6, "floor vert off the parallel cone by {}", (sp - p).length());
        }
        assert!(matches!(mesh.faces[res.floor_face].surface(), Some(AnalyticSurface::Cone { .. })),
            "floor inherits Cone (parallel)");

        // ── BOSS (outward) → parallel cone, apex − ad·(height/sin α) ──
        let mut m2 = Mesh::new();
        let (cap2, ..) = make_cap(&mut m2);
        let height = 2.0;
        let res2 = m2.add_curved_boss(cap2, height).expect("cone boss");
        assert!(m2.verify_face_invariants().is_valid(), "manifold after cone boss");
        assert!(m2.face_set_manifold_info(&active(&m2)).is_closed_solid, "cone boss watertight");
        assert!(matches!(m2.faces[res2.floor_face].surface(), Some(AnalyticSurface::Cone { .. })),
            "roof inherits Cone (parallel)");
    }

    /// ADR-287 β-1/β-3 — curved POCKET + BOSS on a TORUS cap. Offset along the tube-
    /// circle normal; floor/roof = Torus{minor_radius ∓ depth}. Watertight manifold.
    #[test]
    fn adr287_curved_pocket_boss_torus() {
        use crate::surfaces::{torus, AnalyticSurface};
        let mat = MaterialId::new(0);
        let make_cap = |mesh: &mut Mesh| -> FaceId {
            let face = mesh.create_torus_kernel_native(DVec3::ZERO, 10.0, 3.0, mat).expect("torus");
            let (c, ax, rd, rmaj, rmin) = match mesh.face_surface(face).cloned().unwrap() {
                AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } =>
                    (center, axis_dir, ref_dir, major_radius, minor_radius),
                other => panic!("Torus, got {other:?}"),
            };
            let cp = torus::evaluate(c, ax, rd, rmaj, rmin, 0.0, 0.0);
            let rp = torus::evaluate(c, ax, rd, rmaj, rmin, 0.25, 0.0);
            let samples = torus::circle_on_torus(c, ax, rd, rmaj, rmin, cp, rp, 0.05).expect("torus circle");
            let (cap, _host) = mesh.split_torus_face_by_circle(face, &samples).expect("split");
            cap
        };
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };
        // The Path B torus is a single face + self-loop seam (LOCKED #49) — the seam
        // is a boundary-like edge, so `is_closed_solid` is false even at baseline.
        // The pocket/boss must PRESERVE the torus's closed-ness (add no new boundary).
        let base_closed = {
            let mut mb = Mesh::new();
            make_cap(&mut mb);
            mb.face_set_manifold_info(&active(&mb)).is_closed_solid
        };

        // ── POCKET (inward) → Torus{minor − depth} ──
        let mut mesh = Mesh::new();
        let cap = make_cap(&mut mesh);
        let depth = 1.0;
        let res = mesh.carve_curved_pocket(cap, depth).expect("torus pocket");
        assert!(res.wall_faces.len() >= 3);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after torus pocket");
        assert_eq!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid, base_closed,
            "torus pocket preserves closed-ness (no new boundary)");
        assert!(matches!(mesh.faces[res.floor_face].surface(),
            Some(AnalyticSurface::Torus { minor_radius, .. }) if (minor_radius - (3.0 - depth)).abs() < 1e-6),
            "floor inherits Torus at minor − depth");
        // depth reaching the tube center is rejected.
        let mut mr = Mesh::new();
        let capr = make_cap(&mut mr);
        assert!(mr.carve_curved_pocket(capr, 3.0 + 1.0).is_err(), "depth past tube center rejected");

        // ── BOSS (outward) → Torus{minor + height} ──
        let mut m2 = Mesh::new();
        let cap2 = make_cap(&mut m2);
        let height = 2.0;
        let res2 = m2.add_curved_boss(cap2, height).expect("torus boss");
        assert!(m2.verify_face_invariants().is_valid(), "manifold after torus boss");
        assert_eq!(m2.face_set_manifold_info(&active(&m2)).is_closed_solid, base_closed,
            "torus boss preserves closed-ness (no new boundary)");
        assert!(matches!(m2.faces[res2.floor_face].surface(),
            Some(AnalyticSurface::Torus { minor_radius, .. }) if (minor_radius - (3.0 + height)).abs() < 1e-6),
            "roof inherits Torus at minor + height");
    }

    /// ADR-287 §7 de-risk — the Sphere carve arm is CORRECT for an N-vert cap
    /// (polyline split, ADR-284). Isolates the blocker: production
    /// `drawCircleOnSphere` uses the analytic-circle split → a self-loop (1-vert)
    /// cap, which the core bails on gracefully. When a sphere cap is N-vert, the
    /// pocket/boss carve is watertight + inherits Sphere{radius ∓ depth}. (Bridging
    /// the production self-loop cap → N-vert is the remaining ε-sphere step.)
    #[test]
    fn adr287_sphere_carve_correct_for_polyline_cap() {
        use crate::surfaces::{sphere, AnalyticSurface};
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        let c = DVec3::ZERO;
        let r = 5.0;
        let make_polyline_cap = |mesh: &mut Mesh| -> FaceId {
            let faces = mesh.create_sphere_kernel_native(c, r, mat).unwrap();
            let north = faces.iter().copied().find(|&f| {
                matches!(mesh.faces[f].surface(),
                    Some(AnalyticSurface::Sphere { v_range, .. }) if v_range.1 > 0.0)
            }).expect("north");
            let (axis, refd) = match mesh.faces[north].surface() {
                Some(AnalyticSurface::Sphere { axis_dir, ref_dir, .. }) => (*axis_dir, *ref_dir),
                _ => unreachable!(),
            };
            // Latitude ring at v ≈ 1.0 rad (northern hemisphere), N on-sphere points
            // (already exactly on the sphere via `evaluate`, so pass them directly to
            // the N-edge samples split — no resampling needed).
            let n = 16;
            let ring: Vec<DVec3> = (0..n)
                .map(|i| sphere::evaluate(c, r, axis, refd, TAU * i as f64 / n as f64, 1.0))
                .collect();
            let (cap, _ann) = mesh.split_sphere_face_by_polyline(north, &ring).expect("polyline split");
            cap
        };
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };

        // ── POCKET ──
        let mut mesh = Mesh::new();
        let cap = make_polyline_cap(&mut mesh);
        let depth = 1.5;
        let res = mesh.carve_curved_pocket(cap, depth).expect("sphere polyline pocket");
        assert!(res.wall_faces.len() >= 3);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after sphere pocket");
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid, "sphere pocket watertight");
        let floor_vs = mesh.collect_loop_verts(mesh.faces[res.floor_face].outer().start).unwrap();
        for &v in &floor_vs {
            let d = (mesh.vertex_pos(v).unwrap() - c).length();
            assert!((d - (r - depth)).abs() < 1e-6, "floor vert at {d}, want {}", r - depth);
        }
        assert!(matches!(mesh.faces[res.floor_face].surface(),
            Some(AnalyticSurface::Sphere { radius, .. }) if (radius - (r - depth)).abs() < 1e-6),
            "floor inherits Sphere at radius − depth");

        // ── BOSS ──
        let mut m2 = Mesh::new();
        let cap2 = make_polyline_cap(&mut m2);
        let height = 2.0;
        let res2 = m2.add_curved_boss(cap2, height).expect("sphere polyline boss");
        assert!(m2.verify_face_invariants().is_valid(), "manifold after sphere boss");
        assert!(m2.face_set_manifold_info(&active(&m2)).is_closed_solid, "sphere boss watertight");
        assert!(matches!(m2.faces[res2.floor_face].surface(),
            Some(AnalyticSurface::Sphere { radius, .. }) if (radius - (r + height)).abs() < 1e-6),
            "roof inherits Sphere at radius + height");
    }

    /// ADR-271 δ — drill a diametric THROUGH-hole from a sketched Cylinder cap:
    /// the cap + a mirrored exit patch are consumed, N tube walls bridge them, and
    /// the result stays a watertight genus-1 tunnel (closed solid).
    #[test]
    fn adr271_carve_curved_through_cylinder() {
        use crate::surfaces::{cylinder, AnalyticSurface};
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        mesh.set_cylinder_path_b_default(true);
        let faces = mesh.create_cylinder(DVec3::ZERO, 10.0, 20.0, 24, mat).expect("cylinder");
        let annulus = faces[2];
        let (ax_o, ax_d, rad, refd, vlo, vhi) = match mesh.face_surface(annulus).cloned().unwrap() {
            AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. } =>
                (axis_origin, axis_dir, radius, ref_dir, v_range.0, v_range.1),
            _ => unreachable!(),
        };
        let vmid = 0.5 * (vlo + vhi);
        let cp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.0, vmid);
        let rp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.35, vmid);
        let samples = cylinder::circle_on_cylinder(ax_o, ax_d, rad, refd, cp, rp, 0.05).unwrap();
        let (cap, _) = mesh.split_cylinder_face_by_circle(annulus, &samples).unwrap();

        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid, "closed before");
        let n_entry = mesh.collect_loop_verts(mesh.faces[cap].outer().start).unwrap().len();

        let res = mesh.carve_curved_through(cap).expect("curved through must drill");
        assert_eq!(res.tube_faces.len(), n_entry, "one tube wall per entry edge");
        // Diametric bore: from the cap center (chord center, slightly inside the
        // surface) through the axis → just under the full diameter (2·radius).
        assert!(res.depth > 1.8 * rad && res.depth <= 2.0 * rad + 1e-6,
            "bore ≈ diameter (2·{rad}), got {}", res.depth);

        assert!(mesh.verify_face_invariants().is_valid(), "manifold after curved through");
        // A through-tunnel keeps the solid watertight (genus-1, no open boundary).
        assert!(mesh.face_set_manifold_info(&active(&mesh)).is_closed_solid,
            "drilled-through cylinder stays a watertight closed solid (tunnel)");
    }

    /// ADR-287 through-hole ε — CONE diametric bore (mirror of the cylinder
    /// through). The bore reflects the entry ring across the axis-plane → exits the
    /// opposite cone slant at the same height; N tube walls bridge them → a
    /// watertight genus-1 tunnel. (Cone is the clean analogue of the cylinder; the
    /// exit lands on the cone because reflection preserves height ⇒ radius.)
    #[test]
    fn adr287_curved_through_cone() {
        use crate::surfaces::{cone, AnalyticSurface};
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let faces = mesh.create_cone_kernel_native(DVec3::ZERO, 5.0, 20.0, mat).expect("cone");
        let side = faces[1];
        let (apex, ad, ha, rd) = match mesh.face_surface(side).cloned().unwrap() {
            AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, .. } =>
                (apex, axis_dir, half_angle, ref_dir),
            other => panic!("Cone, got {other:?}"),
        };
        let vmid = 10.0;
        let cp = cone::evaluate(apex, ad, ha, rd, 0.0, vmid);
        let rp = cone::evaluate(apex, ad, ha, rd, 0.4, vmid);
        let samples = cone::circle_on_cone(apex, ad, ha, rd, cp, rp, 0.05).expect("circle");
        let (cap, _host) = mesh.split_cone_face_by_circle(side, &samples).expect("split");
        let n_entry = mesh.collect_loop_verts(mesh.faces[cap].outer().start).unwrap().len();
        let active = |m: &Mesh| -> Vec<FaceId> {
            m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
        };
        let res = mesh.carve_curved_through(cap).expect("cone through must drill");
        assert_eq!(res.tube_faces.len(), n_entry, "one tube wall per entry edge");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after cone through");
        // The through-drill yields a watertight genus-1 tunnel: 0 non-manifold, 0
        // open boundary (the entry+exit caps are consumed, N tube walls bridge them).
        let info = mesh.face_set_manifold_info(&active(&mesh));
        assert!(info.is_closed_solid && info.boundary_edge_count == 0,
            "drilled-through cone is a watertight tunnel (closed={}, boundary={})",
            info.is_closed_solid, info.boundary_edge_count);
    }

    /// ADR-287 through-hole ε (de-risk / observe) — TORUS. A diametric bore across
    /// the axis exits the OPPOSITE outer tube (through the central donut hole), NOT
    /// the natural "through the tube" (outer→inner wall via the minor circle). This
    /// test DOCUMENTS what the cylinder-style bore does on a torus: either it stays
    /// manifold (a handle across the hole) or the exit split declines gracefully.
    /// A true torus tube-through (minor-circle bore) is a separate ε-torus-through.
    #[test]
    fn adr287_curved_through_torus_documents_diametric() {
        use crate::surfaces::{torus, AnalyticSurface};
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let face = mesh.create_torus_kernel_native(DVec3::ZERO, 10.0, 3.0, mat).expect("torus");
        let (c, ax, rd, rmaj, rmin) = match mesh.face_surface(face).cloned().unwrap() {
            AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } =>
                (center, axis_dir, ref_dir, major_radius, minor_radius),
            other => panic!("Torus, got {other:?}"),
        };
        let cp = torus::evaluate(c, ax, rd, rmaj, rmin, 0.0, 0.0);
        let rp = torus::evaluate(c, ax, rd, rmaj, rmin, 0.25, 0.0);
        let samples = torus::circle_on_torus(c, ax, rd, rmaj, rmin, cp, rp, 0.05).expect("circle");
        let (cap, _host) = mesh.split_torus_face_by_circle(face, &samples).expect("split");

        // Either the diametric bore succeeds as a manifold, or the exit split
        // declines — both are acceptable (documents that the cylinder-style through
        // is not the natural torus tube-through). No panic, no corruption.
        match mesh.carve_curved_through(cap) {
            Ok(res) => {
                assert!(!res.tube_faces.is_empty());
                assert!(mesh.verify_face_invariants().is_valid(),
                    "if the diametric torus bore succeeds it must be manifold");
            }
            Err(_) => { /* graceful decline — expected for the across-hole bore */ }
        }
    }

    /// Both caps become ring-with-hole (each gains exactly one inner loop).
    #[test]
    fn adr194_b2_drill_caps_have_hole_loop() {
        let (mut mesh, _b, _t) = box200();
        let res = mesh
            .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 25.0, 12)
            .expect("drill");
        for cap in [res.entry_face, res.exit_face] {
            let f = mesh.faces.get(cap).expect("cap face");
            assert_eq!(
                f.inners().len(),
                1,
                "cap {:?} should have exactly 1 hole loop",
                cap
            );
        }
        // 12-segment hole → 12 tube quads.
        assert_eq!(res.tube_faces.len(), 12);
    }

    /// Drilling a standalone face (no opposite wall) → error (not a through-solid).
    #[test]
    fn adr194_b2_drill_no_opposite_wall_errors() {
        let mut mesh = Mesh::default();
        let a = mesh.add_vertex(DVec3::new(-50.0, -50.0, 0.0));
        let b = mesh.add_vertex(DVec3::new(50.0, -50.0, 0.0));
        let c = mesh.add_vertex(DVec3::new(50.0, 50.0, 0.0));
        let d = mesh.add_vertex(DVec3::new(-50.0, 50.0, 0.0));
        mesh.add_face(&[a, b, c, d], MaterialId::new(0)).expect("quad");
        let r = mesh.drill_circular_through_hole(DVec3::ZERO, DVec3::Z, 10.0, 12);
        assert!(r.is_err(), "no opposite wall must error");
    }

    /// Degenerate inputs rejected (radius ≤ 0, segments < 3).
    #[test]
    fn adr194_b2_drill_degenerate_inputs_error() {
        let (mut mesh, _b, _t) = box200();
        let c = DVec3::new(0.0, 0.0, 100.0);
        assert!(mesh.drill_circular_through_hole(c, DVec3::Z, 0.0, 16).is_err());
        assert!(mesh.drill_circular_through_hole(c, DVec3::Z, 30.0, 2).is_err());
        assert!(mesh
            .drill_circular_through_hole(c, DVec3::ZERO, 30.0, 16)
            .is_err());
    }

    /// Two separate (non-overlapping) through-holes in one box stay manifold —
    /// the SECOND drill is performed on caps that already have the first hole,
    /// so `drill_extract_hole_loop` must pick the just-punched loop (the tight
    /// radial band rejects the first, laterally-offset hole). (Lens-4 coverage:
    /// a host face with a pre-existing inner loop.)
    #[test]
    fn adr194_b2_two_drills_face_with_existing_hole_manifold() {
        let (mut mesh, _b, _t) = box200();
        mesh.drill_circular_through_hole(DVec3::new(-50.0, 0.0, 100.0), DVec3::Z, 20.0, 16)
            .expect("drill 1");
        // The 2nd drill is on caps that already carry the first hole.
        let r2 = mesh
            .drill_circular_through_hole(DVec3::new(50.0, 0.0, 100.0), DVec3::Z, 20.0, 16)
            .expect("drill 2 (caps already holed)");
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "two drills must stay manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
        // 6 box faces + 2 × 16 tube quads.
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active, 38, "expected 38 faces, got {}", active);
        // Each re-derived cap now carries TWO hole loops.
        for cap in [r2.entry_face, r2.exit_face] {
            assert_eq!(mesh.faces.get(cap).map(|f| f.inners().len()), Some(2));
        }
    }

    // ── ADR-249: drill_rect_through_hole (P1) ────────────────────────────

    /// Drill a rectangular through-hole in a 200³ box: 2 ring-with-hole caps +
    /// 4 tube quads + 4 sides, manifold-valid, depth = box height.
    #[test]
    fn adr249_drill_rect_through_box_manifold() {
        let (mut mesh, _bottom, _top) = box200();
        let res = mesh
            .drill_rect_through_hole(
                DVec3::new(-30.0, -20.0, 100.0),
                DVec3::new(30.0, 20.0, 100.0),
                DVec3::Z,
            )
            .expect("drill rect through");
        assert_eq!(res.tube_faces.len(), 4, "rect tube = 4 quads");
        assert!(
            (res.depth - 200.0).abs() < 1e-6,
            "depth = box height 200, got {}",
            res.depth
        );
        // 6 box faces (2 caps now ring-with-hole + 4 sides) + 4 tube quads.
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active, 10, "expected 10 active faces, got {}", active);
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "drilled box must be manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
    }

    /// Both caps become ring-with-hole, each with exactly one 4-vertex inner loop.
    #[test]
    fn adr249_drill_rect_caps_have_rect_hole_loop() {
        let (mut mesh, _b, _t) = box200();
        let res = mesh
            .drill_rect_through_hole(
                DVec3::new(-25.0, -15.0, 100.0),
                DVec3::new(25.0, 15.0, 100.0),
                DVec3::Z,
            )
            .expect("drill rect");
        for cap in [res.entry_face, res.exit_face] {
            let f = mesh.faces.get(cap).expect("cap face");
            assert_eq!(f.inners().len(), 1, "cap {:?} should have 1 hole loop", cap);
            let loop_verts = mesh
                .collect_loop_verts(f.inners()[0].start)
                .expect("loop verts");
            assert_eq!(loop_verts.len(), 4, "rect hole loop = 4 verts");
        }
        assert_eq!(res.tube_faces.len(), 4);
    }

    /// Drilling a rect through a standalone face (no opposite wall) → error.
    #[test]
    fn adr249_drill_rect_no_opposite_wall_errors() {
        let mut mesh = Mesh::default();
        let a = mesh.add_vertex(DVec3::new(-50.0, -50.0, 0.0));
        let b = mesh.add_vertex(DVec3::new(50.0, -50.0, 0.0));
        let c = mesh.add_vertex(DVec3::new(50.0, 50.0, 0.0));
        let d = mesh.add_vertex(DVec3::new(-50.0, 50.0, 0.0));
        mesh.add_face(&[a, b, c, d], MaterialId::new(0)).expect("quad");
        let r = mesh.drill_rect_through_hole(
            DVec3::new(-10.0, -10.0, 0.0),
            DVec3::new(10.0, 10.0, 0.0),
            DVec3::Z,
        );
        assert!(r.is_err(), "no opposite wall must error");
    }

    /// Degenerate normal rejected.
    #[test]
    fn adr249_drill_rect_degenerate_normal_errors() {
        let (mut mesh, _b, _t) = box200();
        let r = mesh.drill_rect_through_hole(
            DVec3::new(-30.0, -20.0, 100.0),
            DVec3::new(30.0, 20.0, 100.0),
            DVec3::ZERO,
        );
        assert!(r.is_err(), "degenerate normal must error");
    }

    /// Two separate rect through-holes in one box stay manifold — the SECOND
    /// drill's caps already carry the first hole, so `drill_extract_new_hole_loop`
    /// must pick the just-punched (newest-id) loop.
    #[test]
    fn adr249_drill_rect_two_drills_existing_hole_manifold() {
        let (mut mesh, _b, _t) = box200();
        mesh.drill_rect_through_hole(
            DVec3::new(-60.0, -15.0, 100.0),
            DVec3::new(-30.0, 15.0, 100.0),
            DVec3::Z,
        )
        .expect("drill 1");
        let r2 = mesh
            .drill_rect_through_hole(
                DVec3::new(30.0, -15.0, 100.0),
                DVec3::new(60.0, 15.0, 100.0),
                DVec3::Z,
            )
            .expect("drill 2 (caps already holed)");
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "two rect drills must stay manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
        for cap in [r2.entry_face, r2.exit_face] {
            assert_eq!(mesh.faces.get(cap).map(|f| f.inners().len()), Some(2));
        }
    }

    /// Circular drill is unchanged after the shared-bridge refactor (regression).
    #[test]
    fn adr249_circular_drill_unchanged_after_refactor() {
        let (mut mesh, _b, _t) = box200();
        let res = mesh
            .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 30.0, 16)
            .expect("circular drill still works");
        assert_eq!(res.tube_faces.len(), 16);
        assert!((res.depth - 200.0).abs() < 1e-6);
        assert!(mesh.verify_face_invariants().is_valid());
    }

    // ── ADR-249 (P5): punch_polygon_hole + drill_polygon_through_hole ────

    /// A regular n-gon (CCW around +Z) at `(0,0,z)` of `r`.
    fn ngon(n: usize, r: f64, z: f64) -> Vec<DVec3> {
        (0..n)
            .map(|k| {
                let t = std::f64::consts::TAU * (k as f64) / (n as f64);
                DVec3::new(r * t.cos(), r * t.sin(), z)
            })
            .collect()
    }

    /// Punch an arbitrary pentagon window into the box top → 1 inner loop, 5 verts.
    #[test]
    fn adr249_p5_punch_polygon_pentagon_window() {
        let (mut mesh, _b, top) = box200();
        let pent = ngon(5, 40.0, 100.0);
        let face = mesh
            .punch_polygon_hole(&pent, DVec3::Z)
            .expect("punch pentagon");
        let f = mesh.faces.get(face).expect("face");
        assert_eq!(f.inners().len(), 1, "1 hole loop");
        let lv = mesh.collect_loop_verts(f.inners()[0].start).expect("loop");
        assert_eq!(lv.len(), 5, "pentagon hole = 5 verts");
        assert!(mesh.verify_face_invariants().is_valid());
        let _ = top;
    }

    /// punch_polygon rejects < 3 points and a loop outside the face boundary.
    #[test]
    fn adr249_p5_punch_polygon_rejects_invalid() {
        let (mut mesh, _b, _t) = box200();
        assert!(
            mesh.punch_polygon_hole(&[DVec3::new(0.0, 0.0, 100.0), DVec3::new(10.0, 0.0, 100.0)], DVec3::Z)
                .is_err(),
            "< 3 points must error"
        );
        // A triangle well outside the 200×200 top face (centered at 5000,5000).
        let far = vec![
            DVec3::new(5000.0, 5000.0, 100.0),
            DVec3::new(5050.0, 5000.0, 100.0),
            DVec3::new(5025.0, 5050.0, 100.0),
        ];
        assert!(
            mesh.punch_polygon_hole(&far, DVec3::Z).is_err(),
            "loop outside boundary must error"
        );
    }

    /// Drill a triangular profile through the box → 3 tube quads, manifold.
    #[test]
    fn adr249_p5_drill_polygon_triangle_through_box_manifold() {
        let (mut mesh, _b, _t) = box200();
        let tri = vec![
            DVec3::new(-30.0, -20.0, 100.0),
            DVec3::new(30.0, -20.0, 100.0),
            DVec3::new(0.0, 30.0, 100.0),
        ];
        let res = mesh
            .drill_polygon_through_hole(&tri, DVec3::Z)
            .expect("drill triangle");
        assert_eq!(res.tube_faces.len(), 3, "triangle tube = 3 quads");
        assert!((res.depth - 200.0).abs() < 1e-6);
        // 6 box faces + 3 tube quads.
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active, 9, "expected 9 active faces, got {}", active);
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "drilled triangle must be manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
    }

    /// Drill a pentagon profile through the box → 5 tube quads, both caps holed.
    #[test]
    fn adr249_p5_drill_polygon_pentagon_through_box_manifold() {
        let (mut mesh, _b, _t) = box200();
        let pent = ngon(5, 35.0, 100.0);
        let res = mesh
            .drill_polygon_through_hole(&pent, DVec3::Z)
            .expect("drill pentagon");
        assert_eq!(res.tube_faces.len(), 5, "pentagon tube = 5 quads");
        for cap in [res.entry_face, res.exit_face] {
            let f = mesh.faces.get(cap).expect("cap");
            assert_eq!(f.inners().len(), 1, "cap {:?} has 1 hole loop", cap);
            assert_eq!(
                mesh.collect_loop_verts(f.inners()[0].start).unwrap().len(),
                5,
                "cap hole = 5 verts"
            );
        }
        assert!(mesh.verify_face_invariants().is_valid());
    }

    /// drill_polygon rejects < 3 points and a standalone face (no opposite wall).
    #[test]
    fn adr249_p5_drill_polygon_rejects_invalid() {
        let (mut mesh, _b, _t) = box200();
        assert!(
            mesh.drill_polygon_through_hole(
                &[DVec3::new(0.0, 0.0, 100.0), DVec3::new(10.0, 0.0, 100.0)],
                DVec3::Z
            )
            .is_err(),
            "< 3 points must error"
        );
        let mut sheet = Mesh::default();
        let a = sheet.add_vertex(DVec3::new(-50.0, -50.0, 0.0));
        let b = sheet.add_vertex(DVec3::new(50.0, -50.0, 0.0));
        let c = sheet.add_vertex(DVec3::new(50.0, 50.0, 0.0));
        let d = sheet.add_vertex(DVec3::new(-50.0, 50.0, 0.0));
        sheet.add_face(&[a, b, c, d], MaterialId::new(0)).expect("quad");
        let tri = vec![
            DVec3::new(-10.0, -10.0, 0.0),
            DVec3::new(10.0, -10.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
        ];
        assert!(
            sheet.drill_polygon_through_hole(&tri, DVec3::Z).is_err(),
            "no opposite wall must error"
        );
    }

    // ── ADR-251 (P6): already-working coverage lock-in ──────────────────
    // Phase 2 closure — the feared P6 scope is mostly already covered. These
    // regressions seal the working behavior; only the non-anti-parallel exit
    // (angled/stepped through-hole) remains a niche multi-week future ADR.

    /// P6 non-convex: a non-convex (L-footprint) prism drills fine where the
    /// entry/exit walls are anti-parallel — the drill guard is about the LOCAL
    /// tunnel walls, NOT global convexity. (Lock-in: ADR-251 simulation finding.)
    #[test]
    fn adr251_p6_nonconvex_anti_parallel_drill_works() {
        let mut mesh = Mesh::default();
        let (zb, zt) = (-50.0, 50.0);
        // Non-convex L hexagon footprint (CCW from +Z).
        let fp = [
            (-50.0, -50.0), (50.0, -50.0), (50.0, 0.0),
            (0.0, 0.0), (0.0, 50.0), (-50.0, 50.0),
        ];
        let bot: Vec<_> = fp.iter().map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, zb))).collect();
        let top: Vec<_> = fp.iter().map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, zt))).collect();
        let mut bcap = bot.clone();
        bcap.reverse();
        mesh.add_face(&bcap, MaterialId::new(0)).expect("bottom cap");
        mesh.add_face(&top, MaterialId::new(0)).expect("top cap");
        let n = fp.len();
        for i in 0..n {
            let j = (i + 1) % n;
            mesh.add_face(&[bot[i], bot[j], top[j], top[i]], MaterialId::new(0))
                .expect("side");
        }
        assert!(mesh.verify_face_invariants().is_valid(), "L-prism must be manifold");
        // Drill -X through the foot (y=-25): enters +X wall (x=50), exits -X wall
        // (x=-50) — anti-parallel, so the LOCAL tunnel is straight-through.
        let res = mesh
            .drill_circular_through_hole(DVec3::new(50.0, -25.0, 0.0), DVec3::X, 10.0, 12)
            .expect("non-convex anti-parallel drill works");
        assert_eq!(res.tube_faces.len(), 12);
        assert!((res.depth - 100.0).abs() < 1e-6, "foot width 100, got {}", res.depth);
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "drilled non-convex prism must stay manifold"
        );
    }

    // ── ADR-252 (Pocket carve): "draw rect on a face → push in → pocket" ──

    /// A coplanar rect profile sheet on the front (-Y) wall of a 200³ box (the
    /// drawn-on-face Shape), wound to face -Y. Returns the sheet face id.
    fn front_profile_sheet(mesh: &mut Mesh) -> FaceId {
        // Front wall is the -Y face at y=-100; rect contained inside it.
        let a = mesh.add_vertex(DVec3::new(-40.0, -100.0, -30.0));
        let b = mesh.add_vertex(DVec3::new(40.0, -100.0, -30.0));
        let c = mesh.add_vertex(DVec3::new(40.0, -100.0, 30.0));
        let d = mesh.add_vertex(DVec3::new(-40.0, -100.0, 30.0));
        // A→B→C→D winds to normal -Y (matches the wall outward normal).
        mesh.add_face(&[a, b, c, d], MaterialId::new(0)).expect("profile sheet")
    }

    /// Carve a blind rect pocket from a profile sheet drawn on the wall → ring
    /// opening + recessed floor + 4 side walls, watertight manifold.
    #[test]
    fn adr252_pocket_rect_from_source_sheet_manifold() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let sheet = front_profile_sheet(&mut mesh);
        let res = mesh
            .carve_pocket_from_source_face(sheet, 60.0)
            .expect("rect pocket");
        assert_eq!(res.wall_faces.len(), 4, "rect pocket = 4 side walls");
        assert!((res.depth - 60.0).abs() < 1e-9);
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "pocketed box must be manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
        // The whole solid stays watertight (the pocket is blind).
        let all: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        assert!(
            mesh.face_set_manifold_info(&all).is_closed_solid,
            "blind pocket keeps the solid watertight"
        );
    }

    /// A polygon (triangle) profile sheet → triangular pocket, manifold.
    #[test]
    fn adr252_pocket_triangle_from_source_sheet_manifold() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        // Triangle on the front (-Y) wall, wound to face -Y.
        let a = mesh.add_vertex(DVec3::new(-30.0, -100.0, -20.0));
        let b = mesh.add_vertex(DVec3::new(30.0, -100.0, -20.0));
        let c = mesh.add_vertex(DVec3::new(0.0, -100.0, 30.0));
        let sheet = mesh.add_face(&[a, b, c], MaterialId::new(0)).expect("tri sheet");
        let res = mesh
            .carve_pocket_from_source_face(sheet, 50.0)
            .expect("triangle pocket");
        assert_eq!(res.wall_faces.len(), 3, "triangle pocket = 3 side walls");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
    }

    /// ADR-267 follow-up — `face_outline_points` tessellates a CLOSED-CURVE
    /// (circle) face into a polygon outline (the fix that lets a drawn circle
    /// be cut). Regresses the container/carve gap ("draw circle on face → cut"
    /// was a no-op because the 1-vert self-loop failed the ≥3-vert check).
    #[test]
    fn adr267_face_outline_points_tessellates_closed_curve() {
        let mut mesh = Mesh::new();
        let center = DVec3::ZERO;
        let anchor = mesh.add_vertex(center + DVec3::X * 50.0); // rim, angle 0
        let disk = mesh
            .add_face_closed_curve(
                anchor,
                crate::curves::AnalyticCurve::Circle {
                    center,
                    radius: 50.0,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
                MaterialId::new(0),
            )
            .expect("circle closed-curve face");
        let outline = mesh
            .face_outline_points(disk)
            .expect("closed-curve face must yield a tessellated outline");
        assert!(outline.len() >= 8, "circle tessellates to ≥8 pts, got {}", outline.len());
        for p in &outline {
            assert!((p.length() - 50.0).abs() < 1.0, "point on circle r=50");
            assert!(p.z.abs() < 1e-6, "point in z=0 plane");
        }
    }

    /// ADR-267 follow-up — `bridge_through_loops` (shared by ALL drills) must wind
    /// every tube wall so its normal faces INTO the void (toward the hole axis),
    /// consistently. Regresses the shared winding bug where all walls faced into
    /// the MATERIAL (backside showed from inside the hole → "깨진/캡" 렌더).
    #[test]
    fn adr267_drill_tube_walls_face_into_void() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let sheet = front_profile_sheet(&mut mesh); // -Y wall rect, centroid (0,-100,0)
        let res = mesh
            .carve_through_from_source_face(sheet)
            .expect("drill through");
        assert!(res.tube_faces.len() >= 4, "tube has ≥4 walls");
        for &wf in &res.tube_faces {
            let nrm = mesh.faces[wf].normal().normalize_or_zero();
            let vs = mesh
                .collect_loop_verts(mesh.faces[wf].outer().start)
                .unwrap();
            let c: DVec3 = vs.iter().map(|&v| mesh.vertex_pos(v).unwrap()).sum::<DVec3>()
                / vs.len() as f64;
            // Radial in the axis(Y)-perpendicular plane, from the hole axis (0,c.y,0).
            let radial = DVec3::new(c.x, 0.0, c.z);
            if radial.length() < 1e-6 {
                continue;
            }
            let dot = nrm.dot(radial.normalize());
            assert!(
                dot < 0.0,
                "tube wall {:?} must face INTO the void (toward axis), got dot={:.3}",
                wf,
                dot
            );
        }
    }

    /// ADR-268 — carve_pocket walls + floor must face INTO the void (toward the
    /// pocket axis / -inward), not the material. carve_pocket builds walls inline
    /// (separate from bridge_through_loops), so it needed its own winding fix.
    #[test]
    fn adr268_pocket_walls_and_floor_face_into_void() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let sheet = front_profile_sheet(&mut mesh); // -Y wall, centroid (0,-100,0)
        let res = mesh
            .carve_pocket_from_source_face(sheet, 50.0)
            .expect("blind pocket");
        let axis = DVec3::new(0.0, -100.0, 0.0);
        let inward = DVec3::new(0.0, 1.0, 0.0); // -Y wall → push +Y into the solid
        for &wf in &res.wall_faces {
            let nrm = mesh.faces[wf].normal().normalize_or_zero();
            let verts = mesh.collect_loop_verts(mesh.faces[wf].outer().start).unwrap();
            let c: DVec3 = verts.iter().map(|&v| mesh.vertex_pos(v).unwrap()).sum::<DVec3>()
                / verts.len() as f64;
            let r = c - axis;
            let radial = (r - inward * r.dot(inward)).normalize_or_zero();
            assert!(
                nrm.dot(radial) < 0.0,
                "pocket wall {:?} must face INTO the void (toward axis), got {:.3}",
                wf,
                nrm.dot(radial)
            );
        }
        let fnrm = mesh.faces[res.floor_face].normal().normalize_or_zero();
        assert!(
            fnrm.dot(inward) < 0.0,
            "pocket floor must face the opening (-inward), got {:.3}",
            fnrm.dot(inward)
        );
    }

    /// ADR-268 — a FREEFORM closed curve (Bezier) source carves a pocket too,
    /// guarding the non-Circle branch of `curve_closed_outline` / `face_outline_
    /// points` (polygon/circle/freeform all share the carve path).
    #[test]
    fn adr268_carve_pocket_from_closed_bezier_source() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        // Closed cubic Bezier on the front (-Y) wall (y=-100); cp[0]==cp[last].
        let cp = vec![
            DVec3::new(40.0, -100.0, 0.0),
            DVec3::new(0.0, -100.0, 40.0),
            DVec3::new(-40.0, -100.0, 0.0),
            DVec3::new(0.0, -100.0, -40.0),
            DVec3::new(40.0, -100.0, 0.0),
        ];
        let anchor = mesh.add_vertex(cp[0]);
        let disk = mesh
            .add_face_closed_curve(
                anchor,
                crate::curves::AnalyticCurve::Bezier { control_pts: cp },
                MaterialId::new(0),
            )
            .expect("closed-bezier face on wall");
        assert!(
            mesh.face_has_larger_coplanar_container(disk),
            "freeform closed-curve source must be detected as contained"
        );
        let res = mesh
            .carve_pocket_from_source_face(disk, 40.0)
            .expect("freeform pocket must carve");
        assert!(res.wall_faces.len() >= 8, "freeform pocket faceted walls");
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "freeform pocket must be manifold"
        );
    }

    /// ADR-267 follow-up — `wall_thickness_from_source_face` works for a CLOSED-
    /// CURVE (circle) source, so the scene's through-vs-blind auto-routing fires
    /// for a drawn circle. Without this a deep circle push read as blind → carve
    /// bailed → the tool fell back to a capped extrude (no real through-cut).
    #[test]
    fn adr267_wall_thickness_works_for_circle_source() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let center = DVec3::new(0.0, -100.0, 0.0);
        let basis_u = DVec3::X;
        let anchor = mesh.add_vertex(center + basis_u * 40.0);
        let disk = mesh
            .add_face_closed_curve(
                anchor,
                crate::curves::AnalyticCurve::Circle {
                    center,
                    radius: 40.0,
                    normal: DVec3::new(0.0, -1.0, 0.0),
                    basis_u,
                },
                MaterialId::new(0),
            )
            .expect("circle disk on wall");
        let t = mesh
            .wall_thickness_from_source_face(disk)
            .expect("closed-curve source must report a wall thickness");
        assert!(
            (t - 200.0).abs() < 1.0,
            "front(-Y)→back(+Y) thickness ≈ 200, got {t}"
        );
    }

    /// ADR-267 follow-up — a drawn CIRCLE (closed-curve face) carves a THROUGH
    /// hole (open tube, NO caps top or bottom), same as a rect drill. Regresses
    /// "draw circle → Extrude/Cut all the way through".
    #[test]
    fn adr267_through_circle_from_closed_curve_source() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        // Circle on the front (-Y) wall at y=-100, r=40 (outward normal -Y).
        let center = DVec3::new(0.0, -100.0, 0.0);
        let basis_u = DVec3::X;
        let anchor = mesh.add_vertex(center + basis_u * 40.0);
        let disk = mesh
            .add_face_closed_curve(
                anchor,
                crate::curves::AnalyticCurve::Circle {
                    center,
                    radius: 40.0,
                    normal: DVec3::new(0.0, -1.0, 0.0),
                    basis_u,
                },
                MaterialId::new(0),
            )
            .expect("circle disk on wall");
        let res = mesh
            .carve_through_from_source_face(disk)
            .expect("circle through-hole must drill");
        assert!(
            res.tube_faces.len() >= 8,
            "circular through = many faceted tube walls, got {}",
            res.tube_faces.len()
        );
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "through tube must be manifold"
        );
        // Open tube: no cap covers the circle center on either wall plane.
        // (entry -Y at y=-100, exit +Y at y=+100). A cap face there would be a
        // horizontal disk covering (0,*,0) — assert none exist by checking the
        // solid stays a closed 2-manifold WITH the tube (watertight tunnel).
        let all: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        assert!(
            mesh.face_set_manifold_info(&all).is_closed_solid,
            "through-drilled box (tunnel) stays a watertight solid"
        );
    }

    /// ADR-267 follow-up — a drawn CIRCLE (closed-curve face) coplanar on a wall
    /// carves into a cylindrical blind pocket, same as a rect. This is the
    /// end-to-end regression for "draw circle on face → Extrude/Cut inward".
    #[test]
    fn adr267_pocket_circle_from_closed_curve_source() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        // Circle on the front (-Y) wall at y=-100, r=40 (outward normal -Y).
        let center = DVec3::new(0.0, -100.0, 0.0);
        let basis_u = DVec3::X;
        let anchor = mesh.add_vertex(center + basis_u * 40.0);
        let disk = mesh
            .add_face_closed_curve(
                anchor,
                crate::curves::AnalyticCurve::Circle {
                    center,
                    radius: 40.0,
                    normal: DVec3::new(0.0, -1.0, 0.0),
                    basis_u,
                },
                MaterialId::new(0),
            )
            .expect("circle disk on wall");

        // The gap regression: the closed-curve disk IS now recognized as
        // contained in the larger wall (was false → the cut was skipped).
        assert!(
            mesh.face_has_larger_coplanar_container(disk),
            "closed-curve disk must be detected as contained in the larger wall"
        );

        let res = mesh
            .carve_pocket_from_source_face(disk, 50.0)
            .expect("circle pocket must carve");
        assert!(
            res.wall_faces.len() >= 8,
            "circular pocket = many faceted side walls, got {}",
            res.wall_faces.len()
        );
        assert!((res.depth - 50.0).abs() < 1e-9);
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "circular pocket must be manifold"
        );
        let all: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        assert!(
            mesh.face_set_manifold_info(&all).is_closed_solid,
            "blind circular pocket keeps the solid watertight"
        );
    }

    /// Depth reaching the opposite wall is rejected (→ use a through-hole).
    #[test]
    fn adr252_pocket_depth_through_wall_errors() {
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let sheet = front_profile_sheet(&mut mesh);
        // Box is 200 deep (Y∈[-100,100]); a 250 pocket would breach the back.
        let r = mesh.carve_pocket_from_source_face(sheet, 250.0);
        assert!(r.is_err(), "pocket deeper than the wall must error");
    }

    /// The pocket-candidate detector: a sheet drawn on a wall has a larger
    /// coplanar container (the wall); a plain box wall does not.
    #[test]
    fn adr252_larger_coplanar_container_detects_sheet_vs_wall() {
        let mut mesh = Mesh::new();
        // create_box order: [Bottom -Z, Top +Z, Front -Y, Back +Y, Right +X, Left -X].
        let bf = mesh
            .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .unwrap();
        let front_wall = bf[2]; // -Y front
        let sheet = front_profile_sheet(&mut mesh);
        assert!(
            mesh.face_has_larger_coplanar_container(sheet),
            "a sheet drawn on the wall has a larger coplanar container (→ pocket candidate)"
        );
        assert!(
            !mesh.face_has_larger_coplanar_container(front_wall),
            "a plain box wall has no larger coplanar container (→ not a pocket)"
        );
    }

    /// ADR-252 Amendment 2 — a profile sheet on a THIN wall, drilled through →
    /// entry + exit rings + tube (window), watertight manifold.
    #[test]
    fn adr252_through_from_source_sheet_manifold() {
        let mut mesh = Mesh::new();
        // THIN wall: 200(X) × 200(Z) × 40(Y). front -Y at y=-20, back +Y at y=20.
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 40.0, MaterialId::new(0))
            .unwrap();
        let a = mesh.add_vertex(DVec3::new(-40.0, -20.0, -30.0));
        let b = mesh.add_vertex(DVec3::new(40.0, -20.0, -30.0));
        let c = mesh.add_vertex(DVec3::new(40.0, -20.0, 30.0));
        let d = mesh.add_vertex(DVec3::new(-40.0, -20.0, 30.0));
        let sheet = mesh.add_face(&[a, b, c, d], MaterialId::new(0)).expect("sheet");
        // thickness query (used by the Scene dispatch) = 40 (front→back).
        let t = mesh.wall_thickness_from_source_face(sheet).expect("thickness");
        assert!((t - 40.0).abs() < 1e-6, "wall thickness 40, got {t}");
        let res = mesh
            .carve_through_from_source_face(sheet)
            .expect("through-hole");
        assert_eq!(res.tube_faces.len(), 4, "rect through = 4 tube walls");
        assert!((res.depth - 40.0).abs() < 1e-6, "tube depth = wall thickness 40");
        let inv = mesh.verify_face_invariants();
        assert!(
            inv.is_valid(),
            "drilled-through wall must be manifold: {:?}",
            inv.violations.iter().take(5).collect::<Vec<_>>()
        );
        // A through-hole keeps the solid watertight (genus-1 tunnel, no boundary).
        let all: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        assert!(
            mesh.face_set_manifold_info(&all).is_closed_solid,
            "through-hole solid stays watertight"
        );
    }

    /// P6 multi-solid: two SEPARATE stacked boxes (gap) → drill each at the same
    /// XY → two aligned through-holes, both manifold. Multi-solid = a loop over
    /// solids (each the existing single-solid drill). (Lock-in: ADR-251 finding.)
    #[test]
    fn adr251_p6_multisolid_sequential_drill_works() {
        let mut mesh = Mesh::default();
        mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::new(0))
            .expect("box1"); // Z∈[-100,100]
        mesh.create_box(DVec3::new(0.0, 0.0, 250.0), 200.0, 200.0, 200.0, MaterialId::new(0))
            .expect("box2"); // Z∈[150,350]
        let r1 = mesh
            .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 30.0, 16)
            .expect("drill box1");
        let r2 = mesh
            .drill_circular_through_hole(DVec3::new(0.0, 0.0, 350.0), DVec3::Z, 30.0, 16)
            .expect("drill box2");
        assert_eq!(r1.tube_faces.len(), 16);
        assert_eq!(r2.tube_faces.len(), 16);
        assert!((r1.depth - 200.0).abs() < 1e-6, "box1 depth 200");
        assert!((r2.depth - 200.0).abs() < 1e-6, "box2 depth 200 (own walls, not through gap)");
        // 2 boxes (6 each, 2 caps holed) + 2×16 tube quads.
        let active = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(active, 44, "expected 44 active faces, got {}", active);
        assert!(
            mesh.verify_face_invariants().is_valid(),
            "both drilled boxes must stay manifold"
        );
    }
}
