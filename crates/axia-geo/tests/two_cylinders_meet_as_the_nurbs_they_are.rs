//! What the rational SSI path buys, measured on the case that needed it.
//!
//! ## The gap this closes
//!
//! `cylinder_to_nurbs_surface` (PR #249) writes a cylinder as the rational
//! NURBS it already is. Nothing could consume it: every SSI entry point in this
//! crate ran on control grids with no weights at all — `grep -i weight` over
//! `surfaces/ssi/` returned **0**, `extract_bezier_patches` takes
//! `&[Vec<DVec3>]`, and `nurbs_wrapper`'s own header said "**Non-rational
//! only** … Rational NURBS surfaces need 4D-lifted Bezier extraction (separate
//! work item)". `boolean_dispatch.rs` refuses `NURBSSurface(rational)` citing
//! the same line.
//!
//! So the promotion had no caller it *could* have. A circle is a rational
//! quadratic — corner weights √2/2 — so dropping the weights does not
//! approximate the cylinder, it intersects a different surface.
//!
//! ## What was measured, 2026-09-05
//!
//! Two cylinders of radius `r`, axes meeting at the origin at 90° (A along Z,
//! B along X), each spanning ±100 mm. `tol = 0.1`.
//!
//! ```text
//!                        closed form (#103)          rational SSI (new)
//!   equal radii 40,40    2 closed branches, 129 pts  2 chains, 348 pts
//!                                                    on A / on B  2.81e-5
//!                                                    | |z|-|x| |  5.95e-5
//!
//!   unequal     40,25    1 branch, 0 points,         2 chains, 140 pts
//!                        tangent_warning = true      on A 3.91e-5
//!                                                    on B 1.47e-5
//!                                                    | |z|-|x| |  29.24
//! ```
//!
//! ⚠ The last two numbers are the evidence, and they point opposite ways on
//! purpose. For EQUAL radii the intersection is exactly two ellipses lying in
//! the bisector planes `z = ±x`, so `| |z|-|x| |` must be ~0 — and it is, to
//! 6e-5. For UNEQUAL radii `cylinder_cylinder_branches`'s own doc records that
//! the true curve "leaves either bisector plane by up to **29.194** mm", which
//! is why it defers. The SSI, told nothing of that, reaches **29.24**. Two
//! independent computations of the same quantity.
//!
//! That deferral is cross-bore step 3 (`project-cross-bore-kernel`): unequal
//! radii are a genuine quartic with no plane decomposition, and Stage 2
//! subdivision is what the closed form points at. This is that Stage 2, now
//! reachable from an `AnalyticSurface::Cylinder`.
//!
//! ## What this file deliberately does NOT claim
//!
//! **That two closed loops come back.** They do not — `assemble_chains` links
//! points within `tol * 100`, and at `tol = 0.1` that yields two OPEN chains of
//! the right size, while at `tol = 0.01` it shatters into ~124 fragments of 1-6
//! points each even though the points themselves get *more* accurate (4.6e-6).
//! Chain assembly is the same code the polynomial path has always used and is
//! untouched here. The claim is about the POINTS: they lie on both surfaces and
//! converge as the tolerance tightens.
//!
//! Nor does it claim a Boolean works. Routing `boolean_dispatch` at
//! `NURBSSurface(rational)` and `Cylinder` is a separate decision — it changes
//! what Boolean accepts, which is a large pinned surface.

use axia_geo::surfaces::ssi::{analytic, nurbs_wrapper};
use axia_geo::surfaces::{cylinder, nurbs_surface, AnalyticSurface};
use glam::DVec3;

fn cyl(axis: DVec3, ref_dir: DVec3, radius: f64, half_length: f64) -> AnalyticSurface {
    AnalyticSurface::Cylinder {
        axis_origin: DVec3::ZERO,
        axis_dir: axis,
        radius,
        ref_dir,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-half_length, half_length),
    }
}

type Nurbs = (Vec<Vec<DVec3>>, Vec<Vec<f64>>, Vec<f64>, Vec<f64>, usize, usize);

fn promoted(s: &AnalyticSurface) -> Nurbs {
    let p = cylinder::cylinder_to_nurbs_surface(s).expect("a full cylinder promotes");
    let AnalyticSurface::NURBSSurface {
        ctrl_grid, weights, knots_u, knots_v, deg_u, deg_v, ..
    } = p
    else {
        panic!("the promotion stopped returning a NURBSSurface");
    };
    (ctrl_grid, weights, knots_u, knots_v, deg_u as usize, deg_v as usize)
}

/// Distance from the Z axis, minus `r`.
fn off_a(p: DVec3, r: f64) -> f64 {
    ((p.x * p.x + p.y * p.y).sqrt() - r).abs()
}

/// Distance from the X axis, minus `r`.
fn off_b(p: DVec3, r: f64) -> f64 {
    ((p.y * p.y + p.z * p.z).sqrt() - r).abs()
}

/// The extraction is exact, and it is the shape the promotion implies: a full
/// turn is four quarter-turn patches, each a 3x2 rational quadratic-by-linear
/// grid whose middle column carries the circle's √2/2.
#[test]
fn a_promoted_cylinder_splits_into_four_exact_rational_patches() {
    let (ctrl, w, ku, kv, du, dv) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 50.0));
    let patches = nurbs_surface::extract_rational_bezier_patches(&ctrl, &w, &ku, &kv, du, dv)
        .expect("a promoted cylinder extracts");

    assert_eq!(patches.len(), 4, "one patch per quarter turn");
    let corner = std::f64::consts::FRAC_1_SQRT_2;
    for (i, patch) in patches.iter().enumerate() {
        assert_eq!(patch.ctrl.len(), 3, "patch {i}: deg_u 2 means 3 rows");
        assert_eq!(patch.ctrl[0].len(), 2, "patch {i}: deg_v 1 means 2 columns");
        for col in 0..2 {
            assert!(
                (patch.weights[0][col] - 1.0).abs() < 1e-15
                    && (patch.weights[1][col] - corner).abs() < 1e-15
                    && (patch.weights[2][col] - 1.0).abs() < 1e-15,
                "patch {i} column {col} weights {:?} are not a rational quarter circle",
                (patch.weights[0][col], patch.weights[1][col], patch.weights[2][col])
            );
        }
    }

    // Every patch, sampled, is ON the cylinder — not near it. A Bezier patch is
    // a NURBS with clamped end knots, so the existing evaluator reads it back.
    let bez_u = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let bez_v = [0.0, 0.0, 1.0, 1.0];
    let mut worst_radial: f64 = 0.0;
    let mut worst_axial: f64 = 0.0;
    for patch in &patches {
        for su in 0..=8 {
            for sv in 0..=4 {
                let (u, v) = (su as f64 / 8.0, sv as f64 / 4.0);
                let p = nurbs_surface::evaluate(
                    &patch.ctrl, &patch.weights, &bez_u, &bez_v, 2, 1, u, v,
                )
                .expect("patch evaluates");
                worst_radial = worst_radial.max(off_a(p, 40.0));
                // v spans the cylinder's own v_range, here -50 .. +50.
                worst_axial = worst_axial.max((p.z - (-50.0 + v * 100.0)).abs());
            }
        }
    }
    assert!(
        worst_radial < 1e-12,
        "radial error {worst_radial:e} — the extraction is meant to be exact, not a fit"
    );
    assert!(worst_axial < 1e-12, "axial error {worst_axial:e}");
}

/// The control, and it is load-bearing: the closed form ANSWERS for equal radii
/// and DEFERS for unequal ones. Without this, the test below could pass by
/// agreeing with something that was never asked.
#[test]
fn the_closed_form_answers_equal_radii_and_defers_on_unequal() {
    let equal = analytic::cylinder_cylinder_branches(
        DVec3::ZERO, DVec3::Z, 40.0, DVec3::ZERO, DVec3::X, 40.0, 128, 100.0,
    );
    assert_eq!(equal.len(), 2, "equal radii are two ellipses");
    for br in &equal {
        assert!(br.closed, "each ellipse is closed");
        assert!(!br.tangent_warning);
        assert!(br.points.len() > 100);
    }

    let unequal = analytic::cylinder_cylinder_branches(
        DVec3::ZERO, DVec3::Z, 40.0, DVec3::ZERO, DVec3::X, 25.0, 128, 100.0,
    );
    let deferred = unequal.iter().all(|b| b.points.is_empty() && b.tangent_warning);
    assert!(
        deferred,
        "the closed form is supposed to hand unequal radii to Stage 2, but it \
         returned {} branch(es) with points {:?} — if it learned to answer them, \
         this file's premise changed",
        unequal.len(),
        unequal.iter().map(|b| b.points.len()).collect::<Vec<_>>()
    );
}

/// Equal radii: the rational SSI lands on the two bisector planes the closed
/// form derives, without being told they exist.
#[test]
fn equal_radii_agree_with_the_bisector_planes() {
    let (ca, wa, kua, kva, dua, dva) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 100.0));
    let (cb, wb, kub, kvb, dub, dvb) = promoted(&cyl(DVec3::X, DVec3::Y, 40.0, 100.0));

    let chains = nurbs_wrapper::intersect_rational_bspline_pair(
        &ca, &wa, &kua, &kva, dua, dva, &cb, &wb, &kub, &kvb, dub, dvb, 0.1,
    )
    .expect("the rational pair intersects");

    assert_eq!(chains.len(), 2, "two ellipses");
    let pts: Vec<DVec3> = chains.iter().flat_map(|c| c.points.iter().copied()).collect();
    assert!(pts.len() > 300, "only {} points on a 710 mm curve", pts.len());

    let mut worst_surface: f64 = 0.0;
    let mut worst_plane: f64 = 0.0;
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        worst_surface = worst_surface.max(off_a(*p, 40.0)).max(off_b(*p, 40.0));
        worst_plane = worst_plane.max((p.z.abs() - p.x.abs()).abs());
        ymin = ymin.min(p.y);
        ymax = ymax.max(p.y);
    }
    assert!(worst_surface < 1e-3, "off the cylinders by {worst_surface:e}");
    assert!(
        worst_plane < 1e-3,
        "|z| - |x| reaches {worst_plane:e}; equal radii must lie on z = ±x"
    );
    // The curve is walked end to end — the extremes are the tangent pinch
    // points at y = ±r, which subdivision reaches last.
    assert!(ymin < -39.0 && ymax > 39.0, "y span [{ymin}, {ymax}] misses the ends");
}

/// Unequal radii: the case the closed form hands over. The answer is NOT on
/// either bisector plane, which is the whole reason it could not be answered
/// in closed form.
#[test]
fn unequal_radii_are_answered_where_the_closed_form_stops() {
    let (ca, wa, kua, kva, dua, dva) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 100.0));
    let (cb, wb, kub, kvb, dub, dvb) = promoted(&cyl(DVec3::X, DVec3::Y, 25.0, 100.0));

    let chains = nurbs_wrapper::intersect_rational_bspline_pair(
        &ca, &wa, &kua, &kva, dua, dva, &cb, &wb, &kub, &kvb, dub, dvb, 0.1,
    )
    .expect("the rational pair intersects");

    assert_eq!(chains.len(), 2, "the smaller tube cuts two loops");
    let pts: Vec<DVec3> = chains.iter().flat_map(|c| c.points.iter().copied()).collect();
    assert!(pts.len() > 100, "only {} points", pts.len());

    let mut worst_surface: f64 = 0.0;
    let mut worst_plane: f64 = 0.0;
    let (mut ymin, mut ymax) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in &pts {
        worst_surface = worst_surface.max(off_a(*p, 40.0)).max(off_b(*p, 25.0));
        worst_plane = worst_plane.max((p.z.abs() - p.x.abs()).abs());
        ymin = ymin.min(p.y);
        ymax = ymax.max(p.y);
    }
    assert!(worst_surface < 1e-3, "off the cylinders by {worst_surface:e}");
    assert!(
        worst_plane > 25.0,
        "|z| - |x| only reaches {worst_plane:e}; the closed form's own note says \
         the true curve leaves either bisector plane by up to 29.194 mm, so a \
         small value here means the planes were assumed, not measured"
    );
    assert!(ymin < -24.0 && ymax > 24.0, "y span [{ymin}, {ymax}] misses the ends");
}

/// Tightening the tolerance moves the points onto the surfaces. A rational path
/// that quietly dropped its weights would sit at a fixed offset instead.
#[test]
fn the_points_converge_as_the_tolerance_tightens() {
    let (ca, wa, kua, kva, dua, dva) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 100.0));
    let (cb, wb, kub, kvb, dub, dvb) = promoted(&cyl(DVec3::X, DVec3::Y, 40.0, 100.0));

    let worst_at = |tol: f64| -> f64 {
        let chains = nurbs_wrapper::intersect_rational_bspline_pair(
            &ca, &wa, &kua, &kva, dua, dva, &cb, &wb, &kub, &kvb, dub, dvb, tol,
        )
        .expect("intersects");
        chains
            .iter()
            .flat_map(|c| c.points.iter())
            .fold(0.0_f64, |acc, p| acc.max(off_a(*p, 40.0)).max(off_b(*p, 40.0)))
    };

    let (coarse, medium, fine) = (worst_at(1.0), worst_at(0.1), worst_at(0.01));
    assert!(
        coarse > medium && medium > fine,
        "worst deviation should shrink with tol: 1.0 -> {coarse:e}, \
         0.1 -> {medium:e}, 0.01 -> {fine:e}"
    );
    assert!(fine < 1e-5, "at tol 0.01 the points are {fine:e} off");
}
