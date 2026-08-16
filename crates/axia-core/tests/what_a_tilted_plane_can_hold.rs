//! The axis nobody measured: planes that are not axis-aligned.
//!
//! Every grid in this repo so far has drawn on `z = 0` or `z = 100`, or on a
//! box wall at `x = 200`. All three are cardinal planes, and code that only
//! ever sees a cardinal normal can be wrong in ways no cardinal test reveals —
//! a `.z` where a dot product belongs, an `up` that happens to be `+Y`, a
//! basis picked by `if abs(n.z) > 0.9`. The app has had a three-point work
//! plane and sketch tilt for months (ADR-224, LOCKED #104); what has been
//! missing is the measurement.
//!
//! So the grid is four planes by eight closed-shape tools, and the fourth
//! plane is the point of it: `n = (1,1,1)/√3`, which shares no component
//! pattern with any axis. Measuring only XY, XZ and YZ would let
//! axis-only code through — the three of them are each other's permutations.
//!
//! The verdict is deliberately narrow: **a face appeared, and its normal is
//! the plane's**. Not its area. An area check reads as stronger and is
//! weaker — it pins tessellation and chord tolerance along with the thing
//! being asked about, so it fails for reasons that have nothing to do with
//! the plane and has to be loosened until it means nothing. Normal agreement
//! is the whole claim: the shape landed on the plane it was given, facing the
//! way that plane faces.

use axia_core::{Command, CommandResult, Scene};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// A drawing plane: where it is, which way it faces, and two in-plane axes.
struct Plane {
    name: &'static str,
    origin: DVec3,
    n: DVec3,
    u: DVec3,
    v: DVec3,
}

fn planes() -> Vec<Plane> {
    let n = DVec3::new(1.0, 1.0, 1.0).normalize();
    // An in-plane basis that is not derived from any single axis, so the test
    // does not accidentally hand the engine a cardinal `up`.
    let u = DVec3::new(1.0, -1.0, 0.0).normalize();
    let v = n.cross(u).normalize();
    vec![
        Plane { name: "XY (바닥)", origin: DVec3::new(0.0, 0.0, 50.0), n: DVec3::Z, u: DVec3::X, v: DVec3::Y },
        Plane { name: "XZ (앞벽)", origin: DVec3::new(0.0, 40.0, 0.0), n: DVec3::Y, u: DVec3::X, v: DVec3::Z },
        Plane { name: "YZ (옆벽)", origin: DVec3::new(40.0, 0.0, 0.0), n: DVec3::X, u: DVec3::Y, v: DVec3::Z },
        Plane { name: "사선 (1,1,1)", origin: DVec3::new(30.0, 30.0, 30.0), n, u, v },
    ]
}

/// The closed-shape tools, each drawn on the given plane.
///
/// The three NURBS-family ones take control points rather than a plane, so
/// their normal is whatever the engine best-fits from the loop — which is
/// exactly what wants measuring on a tilted plane.
fn tools() -> Vec<(&'static str, fn(&mut Scene, &Plane) -> CommandResult)> {
    vec![
        ("사각형", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawRectAsShape {
                center: p.origin, normal: p.n, up: p.v, width: 60.0, height: 40.0,
            })
        }),
        ("원 (다각형)", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawCircleAsShape {
                center: p.origin, normal: p.n, radius: 30.0, segments: 24,
            })
        }),
        ("정다각형", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawPolygonAsShape {
                center: p.origin, normal: p.n, radius: 30.0, sides: 6,
            })
        }),
        ("원 (곡선)", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawCircleAsCurve { center: p.origin, normal: p.n, radius: 30.0 })
        }),
        ("타원", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawEllipseAsCurve {
                center: p.origin, ref_dir: p.u, normal: p.n, radius_x: 40.0, radius_y: 20.0,
            })
        }),
        ("닫힌 베지어", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawClosedBezierAsCurve { control_pts: quad_loop(p) })
        }),
        ("닫힌 B-스플라인", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawClosedBSplineAsCurve {
                control_pts: quad_loop(p),
                knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
                degree: 3,
            })
        }),
        ("닫힌 NURBS", |s: &mut Scene, p: &Plane| {
            s.execute(Command::DrawClosedNURBSAsCurve {
                control_pts: quad_loop(p),
                weights: vec![1.0; 5],
                knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
                degree: 3,
            })
        }),
    ]
}

/// Five control points around the plane's origin, closed (last == first).
fn quad_loop(p: &Plane) -> Vec<DVec3> {
    let a = p.origin + p.u * 40.0;
    let b = p.origin + p.v * 30.0;
    let c = p.origin - p.u * 40.0;
    let d = p.origin - p.v * 30.0;
    vec![a, b, c, d, a]
}

/// Faces that appeared, and whether EVERY one of them faces the plane.
///
/// Every, not any: one shape is drawn per scene, so a second face with a
/// different normal is a defect rather than a detail, and "the best one
/// agrees" would hide it.
fn landed_on(s: &Scene, n: DVec3) -> (usize, bool, f64) {
    let mut count = 0usize;
    let mut worst = 1.0_f64;
    for (_, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        count += 1;
        let fn_ = f.normal().normalize_or_zero();
        if fn_.length_squared() > 0.5 {
            worst = worst.min(fn_.dot(n).abs());
        } else {
            worst = 0.0; // a face with no normal cannot be facing anything
        }
    }
    // 0.999 is ~2.6°, far tighter than any tessellation wobble and far looser
    // than the 1e-9 an exact plane would give — it separates "on the plane"
    // from "somewhere near it" without pinning arithmetic.
    (count, count > 0 && worst > 0.999, worst)
}

/// The grid.
#[test]
fn every_closed_shape_lands_on_the_plane_it_was_given() {
    println!("\n평면 × 닫힌 도형 — 면이 생겼는가, 그 면이 평면을 향하는가\n");
    let mut fails: Vec<String> = Vec::new();
    for p in planes() {
        for (tool_name, draw) in tools() {
            let mut s = prod();
            let r = draw(&mut s, &p);
            let refused = match &r {
                CommandResult::Error(e) => Some(e.clone()),
                _ => None,
            };
            let (faces, aligned, dot) = landed_on(&s, p.n);
            let verdict = if let Some(e) = &refused {
                fails.push(format!("{} / {} — 거부: {e}", p.name, tool_name));
                "거부".to_string()
            } else if faces == 0 {
                fails.push(format!("{} / {} — 면 없음", p.name, tool_name));
                "면 없음".to_string()
            } else if !aligned {
                fails.push(format!("{} / {} — 법선 어긋남 (dot {dot:.4})", p.name, tool_name));
                format!("어긋남 {dot:.4}")
            } else {
                "OK".to_string()
            };
            println!("  {:<14} {:<16} {:<14} 면 {faces}", p.name, tool_name, verdict);
        }
    }
    println!("\n  실패 {}건", fails.len());
    for f in &fails {
        println!("    ✗ {f}");
    }
    assert_eq!(
        fails.len(),
        0,
        "every closed shape must land on the plane it was given: {fails:?}"
    );
}

/// The oblique plane on its own, asked more precisely.
///
/// The grid above accepts any face whose normal agrees. This asks that the
/// shape's own vertices are ON the plane — a face can face the right way and
/// still sit somewhere else, and a plane through the origin would hide the
/// difference. The origin here is deliberately off the origin.
#[test]
fn an_oblique_shape_sits_where_it_was_put() {
    let p = planes().into_iter().last().expect("the oblique plane");
    for (tool_name, draw) in tools() {
        let mut s = prod();
        let r = draw(&mut s, &p);
        assert!(!matches!(r, CommandResult::Error(_)), "{tool_name}: {r:?}");
        let d = p.n.dot(p.origin);
        let mut worst = 0.0_f64;
        for (vid, vert) in s.mesh.verts.iter() {
            if !vert.is_active() {
                continue;
            }
            let pos = s.mesh.vertex_pos(vid).expect("position");
            worst = worst.max((p.n.dot(pos) - d).abs());
        }
        println!("  {tool_name:<16} 평면에서 최대 {worst:.6} mm");
        // 1 µm — well inside the 1.5 µm the dedup floor already tolerates
        // (LOCKED #5), so anything failing here is a real drift and not the
        // spatial hash rounding a vertex.
        assert!(
            worst < 1e-3,
            "{tool_name}: a vertex is {worst:.6} mm off the oblique plane"
        );
    }
}
