//! A CYLINDER IS EXACTLY A RATIONAL NURBS, AND NOW IT CAN SAY SO.
//!
//! Every SSI entry point in this engine is control-grid based
//! (`intersect_bezier_pair`, `subdivide_intersect`,
//! `nurbs_wrapper::intersect_bspline_pair`) and none of them takes an
//! `AnalyticSurface::Cylinder`. That is what stops crossing bores of UNEQUAL
//! radii: equal radii meet in two ellipses and decompose into planes, unequal
//! ones meet in a genuine quartic space curve, so the general route has to run —
//! and it had nothing to chew on. Measured before this: a grep for a
//! Cylinder→NURBS promotion returned 0 files.
//!
//! Nothing new is computed. A circle is exactly a degree-2 rational B-spline
//! (nine control points, corner weights √2/2) which `nurbs::ellipse` already
//! builds, and a straight sweep of a rational profile is
//! `sweep::extrusion_surface_nurbs`, which replicates the weights at v = 0 and
//! v = 1. The promotion joins the two, so what this file really asserts is that
//! the join is EXACT — not a fit, not a tessellation.
//!
//! ⚠ The two parameterisations are NOT the same. The NURBS `u ∈ [0,4]` is the
//! circle's knot parameter, one unit per quarter turn, and it is not linear in
//! angle within a quarter; `v ∈ [0,1]` spans the cylinder's `v_range`. So the
//! test compares SETS — every promoted point lies on the cylinder — plus the
//! four exact corners where the two do agree.
use axia_geo::surfaces::{cylinder, AnalyticSurface};
use glam::DVec3;

fn a_cylinder(radius: f64, height: f64) -> AnalyticSurface {
    AnalyticSurface::Cylinder {
        axis_origin: DVec3::new(3.0, -2.0, 1.0),
        // Deliberately not an axis direction: a promotion that quietly assumed
        // +Z would pass on a Z-up cylinder and fail here.
        axis_dir: DVec3::new(1.0, 2.0, 3.0).normalize(),
        radius,
        ref_dir: DVec3::new(2.0, -1.0, 0.0).normalize(),
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-4.0, -4.0 + height),
    }
}

#[test]
fn every_point_of_the_promotion_is_on_the_cylinder() {
    let r = 7.5;
    let h = 11.0;
    let cyl = a_cylinder(r, h);
    let promoted = cylinder::cylinder_to_nurbs_surface(&cyl).expect("a full cylinder promotes");

    let AnalyticSurface::NURBSSurface {
        ctrl_grid, weights, knots_u, knots_v, deg_u, deg_v, ..
    } = &promoted else { panic!("expected a NURBS surface, got {promoted:?}") };
    assert_eq!((*deg_u, *deg_v), (2, 1), "degree-2 around, degree-1 along");

    let AnalyticSurface::Cylinder { axis_origin, axis_dir, v_range, .. } = &cyl
    else { unreachable!() };
    let axis = axis_dir.normalize();

    // Distance to the axis is the radius, and the axial coordinate is inside the
    // span — that IS the cylinder, stated without needing the parameterisations
    // to line up.
    let mut worst_r: f64 = 0.0;
    let mut worst_axial: f64 = 0.0;
    for i in 0..=40 {
        let u = 4.0 * (i as f64) / 40.0;
        for j in 0..=8 {
            let v = (j as f64) / 8.0;
            let p = axia_geo::surfaces::nurbs_surface::evaluate(
                ctrl_grid, weights, knots_u, knots_v, *deg_u as usize, *deg_v as usize, u, v,
            )
            .expect("the promoted surface evaluates");
            let d = p - *axis_origin;
            let along = d.dot(axis);
            let radial = (d - axis * along).length();
            worst_r = worst_r.max((radial - r).abs());
            // v = 0 is v_range.0 and v = 1 is v_range.1, linearly.
            let want_along = v_range.0 + v * (v_range.1 - v_range.0);
            worst_axial = worst_axial.max((along - want_along).abs());
        }
    }
    println!("  worst radial error {worst_r:.3e}   worst axial error {worst_axial:.3e}");
    // Exact, not fitted. A tessellated stand-in would be off by the sagitta —
    // r(1−cos(π/8)) ≈ 0.057 mm here, a billion times this bound.
    assert!(worst_r < 1e-9, "radial error {worst_r:.3e} — this is not the cylinder");
    assert!(worst_axial < 1e-9, "axial error {worst_axial:.3e}");
}

#[test]
fn the_four_quarter_points_land_exactly_where_the_cylinder_puts_them() {
    // Where the two parameterisations DO agree: u = 0,1,2,3 are the quarter
    // turns, so they must equal `Cylinder::evaluate` at 0, π/2, π, 3π/2.
    // Without this the test above would also pass on a cylinder of the right
    // radius that had been rotated about its own axis.
    let cyl = a_cylinder(5.0, 6.0);
    let promoted = cylinder::cylinder_to_nurbs_surface(&cyl).expect("promotes");
    let AnalyticSurface::NURBSSurface {
        ctrl_grid, weights, knots_u, knots_v, deg_u, deg_v, ..
    } = &promoted else { unreachable!() };
    let AnalyticSurface::Cylinder {
        axis_origin, axis_dir, radius, ref_dir, v_range, ..
    } = &cyl else { unreachable!() };

    for (k, angle) in [
        (0.0, 0.0),
        (1.0, std::f64::consts::FRAC_PI_2),
        (2.0, std::f64::consts::PI),
        (3.0, 1.5 * std::f64::consts::PI),
    ] {
        for (v, along) in [(0.0, v_range.0), (1.0, v_range.1)] {
            let got = axia_geo::surfaces::nurbs_surface::evaluate(
                ctrl_grid, weights, knots_u, knots_v, *deg_u as usize, *deg_v as usize, k, v,
            )
            .expect("evaluates");
            let want =
                cylinder::evaluate(*axis_origin, *axis_dir, *radius, *ref_dir, angle, along);
            assert!(
                (got - want).length() < 1e-9,
                "u={k} v={v}: got {got:?} want {want:?} (Δ {:.3e})",
                (got - want).length()
            );
        }
    }
}

#[test]
fn it_declines_what_it_cannot_represent() {
    // A trimmed cylinder is a different surface and the NURBS `u` is a knot
    // parameter, not an angle, so clipping the range would be a guess.
    let mut partial = a_cylinder(4.0, 5.0);
    if let AnalyticSurface::Cylinder { u_range, .. } = &mut partial {
        *u_range = (0.0, std::f64::consts::PI);
    }
    assert!(cylinder::cylinder_to_nurbs_surface(&partial).is_none(), "half a turn");

    let mut flat = a_cylinder(4.0, 0.0);
    if let AnalyticSurface::Cylinder { v_range, .. } = &mut flat {
        *v_range = (2.0, 2.0);
    }
    assert!(cylinder::cylinder_to_nurbs_surface(&flat).is_none(), "zero height");

    let zero_r = a_cylinder(0.0, 5.0);
    assert!(cylinder::cylinder_to_nurbs_surface(&zero_r).is_none(), "zero radius");

    // And it is a CYLINDER promotion — it says nothing about other surfaces.
    let sphere = AnalyticSurface::Sphere {
        center: DVec3::ZERO,
        radius: 3.0,
        axis_dir: DVec3::Z,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-1.0, 1.0),
    };
    assert!(cylinder::cylinder_to_nurbs_surface(&sphere).is_none(), "not a cylinder");
}
