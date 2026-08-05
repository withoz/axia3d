//! HOW FINELY A PARAMETRIC CURVE IS SAMPLED.
//!
//! `bspline::tessellate` and `nurbs::tessellate` took their point count from
//! `control polygon length ÷ chord_tol` — a point every `chord_tol` of ARC,
//! which is not what a chord tolerance asks for and does not depend on how
//! sharply the curve bends. Measured 2026-08-05:
//!
//! ```text
//!   circle  r=40      @0.02 mm ->  100 points   (its own path, curvature-based)
//!   ellipse 40x25     @0.02 mm -> 4096 points   (the 4096 CAP, not a result)
//!   ellipse 40x25     @0.10 mm -> 2600 points   (5x the tolerance, barely fewer)
//! ```
//!
//! The second row is the same shape as the first. It cost 36 seconds to draw one
//! ellipse on a box — 12 of them in the overlap scan alone, earcutting a
//! four-thousand-point hole — and the third row is the tell: the count was never
//! about the tolerance, so loosening it could not help.
//!
//! `bezier` in the same module had always subdivided recursively instead. This
//! pins that the parametric forms now do too, and — the part that matters —
//! that being cheaper did not make them wrong: the chord error is measured
//! against the true curve, not assumed from the point count.
use axia_geo::curves::{bspline, nurbs};
use glam::DVec3;

const A: f64 = 40.0;
const B: f64 = 25.0;

fn ellipse(tol: f64) -> Vec<DVec3> {
    let (cp, w, kn, deg) = nurbs::ellipse(DVec3::ZERO, A, B, DVec3::X, DVec3::Y);
    nurbs::tessellate(&cp, &w, &kn, deg, tol).expect("an ellipse tessellates")
}

/// How far a point is from a polyline.
fn dist_to_polyline(p: DVec3, poly: &[DVec3]) -> f64 {
    let mut best = f64::MAX;
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let len2 = ab.length_squared();
        let t = if len2 < 1e-24 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
        best = best.min((p - (a + ab * t)).length());
    }
    best
}

/// The worst the polyline misses the true ellipse by, sampled densely.
fn worst_miss(poly: &[DVec3]) -> f64 {
    let mut worst: f64 = 0.0;
    for i in 0..20_000 {
        let th = std::f64::consts::TAU * i as f64 / 20_000.0;
        let p = DVec3::new(A * th.cos(), B * th.sin(), 0.0);
        worst = worst.max(dist_to_polyline(p, poly));
    }
    worst
}

#[test]
fn an_ellipse_costs_about_what_a_circle_of_the_same_size_costs() {
    let pts = ellipse(0.02);
    // The circle path, on its own formula, gives 100 for r=40 at this tolerance.
    assert!(
        (40..400).contains(&pts.len()),
        "an ellipse this size should cost roughly a circle's 100 points, got {}",
        pts.len()
    );
    // And it is really round — the old cap would also have satisfied "> 40".
    assert!(pts.len() < 500, "the 4096 cap is what this replaced, got {}", pts.len());
}

#[test]
fn the_chord_error_is_actually_within_the_tolerance() {
    for tol in [0.5, 0.1, 0.02] {
        let pts = ellipse(tol);
        let miss = worst_miss(&pts);
        assert!(
            miss <= tol,
            "at tol {tol} the polyline misses the ellipse by {miss} over {} points",
            pts.len()
        );
        for p in &pts {
            let f = (p.x / A).powi(2) + (p.y / B).powi(2);
            assert!((f - 1.0).abs() < 1e-9, "every point is ON the ellipse, got {f}");
        }
    }
}

/// Asking for less precision has to cost less — the whole point of the number
/// being a tolerance. It did not before: 0.10 gave 2600 where 0.02 gave 4096.
#[test]
fn a_looser_tolerance_costs_less() {
    let fine = ellipse(0.02).len();
    let coarse = ellipse(0.5).len();
    assert!(coarse * 2 <= fine, "0.5 mm should be far cheaper than 0.02 mm: {coarse} vs {fine}");
}

/// The non-rational form took the same route and is fixed the same way.
#[test]
fn a_b_spline_is_sampled_the_same_way() {
    // A closed-ish quadratic arc through four control points.
    let cp = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(30.0, 60.0, 0.0),
        DVec3::new(90.0, 60.0, 0.0),
        DVec3::new(120.0, 0.0, 0.0),
    ];
    let knots = vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
    let fine = bspline::tessellate(&cp, &knots, 3, 0.02).expect("fine");
    let coarse = bspline::tessellate(&cp, &knots, 3, 1.0).expect("coarse");
    assert!(fine.len() < 400, "not the 4096 cap, got {}", fine.len());
    assert!(coarse.len() < fine.len(), "{} vs {}", coarse.len(), fine.len());
    assert!(fine.len() >= 8, "still enough to be a curve, got {}", fine.len());
    // Ends are exact, and it does not wander off the hull.
    assert!((fine[0] - cp[0]).length() < 1e-9);
    assert!((fine[fine.len() - 1] - cp[3]).length() < 1e-9);
}

/// A knot is where the curve is free to kink, so it must be one of the samples
/// however flat the span around it looks.
#[test]
fn every_knot_is_a_sample_point() {
    // Degree 1 (polyline) — the corners ARE the interior knots, and a flatness
    // test that only looks at span midpoints would sail straight past them.
    //
    // The knot is deliberately NOT halfway along: with knots [0,0,1,2,2] the
    // corner sits at t=1, which is exactly where the first bisection lands, so
    // the test passed even with the knot samples removed. Measured — that
    // mutation went unnoticed until the fixture was moved off the midpoint.
    let cp = vec![
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(50.0, 80.0, 0.0),
        DVec3::new(100.0, 0.0, 0.0),
    ];
    let knots = vec![0.0, 0.0, 1.0, 3.0, 3.0];
    let pts = bspline::tessellate(&cp, &knots, 1, 0.02).expect("a polyline");
    assert!(
        pts.iter().any(|p| (*p - cp[1]).length() < 1e-9),
        "the corner at the interior knot is missing from {pts:?}"
    );
}
