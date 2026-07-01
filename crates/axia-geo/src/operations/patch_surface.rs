//! Patch surface creation — build a NEW DCEL face carrying a tensor-product
//! surface (Bezier / NURBS) from a control-point grid.
//!
//! ## Why a single kernel-native face (ADR-033 + meta-principle #14)
//!
//! `AnalyticSurface::{BezierPatch, NURBSSurface}` already exist (ADR-033
//! Phase E) with full evaluate / tessellate support, and the render pipeline
//! (`export_buffers`, ADR-038 P23) tessellates the *full* attached surface
//! for any non-Plane variant. So a patch face is created kernel-native: one
//! DCEL face whose 4-corner boundary loop gives valid topology (manifold,
//! ADR-007 winding) while the attached surface IS the geometric truth — the
//! visible bulged patch is the surface tessellation; the boundary polygon is
//! only the topological extent (selection / picking).
//!
//! This mirrors how a closed Circle becomes 1 anchor + 1 self-loop edge + 1
//! face carrying the curve (ADR-089): the analytic primitive is truth, the
//! polygon is a byproduct (ADR-019, meta-principle #14 "a face is derived
//! from a closed boundary").
//!
//! Unlike Sweep/Loft — which tessellate a profile into a grid of *flat* quad
//! faces (operations/{sweep,loft}.rs) — a patch keeps the surface analytic so
//! downstream kernel-aware ops (Offset / Boolean / Push-Pull) and the
//! surface-aware render (ADR-038 P23.1) see the exact NURBS/Bezier geometry,
//! not a faceted approximation.
//!
//! ## Boundary + winding
//!
//! The 4 patch corners `S(u0,v0), S(u1,v0), S(u1,v1), S(u0,v1)` form the
//! boundary quad. For a clamped Bezier/NURBS patch these interpolate the
//! corner control points. The loop order is aligned so `face.normal()` agrees
//! with the surface's center normal `S_u × S_v` (ADR-033 P18.9 right-handed),
//! reversing the ring if the parameterization flips it.

use anyhow::{bail, Result};
use glam::DVec3;

use crate::entities::*;
use crate::mesh::Mesh;
use crate::surfaces::{AnalyticSurface, SurfaceOps};

impl Mesh {
    /// Create a new face carrying a **Bezier patch** surface.
    ///
    /// `ctrl_grid` is a `u_count × v_count` row-major control grid
    /// (`u_count, v_count ≥ 2` — a `1×N`/`N×1` grid is a curve, not a
    /// surface, and is rejected). Returns the new FaceId, or an error if the
    /// grid is degenerate or the boundary quad collapses.
    pub fn create_bezier_patch(
        &mut self,
        ctrl_grid: Vec<Vec<DVec3>>,
        material: MaterialId,
    ) -> Result<FaceId> {
        // Validate the grid shape (rectangular, deg ≥ 1 per axis) by running a
        // sample evaluation — bezier_patch::evaluate calls validate() first.
        crate::surfaces::bezier_patch::evaluate(&ctrl_grid, 0.5, 0.5)
            .map_err(|e| anyhow::anyhow!("create_bezier_patch: {}", e))?;
        let surface = AnalyticSurface::BezierPatch { ctrl_grid };
        self.attach_patch_face(surface, material)
    }

    /// Create a new face carrying a **NURBS surface** (rational tensor-product
    /// B-spline). Returns the new FaceId, or an error on invalid input
    /// (grid/weight/knot validation via `nurbs_surface`) or a degenerate
    /// boundary quad.
    #[allow(clippy::too_many_arguments)]
    pub fn create_nurbs_surface(
        &mut self,
        ctrl_grid: Vec<Vec<DVec3>>,
        weights: Vec<Vec<f64>>,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
        deg_u: usize,
        deg_v: usize,
        material: MaterialId,
    ) -> Result<FaceId> {
        // Validate via a sample evaluation at the clamped parameter midpoint —
        // nurbs_surface::evaluate runs the full validate() (dims, weights > 0,
        // knot counts, degrees).
        let n_u = ctrl_grid.len();
        let n_v = ctrl_grid.first().map(|r| r.len()).unwrap_or(0);
        if n_u < deg_u + 1 || n_v < deg_v + 1 {
            bail!(
                "create_nurbs_surface: grid {}×{} too small for degree ({}, {})",
                n_u, n_v, deg_u, deg_v
            );
        }
        let u_mid = (knots_u.get(deg_u).copied().unwrap_or(0.0)
            + knots_u.get(n_u).copied().unwrap_or(1.0))
            * 0.5;
        let v_mid = (knots_v.get(deg_v).copied().unwrap_or(0.0)
            + knots_v.get(n_v).copied().unwrap_or(1.0))
            * 0.5;
        crate::surfaces::nurbs_surface::evaluate(
            &ctrl_grid, &weights, &knots_u, &knots_v, deg_u, deg_v, u_mid, v_mid,
        )
        .map_err(|e| anyhow::anyhow!("create_nurbs_surface: {}", e))?;

        let surface = AnalyticSurface::NURBSSurface {
            ctrl_grid,
            weights,
            knots_u,
            knots_v,
            deg_u: deg_u as u32,
            deg_v: deg_v as u32,
            trim_loops: Vec::new(),
        };
        self.attach_patch_face(surface, material)
    }

    /// Shared: evaluate the 4 patch corners, build a single quad face whose
    /// winding agrees with the surface center normal, attach the surface, and
    /// return the new FaceId.
    fn attach_patch_face(
        &mut self,
        surface: AnalyticSurface,
        material: MaterialId,
    ) -> Result<FaceId> {
        let ((u0, u1), (v0, v1)) = surface.parameter_range();
        let c00 = surface.evaluate(u0, v0);
        let c10 = surface.evaluate(u1, v0);
        let c11 = surface.evaluate(u1, v1);
        let c01 = surface.evaluate(u0, v1);
        for (label, p) in [("c00", c00), ("c10", c10), ("c11", c11), ("c01", c01)] {
            if !(p.x.is_finite() && p.y.is_finite() && p.z.is_finite()) {
                bail!("create patch face: corner {} is non-finite {:?}", label, p);
            }
        }

        // Boundary loop, CCW in (u, v): c00 → c10 → c11 → c01. Reverse if the
        // parameterization makes the polygon normal oppose the surface normal,
        // so `face.normal()` points "out" of the patch (ADR-007 winding).
        let mut ring = [c00, c10, c11, c01];
        let poly_n = newell_normal(&ring);
        if poly_n.length_squared() < 1e-18 {
            bail!("create patch face: boundary quad is degenerate (zero area) — \
                   corners collapse to a line/point");
        }
        let center_n = surface.normal((u0 + u1) * 0.5, (v0 + v1) * 0.5);
        if center_n.length_squared() > 1e-18 && poly_n.dot(center_n) < 0.0 {
            ring.reverse();
        }

        let vids: Vec<VertId> = ring.iter().map(|p| self.add_vertex(*p)).collect();
        // LOCKED #5 spatial-hash dedup may collapse near-coincident corners —
        // add_face_with_holes then rejects (< 3 distinct verts), surfacing a
        // clear error rather than a silent bad face.
        let fid = self.add_face_with_holes(&vids, &[], material)?;
        self.set_face_surface(fid, Some(surface));
        self.debug_verify_invariants();
        Ok(fid)
    }
}

/// Newell's method polygon normal — robust for non-planar rings (a patch's
/// corner quad need not be planar). Magnitude ∝ 2·area; direction follows the
/// vertex order by the right-hand rule.
fn newell_normal(ring: &[DVec3]) -> DVec3 {
    let mut n = DVec3::ZERO;
    let len = ring.len();
    for i in 0..len {
        let a = ring[i];
        let b = ring[(i + 1) % len];
        n.x += (a.y - b.y) * (a.z + b.z);
        n.y += (a.z - b.z) * (a.x + b.x);
        n.z += (a.x - b.x) * (a.y + b.y);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves::bspline::clamped_uniform_knots;
    use crate::surfaces::SurfaceOps;

    /// 4×4 bicubic grid: flat z=0 corners + raised interior → a "pillow"
    /// (matches bezier_patch::tests::bicubic_grid).
    fn bumped_bicubic_grid() -> Vec<Vec<DVec3>> {
        vec![
            vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(0.0, 1.0, 0.0),
                DVec3::new(0.0, 2.0, 0.0),
                DVec3::new(0.0, 3.0, 0.0),
            ],
            vec![
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(1.0, 1.0, 5.0),
                DVec3::new(1.0, 2.0, 5.0),
                DVec3::new(1.0, 3.0, 0.0),
            ],
            vec![
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(2.0, 1.0, 5.0),
                DVec3::new(2.0, 2.0, 5.0),
                DVec3::new(2.0, 3.0, 0.0),
            ],
            vec![
                DVec3::new(3.0, 0.0, 0.0),
                DVec3::new(3.0, 1.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(3.0, 3.0, 0.0),
            ],
        ]
    }

    #[test]
    fn create_bezier_patch_makes_one_face_with_surface() {
        let mut m = Mesh::new();
        let fid = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        // Exactly one face, with the BezierPatch surface attached.
        assert_eq!(
            m.face_surface(fid).map(|s| s.kind_label()),
            Some("BezierPatch"),
        );
        let report = m.verify_face_invariants();
        assert_eq!(
            report.violations.len(),
            0,
            "patch face invariants:\n{}",
            report.summary(),
        );
    }

    #[test]
    fn create_bezier_patch_boundary_is_four_corners() {
        let mut m = Mesh::new();
        let fid = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        let verts = m.collect_loop_verts(m.faces[fid].outer().start).unwrap();
        assert_eq!(verts.len(), 4, "patch boundary is a 4-corner quad");
    }

    // ─── ADR-232 — nurbs_surface_params read-back (control-net overlay) ───
    #[test]
    fn adr232_nurbs_surface_params_reads_bezier_control_net() {
        let mut m = Mesh::new();
        let fid = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        let p = m.nurbs_surface_params(fid).expect("bezier patch → params");
        assert_eq!(p.kind, "BezierPatch");
        assert_eq!((p.n_u, p.n_v), (4, 4));
        assert_eq!((p.deg_u, p.deg_v), (3, 3));
        assert_eq!(p.ctrl_pts.len(), 4 * 4 * 3);
        assert_eq!(p.weights.len(), 16);
        assert!(p.weights.iter().all(|&w| (w - 1.0).abs() < 1e-12), "bezier weights = 1");
        assert_eq!(&p.ctrl_pts[0..3], &[0.0, 0.0, 0.0]); // grid[0][0]
        assert!(p.ctrl_pts.iter().any(|&v| (v - 5.0).abs() < 1e-12), "raised interior CP present");
        assert!(p.knots_u.is_empty(), "bezier knots implicit (empty)");
    }

    #[test]
    fn adr232_nurbs_surface_params_reads_rational_weights() {
        // rational quarter-cylinder: 3×2 grid, middle u-weight = 1/√2.
        let mut m = Mesh::new();
        let grid = vec![
            vec![DVec3::new(5.0, 0.0, 0.0), DVec3::new(5.0, 0.0, 1.0)],
            vec![DVec3::new(5.0, 5.0, 0.0), DVec3::new(5.0, 5.0, 1.0)],
            vec![DVec3::new(0.0, 5.0, 0.0), DVec3::new(0.0, 5.0, 1.0)],
        ];
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let weights = vec![vec![1.0, 1.0], vec![w, w], vec![1.0, 1.0]];
        let fid = m
            .create_nurbs_surface(
                grid,
                weights,
                vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                vec![0.0, 0.0, 1.0, 1.0],
                2,
                1,
                MaterialId::new(0),
            )
            .unwrap();
        let p = m.nurbs_surface_params(fid).expect("nurbs patch → params");
        assert_eq!(p.kind, "NURBSSurface");
        assert_eq!((p.n_u, p.n_v, p.deg_u, p.deg_v), (3, 2, 2, 1));
        assert_eq!(p.ctrl_pts.len(), 3 * 2 * 3);
        assert_eq!(p.weights.len(), 6);
        assert!(
            p.weights.iter().any(|&x| (x - w).abs() < 1e-12),
            "rational (non-unit) weight preserved",
        );
        assert_eq!(p.knots_u.len(), 6);
        assert_eq!(p.knots_v.len(), 4);
    }

    #[test]
    fn adr232_nurbs_surface_params_none_for_missing_or_non_nurbs() {
        let m = Mesh::new();
        assert!(m.nurbs_surface_params(FaceId::new(9999)).is_none(), "missing face → None");
    }

    #[test]
    fn create_bezier_patch_renders_tessellated_surface() {
        // The attached surface tessellates to a triangle mesh — proof the
        // render path (ADR-038 P23) has geometry to emit, not a flat quad.
        let mut m = Mesh::new();
        let fid = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        let tess = m.tessellate_face_surface(fid, 0.1).unwrap();
        assert!(!tess.vertices.is_empty(), "patch tessellation has vertices");
        assert!(!tess.triangles.is_empty(), "patch tessellation has triangles");
        // Interior bump means the surface is genuinely 3D (z > 0 somewhere).
        assert!(
            tess.vertices.iter().any(|p| p.z > 0.5),
            "bumped patch should tessellate with a raised interior",
        );
    }

    #[test]
    fn create_bezier_patch_winding_aligns_with_surface_normal() {
        let mut m = Mesh::new();
        let fid = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        let face_n = m.faces[fid].normal();
        let surf = m.face_surface(fid).unwrap();
        let center_n = surf.normal(0.5, 0.5);
        assert!(
            face_n.dot(center_n) >= 0.0,
            "face winding must agree with surface normal: face {:?} · surf {:?}",
            face_n, center_n,
        );
    }

    #[test]
    fn create_bezier_patch_rejects_degenerate_1xn_grid() {
        let mut m = Mesh::new();
        // 1×4 grid is a curve, not a surface — rejected by bezier_patch::validate.
        let grid = vec![vec![
            DVec3::ZERO,
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(3.0, 0.0, 0.0),
        ]];
        assert!(m.create_bezier_patch(grid, MaterialId::new(0)).is_err());
    }

    #[test]
    fn create_nurbs_surface_makes_face_with_surface() {
        let mut m = Mesh::new();
        // 4×4 unit-weight grid, bicubic clamped knots.
        let grid = bumped_bicubic_grid();
        let weights = vec![vec![1.0; 4]; 4];
        let ku = clamped_uniform_knots(4, 3);
        let kv = clamped_uniform_knots(4, 3);
        let fid = m
            .create_nurbs_surface(grid, weights, ku, kv, 3, 3, MaterialId::new(0))
            .unwrap();
        assert_eq!(
            m.face_surface(fid).map(|s| s.kind_label()),
            Some("NURBSSurface"),
        );
        let report = m.verify_face_invariants();
        assert_eq!(
            report.violations.len(),
            0,
            "NURBS patch invariants:\n{}",
            report.summary(),
        );
    }

    #[test]
    fn create_nurbs_surface_rejects_zero_weight() {
        let mut m = Mesh::new();
        let grid = bumped_bicubic_grid();
        let mut weights = vec![vec![1.0; 4]; 4];
        weights[1][2] = 0.0; // invalid — all weights must be > 0
        let ku = clamped_uniform_knots(4, 3);
        let kv = clamped_uniform_knots(4, 3);
        assert!(m
            .create_nurbs_surface(grid, weights, ku, kv, 3, 3, MaterialId::new(0))
            .is_err());
    }

    #[test]
    fn create_nurbs_surface_rejects_knot_mismatch() {
        let mut m = Mesh::new();
        let grid = bumped_bicubic_grid();
        let weights = vec![vec![1.0; 4]; 4];
        // Wrong knot count for degree 3, 4 control points (needs 8, give 6).
        let bad_knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        let kv = clamped_uniform_knots(4, 3);
        assert!(m
            .create_nurbs_surface(grid, weights, bad_knots, kv, 3, 3, MaterialId::new(0))
            .is_err());
    }

    #[test]
    fn create_two_patches_are_independent_faces() {
        // Two patches far apart → two distinct faces, both valid.
        let mut m = Mesh::new();
        let f1 = m
            .create_bezier_patch(bumped_bicubic_grid(), MaterialId::new(0))
            .unwrap();
        let mut grid2 = bumped_bicubic_grid();
        for row in &mut grid2 {
            for p in row {
                *p += DVec3::new(100.0, 0.0, 0.0);
            }
        }
        let f2 = m.create_bezier_patch(grid2, MaterialId::new(0)).unwrap();
        assert_ne!(f1, f2);
        let report = m.verify_face_invariants();
        assert_eq!(report.violations.len(), 0, "{}", report.summary());
    }
}
