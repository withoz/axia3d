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
const COPLANAR_DOT: f64 = 0.999;
/// LOCKED #5 spatial-hash tolerance (mm) for "same plane offset".
const COPLANAR_OFFSET: f64 = 1.5e-3;
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

        // 2) Punch the entry rect + grab its (newest) hole loop.
        let entry_face = self.punch_rect_hole(corner_a, corner_b, n)?;
        let e_loop = self
            .drill_extract_new_hole_loop(entry_face)
            .ok_or_else(|| anyhow::anyhow!("drill rect: entry hole loop not found"))?;

        // 3) Punch the exit rect on the opposite plane (projected corners).
        let exit_a = corner_a - n * depth;
        let exit_b = corner_b - n * depth;
        let exit_face = self.punch_rect_hole(exit_a, exit_b, n)?;
        let b_loop = self
            .drill_extract_new_hole_loop(exit_face)
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

        // 2) Punch the entry profile + grab its (newest) hole loop.
        let entry_face = self.punch_polygon_hole(loop_pts, n)?;
        let e_loop = self
            .drill_extract_new_hole_loop(entry_face)
            .ok_or_else(|| anyhow::anyhow!("drill polygon: entry hole loop not found"))?;

        // 3) Punch the exit profile on the opposite plane (projected loop).
        let exit_pts: Vec<DVec3> = loop_pts.iter().map(|&p| p - n * depth).collect();
        let exit_face = self.punch_polygon_hole(&exit_pts, n)?;
        let b_loop = self
            .drill_extract_new_hole_loop(exit_face)
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

        // 1) Read the profile outline + its (outward) normal + boundary edges.
        let sf = self
            .faces
            .get(source_face)
            .filter(|f| f.is_active())
            .ok_or_else(|| anyhow::anyhow!("pocket: source face inactive/missing"))?;
        let outline_verts = self.collect_loop_verts(sf.outer().start)?;
        if outline_verts.len() < 3 {
            bail!("pocket: source face has a degenerate loop");
        }
        let outline: Vec<DVec3> = outline_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<_>>()?;
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
        let ring = self.punch_polygon_hole(&outline, n_s)?;
        let opening = self
            .drill_extract_new_hole_loop(ring)
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

        // 6) Side walls — bridge opening → floor (ADR-249 reverse + align + quad).
        let seed = if inward.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (seed - inward * seed.dot(inward)).normalize_or_zero();
        let vv = inward.cross(u);
        let proj = |p: DVec3| (p.dot(u), p.dot(vv));
        let mut b_rev = floor.clone();
        b_rev.reverse();
        let e0 = proj(self.vertex_pos(opening[0])?);
        let mut k = 0usize;
        let mut best = f64::INFINITY;
        for (j, &bv) in b_rev.iter().enumerate() {
            let bp = proj(self.vertex_pos(bv)?);
            let d = (bp.0 - e0.0).powi(2) + (bp.1 - e0.1).powi(2);
            if d < best {
                best = d;
                k = j;
            }
        }
        let cnt = opening.len();
        let material = self.faces[ring].material();
        let mut wall_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = opening[i];
            let a2 = opening[(i + 1) % cnt];
            let b = b_rev[(k + i) % cnt];
            let b2 = b_rev[(k + i + 1) % cnt];
            wall_faces.push(self.add_face(&[a2, a, b, b2], material)?);
        }

        // 7) Floor cap (closes the bottom; faces the opening).
        let floor_face = self.add_face(&b_rev, material)?;

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
        let sf = self
            .faces
            .get(source_face)
            .filter(|f| f.is_active())
            .ok_or_else(|| anyhow::anyhow!("through: source face inactive/missing"))?;
        let outline_verts = self.collect_loop_verts(sf.outer().start)?;
        if outline_verts.len() < 3 {
            bail!("through: source face has a degenerate loop");
        }
        let outline: Vec<DVec3> = outline_verts
            .iter()
            .map(|&v| self.vertex_pos(v))
            .collect::<Result<_>>()?;
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
            bail!(
                "drill: exit wall is not anti-parallel to the drill axis \
                 (non-convex / internal feature — MVP supports straight-through convex solids)"
            );
        }
        let cnt = e_loop.len();
        if b_loop.len() != cnt {
            bail!(
                "drill: entry/exit hole loop size mismatch ({} vs {})",
                cnt,
                b_loop.len()
            );
        }

        // Reverse the exit loop + rotationally align b_rev[k] with e_loop[0] by
        // nearest position in the axis-perpendicular plane.
        let mut b_rev = b_loop.clone();
        b_rev.reverse();
        let seed = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (seed - n * seed.dot(n)).normalize_or_zero();
        let v = n.cross(u);
        let proj = |p: DVec3| (p.dot(u), p.dot(v));
        let e0 = proj(self.vertex_pos(e_loop[0])?);
        let mut k = 0usize;
        let mut best = f64::INFINITY;
        for (j, &bv) in b_rev.iter().enumerate() {
            let bp = proj(self.vertex_pos(bv)?);
            let d = (bp.0 - e0.0).powi(2) + (bp.1 - e0.1).powi(2);
            if d < best {
                best = d;
                k = j;
            }
        }

        // Bridge: one quad per segment.
        let material = self.faces[entry_face].material();
        let mut tube_faces = Vec::with_capacity(cnt);
        for i in 0..cnt {
            let a = e_loop[i];
            let a2 = e_loop[(i + 1) % cnt];
            let b = b_rev[(k + i) % cnt];
            let b2 = b_rev[(k + i + 1) % cnt];
            let quad = [a2, a, b, b2];
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
        let verts = match self.collect_loop_verts(f.outer().start) {
            Ok(v) if v.len() >= 3 => v,
            _ => return None,
        };
        let pts: Vec<DVec3> = verts.iter().filter_map(|&v| self.vertex_pos(v).ok()).collect();
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
        let verts = self.collect_loop_verts(self.faces.get(source_face)?.outer().start).ok()?;
        let pts: Vec<DVec3> = verts.iter().filter_map(|&v| self.vertex_pos(v).ok()).collect();
        if pts.len() < 3 {
            return None;
        }
        let centroid = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        // Inward = into the solid (opposite the host wall's outward normal). The
        // source sheet (coplanar with the host) is skipped by the coplanar guard.
        self.carve_ray_nearest_face(centroid, -n_host, host, n_host, centroid)
    }

    /// ADR-249 — extract the **just-punched** inner loop of `face` — the inner
    /// loop whose vertices are the NEWEST (largest min VertId). `punch_*` creates
    /// the new hole's verts via `add_vertex` (monotonic ids) so the newest loop
    /// is unambiguously the one just added, even when the face already had holes
    /// (the rect analog of the circular drill's radial-band match).
    fn drill_extract_new_hole_loop(&self, face: FaceId) -> Option<Vec<VertId>> {
        let f = self.faces.get(face)?;
        let mut best: Option<(u32, Vec<VertId>)> = None;
        for inner in f.inners() {
            if inner.start.is_null() {
                continue;
            }
            let verts = match self.collect_loop_verts(inner.start) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            let min_vid = verts.iter().map(|v| v.raw()).min().unwrap_or(0);
            if best.as_ref().map(|(m, _)| min_vid > *m).unwrap_or(true) {
                best = Some((min_vid, verts));
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
