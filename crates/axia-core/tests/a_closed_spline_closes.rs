//! A CLOSED SPLINE CLOSES — the engine takes the shape the tool already builds.
//!
//! `DrawSplineTool` draws an open B-spline: it collects clicks, sets
//! `degree = min(3, n-1)`, builds a clamped uniform knot vector of length
//! `n + degree + 1`, and calls `drawBSplineWithCurve`. Its sibling
//! `DrawBezierTool` detects that the last click landed on the first and routes
//! to `drawClosedBezierAsCurve` instead (ADR-089 A-ψ). The spline tool had no
//! such branch, so `drawClosedBSplineAsCurve` — engine op, WASM export and
//! bridge wrapper all present since ADR-089 A-Α — had **zero callers**.
//!
//! Found by walking the wiring map backwards (bridge methods nothing calls),
//! the same pass that found the chamfer menu item and the Outliner's rename.
//! No guard could see it: nothing pointed at the wrapper, and the wrapper calls
//! a name that IS exported, so link D was satisfied. A wrapper nobody calls
//! breaks no link.
//!
//! ⚠ What this file pins is the ENGINE side, measured before the tool was
//! touched: that the engine accepts the tool's own shape, and where its floor
//! is. The tool side is held by `aClosedSplineClosesTheLoop` in vitest and
//! `a-closed-spline-closes` in Playwright.
//!
//! Measured 2026-10-01, points round a circle with the last exactly on the first:
//!
//! ```text
//!   n=3  degree=2   faces 0   refused (degree 2 routes to the Bezier arm)
//!   n=4  degree=3   faces 1   ShapeCreated
//!   n=5  degree=3   faces 1   ShapeCreated
//!   n=6  degree=3   faces 1   ShapeCreated
//!   n=7  degree=3   faces 1   ShapeCreated
//! ```
//!
//! So the tool's guard is `n >= 4`, and it is measured rather than assumed.

use axia_core::commands::{Command, CommandResult};
use axia_core::Scene;
use glam::DVec3;

/// Exactly what `DrawSplineTool.commit()` builds: `degree = min(3, n-1)` and a
/// clamped uniform knot vector of length `n + degree + 1`.
fn tool_shape(pts: &[[f64; 3]]) -> (Vec<DVec3>, Vec<f64>, usize) {
    let n = pts.len();
    let degree = std::cmp::min(3, n - 1);
    let mut knots = vec![0.0; degree + 1];
    for i in 1..(n - degree) {
        knots.push(i as f64);
    }
    let last = (n - degree) as f64;
    for _ in 0..=degree {
        knots.push(last);
    }
    // PREMISE: the knot vector the tool builds is the one the engine asks for.
    // If this ever fails the fixture has drifted from the tool and every
    // assertion below is about something the tool does not send.
    assert_eq!(knots.len(), n + degree + 1, "engine knot-vector contract");
    (
        pts.iter().map(|p| DVec3::new(p[0], p[1], p[2])).collect(),
        knots,
        degree,
    )
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// `n` points round a circle of r=100 on z=0, the last exactly on the first.
fn ring(n: usize) -> Vec<[f64; 3]> {
    let mut pts: Vec<[f64; 3]> = (0..(n - 1))
        .map(|i| {
            let a = (i as f64) * std::f64::consts::TAU / ((n - 1) as f64);
            [100.0 * a.cos(), 100.0 * a.sin(), 0.0]
        })
        .collect();
    pts.push(pts[0]);
    pts
}

fn draw(pts: &[[f64; 3]]) -> (Scene, CommandResult) {
    let (control_pts, knots, degree) = tool_shape(pts);
    let mut s = Scene::new();
    let r = s.execute(Command::DrawClosedBSplineAsCurve {
        control_pts,
        knots,
        degree: degree as u32,
    });
    (s, r)
}

#[test]
fn the_shape_the_tool_builds_becomes_a_face() {
    // Five clicks round a square, the last landing on the first.
    let (s, r) = draw(&[
        [0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [100.0, 100.0, 0.0],
        [0.0, 100.0, 0.0],
        [0.0, 0.0, 0.0],
    ]);
    println!("  square, n=5 -> {r:?}, faces {}", active(&s));
    assert!(
        matches!(r, CommandResult::ShapeCreated(_)),
        "the engine must take the tool's own shape: {r:?}"
    );
    assert_eq!(active(&s), 1, "one closed loop, one face");
    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "and it must be sound: {:?}", inv.violations);
}

/// The control the tool's guard rests on: an open polygon must NOT reach here.
/// The engine says so itself, which is why the tool can route by distance.
#[test]
fn an_open_control_polygon_is_refused() {
    let (s, r) = draw(&[
        [0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0],
        [100.0, 100.0, 0.0],
        [0.0, 100.0, 0.0],
    ]);
    println!("  open, n=4 -> {r:?}");
    assert_eq!(active(&s), 0, "an open polygon must not become a face: {r:?}");
    let msg = format!("{r:?}");
    assert!(
        msg.contains("not closed"),
        "and it must say WHY, so a tool can report it: {msg}"
    );
}

/// The floor, measured. If a later change lets n=3 close, this fails and the
/// tool's `n >= 4` guard should come down with it.
#[test]
fn four_points_is_the_floor() {
    let (s3, r3) = draw(&ring(3));
    println!("  n=3 -> faces {}, {r3:?}", active(&s3));
    assert_eq!(active(&s3), 0, "three points (degree 2) do not close yet: {r3:?}");

    for n in 4..=7usize {
        let (s, r) = draw(&ring(n));
        println!("  n={n} -> faces {}", active(&s));
        assert!(
            matches!(r, CommandResult::ShapeCreated(_)),
            "n={n} must close: {r:?}"
        );
        assert_eq!(active(&s), 1, "n={n}: one face");
        assert!(s.mesh.verify_face_invariants().is_valid(), "n={n}: sound");
    }
}
