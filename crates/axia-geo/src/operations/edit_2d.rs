//! ADR-211 — 2D Sketch Editing: **Extend** (free wire edges).
//!
//! Composes existing DCEL primitives — no new geometric kernel (Pattern-12):
//!   - `move_vertex(vid, pos)`            (mesh.rs) — pull an endpoint out
//!   - `is_edge_completely_free(edge)`    (mesh.rs) — free-wire guard
//!
//! over a closed-form segment/segment closest-approach intersection.
//!
//! Note on TRIM: AxiA auto-splits crossing wire lines at their intersections on
//! draw (ADR-172), so the segments are already bounded by the crossing points.
//! "Trim" is therefore a *segment delete* (`deleteEdgeCascade`) handled in the
//! UI (`TrimTool`) — no engine-level split-and-remove is needed.
//!
//! Scope (MVP): the target is a **free WIRE edge** (`is_edge_completely_free`).
//! Face-boundary-edge extend — which would reshape faces — is deferred.

use std::collections::HashMap;

use anyhow::{Result, bail, ensure};
use glam::{DMat3, DMat4, DVec3};

use crate::mesh::Mesh;
use crate::tolerances::EPSILON_LENGTH;
use crate::{EdgeId, VertId};

/// Intersection of two segments' supporting lines.
struct LineHit {
    /// Point on the **target** line (P0 + s·(P1−P0)).
    point: DVec3,
    /// Target parameter s (0..1 ⇒ within the target segment).
    s: f64,
    /// Boundary parameter t (0..1 ⇒ within the boundary segment).
    t: f64,
}

/// Closest-approach of the target line (`p0`→`p1`) and boundary line
/// (`q0`→`q1`): returns the point on the **target** line nearest the boundary
/// line plus both parameters. `None` when the lines are (near) parallel.
///
/// For coplanar crossing segments this is the exact intersection; the caller
/// rejects skew lines by checking the residual gap.
fn line_line_closest(p0: DVec3, p1: DVec3, q0: DVec3, q1: DVec3) -> Option<LineHit> {
    let u = p1 - p0;
    let v = q1 - q0;
    let w0 = p0 - q0;
    let a = u.dot(u);
    let b = u.dot(v);
    let c = v.dot(v);
    let d = u.dot(w0);
    let e = v.dot(w0);
    let denom = a * c - b * b;
    // Relative parallel test (scale-aware).
    if denom.abs() <= 1e-9 * (a * c).max(1.0) {
        return None;
    }
    let s = (b * e - c * d) / denom;
    let t = (a * e - b * d) / denom;
    Some(LineHit { point: p0 + u * s, s, t })
}

impl Mesh {
    /// Endpoint positions + vert ids of an active edge (`v_small`, `v_large` order).
    fn edge_endpoints_pos(&self, edge: EdgeId) -> Result<(DVec3, DVec3, VertId, VertId)> {
        let e = self
            .edges
            .get(edge)
            .filter(|e| e.is_active())
            .ok_or_else(|| anyhow::anyhow!("edge {:?} not active", edge))?;
        let va = e.v_small();
        let vb = e.v_large();
        let pa = self
            .verts
            .get(va)
            .ok_or_else(|| anyhow::anyhow!("edge {:?} v_small missing", edge))?
            .pos();
        let pb = self
            .verts
            .get(vb)
            .ok_or_else(|| anyhow::anyhow!("edge {:?} v_large missing", edge))?
            .pos();
        Ok((pa, pb, va, vb))
    }

    /// **ADR-211 EXTEND** — lengthen free wire edge `target` by moving the
    /// endpoint nearest the intersection out to meet `boundary`'s supporting
    /// line. The intersection must lie BEYOND a target endpoint (`s≤0` or
    /// `s≥1`); a target whose interior already crosses the boundary is rejected.
    /// `boundary` is left untouched. The target edge id is preserved
    /// (`move_vertex` changes geometry, not topology).
    pub fn extend_edge_to_boundary(&mut self, target: EdgeId, boundary: EdgeId) -> Result<()> {
        ensure!(
            target != boundary,
            "extend: target and boundary are the same edge"
        );
        ensure!(
            self.is_edge_completely_free(target),
            "extend: target {:?} is not a free wire edge",
            target
        );

        let (p0, p1, va, vb) = self.edge_endpoints_pos(target)?;
        let (q0, q1, _, _) = self.edge_endpoints_pos(boundary)?;

        let hit = line_line_closest(p0, p1, q0, q1)
            .ok_or_else(|| anyhow::anyhow!("extend: edges are parallel"))?;

        let eps = 1e-6;
        ensure!(
            hit.t > -eps && hit.t < 1.0 + eps,
            "extend: target line does not meet boundary segment (t={:.4})",
            hit.t
        );
        let q_closest = q0 + (q1 - q0) * hit.t;
        let resid = (hit.point - q_closest).length();
        ensure!(
            resid < 1e-3,
            "extend: lines are skew (gap {:.4}mm)",
            resid
        );

        // Which endpoint does the intersection extend past?
        let v_move = if hit.s >= 1.0 - eps {
            vb // beyond v_large (P1) end
        } else if hit.s <= eps {
            va // beyond v_small (P0) end
        } else {
            bail!(
                "extend: intersection already within target — nothing to extend (s={:.4})",
                hit.s
            );
        };

        let anchor_pos = if v_move == va { p1 } else { p0 };
        ensure!(
            (hit.point - anchor_pos).length() > EPSILON_LENGTH,
            "extend: would collapse edge to zero length"
        );

        self.move_vertex(v_move, hit.point)?;
        self.debug_verify_invariants();
        Ok(())
    }

    /// Corner geometry at a valence-2 vertex: the two edges, corner position,
    /// unit directions toward each far endpoint, the two edge lengths, and the
    /// interior angle θ ∈ (0, π). Error on non-corner / degenerate input.
    #[allow(clippy::type_complexity)]
    fn corner_2d_geom(
        &self,
        corner: VertId,
    ) -> Result<(EdgeId, EdgeId, DVec3, DVec3, DVec3, f64, f64, f64)> {
        let (e1, e2) = self.two_edges_at_corner(corner).ok_or_else(|| {
            anyhow::anyhow!("corner: vertex {:?} is not a valence-2 corner", corner)
        })?;
        let vpos = self
            .verts
            .get(corner)
            .ok_or_else(|| anyhow::anyhow!("corner vertex missing"))?
            .pos();
        let far = |m: &Mesh, e: EdgeId| -> Result<DVec3> {
            let (pa, pb, va, _vb) = m.edge_endpoints_pos(e)?;
            Ok(if va == corner { pb } else { pa })
        };
        let v1 = far(self, e1)? - vpos;
        let v2 = far(self, e2)? - vpos;
        let len1 = v1.length();
        let len2 = v2.length();
        ensure!(
            len1 > EPSILON_LENGTH && len2 > EPSILON_LENGTH,
            "corner: degenerate (zero-length) edge"
        );
        let d1 = v1 / len1;
        let d2 = v2 / len2;
        let theta = d1.dot(d2).clamp(-1.0, 1.0).acos();
        Ok((e1, e2, vpos, d1, d2, len1, len2, theta))
    }

    /// Replace the corner with a connecting edge between trim points `p1` (on
    /// `e1`) and `p2` (on `e2`): split each edge, drop the stub toward the
    /// corner, add the connecting edge (line, or arc when `curve` is given).
    fn cut_corner(
        &mut self,
        corner: VertId,
        e1: EdgeId,
        e2: EdgeId,
        p1: DVec3,
        p2: DVec3,
        curve: Option<crate::curves::AnalyticCurve>,
    ) -> Result<EdgeId> {
        let drop_stub = |m: &mut Mesh, e: EdgeId, p: DVec3| -> Result<VertId> {
            let (vp, ea, eb) = m.split_edge(e, p)?;
            let (_, _, va, vb) = m.edge_endpoints_pos(ea)?;
            let stub = if va == corner || vb == corner { ea } else { eb };
            m.remove_edge_and_halfedges(stub)?;
            Ok(vp)
        };
        let vp1 = drop_stub(self, e1, p1)?;
        let vp2 = drop_stub(self, e2, p2)?;
        let new_edge = match curve {
            Some(c) => self.add_edge_with_curve(vp1, vp2, c)?,
            None => self.add_edge(vp1, vp2)?.0,
        };
        self.debug_verify_invariants();
        Ok(new_edge)
    }

    /// **ADR-211 C2 CHAMFER (2D corner)** — cut a valence-2 corner with a
    /// straight line: trim each edge back by `dist` from the corner, connect the
    /// trim points with a line. Returns the new chamfer edge id.
    pub fn chamfer_corner_2d(&mut self, corner: VertId, dist: f64) -> Result<EdgeId> {
        ensure!(dist > EPSILON_LENGTH, "chamfer: distance must be positive");
        let (e1, e2, vpos, d1, d2, len1, len2, theta) = self.corner_2d_geom(corner)?;
        ensure!(
            theta > 1e-4 && theta < std::f64::consts::PI - 1e-4,
            "chamfer: edges are collinear — no corner"
        );
        ensure!(
            dist < len1 && dist < len2,
            "chamfer: distance {:.3} exceeds an edge length",
            dist
        );
        let p1 = vpos + d1 * dist;
        let p2 = vpos + d2 * dist;
        self.cut_corner(corner, e1, e2, p1, p2, None)
    }

    /// **ADR-211 C2 FILLET (2D corner)** — round a valence-2 corner with a
    /// circular arc of `radius`, tangent to both edges. Trims each edge back by
    /// `radius / tan(θ/2)` and inserts an `AnalyticCurve::Arc`. Returns the new
    /// arc edge id.
    pub fn fillet_corner_2d(&mut self, corner: VertId, radius: f64) -> Result<EdgeId> {
        ensure!(radius > EPSILON_LENGTH, "fillet: radius must be positive");
        let (e1, e2, vpos, d1, d2, len1, len2, theta) = self.corner_2d_geom(corner)?;
        ensure!(
            theta > 1e-4 && theta < std::f64::consts::PI - 1e-4,
            "fillet: edges are collinear — no corner"
        );
        let half = theta * 0.5;
        let t = radius / half.tan();
        ensure!(
            t < len1 && t < len2,
            "fillet: radius too large for the corner (trim {:.3} exceeds edge)",
            t
        );
        let p1 = vpos + d1 * t;
        let p2 = vpos + d2 * t;
        // Arc center on the interior bisector, tangent to both edges.
        let bis = (d1 + d2).normalize();
        let center = vpos + bis * (radius / half.sin());
        let normal = d1.cross(d2).normalize();
        let u0 = (p1 - center).normalize();
        let to_p2 = (p2 - center).normalize();
        let end_angle = u0.cross(to_p2).dot(normal).atan2(u0.dot(to_p2)); // short way
        let arc = crate::curves::AnalyticCurve::Arc {
            center,
            radius,
            normal,
            basis_u: u0,
            start_angle: 0.0,
            end_angle,
        };
        self.cut_corner(corner, e1, e2, p1, p2, Some(arc))
    }

    /// **ADR-213 JOIN (collinear merge)** — merge the two collinear straight
    /// edges meeting at a valence-2 vertex into a single edge (the inverse of
    /// `split_edge`). Both edges must be straight (no analytic curve) and nearly
    /// collinear (`dir1 · dir2 ≤ −0.999`, ~2.5° tolerance). Dissolves the shared
    /// vertex. Returns the merged edge id.
    pub fn join_collinear_at(&mut self, vertex: VertId) -> Result<EdgeId> {
        let (e1, e2) = self.two_edges_at_corner(vertex).ok_or_else(|| {
            anyhow::anyhow!("join: vertex {:?} is not a valence-2 vertex", vertex)
        })?;
        ensure!(
            self.edge_curve(e1).is_none() && self.edge_curve(e2).is_none(),
            "join: collinear merge applies to straight edges only"
        );
        let vpos = self
            .verts
            .get(vertex)
            .ok_or_else(|| anyhow::anyhow!("join: vertex missing"))?
            .pos();
        let far = |m: &Mesh, e: EdgeId| -> Result<(VertId, DVec3)> {
            let (pa, pb, va, vb) = m.edge_endpoints_pos(e)?;
            Ok(if va == vertex { (vb, pb) } else { (va, pa) })
        };
        let (a, apos) = far(self, e1)?;
        let (b, bpos) = far(self, e2)?;
        let d1 = apos - vpos;
        let d2 = bpos - vpos;
        let l1 = d1.length();
        let l2 = d2.length();
        ensure!(
            l1 > EPSILON_LENGTH && l2 > EPSILON_LENGTH,
            "join: degenerate (zero-length) edge"
        );
        let dot = (d1 / l1).dot(d2 / l2);
        ensure!(
            dot <= -0.999,
            "join: edges are not collinear (dir·dir = {:.4})",
            dot
        );
        ensure!(a != b, "join: would create a degenerate self-edge");

        self.remove_edge_and_halfedges(e1)?;
        self.remove_edge_and_halfedges(e2)?;
        let (merged, _) = self.add_edge(a, b)?;
        self.debug_verify_invariants();
        Ok(merged)
    }

    /// **ADR-214** — replicate `edge_ids` under each `DMat4` in `transforms`,
    /// emitting new wire edges (source untouched). The analytic curve is
    /// preserved when it transforms rigidly (`AnalyticCurve::transform`), else
    /// the copy degrades to a chord line. Shared source endpoints stay shared
    /// within each copy (spatial-hash dedup on the transformed position).
    fn replicate_edges(&mut self, edge_ids: &[EdgeId], transforms: &[DMat4]) -> Result<Vec<EdgeId>> {
        for &e in edge_ids {
            ensure!(
                self.edges.get(e).map(|x| x.is_active()).unwrap_or(false),
                "edge transform: edge {:?} not active",
                e
            );
        }
        if edge_ids.is_empty() || transforms.is_empty() {
            return Ok(Vec::new());
        }

        struct SrcEdge {
            va: VertId,
            vb: VertId,
            curve: Option<crate::curves::AnalyticCurve>,
        }
        let mut src: Vec<SrcEdge> = Vec::with_capacity(edge_ids.len());
        let mut vpos: HashMap<VertId, DVec3> = HashMap::new();
        for &e in edge_ids {
            let (pa, pb, va, vb) = self.edge_endpoints_pos(e)?;
            vpos.entry(va).or_insert(pa);
            vpos.entry(vb).or_insert(pb);
            src.push(SrcEdge { va, vb, curve: self.edge_curve(e).cloned() });
        }

        let mut new_edges = Vec::with_capacity(edge_ids.len() * transforms.len());
        for m in transforms {
            // Per-copy source-vert → new-vert map (shared edges stay shared).
            let mut vmap: HashMap<VertId, VertId> = HashMap::with_capacity(vpos.len());
            for (&v, &p) in &vpos {
                vmap.insert(v, self.add_vertex(m.transform_point3(p)));
            }
            for s in &src {
                let na = vmap[&s.va];
                let nb = vmap[&s.vb];
                let ne = match &s.curve {
                    Some(c) => match c.transform(m, self) {
                        Ok(tc) => self.add_edge_with_curve(na, nb, tc)?,
                        // Curve cannot transform rigidly (e.g. reflection of an
                        // arc, non-uniform scale) — degrade to a chord line.
                        Err(_) => self.add_edge(na, nb)?.0,
                    },
                    None => self.add_edge(na, nb)?.0,
                };
                new_edges.push(ne);
            }
        }
        self.debug_verify_invariants();
        Ok(new_edges)
    }

    /// **ADR-214 MIRROR (edges)** — reflect `edge_ids` across the plane
    /// (`plane_origin`, `plane_normal`). Source untouched. Returns new edge ids.
    pub fn mirror_edges(
        &mut self,
        edge_ids: &[EdgeId],
        plane_origin: DVec3,
        plane_normal: DVec3,
    ) -> Result<Vec<EdgeId>> {
        ensure!(
            plane_normal.length_squared() > EPSILON_LENGTH * EPSILON_LENGTH,
            "mirror edges: plane normal must be non-zero"
        );
        let n = plane_normal.normalize();
        // Householder reflection R = I − 2 n nᵀ, affine about plane_origin.
        let r = DMat3::from_cols(
            DVec3::X - 2.0 * n.x * n,
            DVec3::Y - 2.0 * n.y * n,
            DVec3::Z - 2.0 * n.z * n,
        );
        let m = DMat4::from_translation(plane_origin - r * plane_origin) * DMat4::from_mat3(r);
        self.replicate_edges(edge_ids, &[m])
    }

    /// **ADR-214 LINEAR ARRAY (edges)** — copy `edge_ids` `count` times; copy
    /// `k` (1..=count) translated by `offset · k`. Returns new edge ids.
    pub fn array_linear_edges(
        &mut self,
        edge_ids: &[EdgeId],
        count: u32,
        offset: DVec3,
    ) -> Result<Vec<EdgeId>> {
        ensure!(count >= 1, "array_linear edges: count must be ≥ 1");
        ensure!(
            offset.length_squared() > EPSILON_LENGTH * EPSILON_LENGTH,
            "array_linear edges: offset must be non-zero"
        );
        let transforms: Vec<DMat4> = (1..=count)
            .map(|k| DMat4::from_translation(offset * (k as f64)))
            .collect();
        self.replicate_edges(edge_ids, &transforms)
    }

    /// **ADR-214 RADIAL ARRAY (edges)** — copy `edge_ids` `count` times; copy
    /// `k` rotated by `total_angle · k / count` about `axis_origin`/`axis_dir`.
    pub fn array_radial_edges(
        &mut self,
        edge_ids: &[EdgeId],
        count: u32,
        axis_origin: DVec3,
        axis_dir: DVec3,
        total_angle: f64,
    ) -> Result<Vec<EdgeId>> {
        ensure!(count >= 1, "array_radial edges: count must be ≥ 1");
        ensure!(
            axis_dir.length_squared() > EPSILON_LENGTH * EPSILON_LENGTH,
            "array_radial edges: axis must be non-zero"
        );
        let axis = axis_dir.normalize();
        let to_origin = DMat4::from_translation(-axis_origin);
        let from_origin = DMat4::from_translation(axis_origin);
        let transforms: Vec<DMat4> = (1..=count)
            .map(|k| {
                let ang = total_angle * (k as f64) / (count as f64);
                from_origin * DMat4::from_axis_angle(axis, ang) * to_origin
            })
            .collect();
        self.replicate_edges(edge_ids, &transforms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::Mesh;
    use glam::DVec3;

    fn wire(mesh: &mut Mesh, a: DVec3, b: DVec3) -> EdgeId {
        let va = mesh.add_vertex(a);
        let vb = mesh.add_vertex(b);
        mesh.add_edge(va, vb).expect("add_edge").0
    }

    fn near(p: DVec3, q: DVec3) -> bool {
        p.distance(q) < 1e-6
    }

    /// ADR-211 de-risk: EXTEND a short wire edge to meet a boundary line.
    #[test]
    fn adr211_extend_wire_to_boundary_line() {
        let mut mesh = Mesh::new();
        // short target (0,0)→(5,0); boundary vertical segment at x=10
        let target = wire(&mut mesh, DVec3::new(0.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0));
        let boundary = wire(&mut mesh, DVec3::new(10.0, -5.0, 0.0), DVec3::new(10.0, 5.0, 0.0));

        mesh.extend_edge_to_boundary(target, boundary).expect("extend");

        // target id preserved (move_vertex is geometry-only)
        let (pa, pb, _, _) = mesh.edge_endpoints_pos(target).unwrap();
        let pts = [pa, pb];
        assert!(pts.iter().any(|p| near(*p, DVec3::ZERO)), "fixed endpoint stays");
        assert!(
            pts.iter().any(|p| near(*p, DVec3::new(10.0, 0.0, 0.0))),
            "moved endpoint reaches boundary line"
        );
    }

    /// Guard: EXTEND rejects a target whose interior already crosses boundary.
    #[test]
    fn adr211_extend_rejects_interior_crossing() {
        let mut mesh = Mesh::new();
        // target crosses boundary at its midpoint (s≈0.5) ⇒ nothing to extend
        let target = wire(&mut mesh, DVec3::new(-10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0));
        let boundary = wire(&mut mesh, DVec3::new(0.0, -5.0, 0.0), DVec3::new(0.0, 5.0, 0.0));
        assert!(mesh.extend_edge_to_boundary(target, boundary).is_err());
    }

    /// Guard: parallel edges never meet → extend errors cleanly.
    #[test]
    fn adr211_extend_parallel_edges_reject() {
        let mut mesh = Mesh::new();
        let target = wire(&mut mesh, DVec3::new(0.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 0.0));
        let boundary = wire(&mut mesh, DVec3::new(0.0, 5.0, 0.0), DVec3::new(10.0, 5.0, 0.0));
        assert!(mesh.extend_edge_to_boundary(target, boundary).is_err());
    }

    /// Guard: skew (non-coplanar) boundary line does not extend.
    #[test]
    fn adr211_extend_skew_lines_reject() {
        let mut mesh = Mesh::new();
        // target in z=0; boundary lifted to z=5 ⇒ lines never meet
        let target = wire(&mut mesh, DVec3::new(0.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 0.0));
        let boundary = wire(&mut mesh, DVec3::new(10.0, -5.0, 5.0), DVec3::new(10.0, 5.0, 5.0));
        assert!(mesh.extend_edge_to_boundary(target, boundary).is_err());
    }

    // ── C2: Fillet / Chamfer 2D corner ──────────────────────────────────

    /// Build a 90° L-corner at the origin: two length-10 wire edges sharing V.
    fn l_corner(mesh: &mut Mesh) -> VertId {
        let v = mesh.add_vertex(DVec3::ZERO);
        let a = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let b = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        mesh.add_edge(v, a).expect("e1");
        mesh.add_edge(v, b).expect("e2");
        v
    }

    /// ADR-211 C2 de-risk: CHAMFER an L-corner with a straight line.
    #[test]
    fn adr211_c2_chamfer_l_corner() {
        let mut mesh = Mesh::new();
        let v = l_corner(&mut mesh);
        assert_eq!(mesh.count_incident_edges(v), 2);

        let e = mesh.chamfer_corner_2d(v, 3.0).expect("chamfer");

        let (pa, pb, _, _) = mesh.edge_endpoints_pos(e).unwrap();
        let pts = [pa, pb];
        assert!(pts.iter().any(|p| near(*p, DVec3::new(3.0, 0.0, 0.0))));
        assert!(pts.iter().any(|p| near(*p, DVec3::new(0.0, 3.0, 0.0))));
        assert!(mesh.edge_curve(e).is_none(), "chamfer is a straight line");
        assert_eq!(mesh.count_incident_edges(v), 0, "corner stubs removed");
    }

    /// ADR-211 C2 de-risk: FILLET an L-corner with a tangent arc.
    #[test]
    fn adr211_c2_fillet_l_corner() {
        let mut mesh = Mesh::new();
        let v = l_corner(&mut mesh);

        let e = mesh.fillet_corner_2d(v, 3.0).expect("fillet");

        let (pa, pb, _, _) = mesh.edge_endpoints_pos(e).unwrap();
        let pts = [pa, pb];
        // t = R/tan(45°) = 3 → trim points (3,0,0) and (0,3,0)
        assert!(pts.iter().any(|p| near(*p, DVec3::new(3.0, 0.0, 0.0))));
        assert!(pts.iter().any(|p| near(*p, DVec3::new(0.0, 3.0, 0.0))));

        match mesh.edge_curve(e).expect("arc curve attached") {
            crate::curves::AnalyticCurve::Arc { center, radius, .. } => {
                assert!((*radius - 3.0).abs() < 1e-9, "radius = R");
                // center on the bisector: (3,3,0) for a 90° corner
                assert!(center.distance(DVec3::new(3.0, 3.0, 0.0)) < 1e-6, "arc center");
                // tangency: center is exactly `radius` from each trim point
                assert!((center.distance(DVec3::new(3.0, 0.0, 0.0)) - 3.0).abs() < 1e-6);
                assert!((center.distance(DVec3::new(0.0, 3.0, 0.0)) - 3.0).abs() < 1e-6);
            }
            _ => panic!("expected an Arc curve"),
        }
        assert_eq!(mesh.count_incident_edges(v), 0, "corner stubs removed");
    }

    /// Guard: a valence-3 junction is not a corner.
    #[test]
    fn adr211_c2_non_corner_reject() {
        let mut mesh = Mesh::new();
        let v = mesh.add_vertex(DVec3::ZERO);
        for p in [
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(0.0, 10.0, 0.0),
            DVec3::new(-10.0, 0.0, 0.0),
        ] {
            let w = mesh.add_vertex(p);
            mesh.add_edge(v, w).unwrap();
        }
        assert!(mesh.chamfer_corner_2d(v, 3.0).is_err());
        assert!(mesh.fillet_corner_2d(v, 3.0).is_err());
    }

    /// Guard: radius too large for the corner edges is rejected.
    #[test]
    fn adr211_c2_fillet_radius_too_large_reject() {
        let mut mesh = Mesh::new();
        let v = l_corner(&mut mesh); // edges length 10; R=20 ⇒ t=20 > 10
        assert!(mesh.fillet_corner_2d(v, 20.0).is_err());
    }

    /// Guard: collinear (straight) edges have no corner to round/cut.
    #[test]
    fn adr211_c2_collinear_reject() {
        let mut mesh = Mesh::new();
        let v = mesh.add_vertex(DVec3::ZERO);
        let a = mesh.add_vertex(DVec3::new(-10.0, 0.0, 0.0));
        let b = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        mesh.add_edge(v, a).unwrap();
        mesh.add_edge(v, b).unwrap();
        assert!(mesh.fillet_corner_2d(v, 3.0).is_err());
        assert!(mesh.chamfer_corner_2d(v, 3.0).is_err());
    }

    // ── C3: Join (collinear merge) — ADR-213 ────────────────────────────

    /// Build two collinear straight segments A—V—B sharing the midpoint V.
    fn collinear_pair(mesh: &mut Mesh) -> VertId {
        let a = mesh.add_vertex(DVec3::new(-5.0, 0.0, 0.0));
        let v = mesh.add_vertex(DVec3::ZERO);
        let b = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));
        mesh.add_edge(a, v).expect("e1");
        mesh.add_edge(v, b).expect("e2");
        v
    }

    /// ADR-213 de-risk: JOIN merges two collinear edges into one, dissolving V.
    #[test]
    fn adr213_join_collinear_merges() {
        let mut mesh = Mesh::new();
        let v = collinear_pair(&mut mesh);
        assert_eq!(mesh.count_incident_edges(v), 2);

        let merged = mesh.join_collinear_at(v).expect("join");

        let (pa, pb, _, _) = mesh.edge_endpoints_pos(merged).unwrap();
        let pts = [pa, pb];
        assert!(pts.iter().any(|p| near(*p, DVec3::new(-5.0, 0.0, 0.0))));
        assert!(pts.iter().any(|p| near(*p, DVec3::new(5.0, 0.0, 0.0))));
        assert!(mesh.edge_curve(merged).is_none(), "merged edge is a line");
        assert_eq!(mesh.count_incident_edges(v), 0, "shared vertex dissolved");
    }

    /// Guard: a real (non-collinear) corner is not merged.
    #[test]
    fn adr213_join_rejects_non_collinear() {
        let mut mesh = Mesh::new();
        let v = l_corner(&mut mesh); // 90° corner
        assert!(mesh.join_collinear_at(v).is_err());
    }

    /// Guard: an edge carrying an analytic curve is not collinear-merged.
    #[test]
    fn adr213_join_rejects_curved() {
        let mut mesh = Mesh::new();
        let a = mesh.add_vertex(DVec3::new(-5.0, 0.0, 0.0));
        let v = mesh.add_vertex(DVec3::ZERO);
        let b = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));
        let arc = crate::curves::AnalyticCurve::Arc {
            center: DVec3::new(-2.5, 5.0, 0.0),
            radius: 5.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: 0.2,
        };
        mesh.add_edge_with_curve(a, v, arc).expect("e1 curved");
        mesh.add_edge(v, b).expect("e2");
        assert!(mesh.join_collinear_at(v).is_err());
    }

    /// Guard: a valence-3 junction is not a join candidate.
    #[test]
    fn adr213_join_rejects_non_valence2() {
        let mut mesh = Mesh::new();
        let v = mesh.add_vertex(DVec3::ZERO);
        for p in [
            DVec3::new(5.0, 0.0, 0.0),
            DVec3::new(-5.0, 0.0, 0.0),
            DVec3::new(0.0, 5.0, 0.0),
        ] {
            let w = mesh.add_vertex(p);
            mesh.add_edge(v, w).unwrap();
        }
        assert!(mesh.join_collinear_at(v).is_err());
    }

    // ── C3 part 2: Edge Mirror / Array — ADR-214 ────────────────────────

    /// ADR-214 de-risk: MIRROR a wire edge across the YZ plane (x = 0).
    #[test]
    fn adr214_mirror_line_edge() {
        let mut mesh = Mesh::new();
        let e = wire(&mut mesh, DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 10.0, 0.0));
        let copies = mesh.mirror_edges(&[e], DVec3::ZERO, DVec3::X).expect("mirror");
        assert_eq!(copies.len(), 1);
        let (pa, pb, _, _) = mesh.edge_endpoints_pos(copies[0]).unwrap();
        let pts = [pa, pb];
        assert!(pts.iter().any(|p| near(*p, DVec3::new(-5.0, 0.0, 0.0))));
        assert!(pts.iter().any(|p| near(*p, DVec3::new(-5.0, 10.0, 0.0))));
        // source untouched
        let (sa, sb, _, _) = mesh.edge_endpoints_pos(e).unwrap();
        assert!([sa, sb].iter().all(|p| (p.x - 5.0).abs() < 1e-9));
    }

    /// ADR-214 de-risk: LINEAR ARRAY of a wire edge.
    #[test]
    fn adr214_array_linear_line_edges() {
        let mut mesh = Mesh::new();
        let e = wire(&mut mesh, DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 5.0, 0.0));
        let copies = mesh
            .array_linear_edges(&[e], 2, DVec3::new(10.0, 0.0, 0.0))
            .expect("array linear");
        assert_eq!(copies.len(), 2);
        // copy 0 at x=10, copy 1 at x=20
        let (a0, b0, _, _) = mesh.edge_endpoints_pos(copies[0]).unwrap();
        assert!([a0, b0].iter().all(|p| (p.x - 10.0).abs() < 1e-9));
        let (a1, b1, _, _) = mesh.edge_endpoints_pos(copies[1]).unwrap();
        assert!([a1, b1].iter().all(|p| (p.x - 20.0).abs() < 1e-9));
    }

    /// ADR-214 de-risk: RADIAL ARRAY of a wire edge around the Z axis.
    #[test]
    fn adr214_array_radial_line_edges() {
        let mut mesh = Mesh::new();
        let e = wire(&mut mesh, DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 5.0, 0.0));
        let copies = mesh
            .array_radial_edges(&[e], 4, DVec3::ZERO, DVec3::Z, std::f64::consts::TAU)
            .expect("array radial");
        assert_eq!(copies.len(), 4);
        // copy 0 = 90° rotation: (10,0,0) → (0,10,0)
        let (a0, b0, _, _) = mesh.edge_endpoints_pos(copies[0]).unwrap();
        assert!([a0, b0].iter().any(|p| near(*p, DVec3::new(0.0, 10.0, 0.0))));
    }

    /// ADR-214 de-risk: linear array preserves a transformed analytic curve.
    #[test]
    fn adr214_array_linear_preserves_arc_curve() {
        let mut mesh = Mesh::new();
        let va = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let vb = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));
        let arc = crate::curves::AnalyticCurve::Arc {
            center: DVec3::new(2.5, 3.0, 0.0),
            radius: 4.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: 1.0,
        };
        let e = mesh.add_edge_with_curve(va, vb, arc).expect("arc edge");
        let copies = mesh
            .array_linear_edges(&[e], 1, DVec3::new(10.0, 0.0, 0.0))
            .expect("array");
        match mesh.edge_curve(copies[0]).expect("curve preserved") {
            crate::curves::AnalyticCurve::Arc { center, radius, .. } => {
                assert!((*radius - 4.0).abs() < 1e-9);
                // center translated by +10 in x
                assert!(center.distance(DVec3::new(12.5, 3.0, 0.0)) < 1e-6);
            }
            _ => panic!("expected Arc preserved"),
        }
    }

    /// Guard: empty input + degenerate transform params.
    #[test]
    fn adr214_edge_transform_guards() {
        let mut mesh = Mesh::new();
        let e = wire(&mut mesh, DVec3::ZERO, DVec3::new(5.0, 0.0, 0.0));
        // empty selection → empty result
        assert!(mesh.mirror_edges(&[], DVec3::ZERO, DVec3::X).unwrap().is_empty());
        // zero offset / zero normal / zero axis → error
        assert!(mesh.array_linear_edges(&[e], 2, DVec3::ZERO).is_err());
        assert!(mesh.mirror_edges(&[e], DVec3::ZERO, DVec3::ZERO).is_err());
        assert!(mesh
            .array_radial_edges(&[e], 3, DVec3::ZERO, DVec3::ZERO, 1.0)
            .is_err());
    }
}
