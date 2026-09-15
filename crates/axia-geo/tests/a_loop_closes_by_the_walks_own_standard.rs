//! When a chain of intersection points is a loop, and why it was not saying so.
//!
//! ## The threshold nobody had to meet
//!
//! `assemble_chains` walks candidate points greedily, linking the next one
//! whenever it is within `gap_tol`. Then it asked whether the chain was CLOSED
//! by a different measure entirely — `merge_tol * 4` — and every call site
//! passes `gap_tol = tol * 100` against `merge_tol = tol`. So the closing gap
//! had to be four hundred times tighter than any gap the walk itself had
//! accepted, and a loop sampled at the subdivision's own spacing could not
//! close.
//!
//! Measured 2026-09-05 on a plane cutting a promoted cylinder — the exact case
//! a Boolean against a bore is:
//!
//! ```text
//!   72 points on the r=40 circle, off it by 3.03e-10 mm
//!   closing gap  7.4544
//!   widest step INSIDE the chain  7.4544      ← the same number
//!   merge_tol * 4 = 0.4000  →  "open"
//! ```
//!
//! The chain was closed by the walk's own standard and the test said otherwise.
//! `nurbs_boolean_v2` then drops every chain that is not closed, so a Boolean
//! against a cylinder produced no trim loops at all.
//!
//! The rule is the walk's now: **the ends join if the gap between them is no
//! wider than the gaps already inside.** Self-scaling, no tolerance to pick, and
//! a LINE still reads open because a line's ends are further apart than any step
//! along it. The old `merge_tol * 4` is kept as an OR.
//!
//! ⚠ That is no longer the whole rule, and "anything that closed before still
//! closes" no longer holds: a closed chain must also turn back, and the walk
//! that builds it changed. See *What a closed chain has to be*.
//!
//! ⚠ `assemble_closed_loop_detected` in `topology.rs` already recorded this in
//! its own comment — *"with merge_tol=0.01, merge_tol*4=0.04 — too tight for raw
//! 8-sample circle. Closure detection is a heuristic; relax tol."* — and worked
//! around it by passing a looser tolerance rather than fixing it.
//!
//! ## What this does NOT fix, measured
//!
//! A Boolean against a cylinder still produces nothing. On the ±100 plane used
//! below:
//!
//! ```text
//!   geometric  0.001 (the production default)   72 chains, 0 closed
//!   geometric  0.05                             32 chains, 0 closed
//!   geometric  0.1                               1 chain,  1 closed   (the circle)
//! ```
//!
//! ⚠ This table used to read **8 closed, 8 trim loops** at 0.05. None of those
//! eight was a loop — see *What a closed chain has to be*, below.
//!
//! 1. **The assembler's reach is tied to accuracy, not to sampling.**
//!    `gap_tol = tol * 100` is 0.1 mm at the production tolerance, while the
//!    candidates are about 1 mm apart because the subdivision hits
//!    `DEFAULT_MAX_DEPTH` long before it reaches 1 μm. So the walk links nothing
//!    and 72 points come back as 72 chains of one.
//! 2. ⚠ **This file used to say "the DCEL rebuild does nothing with a closed
//!    trim loop". That was a misdiagnosis.** The dispatch never builds the ±100
//!    plane: `surface_to_bspline` spans the face's Plane `u_range`, ±150 for a
//!    face of a 200 mm box, and on that grid the circle does not come back whole
//!    at 0.1, 0.05, 0.01 or 0.001 — only fragments, some of which were being
//!    called closed. Handed a REAL loop (the ±100 circle, on an unmerged routing
//!    spike, 2026-09-15), the rebuild does worse than nothing: it evaluates the
//!    loop's knot-space `uv` on the analytic surface, so an r = 40 circle became
//!    a face of area 0.1 (= π · 0.2²) and the face it replaced was removed.
//!    Not fixed or pinned here.
//!
//! So routing a rational surface into the Boolean — a four-line change — was
//! deliberately NOT landed. At the production tolerance it reports the NURBS
//! path and hands back an unchanged mesh; at looser ones it removes faces and
//! builds wrong ones. What a Boolean against a cylinder costs meanwhile,
//! measured against the drill:
//!
//! ```text
//!            faces   with a surface   volume
//!   drill      38    38 (32 Cylinder) 6,994,690.4   = the analytic value
//!   Boolean    38     4 (Plane only)  7,001,137.6   +6,447
//! ```
//!
//! ## What a closed chain has to be
//!
//! Measured 2026-09-15 on the ±150 grid: the rule above — ends joined when the
//! gap is no wider than the steps inside — called eight chains closed at each of
//! tol 0.1, 0.05 and 0.01, and **none went round the bore**:
//!
//! ```text
//!   four 4-point runs   ~1.0° of arc   closing 0.7158, widest step 0.7153
//!   four 6-point runs   ~1.2° wide     visited out of order: the walk ran to one
//!                                      end, then leapt 0.719 back across its
//!                                      own start to carry on the other side
//! ```
//!
//! Neither the rule nor the walk survived. Two changes, each mutation-checked on
//! its own (undo either and its tests fail):
//!
//! - **The walk grows from both ends at once**, always taking the nearer
//!   neighbour, so a run seeded in its middle is laid out end to end.
//! - **A closed chain must turn back**: its end gap may be at most half of what
//!   it walked. A triangle sits exactly on that line, a real loop well inside
//!   it, a straight run outside it.
//!
//! The circle still closes. On the ±150 grid nothing closes at any tolerance
//! measured — which is the truth: no loop comes back there.

use axia_geo::surfaces::ssi::nurbs_wrapper::intersect_rational_bspline_pair;
use axia_geo::surfaces::{cylinder, AnalyticSurface};
use glam::DVec3;

/// A plane, as the 2×2 degree-1 grid the Boolean dispatch builds for one.
fn plane(z: f64, half: f64) -> (Vec<Vec<DVec3>>, Vec<Vec<f64>>, Vec<f64>) {
    let grid = vec![
        vec![DVec3::new(-half, -half, z), DVec3::new(-half, half, z)],
        vec![DVec3::new(half, -half, z), DVec3::new(half, half, z)],
    ];
    let weights = vec![vec![1.0, 1.0], vec![1.0, 1.0]];
    (grid, weights, vec![0.0, 0.0, 1.0, 1.0])
}

type Nurbs = (Vec<Vec<DVec3>>, Vec<Vec<f64>>, Vec<f64>, Vec<f64>, usize, usize);

fn promoted_cylinder(radius: f64, half_length: f64) -> Nurbs {
    let cyl = AnalyticSurface::Cylinder {
        axis_origin: DVec3::ZERO,
        axis_dir: DVec3::Z,
        radius,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-half_length, half_length),
    };
    let AnalyticSurface::NURBSSurface {
        ctrl_grid, weights, knots_u, knots_v, deg_u, deg_v, ..
    } = cylinder::cylinder_to_nurbs_surface(&cyl).expect("a full cylinder promotes")
    else {
        panic!("the promotion stopped returning a NURBSSurface");
    };
    (ctrl_grid, weights, knots_u, knots_v, deg_u as usize, deg_v as usize)
}

/// A plane cutting a cylinder gives a circle, and a circle is a loop.
#[test]
fn a_circle_sampled_at_the_walks_own_spacing_closes() {
    let (pg, pw, pk) = plane(100.0, 100.0);
    let (cg, cw, cku, ckv, cdu, cdv) = promoted_cylinder(40.0, 150.0);

    let chains = intersect_rational_bspline_pair(
        &pg, &pw, &pk, &pk, 1, 1, &cg, &cw, &cku, &ckv, cdu, cdv, 0.1,
    )
    .expect("a plane and a cylinder intersect");

    assert_eq!(chains.len(), 1, "one circle, not {}", chains.len());
    let chain = &chains[0];
    assert!(chain.points.len() > 50, "only {} points", chain.points.len());

    // The points are on the circle to machine precision — the accuracy was
    // never the problem.
    let worst = chain.points.iter().fold(0.0_f64, |m, p| {
        m.max(((p.x * p.x + p.y * p.y).sqrt() - 40.0).abs()).max((p.z - 100.0).abs())
    });
    assert!(worst < 1e-6, "off the circle by {worst:e}");

    // The closing gap is no wider than the steps the walk already took.
    let closing = (*chain.points.first().unwrap() - *chain.points.last().unwrap()).length();
    let widest = chain
        .points
        .windows(2)
        .map(|w| (w[1] - w[0]).length())
        .fold(0.0_f64, f64::max);
    assert!(
        closing <= widest * 1.5,
        "closing gap {closing} against a widest interior step of {widest} — if the \
         sampling changed, the rule below is being asked something different"
    );

    assert!(
        chain.closed,
        "the chain is a circle sampled at {:.4} mm and closing at {closing:.4} mm, and \
         it says it is open. That is the `merge_tol * 4` rule, which the walk itself \
         never had to meet.",
        widest
    );
}

/// The control, and the reason the rule is not simply `gap_tol`: a line's ends
/// are further apart than any step along it, so it stays open however far the
/// walk can reach.
#[test]
fn a_line_does_not_close() {
    // Two planes at right angles meet in a line, not a loop.
    let (pg, pw, pk) = plane(0.0, 100.0);
    let upright = vec![
        vec![DVec3::new(0.0, -100.0, -100.0), DVec3::new(0.0, -100.0, 100.0)],
        vec![DVec3::new(0.0, 100.0, -100.0), DVec3::new(0.0, 100.0, 100.0)],
    ];
    let chains = intersect_rational_bspline_pair(
        &pg, &pw, &pk, &pk, 1, 1, &upright, &pw, &pk, &pk, 1, 1, 0.5,
    )
    .expect("two planes meet");

    assert!(!chains.is_empty(), "the two planes did not meet at all");
    for chain in &chains {
        assert!(
            !chain.closed,
            "a straight line was reported as a closed loop ({} points, ends {:.4} apart)",
            chain.points.len(),
            (*chain.points.first().unwrap() - *chain.points.last().unwrap()).length()
        );
    }
}

/// ⚠ Necessary, not sufficient. Kept so the next reader does not have to
/// re-measure why a Boolean against a cylinder still does nothing.
///
/// At the production tolerance the assembler cannot link the candidates at all,
/// because its reach is `tol * 100` while the sampling is set by the
/// subdivision's depth cap.
#[test]
fn at_the_production_tolerance_the_walk_still_cannot_link_them() {
    let (pg, pw, pk) = plane(100.0, 100.0);
    let (cg, cw, cku, ckv, cdu, cdv) = promoted_cylinder(40.0, 150.0);

    // 1e-3 is `BooleanTolerance::default().geometric`.
    let chains = intersect_rational_bspline_pair(
        &pg, &pw, &pk, &pk, 1, 1, &cg, &cw, &cku, &ckv, cdu, cdv, 1e-3,
    )
    .expect("intersects");

    let points: usize = chains.iter().map(|c| c.points.len()).sum();
    let closed = chains.iter().filter(|c| c.closed).count();
    assert!(
        chains.len() > 10 && closed == 0,
        "the production tolerance now yields {} chains ({closed} closed) from {points} \
         points. If the walk can link them, `gap_tol = tol * 100` was changed or the \
         subdivision reaches further — either way the header's second blocker is gone \
         and this test should be rewritten to guard that.",
        chains.len()
    );
}

/// The grid a Boolean actually builds, and what closes on it.
///
/// `surface_to_bspline` spans a face's Plane `u_range`, which for a face of a
/// 200 mm box is ±150 — not the ±100 used above. On that grid the circle comes
/// back as fragments at every tolerance measured, and eight of them at each
/// tolerance were being called closed: four 4-point runs a degree long, and
/// four 6-point runs the walk had folded back across their own start.
#[test]
fn a_fragment_that_does_not_go_round_does_not_close() {
    let (pg, pw, pk) = plane(100.0, 150.0);
    let (cg, cw, cku, ckv, cdu, cdv) = promoted_cylinder(40.0, 150.0);

    for tol in [0.1, 0.05, 0.01] {
        let chains = intersect_rational_bspline_pair(
            &pg, &pw, &pk, &pk, 1, 1, &cg, &cw, &cku, &ckv, cdu, cdv, tol,
        )
        .expect("intersects");

        // The question has to arrive: there must be fragments to misjudge.
        assert!(
            chains.len() > 1,
            "tol {tol}: {} chain(s) — if the circle now comes back whole on this \
             grid, the fragments this test asks about are gone; rewrite it",
            chains.len()
        );

        for chain in chains.iter().filter(|c| c.closed) {
            // How far round the bore the chain actually walks.
            let sweep: f64 = chain
                .points
                .windows(2)
                .map(|w| {
                    let d = w[1].y.atan2(w[1].x) - w[0].y.atan2(w[0].x);
                    (d + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI
                })
                .sum();
            assert!(
                sweep.abs() > 1.9 * std::f64::consts::PI,
                "tol {tol}: a chain of {} points walking {:.2}° round the bore was \
                 called closed",
                chain.points.len(),
                sweep.abs().to_degrees()
            );
        }
    }
}
