//! Fillet (edge round) — replace a sharp edge between two faces with a
//! smoothly curved arc surface of given radius.
//!
//! Classical CAD operation. For the typical cube-style edge (shared by
//! exactly two planar faces, each endpoint owned by ≤ 3 faces) this
//! implementation:
//!
//! 1. For each endpoint v, walks the two adjacent faces (F1, F2) to find
//!    the "other" edge at v — the one that isn't the fillet edge. Call
//!    its direction `dir_f1_v` and `dir_f2_v`. Offset v by `radius` along
//!    each to get two "rolled back" points p1_v and p2_v.
//! 2. Computes the arc center at v as `v + radius / sin(θ/2) · bisector`
//!    where bisector = (n1+n2).normalize() and θ is the dihedral angle.
//!    The arc lies in the plane perpendicular to the edge, sweeping from
//!    p1_v to p2_v around the center.
//! 3. Samples the arc at `segments + 1` points per endpoint.
//! 4. Rebuilds geometry:
//!      F1 outer loop:  (..., u_F1_prev_a, p1_a, p1_b, u_F1_next_b, ...)
//!      F2 outer loop:  (..., u_F2_prev_b, p2_b, p2_a, u_F2_next_a, ...)
//!      Fillet strip:   `segments` quads between arc_a[k] ↔ arc_b[k]
//!      F3 at v_a / v_b (if present): corner vertex v replaced by the
//!                      arc sampled at that endpoint.
//! 5. Removes the original edge + its two face-ids, and any orphan
//!    vertex (v_a, v_b when no face references them anymore).
//!
//! ## Constraints (MVP)
//!
//! - Edge shared by exactly two active faces.
//! - Each endpoint has ≤ 3 incident active faces.
//! - Both adjacent faces are planar (tolerances verified upstream).
//! - Convex edge only: `bisector · (edge_midpoint - center) > 0` must
//!   hold so the arc actually curves outward.
//! - Inner loops (holes) on F1/F2 are preserved if present; F3 holes
//!   are left alone (vertex only appears on outer boundary in practice).
//! - Fixed radius, uniform segments.

use std::collections::HashMap;

use anyhow::{Result, bail, ensure};
use glam::DVec3;

use crate::entities::*;
use crate::mesh::Mesh;
use crate::tolerances::EPSILON_LENGTH;

/// Result of a successful fillet operation.
#[derive(Clone, Debug)]
pub struct FilletResult {
    /// The rebuilt face that replaces the original F1 (adjacent to the edge).
    pub new_f1: FaceId,
    /// The rebuilt face that replaces the original F2.
    pub new_f2: FaceId,
    /// The new strip of quads forming the curved fillet surface.
    pub fillet_faces: Vec<FaceId>,
}

impl Mesh {
    /// Round off `edge_id` with a cylindrical arc of the given `radius`,
    /// sampled with `segments` quads around the arc. See module docs for
    /// the full algorithm and constraints.
    pub fn fillet_edge(
        &mut self,
        edge_id: EdgeId,
        radius: f64,
        segments: u32,
    ) -> Result<FilletResult> {
        // ─── Guards ──────────────────────────────────────────────
        ensure!(radius > EPSILON_LENGTH, "fillet: radius must be positive");
        ensure!(segments >= 2, "fillet: segments must be ≥ 2, got {}", segments);
        ensure!(self.edges.contains(edge_id), "fillet: edge {} not found", edge_id.raw());

        let edge = &self.edges[edge_id];
        let v_a = edge.v_small();
        let v_b = edge.v_large();

        let (shared_faces, _) = self.get_faces_sharing_edge(edge_id);
        let active_shared: Vec<FaceId> = shared_faces.into_iter()
            .filter(|&f| self.faces.contains(f) && self.faces[f].is_active())
            .collect();
        ensure!(
            active_shared.len() == 2,
            "fillet: edge must be shared by exactly 2 active faces, got {}",
            active_shared.len(),
        );
        // `get_faces_sharing_edge` returns the two faces in an arbitrary order,
        // but every step below assumes F1 traverses the shared edge v_a → v_b
        // (and F2 the reverse v_b → v_a). On a manifold edge exactly one face
        // has each orientation, so pick F1 explicitly. Without this, ~half of
        // all edges failed with "F1 loop doesn't contain the edge" (the arbitrary
        // order put the v_b → v_a face first) — fillet silently no-op'd.
        let (f1, f2) = {
            let f0 = active_shared[0];
            let f0_loop = self.collect_loop_verts(self.faces[f0].outer().start)?;
            if loop_neighbors(&f0_loop, v_a, v_b).is_some() {
                (f0, active_shared[1])
            } else {
                (active_shared[1], f0)
            }
        };

        // ─── Gather face geometry ────────────────────────────────
        let n1 = self.faces[f1].normal().normalize();
        let n2 = self.faces[f2].normal().normalize();
        ensure!(
            n1.length_squared() > 0.5,
            "fillet: face {} has a degenerate normal", f1.raw(),
        );
        ensure!(
            n2.length_squared() > 0.5,
            "fillet: face {} has a degenerate normal", f2.raw(),
        );

        let f1_verts = self.collect_loop_verts(self.faces[f1].outer().start)?;
        let f2_verts = self.collect_loop_verts(self.faces[f2].outer().start)?;

        // ─── Find adjacent-edge neighbors at each endpoint ───────
        // On F1 at v_a: the vertex that comes before v_a in F1's loop
        // walking so that after v_a we reach v_b (the fillet edge).
        // Effectively, the vertex that the "other" (non-fillet) edge at
        // v_a on F1 points to. Same concept for every (face, endpoint)
        // pair below.
        let (f1_prev_a, f1_next_b) = loop_neighbors(&f1_verts, v_a, v_b)
            .ok_or_else(|| anyhow::anyhow!("fillet: F1 loop doesn't contain the edge"))?;
        let (f2_prev_b, f2_next_a) = loop_neighbors(&f2_verts, v_b, v_a)
            .ok_or_else(|| anyhow::anyhow!("fillet: F2 loop doesn't contain the edge"))?;

        let va_pos = self.vertex_pos(v_a)?;
        let vb_pos = self.vertex_pos(v_b)?;

        let dir_f1_va = (self.vertex_pos(f1_prev_a)? - va_pos).normalize();
        let dir_f2_va = (self.vertex_pos(f2_next_a)? - va_pos).normalize();
        let dir_f1_vb = (self.vertex_pos(f1_next_b)? - vb_pos).normalize();
        let dir_f2_vb = (self.vertex_pos(f2_prev_b)? - vb_pos).normalize();

        let p1_a = va_pos + dir_f1_va * radius;
        let p2_a = va_pos + dir_f2_va * radius;
        let p1_b = vb_pos + dir_f1_vb * radius;
        let p2_b = vb_pos + dir_f2_vb * radius;

        // ─── Arc centers (dihedral-aware) ────────────────────────
        // n1 + n2 points OUTWARD (away from the solid) on a convex edge
        // because both face normals face outward. The arc center sits
        // INSIDE the solid at distance `radius / sin(α)` along the
        // INWARD bisector, where α is the half of the dihedral.
        //
        //   bisector_out = (n1 + n2).normalize()
        //   bisector_in  = -bisector_out
        //   α = angle between n1 and bisector_out (half of angle
        //       between n1 and n2)
        //   center = v + bisector_in · (radius / sin α)
        //
        // For right-angle edges (α = 45°), sin α = √2/2, so the center
        // sits at r·√2 inside — i.e. at the corner of a square of side
        // r tangent to both faces on the solid side.
        let bisector_out = (n1 + n2).normalize();
        ensure!(
            bisector_out.length_squared() > 0.5,
            "fillet: faces are (nearly) parallel — no well-defined arc center",
        );
        let bisector_in = -bisector_out;
        // sin α = |n1 × bisector_out| (both unit, and they have the same
        // plane so the cross magnitude is the sine of the angle between).
        let half_angle_sin = n1.cross(bisector_out).length().max(1e-6);
        let offset_scale = radius / half_angle_sin;
        let center_a = va_pos + bisector_in * offset_scale;
        let center_b = vb_pos + bisector_in * offset_scale;

        // Verify the offset points really lie on the arc (sanity check).
        let r_a1 = (p1_a - center_a).length();
        let r_a2 = (p2_a - center_a).length();
        ensure!(
            (r_a1 - radius).abs() < radius * 0.05 &&
            (r_a2 - radius).abs() < radius * 0.05,
            "fillet: arc radius check failed at v_a ({}, {})", r_a1, r_a2,
        );

        // Convex check: the mid-arc point (center + r · bisector_out)
        // should lie "outside" the arc center — i.e. to_mid must align
        // with bisector_out. Negative dot → concave edge.
        let mid_a = (p1_a + p2_a) * 0.5;
        let to_mid = (mid_a - center_a).normalize();
        ensure!(
            to_mid.dot(bisector_out) > 0.0,
            "fillet: edge appears concave — MVP supports convex edges only",
        );

        // ─── Sample the arc at each endpoint ─────────────────────
        let arc_axis = (vb_pos - va_pos).normalize();
        let r_start_a = p1_a - center_a;
        let r_start_b = p1_b - center_b;

        // Angle from p1 to p2 going around (-bisector) side. The dot-
        // product arccos gives the unsigned angle; we trust the convex
        // geometry we just validated and rotate in the positive direction
        // around arc_axis.
        let cos_angle = (r_start_a.normalize()).dot((p2_a - center_a).normalize())
            .clamp(-1.0, 1.0);
        let total_angle = cos_angle.acos();
        ensure!(
            total_angle > 1e-4,
            "fillet: arc angle is ~0 — faces coincident or already rounded",
        );

        // Decide rotation direction by comparing rotation of +step against
        // actual p2 direction.
        let step = total_angle / segments as f64;
        let test_rot = rotate_axis(r_start_a, arc_axis, step);
        let dir_sign = if (test_rot.normalize()).dot((p2_a - center_a).normalize())
            > (r_start_a.normalize()).dot((p2_a - center_a).normalize())
        { 1.0 } else { -1.0 };

        let arc_a_pts: Vec<DVec3> = (0..=segments).map(|k| {
            if k == 0 { p1_a }
            else if k == segments { p2_a }
            else {
                let theta = dir_sign * step * k as f64;
                center_a + rotate_axis(r_start_a, arc_axis, theta)
            }
        }).collect();
        let arc_b_pts: Vec<DVec3> = (0..=segments).map(|k| {
            if k == 0 { p1_b }
            else if k == segments { p2_b }
            else {
                let theta = dir_sign * step * k as f64;
                center_b + rotate_axis(r_start_b, arc_axis, theta)
            }
        }).collect();

        // ─── Materialize new arc vertices ────────────────────────
        let arc_a_verts: Vec<VertId> =
            arc_a_pts.iter().map(|p| self.add_vertex(*p)).collect();
        let arc_b_verts: Vec<VertId> =
            arc_b_pts.iter().map(|p| self.add_vertex(*p)).collect();

        // ─── Prepare F1' / F2' vertex lists ──────────────────────
        // F1's loop walks (..., f1_prev_a, v_a, v_b, f1_next_b, ...). We
        // splice out `v_a, v_b` and insert `p1_a, p1_b`.
        //
        // Vertex list of a replace operation preserves all other entries,
        // so we just walk `f1_verts` and swap out the {v_a, v_b} segment.
        let f1_mat = self.faces[f1].material();
        let f2_mat = self.faces[f2].material();
        let f1_new_verts = splice_edge_replacement(&f1_verts, v_a, v_b,
            arc_a_verts[0], arc_b_verts[0])?;
        // F2's loop walks in the opposite winding at this edge — it has
        // (..., f2_prev_b, v_b, v_a, f2_next_a, ...). So we splice the
        // {v_b, v_a} segment with {p2_b, p2_a}.
        let f2_new_verts = splice_edge_replacement(&f2_verts, v_b, v_a,
            *arc_b_verts.last().unwrap(),
            *arc_a_verts.last().unwrap())?;

        // ─── Detect optional F3 at each endpoint ─────────────────
        // A third face at v_a (not F1 or F2) — if present, its corner
        // vertex will need replacement by the arc.
        let f3_a = third_face_at_vert(self, v_a, f1, f2)?;
        let f3_b = third_face_at_vert(self, v_b, f1, f2)?;

        // ─── Snapshot F3 boundaries before mutation ──────────────
        let f3_a_info = match f3_a {
            Some(fid) => Some((fid,
                self.collect_loop_verts(self.faces[fid].outer().start)?,
                self.faces[fid].material())),
            None => None,
        };
        let f3_b_info = match f3_b {
            Some(fid) => Some((fid,
                self.collect_loop_verts(self.faces[fid].outer().start)?,
                self.faces[fid].material())),
            None => None,
        };

        // ─── Tear down affected faces ───────────────────────────
        // Collect IDs first — mutation inside the loop below.
        let mut faces_to_remove: Vec<FaceId> = vec![f1, f2];
        if let Some(fid) = f3_a { faces_to_remove.push(fid); }
        if let Some(fid) = f3_b {
            if Some(fid) != f3_a { faces_to_remove.push(fid); }
        }
        for fid in &faces_to_remove {
            let _ = self.remove_face(*fid);
            if self.faces.contains(*fid) {
                self.faces.remove(*fid);
            }
        }

        // ─── Rebuild F1 and F2 ──────────────────────────────────
        let new_f1 = self.add_face_with_holes(&f1_new_verts, &[], f1_mat)?;
        let new_f2 = self.add_face_with_holes(&f2_new_verts, &[], f2_mat)?;

        // ─── Rebuild F3 at each endpoint (replace vertex with arc) ─
        // F3_a's boundary contained v_a between some u_F1 (shared edge
        // with F1) and u_F2 (shared edge with F2). We splice {v_a} out
        // and splice in the arc vertices. Direction (forward vs reversed)
        // is chosen so that the first inserted arc vertex matches the
        // F1-side neighbor (to keep winding outward).
        if let Some((_fid, ref verts, material)) = f3_a_info {
            // Orient the arc to F3_a's winding (sweep #2 — see orient_arc_for_f3).
            let arc = orient_arc_for_f3(self, verts, v_a, &arc_a_verts, dir_f1_va)?;
            let new_verts = splice_vertex_replacement(verts, v_a, &arc)?;
            self.add_face_with_holes(&new_verts, &[], material)?;
        }
        if let Some((f3_b_id, ref verts, material)) = f3_b_info {
            // Guard against F3_a == F3_b (single face wrapping both ends
            // of the edge — e.g., a 2-face cylinder mesh). MVP bails.
            if let Some((f3_a_id, _, _)) = f3_a_info {
                if f3_a_id == f3_b_id {
                    bail!("fillet: same face on both endpoints of edge — \
                           single-ring topology not yet supported");
                }
            }
            let arc = orient_arc_for_f3(self, verts, v_b, &arc_b_verts, dir_f1_vb)?;
            let new_verts = splice_vertex_replacement(verts, v_b, &arc)?;
            self.add_face_with_holes(&new_verts, &[], material)?;
        }

        // ─── Fillet strip ───────────────────────────────────────
        // For each k: [arc_a[k], arc_a[k+1], arc_b[k+1], arc_b[k]] walked
        // to produce an outward-facing normal. On a convex edge the
        // outward direction at the mid-arc point is the radial vector
        // from center_a outward (opposite of bisector). Walking
        // "a[k] → a[k+1] → b[k+1] → b[k]" gives CCW from outside for
        // the default dir_sign = +1; for dir_sign = -1 the natural walk
        // flips so we reverse.
        let mut fillet_faces = Vec::with_capacity(segments as usize);
        for k in 0..segments as usize {
            let quad = if dir_sign > 0.0 {
                [arc_a_verts[k], arc_a_verts[k + 1],
                 arc_b_verts[k + 1], arc_b_verts[k]]
            } else {
                [arc_a_verts[k + 1], arc_a_verts[k],
                 arc_b_verts[k], arc_b_verts[k + 1]]
            };
            let fid = self.add_face_with_holes(&quad, &[], f1_mat)?;
            fillet_faces.push(fid);
        }

        // ─── Cleanup: orphan edges + isolated verts ──────────────
        let all_edges: Vec<EdgeId> = self.edges.iter().map(|(id, _)| id).collect();
        for eid in all_edges {
            if !self.edges.contains(eid) { continue; }
            let (faces, _) = self.get_faces_sharing_edge(eid);
            let has_active_face = faces.iter().any(|&f|
                self.faces.contains(f) && self.faces[f].is_active());
            if !has_active_face {
                let _ = self.remove_edge_and_halfedges(eid);
                if self.edges.contains(eid) {
                    self.edges.remove(eid);
                }
            }
        }
        self.remove_isolated_verts();

        // ADR-007
        self.debug_verify_invariants();

        Ok(FilletResult { new_f1, new_f2, fillet_faces })
    }
}

/// ADR-024 P10 result of `chamfer_vertex_3way`.
#[derive(Clone, Debug)]
pub struct ChamferResult {
    /// The new triangular chamfer face replacing the corner.
    pub trim_face: FaceId,
    /// The 3 rebuilt incident faces (with v replaced by the trim points).
    pub modified_faces: Vec<FaceId>,
}

impl Mesh {
    /// ADR-024 P10 — Flat triangular chamfer at a 3-way corner vertex.
    ///
    /// MVP: replaces a valence-3 vertex with 3 trim points (one per
    /// incident face) and a single triangular face. Future expansion
    /// will tessellate as a spherical patch when segments ≥ 2.
    pub fn chamfer_vertex_3way(
        &mut self,
        v: VertId,
        radius: f64,
    ) -> Result<ChamferResult> {
        ensure!(radius > EPSILON_LENGTH, "chamfer: radius must be positive");
        ensure!(self.verts.contains(v) && self.verts[v].is_active(),
            "chamfer: vertex {} not active", v.raw());

        // 1) Collect 3 active incident faces.
        let faces = incident_faces_at_vertex(self, v);
        ensure!(faces.len() == 3,
            "chamfer: MVP requires valence==3, got {} incident faces", faces.len());
        let (f1, f2, f3) = (faces[0], faces[1], faces[2]);

        // 2) Loop verts per face.
        let f1_verts = self.collect_loop_verts(self.faces[f1].outer().start)?;
        let f2_verts = self.collect_loop_verts(self.faces[f2].outer().start)?;
        let f3_verts = self.collect_loop_verts(self.faces[f3].outer().start)?;

        // 3) EDGE trim points — one per incident edge, not a per-face bisector.
        //    (Adversarial sweep pattern #2.) The ADR-024 MVP replaced the corner
        //    with a single per-face bisector *interior* point, so the edge (v,X)
        //    shared by two faces got a DIFFERENT replacement point in each face →
        //    the shared edge broke and the solid silently opened (boundary edges)
        //    while `verify_face_invariants` still reported 0 violations. Trimming
        //    each edge at `radius` along it yields a point identical from both
        //    faces that share that edge (`add_vertex` dedups → same VertId), so
        //    the shared edges stay matched and the solid stays closed.
        let (tp1, tn1) = edge_trim_points(self, &f1_verts, v, radius)?;
        let (tp2, tn2) = edge_trim_points(self, &f2_verts, v, radius)?;
        let (tp3, tn3) = edge_trim_points(self, &f3_verts, v, radius)?;

        // 4) Capture face data + normals before mutation.
        let m1 = self.faces[f1].material();
        let m2 = self.faces[f2].material();
        let m3 = self.faces[f3].material();
        let n_sum = self.faces[f1].normal() + self.faces[f2].normal() + self.faces[f3].normal();

        // 5) Materialize edge trim vertices (dedup welds each shared edge point).
        let a1 = self.add_vertex(tp1);
        let b1 = self.add_vertex(tn1);
        let a2 = self.add_vertex(tp2);
        let b2 = self.add_vertex(tn2);
        let a3 = self.add_vertex(tp3);
        let b3 = self.add_vertex(tn3);

        // 6) Splice each face's loop: replace v with its two edge points
        //    [prev-edge point, next-edge point], preserving loop order.
        let f1_new = splice_vertex_replacement(&f1_verts, v, &[a1, b1])?;
        let f2_new = splice_vertex_replacement(&f2_verts, v, &[a2, b2])?;
        let f3_new = splice_vertex_replacement(&f3_verts, v, &[a3, b3])?;

        // 7) Tear down original faces.
        for fid in &[f1, f2, f3] {
            let _ = self.remove_face(*fid);
            if self.faces.contains(*fid) {
                self.faces.remove(*fid);
            }
        }

        // 8) Rebuild incident faces.
        let new_f1 = self.add_face_with_holes(&f1_new, &[], m1)?;
        let new_f2 = self.add_face_with_holes(&f2_new, &[], m2)?;
        let new_f3 = self.add_face_with_holes(&f3_new, &[], m3)?;

        // 9) Chamfer triangle from the 3 UNIQUE edge trim points. Each of v's
        //    3 edges is shared by two of the faces, so {a*, b*} collapses to
        //    exactly 3 ids. Winding: point outward (n_sum direction).
        let mut uniq: Vec<VertId> = Vec::with_capacity(3);
        for x in [a1, b1, a2, b2, a3, b3] {
            if !uniq.contains(&x) { uniq.push(x); }
        }
        ensure!(uniq.len() == 3,
            "chamfer: expected 3 unique edge trim points, got {}", uniq.len());
        let (q1, q2, q3) = (uniq[0], uniq[1], uniq[2]);
        let (pq1, pq2, pq3) =
            (self.vertex_pos(q1)?, self.vertex_pos(q2)?, self.vertex_pos(q3)?);
        let tri_normal_ccw = (pq2 - pq1).cross(pq3 - pq1);
        let winding: [VertId; 3] = if tri_normal_ccw.dot(n_sum) > 0.0 {
            [q1, q2, q3]
        } else {
            [q1, q3, q2]
        };
        let trim_face = self.add_face_with_holes(&winding, &[], m1)?;

        // 10) Cleanup orphan edges + isolated v.
        let all_edges: Vec<EdgeId> = self.edges.iter().map(|(id, _)| id).collect();
        for eid in all_edges {
            if !self.edges.contains(eid) { continue; }
            let (faces, _) = self.get_faces_sharing_edge(eid);
            let has_active = faces.iter().any(|&f|
                self.faces.contains(f) && self.faces[f].is_active());
            if !has_active {
                let _ = self.remove_edge_and_halfedges(eid);
                if self.edges.contains(eid) { self.edges.remove(eid); }
            }
        }
        self.remove_isolated_verts();

        // ADR-007
        self.debug_verify_invariants();

        Ok(ChamferResult {
            trim_face,
            modified_faces: vec![new_f1, new_f2, new_f3],
        })
    }
}

/// Returns the two trim points for face `loop_verts` at corner `v`: one on the
/// edge (v → prev) and one on the edge (v → next), each `radius` along the edge
/// from `v`. Using per-edge points (rather than a face bisector) keeps the point
/// on an edge shared by two faces identical from both sides, so the shared edge
/// survives the chamfer and the solid stays closed (adversarial sweep #2).
fn edge_trim_points(
    mesh: &Mesh,
    loop_verts: &[VertId],
    v: VertId,
    radius: f64,
) -> Result<(DVec3, DVec3)> {
    let n = loop_verts.len();
    for i in 0..n {
        if loop_verts[i] == v {
            let prev = loop_verts[(i + n - 1) % n];
            let next = loop_verts[(i + 1) % n];
            let v_pos = mesh.vertex_pos(v)?;
            let e_prev = mesh.vertex_pos(prev)? - v_pos;
            let e_next = mesh.vertex_pos(next)? - v_pos;
            let l_prev = e_prev.length();
            let l_next = e_next.length();
            ensure!(l_prev > EPSILON_LENGTH && l_next > EPSILON_LENGTH,
                "chamfer: degenerate edge at v{} (zero-length)", v.raw());
            // Input-validation guard (adversarial sweep — flap class). The trim
            // point sits `radius` along each incident edge, so `radius` must be
            // strictly SHORTER than every incident edge — otherwise it overshoots
            // past the neighbour and the trim triangle folds back through the
            // adjacent faces (a self-intersection that stays manifold + closed,
            // so no other check catches it). This also rejects chamfering a
            // vertex that is not a clean corner — e.g. a fillet-strip vertex,
            // whose incident edges are short arc segments (< radius). Fail loud
            // BEFORE any destructive edit so the caller's transaction rolls back.
            ensure!(radius < l_prev && radius < l_next,
                "chamfer: radius {:.3} exceeds an incident edge length at v{} \
                 (edges {:.3} / {:.3}) — not a clean corner (e.g. a filleted/curved \
                 vertex); pick a corner or use a smaller radius",
                radius, v.raw(), l_prev, l_next);
            let dir_prev = e_prev / l_prev;
            let dir_next = e_next / l_next;
            return Ok((v_pos + dir_prev * radius, v_pos + dir_next * radius));
        }
    }
    bail!("chamfer: vertex {} not in face loop", v.raw())
}

/// Collect unique active incident faces of a vertex via the v_next radial
/// chain. Returns at most ~32 faces (real-world meshes are far smaller).
fn incident_faces_at_vertex(mesh: &Mesh, v: VertId) -> Vec<FaceId> {
    use std::collections::HashSet;
    let anchor = match mesh.verts.get(v).and_then(|vt| vt.outgoing()) {
        Some(h) if !h.is_null() => h,
        _ => return Vec::new(),
    };
    let mut seen: HashSet<FaceId> = HashSet::new();
    let mut cur = anchor;
    for _ in 0..128 {
        if !mesh.hes.contains(cur) { break; }
        let f = mesh.hes[cur].face();
        if !f.is_null() && mesh.faces.contains(f) && mesh.faces[f].is_active() {
            seen.insert(f);
        }
        let nxt = mesh.hes[cur].v_next();
        if nxt.is_null() || nxt == anchor { break; }
        cur = nxt;
    }
    seen.into_iter().collect()
}

/// Find the loop-neighbor verts: the vertex before `a` and the one after
/// `b` in the cyclic walk, ensuring `b` comes right after `a` (i.e. edge
/// `(a, b)` is a walked edge in this direction).
fn loop_neighbors(
    loop_verts: &[VertId],
    a: VertId,
    b: VertId,
) -> Option<(VertId, VertId)> {
    let n = loop_verts.len();
    for i in 0..n {
        if loop_verts[i] == a && loop_verts[(i + 1) % n] == b {
            let prev = loop_verts[(i + n - 1) % n];
            let next = loop_verts[(i + 2) % n];
            return Some((prev, next));
        }
    }
    None
}

/// Replace the consecutive segment `{v_a, v_b}` in a vertex loop with
/// the single pair `{rep_a, rep_b}`. Preserves all other entries and
/// keeps the original winding. Fillet-internal helper for splicing the
/// arc endpoints into an adjacent face's outer loop.
fn splice_edge_replacement(
    loop_verts: &[VertId],
    a: VertId,
    b: VertId,
    rep_a: VertId,
    rep_b: VertId,
) -> Result<Vec<VertId>> {
    let n = loop_verts.len();
    for i in 0..n {
        if loop_verts[i] == a && loop_verts[(i + 1) % n] == b {
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                let v = loop_verts[(i + k) % n];
                out.push(if v == a { rep_a } else if v == b { rep_b } else { v });
            }
            return Ok(out);
        }
    }
    bail!("splice_edge_replacement: edge {{{:?}, {:?}}} not found in loop",
          a, b)
}

/// Replace a single vertex in a loop with a sequence of arc vertices.
/// The arc is inserted in natural order; caller guarantees `arc_verts[0]`
/// is the F1-side endpoint and `arc_verts[last]` is the F2-side endpoint
/// so the result preserves the parent face's winding.
/// Orient an endpoint arc so its F1-side end (`arc[0]`, offset along `f1_dir`)
/// sits next to the F1-side neighbour of `v` in face `verts`.
///
/// (Adversarial sweep pattern #2.) The ADR-024 fillet MVP always spliced the
/// arc forward, even though its own comment promised a direction choice. On
/// faces whose winding places the F1-side neighbour AFTER `v`, the forward arc
/// went in reversed → the arc edges no longer matched the fillet strip / F1'
/// edges and the solid silently opened (boundary edges), while
/// `verify_face_invariants` still passed. Choosing the order by which neighbour
/// aligns with `f1_dir` keeps every shared edge matched.
fn orient_arc_for_f3(
    mesh: &Mesh,
    verts: &[VertId],
    v: VertId,
    arc: &[VertId],
    f1_dir: DVec3,
) -> Result<Vec<VertId>> {
    let n = verts.len();
    let idx = verts.iter().position(|&x| x == v)
        .ok_or_else(|| anyhow::anyhow!("fillet: endpoint {} not in F3 loop", v.raw()))?;
    let prev = verts[(idx + n - 1) % n];
    let next = verts[(idx + 1) % n];
    let vp = mesh.vertex_pos(v)?;
    let d_prev = (mesh.vertex_pos(prev)? - vp).normalize_or_zero();
    let d_next = (mesh.vertex_pos(next)? - vp).normalize_or_zero();
    // arc[0] is on the F1 side; keep it adjacent to whichever neighbour points
    // that way. If `prev` is the F1-side neighbour, forward keeps arc[0] next to
    // it; otherwise reverse so arc[0] ends up next to `next`.
    if d_prev.dot(f1_dir) >= d_next.dot(f1_dir) {
        Ok(arc.to_vec())
    } else {
        Ok(arc.iter().rev().copied().collect())
    }
}

fn splice_vertex_replacement(
    loop_verts: &[VertId],
    v: VertId,
    arc_verts: &[VertId],
) -> Result<Vec<VertId>> {
    let n = loop_verts.len();
    for i in 0..n {
        if loop_verts[i] == v {
            let mut out = Vec::with_capacity(n + arc_verts.len() - 1);
            for k in 0..n {
                let cur = loop_verts[(i + k) % n];
                if cur == v {
                    out.extend_from_slice(arc_verts);
                } else {
                    out.push(cur);
                }
            }
            return Ok(out);
        }
    }
    bail!("splice_vertex_replacement: vertex not found in loop")
}

/// If a third face is attached to vertex `v` (besides `exclude_f1` and
/// `exclude_f2`), return it. MVP bails on >3 incident faces.
fn third_face_at_vert(
    mesh: &Mesh,
    v: VertId,
    exclude_f1: FaceId,
    exclude_f2: FaceId,
) -> Result<Option<FaceId>> {
    let mut seen: HashMap<FaceId, usize> = HashMap::new();
    // Walk vertex outgoing HEs and collect unique incident face ids.
    let anchor = match mesh.verts.get(v).and_then(|vt| vt.outgoing()) {
        Some(h) if !h.is_null() => h,
        _ => return Ok(None),
    };
    let mut cur = anchor;
    for _ in 0..128 {
        if !mesh.hes.contains(cur) { break; }
        let f = mesh.hes[cur].face();
        if !f.is_null() && mesh.faces.contains(f) && mesh.faces[f].is_active() {
            *seen.entry(f).or_insert(0) += 1;
        }
        let nxt = mesh.hes[cur].v_next();
        if nxt.is_null() || nxt == anchor { break; }
        cur = nxt;
    }
    seen.remove(&exclude_f1);
    seen.remove(&exclude_f2);
    match seen.len() {
        0 => Ok(None),
        1 => Ok(Some(*seen.keys().next().unwrap())),
        n => bail!("fillet: vertex has {} faces beyond the filleted edge; \
                    MVP supports ≤ 1 additional face", n),
    }
}

/// Rodrigues rotation of `v` around unit axis by `angle_rad`.
#[inline]
fn rotate_axis(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let c = angle.cos();
    let s = angle.sin();
    v * c + axis.cross(v) * s + axis * (axis.dot(v) * (1.0 - c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a unit cube mesh (CCW from outside) and return (mesh,
    /// vertex handles array v000..v111). Corner indexing follows
    /// bits x|y|z — v000 is origin, v111 is (1,1,1).
    fn cube_mesh() -> (Mesh, [VertId; 8]) {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let v000 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v100 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v110 = m.add_vertex(DVec3::new(10.0,10.0, 0.0));
        let v010 = m.add_vertex(DVec3::new(0.0,10.0, 0.0));
        let v001 = m.add_vertex(DVec3::new(0.0, 0.0,10.0));
        let v101 = m.add_vertex(DVec3::new(10.0, 0.0,10.0));
        let v111 = m.add_vertex(DVec3::new(10.0,10.0,10.0));
        let v011 = m.add_vertex(DVec3::new(0.0,10.0,10.0));
        m.add_face_with_holes(&[v000, v010, v110, v100], &[], mat).unwrap();
        m.add_face_with_holes(&[v001, v101, v111, v011], &[], mat).unwrap();
        m.add_face_with_holes(&[v000, v100, v101, v001], &[], mat).unwrap();
        m.add_face_with_holes(&[v010, v011, v111, v110], &[], mat).unwrap();
        m.add_face_with_holes(&[v000, v001, v011, v010], &[], mat).unwrap();
        m.add_face_with_holes(&[v100, v110, v111, v101], &[], mat).unwrap();
        (m, [v000, v100, v110, v010, v001, v101, v111, v011])
    }

    #[test]
    fn fillet_cube_top_front_edge() {
        // Fillet the edge between v001-v101 (shared by top face and front
        // face). Corners v001 and v101 each have 3 incident faces (top,
        // front, side), so the top + front + both sides get rebuilt with
        // arc segments.
        let (mut m, v) = cube_mesh();
        let (v001, v101) = (v[4], v[5]);
        let edge = m.find_edge(v001, v101)
            .expect("top-front edge should exist");

        let segments = 4u32;
        let before_faces = m.face_count();
        let res = m.fillet_edge(edge, 2.0, segments).unwrap();

        // 6 original faces, removed: top(1) + front(1) + 2 sides(2) = 4
        // added: top' + front' + 2 sides' + `segments` fillet quads
        //       = 4 + segments
        // net = -4 + (4 + segments) = +segments
        assert_eq!(m.face_count(), before_faces + segments as usize,
            "fillet should add {} faces net (got {})", segments, m.face_count() - before_faces);
        assert_eq!(res.fillet_faces.len(), segments as usize);
        let report = m.verify_face_invariants();
        assert_eq!(report.violations.len(), 0,
            "invariants after cube fillet:\n{}", report.summary());
    }

    /// Adversarial sweep (pattern #2 — fillet must not silently open the solid).
    /// The ADR-024 MVP always spliced the endpoint arc into F3 forward, so on
    /// half the cube edges the arc went in reversed → shared edges broke and the
    /// cube opened (boundary edges) while `verify_face_invariants` still passed.
    /// `orient_arc_for_f3` picks the correct direction.
    #[test]
    fn fillet_cube_edge_keeps_closed() {
        fn manifold(m: &Mesh) -> crate::mesh::ManifoldInfo {
            let active: Vec<_> = m.faces.iter()
                .filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
            m.face_set_manifold_info(&active)
        }
        // EVERY cube edge must fillet successfully and stay a closed solid.
        // (The f1/f2 orientation fix means no edge fails with "F1 loop doesn't
        // contain the edge" any more — previously ~half the edges no-op'd.)
        let mut filleted = 0;
        for i in 0..12 {
            let (mut m, _) = cube_mesh();
            assert!(manifold(&m).is_closed_solid, "cube starts closed");
            let edges: Vec<_> = m.edges.iter()
                .filter(|(_, e)| e.is_active()).map(|(id, _)| id).collect();
            let eid = edges[i];
            m.fillet_edge(eid, 2.0, 4)
                .unwrap_or_else(|e| panic!("REGRESSION: fillet of edge #{} failed: {}", i, e));
            filleted += 1;
            let info = manifold(&m);
            assert!(info.is_closed_solid,
                "REGRESSION: fillet of edge #{} opened the solid \
                 (boundary={}, non_manifold={})",
                i, info.boundary_edge_count, info.non_manifold_edge_count);
            assert_eq!(info.boundary_edge_count, 0, "no boundary edges");
            assert!(m.verify_face_invariants().violations.is_empty());
        }
        assert_eq!(filleted, 12, "all 12 cube edges must fillet (orientation fix)");
    }

    #[test]
    fn fillet_rejects_boundary_edge() {
        // An edge shared by only one face (boundary) must be rejected.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let b = m.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let c = m.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        m.add_face_with_holes(&[a, b, c], &[], mat).unwrap();
        let edge = m.find_edge(a, b).unwrap();
        assert!(m.fillet_edge(edge, 0.1, 4).is_err());
    }

    /// ADR-024 P10 — Flat triangular chamfer at cube corner v000.
    /// 6 cube faces. v000 has 3 incident faces (bottom, front, left).
    /// After chamfer: 3 modified faces + 1 new triangle. v000 removed.
    #[test]
    fn chamfer_3way_cube_corner_creates_triangle() {
        let (mut m, v) = cube_mesh();
        let v000 = v[0];
        let before_faces = m.face_count();

        let res = m.chamfer_vertex_3way(v000, 2.0).unwrap();
        assert_eq!(res.modified_faces.len(), 3, "3 incident faces rebuilt");

        // Net: 3 removed + 4 added (3 incident + 1 triangle) = +1 face.
        assert_eq!(m.face_count(), before_faces + 1,
            "chamfer should add 1 face net");

        // The trim face has 3 vertices.
        let trim_verts = m.collect_loop_verts(m.faces[res.trim_face].outer().start).unwrap();
        assert_eq!(trim_verts.len(), 3, "trim face is triangular");

        // v000 should no longer be active (removed by remove_isolated_verts).
        assert!(!m.verts.contains(v000) || !m.verts[v000].is_active(),
            "v000 should be removed after chamfer");

        let report = m.verify_face_invariants();
        assert_eq!(report.violations.len(), 0,
            "invariants after 3-way chamfer:\n{}", report.summary());
    }

    /// Adversarial sweep (pattern #2 — chamfer must not silently open the solid).
    /// The ADR-024 MVP replaced the corner with a per-face bisector interior
    /// point, breaking the shared edges → the cube opened (boundary edges) even
    /// though `verify_face_invariants` still passed. Edge-based trim keeps it
    /// watertight.
    #[test]
    fn chamfer_3way_keeps_cube_closed() {
        let (mut m, v) = cube_mesh();
        let active: Vec<_> = m.faces.iter()
            .filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
        assert!(m.face_set_manifold_info(&active).is_closed_solid, "cube starts closed");

        m.chamfer_vertex_3way(v[0], 2.0).unwrap();

        let active: Vec<_> = m.faces.iter()
            .filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
        let info = m.face_set_manifold_info(&active);
        assert!(info.is_closed_solid,
            "REGRESSION: chamfer opened the solid (boundary={}, non_manifold={})",
            info.boundary_edge_count, info.non_manifold_edge_count);
        assert_eq!(info.boundary_edge_count, 0, "no boundary edges after chamfer");
        assert_eq!(m.collect_non_manifold_edges().len(), 0, "stays manifold");
        assert!(m.verify_face_invariants().violations.is_empty(), "invariants hold");
    }

    /// ADR-207 de-risk — `chamfer_vertex_3way` (engine ALREADY exists, ADR-024 P10)
    /// produces a RENDERABLE result: the cube corner v000=(0,0,0) is cut into a trim
    /// triangle (~radius from the corner), the mesh tessellates + stays manifold, and
    /// no rendered vertex remains at the removed corner. Confirms the ADR-207
    /// WASM/bridge/UI path yields a viewport-ready chamfer with ZERO new engine work.
    #[test]
    fn adr207_chamfer_vertex_renders() {
        let (mut m, v) = cube_mesh();
        let res = m.chamfer_vertex_3way(v[0], 2.0).unwrap();
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        assert!(!idx.is_empty(), "chamfered cube tessellates");
        assert!(fmap.iter().any(|&f| f == res.trim_face.raw()), "trim face renders");
        let nv = pos.len() / 3;
        let mut min_d = f64::MAX;
        for i in 0..nv {
            let p = DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            assert!(p.x.is_finite() && p.y.is_finite() && p.z.is_finite(), "finite position");
            min_d = min_d.min(p.length()); // distance to the removed corner (0,0,0)
        }
        assert!(min_d > 1.5, "corner cut — nearest vertex {:.2} from the removed corner", min_d);
        assert!(nrm.iter().all(|c| c.is_finite()), "finite normals");
        assert_eq!(m.collect_non_manifold_edges().len(), 0, "chamfered cube stays manifold");
    }

    /// ADR-024 P10 — Reject vertex with valence != 3.
    #[test]
    fn chamfer_3way_rejects_non_3way() {
        // Build a flat 4-vertex square: vertex shared by only 1 face.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let b = m.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let c = m.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let d = m.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        m.add_face_with_holes(&[a, b, c, d], &[], mat).unwrap();
        // Vertex `a` has only 1 incident face.
        assert!(m.chamfer_vertex_3way(a, 0.1).is_err());
    }

    /// ADR-024 P10 — Reject zero / negative radius.
    #[test]
    fn chamfer_3way_rejects_bad_radius() {
        let (mut m, v) = cube_mesh();
        assert!(m.chamfer_vertex_3way(v[0],  0.0).is_err());
        assert!(m.chamfer_vertex_3way(v[0], -1.0).is_err());
    }

    /// Adversarial sweep (flap class — self-intersection that stays manifold).
    /// A radius >= an incident edge length overshoots the neighbour, so the trim
    /// triangle folds back through the adjacent faces. The result is
    /// self-intersecting yet still passes closed/manifold/crack/winding checks
    /// (and the closure gate) — the demo-gate found it when a fillet-strip vertex
    /// (short arc edges) got chamfered with a large radius. The edge-length guard
    /// rejects it BEFORE any destructive edit.
    #[test]
    fn chamfer_3way_rejects_radius_exceeding_edge() {
        let (mut m, v) = cube_mesh(); // 10×10×10 → incident edges are length 10
        let before = m.face_count();
        assert!(m.chamfer_vertex_3way(v[0], 12.0).is_err(),
            "radius exceeding an incident edge must be rejected (flap class)");
        assert_eq!(m.face_count(), before, "no mutation on a rejected chamfer");
        // A radius on the boundary (== edge length) is also rejected (degenerate).
        assert!(m.chamfer_vertex_3way(v[0], 10.0).is_err());
        // A valid radius still succeeds and stays closed.
        assert!(m.chamfer_vertex_3way(v[0], 2.0).is_ok());
    }

    #[test]
    fn fillet_rejects_bad_params() {
        let (mut m, v) = cube_mesh();
        let edge = m.find_edge(v[4], v[5]).unwrap();
        assert!(m.fillet_edge(edge, 0.0, 4).is_err());      // zero radius
        assert!(m.fillet_edge(edge, -1.0, 4).is_err());     // negative
        assert!(m.fillet_edge(edge, 1.0, 0).is_err());      // segments < 2
    }
}
