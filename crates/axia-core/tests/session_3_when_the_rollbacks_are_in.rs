//! The session the two rollbacks looked like they cost — and did not.
//!
//! The fix that follows from locating the downstream step — the re-derive undone
//! when self-intersections rise, the post-draw repair undone when invariant
//! violations rise — makes defect 3 and session 10's four-operation repro both
//! sound. The fuzz answers that session 3 was sound and is not any more, which
//! looked like a regression. Transcribed and shrunk the way the other four were,
//! it is two operations — and those two break the SAME WAY on main, with the
//! rollbacks nowhere in sight.
//!
//! The fuzz's operations depend on the mesh: a push or an extrude picks its face
//! out of the live set, so any change to how many faces a draw makes shifts
//! every later operation. Session 3 did not start breaking; its stream moved
//! onto a defect that was already there.
//!
//! ```text
//!   op 10: edge EdgeId(106): shared by 3 active faces (non-manifold) —
//!          FaceId(46) / FaceId(44) cover the same ground (stacked)
//! ```

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const STACKED: &str = "cover the same ground";

#[derive(Clone, Copy, Debug)]
enum Op {
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Punch { x: f64, y: f64, z: f64 },
    Extrude { face: u32, d: f64 },
    PushIn { face: u32, d: f64 },
}

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn apply(s: &mut Scene, op: Op) {
    match op {
        Op::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, segments: 24,
            });
        }
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r,
            });
        }
        Op::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z), ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: rx, radius_y: ry,
            });
        }
        Op::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, sides: n,
            });
        }
        Op::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, up: DVec3::X,
                width: w, height: w * 0.75,
            });
        }
        Op::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(
                DVec3::new(x, y, z) + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL,
            );
        }
        Op::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z,
            );
        }
        Op::Extrude { face, d } | Op::PushIn { face, d } => {
            s.execute(Command::CreateSolid {
                face_id: FaceId::new(face),
                mode: CreateSolidMode::Extrude { distance: d },
            });
        }
    }
}

fn run(ops: &[Op]) -> Option<(usize, String)> {
    let mut s = prod();
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        if let Some(v) = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|v| format!("{v:?}"))
            .find(|t| t.contains(STACKED))
        {
            return Some((i, v));
        }
    }
    None
}

/// Session 3 as the fuzz generates it. Its op 0 is `pushIn(skip: nothing there)`
/// and its op 3 a refused punch, both omitted as no-ops.
fn session_3() -> Vec<Op> {
    vec![
        Op::Circle { x: 0.0, y: 150.0, z: 0.0, r: 30.0 },
        Op::Ellipse { x: -150.0, y: 150.0, z: 100.0, rx: 90.0, ry: 54.0 },
        Op::Punch { x: -50.0, y: 200.0, z: 0.0 },
        Op::PushIn { face: 2, d: -100.0 },
        Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::CircleCurve { x: -200.0, y: 0.0, z: 100.0, r: 50.0 },
        Op::Rect { x: 150.0, y: 0.0, z: 0.0, w: 100.0 },
        Op::Extrude { face: 1, d: 200.0 },
        Op::PushIn { face: 27, d: -100.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
    ]
}

#[test]
fn the_sequence_shrinks_to_something_readable() {
    let full = session_3();
    assert!(
        run(&full).is_some(),
        "the transcription does not reproduce the stacked faces — check it \
         against the fuzz log before shrinking anything"
    );
    let n0 = full.len();
    let mut ops = full;
    let mut i = 0;
    while i < ops.len() {
        let mut without = ops.clone();
        without.remove(i);
        if run(&without).is_some() {
            ops = without;
        } else {
            i += 1;
        }
    }
    println!("\n  {n0} 연산 → {} 연산\n", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("    {i}: {op:?}");
    }
    let (at, why) = run(&ops).expect("still fails");
    println!("\n  op {at} 에서 {why}\n");
    assert!(ops.len() <= n0);
}

/// The shrunk sequence: a box, and a square drawn at a height INSIDE it.
///
/// z=200 is not a face of the box — it spans 100 to 220 — so the square is a
/// sheet passing through the solid's middle, crossing its walls.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
    ]
}

/// Not a regression: measured both ways.
///
/// With the rollbacks in and with them out, the two operations leave the same
/// violation on the same edge between the same two faces — `EdgeId(22)`,
/// `FaceId(11)` and `FaceId(9)`. So this is a defect the fuzz's session 3 had
/// not been reaching, not one the rollbacks made.
#[test]
fn the_shrunk_sequence_stacks() {
    let got = run(&shrunk());
    println!("\n  줄인 순서 2 연산 → {got:?}");
    assert!(
        got.is_some(),
        "the two-operation reduction has to reproduce it: {got:?}"
    );
}
