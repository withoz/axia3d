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
//!
//! ## Two measured limits, both older than this change
//!
//! **The recursion never terminates by smallness at these scales.** Every
//! candidate on every pair measured here came back `depth_capped` — a patch
//! spans a quarter turn and the cylinder's whole 200 mm length, so
//! `DEFAULT_MAX_DEPTH = 16` alternating splits leaves the box near 0.8 mm,
//! above any tolerance worth asking for. So `tol` here is not the termination
//! threshold it reads as; it controls how many seeds come out, and the accuracy
//! is Newton's. Raising the depth does improve it, monotonically:
//!
//! ```text
//!   worst deviation, r 40 x 8      depth 16   6.15e-3
//!                                  depth 22   4.20e-4
//!                                  depth 28   1.14e-4
//! ```
//!
//! **A much smaller tube is answered less well.** At depth 16, r 40 x 8 sits at
//! 6.1e-3 and r 40 x 4 at 3.3e-1, and in both cases exactly 8 points carry the
//! error while the rest are at 1e-5. Neither number moves with `tol`. Both are
//! the depth cap, which is the polynomial path's cap too — unchanged here, and
//! worth its own measurement before anyone relies on small-radius crossings.

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

/// Why the SPLIT has to happen in homogeneous coordinates, measured through
/// the constructor the pipeline actually calls.
///
/// ⚠ The end-to-end tests above do NOT hold this. Dropping the weights in
/// `PatchRegion::from_full_rational` and treating the patch as polynomial
/// leaves all five of them green — the subdivision only places candidate seeds
/// and Newton, still rational, pulls them onto the true surfaces anyway. So a
/// scene is the wrong instrument for this one.
///
/// What is at stake is the pruning box. A sub-region's box is trusted to
/// contain its piece of the surface; a branch whose box misses the other
/// patch's is discarded and never looked at again. Split the ORDINARY control
/// points with the polynomial de Casteljau and the resulting box does not
/// contain the surface: measured on a quarter of a r=40 cylinder, samples of
/// the true sub-patch fall outside by up to **1.7157 mm**, which is exactly
/// `40 - 40·cos 45°` — the arc's bulge past its chord, thrown away.
///
/// An intersection lying in that sliver would be pruned and the curve would
/// come back quietly incomplete.
#[test]
fn a_polynomial_split_of_a_rational_patch_loses_the_surface() {
    use axia_geo::surfaces::ssi::subdivide::PatchRegion;

    // A SHORT cylinder on purpose. `split_adaptive` picks the axis with the
    // longer chord, and on a long one that is v — the straight sweep, where a
    // polynomial split is exact because the weights are constant along it and
    // the degree is 1. The arc lives in u, so the test has to make u the longer
    // chord: quarter-arc chord is 40√2 ≈ 56.6, so 20 mm of length puts u first.
    let (ctrl, w, ku, kv, du, dv) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 10.0));
    let patches = nurbs_surface::extract_rational_bezier_patches(&ctrl, &w, &ku, &kv, du, dv)
        .expect("extracts");
    let patch = &patches[0];

    let bez_u = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
    let bez_v = [0.0, 0.0, 1.0, 1.0];

    // How far the true surface escapes a region's own box, over that region's
    // own uv rectangle. `uv_bounds` says which half the split produced, so this
    // does not have to know which axis `split_adaptive` chose.
    let escapes = |region: &PatchRegion| -> (usize, f64) {
        let ((u0, u1), (v0, v1)) = region.uv_bounds;
        let (mn, mx) = region.bbox();
        let (mut n, mut worst) = (0usize, 0.0_f64);
        for su in 0..=40 {
            for sv in 0..=8 {
                let u = u0 + (u1 - u0) * su as f64 / 40.0;
                let v = v0 + (v1 - v0) * sv as f64 / 8.0;
                let p =
                    nurbs_surface::evaluate(&patch.ctrl, &patch.weights, &bez_u, &bez_v, 2, 1, u, v)
                        .expect("evaluates");
                let d = (mn - p).max(p - mx).max(DVec3::ZERO).length();
                if d > 1e-12 {
                    n += 1;
                    worst = worst.max(d);
                }
            }
        }
        (n, worst)
    };

    // The path in use: weights carried, split in 4D.
    let rational = PatchRegion::from_full_rational(&patch.ctrl, &patch.weights);
    let (r_left, r_right) = rational.split_adaptive().expect("a 3x2 patch splits");
    // If the heuristic ever picks v here the control below goes quiet, because a
    // v-split of this patch IS exact polynomially. Say so rather than pass.
    let ((u0, u1), (v0, v1)) = r_left.uv_bounds;
    assert!(
        (u1 - u0 - 0.5).abs() < 1e-12 && (v1 - v0 - 1.0).abs() < 1e-12,
        "expected the arc direction (u) to be split first, got u [{u0}, {u1}]          v [{v0}, {v1}] — a v-split would make this test vacuous"
    );
    for (name, half) in [("left", &r_left), ("right", &r_right)] {
        let (n, worst) = escapes(half);
        assert_eq!(
            n, 0,
            "the {name} half's box must contain its piece of the surface, but              {n} samples escape by up to {worst:e} — the split stopped being              homogeneous"
        );
    }

    // The control: what dropping the weights would give. This half of the test
    // is what makes the half above mean something — if a polynomial split were
    // also sound, carrying the weights would be decoration.
    let as_poly = PatchRegion::from_full(patch.ctrl.clone());
    let (p_left, p_right) = as_poly.split_adaptive().expect("splits");
    let (n_l, worst_l) = escapes(&p_left);
    let (n_r, worst_r) = escapes(&p_right);
    assert!(
        n_l + n_r >= 10 && worst_l.max(worst_r) > 1.0,
        "a polynomial split of these control points was expected to MISS the          surface ({} samples out, worst {:e}); if it now contains it, either the          promotion stopped being rational or the splitter changed, and the          reason for lifting is gone",
        n_l + n_r,
        worst_l.max(worst_r)
    );
}

/// Why Newton's partials go through the quotient rule, measured in steps
/// rather than in millimetres.
///
/// ⚠ Accuracy does not hold this either. Feed Newton `H'` instead of
/// `(H' - W'·S)/W` and the answers still land on both cylinders in the easy
/// cases — the residual test reads the *evaluated* points, which are rational
/// and correct, so a wrong Jacobian only picks a worse direction and damping
/// absorbs it. What it costs shows up in the count of steps, and on the hard
/// cases in the answer:
///
/// ```text
///                    quotient rule      H' alone
///   r 40 x 40  iters      1312            5392      (4.1x)
///   r 40 x 25  iters       896            2008      (2.2x)
///   r 40 x  8  worst    6.15e-3         7.60e-2     (12x worse)
/// ```
///
/// The step count is the sharper instrument, so that is what is pinned.
#[test]
fn newtons_partials_pay_for_themselves_in_steps() {
    use axia_geo::surfaces::ssi::{newton, subdivide};

    let (ca, wa, kua, kva, dua, dva) = promoted(&cyl(DVec3::Z, DVec3::X, 40.0, 100.0));
    let (cb, wb, kub, kvb, dub, dvb) = promoted(&cyl(DVec3::X, DVec3::Y, 40.0, 100.0));
    let pa = nurbs_surface::extract_rational_bezier_patches(&ca, &wa, &kua, &kva, dua, dva)
        .expect("extracts");
    let pb = nurbs_surface::extract_rational_bezier_patches(&cb, &wb, &kub, &kvb, dub, dvb)
        .expect("extracts");

    let (mut candidates, mut iterations, mut unconverged) = (0usize, 0usize, 0usize);
    for a in &pa {
        for b in &pb {
            for c in subdivide::subdivide_intersect_rational(
                &a.ctrl, &a.weights, &b.ctrl, &b.weights, 0.1, subdivide::DEFAULT_MAX_DEPTH,
            ) {
                candidates += 1;
                let r = newton::refine_rational_pair(
                    &a.ctrl, &a.weights, &b.ctrl, &b.weights, c.uv_a, c.uv_b, 1e-4, 50,
                );
                iterations += r.iterations;
                if !r.converged {
                    unconverged += 1;
                }
            }
        }
    }

    assert_eq!(candidates, 640, "the seeding changed; the budget below was measured for 640");
    assert_eq!(unconverged, 0, "every seed on this pair should converge");
    assert!(
        iterations < 2500,
        "{iterations} Newton steps for {candidates} seeds. With the quotient rule \
         this is 1312; feeding the homogeneous derivative straight in costs 5392. \
         A number in that range means the partials stopped being rational."
    );
}
