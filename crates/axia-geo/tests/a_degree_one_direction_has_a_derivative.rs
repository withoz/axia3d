//! A DEGREE-1 DIRECTION HAS A DERIVATIVE, AND A SWEPT SIDE HAS A NORMAL.
//!
//! `bspline::derivative` builds the derivative spline — degree p−1 — and hands it
//! to `evaluate`, which validates it first. For p = 1 that asks `validate` to
//! accept degree 0, which it refuses by design ("degree must be ≥ 1, got 0",
//! pinned by `validate_rejects_degree_zero`): a step function is not a curve this
//! module evaluates. So the derivative of the straightest possible spline came
//! back an error, and `bspline_surface::derivative_v` turned that error into ZERO.
//!
//! Every extrusion sweep is degree 1 along v — `surfaces::sweep::extrusion_surface`
//! sets `knots_v = [0, 0, 1, 1]` — so ∂v was zero on all of them, and the surface
//! normal, ∂u × ∂v, was zero with it. Measured 2026-09-23 on a cubic profile swept
//! 120 along +Z:
//!
//! ```text
//!   bspline::derivative(deg 1)   Err("bspline: degree must be ≥ 1, got 0")
//!   ∂S/∂v at v = 0, 0.5, 1       (0, 0, 0)          — should be (0, 0, 120)
//!   ∂S/∂u at (0.5, 0.5)          (45, 0, 0)         — always worked
//!   surface.normal(0.5, 0.5)     (0, 0, 0), len 0   — should be (0, −1, 0)
//!   normal_at_world_pos(mid)     (0, 0, 0)
//! ```
//!
//! The sibling already does it right: `nurbs::derivative` never calls `evaluate`.
//! It reads the derivative spline with `find_knot_span` + `de_boor` directly, and
//! those handle degree 0 — de Boor's refinement loop is simply empty, so it returns
//! the control point of the span the parameter falls in, which is exactly what a
//! piecewise-constant derivative is. This does the same.

use axia_geo::curves::bspline;
use axia_geo::surfaces::{bspline_surface, sweep, AnalyticSurface, SurfaceOps};
use glam::DVec3;

/// A cubic profile, clamped, and the sweep applied to it.
const PROFILE_DEGREE: usize = 3;
const SWEEP: f64 = 120.0;

fn profile() -> (Vec<DVec3>, Vec<f64>) {
    (
        vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(10.0, 30.0, 0.0),
            DVec3::new(30.0, 30.0, 0.0),
            DVec3::new(40.0, 0.0, 0.0),
        ],
        vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
    )
}

fn extrusion() -> (Vec<Vec<DVec3>>, Vec<f64>, Vec<f64>, usize, usize) {
    let (ctrl, knots) = profile();
    sweep::extrusion_surface(&ctrl, &knots, PROFILE_DEGREE, DVec3::Z, SWEEP)
        .expect("extrusion surface")
}

/// A degree-1 spline is piecewise linear, so its derivative is the slope of the
/// segment the parameter falls in — piecewise CONSTANT, and a constant is still a
/// number. Read two ways: the formula, and a finite difference of the curve
/// itself, which knows nothing about degrees.
#[test]
fn a_degree_one_spline_reports_its_slope() {
    let pts = vec![
        DVec3::ZERO,
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 20.0, 0.0),
    ];
    let knots = vec![0.0, 0.0, 0.5, 1.0, 1.0];

    for (t, want) in [(0.25, DVec3::new(20.0, 0.0, 0.0)), (0.75, DVec3::new(0.0, 40.0, 0.0))] {
        let got = bspline::derivative(&pts, &knots, 1, t)
            .unwrap_or_else(|e| panic!("the derivative at t = {t} came back an error: {e}"));
        assert!(
            (got - want).length() < 1e-9,
            "a segment covering half the parameter range doubles its own length: \
             at t = {t} the slope is {want:?}, it read {got:?}"
        );

        // The second instrument: the curve's own difference quotient.
        let h = 1e-6;
        let a = bspline::evaluate(&pts, &knots, 1, t - h).expect("evaluate");
        let b = bspline::evaluate(&pts, &knots, 1, t + h).expect("evaluate");
        let numeric = (b - a) / (2.0 * h);
        assert!(
            (got - numeric).length() < 1e-4,
            "at t = {t} the formula says {got:?} and the curve itself says {numeric:?}"
        );
    }
}

/// And a degree-0 spline is still not a curve this module will evaluate. The fix
/// reads the derivative directly rather than asking `validate` to change its mind.
#[test]
fn a_degree_zero_spline_is_still_not_a_curve() {
    let pts = vec![DVec3::ZERO; 3];
    let knots = vec![0.0, 0.0, 0.0, 0.0];
    assert!(bspline::validate(&pts, &knots, 0).is_err());
    assert!(bspline::evaluate(&pts, &knots, 0, 0.5).is_err());
}

/// A sweep moves the profile along one straight direction, so its v-tangent is
/// that direction times the distance — the same everywhere on the surface.
#[test]
fn an_extrusion_sweep_has_a_tangent_along_its_direction() {
    let (grid, ku, kv, du, dv) = extrusion();
    assert_eq!(dv, 1, "an extrusion is degree 1 along v — that is the whole point");
    let want = DVec3::Z * SWEEP;
    for u in [0.0, 0.3, 0.5, 1.0] {
        for v in [0.0, 0.5, 1.0] {
            let got = bspline_surface::derivative_v(&grid, &ku, &kv, du, dv, u, v)
                .unwrap_or_else(|e| panic!("∂S/∂v at ({u}, {v}) came back an error: {e}"));
            assert!(
                (got - want).length() < 1e-9,
                "∂S/∂v at ({u}, {v}) is {want:?}, it read {got:?}"
            );
        }
    }
    // The u-tangent always worked; it must not move.
    let d_u = bspline_surface::derivative_u(&grid, &ku, &kv, du, dv, 0.5, 0.5).expect("∂S/∂u");
    assert!(
        (d_u - DVec3::new(45.0, 0.0, 0.0)).length() < 1e-9,
        "∂S/∂u at (0.5, 0.5) is (45, 0, 0), it read {d_u:?}"
    );
}

/// So the side of a swept solid has a normal: perpendicular to the profile's
/// tangent and to the sweep, and of unit length — where it was the zero vector.
#[test]
fn a_swept_side_has_a_unit_normal() {
    let (grid, ku, kv, du, dv) = extrusion();
    let surface = AnalyticSurface::BSplineSurface {
        ctrl_grid: grid.clone(),
        knots_u: ku.clone(),
        knots_v: kv.clone(),
        deg_u: du as u32,
        deg_v: dv as u32,
    };

    for (u, v) in [(0.25, 0.0), (0.5, 0.5), (0.75, 1.0)] {
        let n = surface.normal(u, v);
        assert!(
            (n.length() - 1.0).abs() < 1e-9,
            "the normal at ({u}, {v}) has length {:.6}, it should be 1",
            n.length()
        );
        let t_u = bspline_surface::derivative_u(&grid, &ku, &kv, du, dv, u, v).expect("∂u");
        let t_v = bspline_surface::derivative_v(&grid, &ku, &kv, du, dv, u, v).expect("∂v");
        assert!(
            n.dot(t_u.normalize()).abs() < 1e-9 && n.dot(t_v.normalize()).abs() < 1e-9,
            "the normal at ({u}, {v}) is {n:?}, which is not square to {t_u:?} and {t_v:?}"
        );
    }

    // This profile rises in +Y and is swept along +Z, so its side faces −Y.
    let mid = surface.normal(0.5, 0.5);
    assert!(
        (mid - DVec3::NEG_Y).length() < 1e-9,
        "the middle of this side looks {mid:?}, it should look {:?}",
        DVec3::NEG_Y
    );

    // And what the readers actually call: a normal at a point in the world.
    let pos = surface.evaluate(0.5, 0.5);
    let at_pos = surface.normal_at_world_pos(pos);
    assert!(
        (at_pos.length() - 1.0).abs() < 1e-9,
        "normal_at_world_pos({pos:?}) has length {:.6}",
        at_pos.length()
    );
}

/// The rational sweep is built by the same function's sibling and reaches the
/// same derivative through `nurbs_surface`, so it comes back with it.
#[test]
fn a_rational_sweep_has_a_unit_normal_too() {
    let (ctrl, knots) = profile();
    let weights = vec![1.0, 0.8, 0.8, 1.0];
    let (grid, wgrid, ku, kv, du, dv) =
        sweep::extrusion_surface_nurbs(&ctrl, &weights, &knots, PROFILE_DEGREE, DVec3::Z, SWEEP)
            .expect("rational extrusion");
    let surface = AnalyticSurface::NURBSSurface {
        ctrl_grid: grid,
        weights: wgrid,
        knots_u: ku,
        knots_v: kv,
        deg_u: du as u32,
        deg_v: dv as u32,
        trim_loops: Vec::new(),
    };
    let n = surface.normal(0.5, 0.5);
    assert!(
        (n.length() - 1.0).abs() < 1e-9,
        "the rational sweep's normal has length {:.6}, it should be 1",
        n.length()
    );
    assert!(
        (n - DVec3::NEG_Y).length() < 1e-9,
        "weighting the control points does not turn the side: it looks {n:?}"
    );
}

/// The control: a cubic's derivative went through `evaluate` and always worked.
/// It must read the same afterwards, and still agree with the curve itself.
#[test]
fn a_cubic_derivative_is_unchanged() {
    let (pts, knots) = profile();
    for t in [0.1, 0.5, 0.9] {
        let got = bspline::derivative(&pts, &knots, PROFILE_DEGREE, t).expect("cubic derivative");
        let h = 1e-6;
        let a = bspline::evaluate(&pts, &knots, PROFILE_DEGREE, t - h).expect("evaluate");
        let b = bspline::evaluate(&pts, &knots, PROFILE_DEGREE, t + h).expect("evaluate");
        let numeric = (b - a) / (2.0 * h);
        assert!(
            (got - numeric).length() < 1e-4,
            "at t = {t} the cubic's formula says {got:?} and the curve says {numeric:?}"
        );
    }
}
