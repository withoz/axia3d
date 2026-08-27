//! Draw a rectangle across a box and the box keeps its corners.
//!
//! The wide fuzz (100 sessions x 50 operations) reported five sessions with
//! `cached normal opposite to winding`. Session 97, seed `0x5EED0061`, shrunk by
//! greedy delta debugging from thirteen operations to **two**: make a box, draw
//! a rect whose edge lies in the plane of one of its walls.
//!
//! ⚠ The message named the wrong thing. Nothing was wrong with the cached
//! normal; the geometry underneath it had been destroyed. Measured before any
//! change, box alone versus box-then-rect:
//!
//! ```text
//!   box alone      VertId(2)=(230,-20,  0)  VertId(3)=(170,-20,  0)
//!                  VertId(6)=(230,-20,120)  VertId(7)=(170,-20,120)
//!   box + rect     VertId(2)=(230,-20,100)  VertId(3)=(170,-20,100)
//!                  VertId(6)=(230,-20,100)  VertId(7)=(170,-20,100)
//! ```
//!
//! All four vertices of the wall at y=-20 were laid flat onto z=100, the plane
//! the rect was drawn on. The wall became a zero-area line, and the box's floor
//! -- which shares two of those vertices -- became a slanted quad still carrying
//! its old downward normal. That mismatch is what the verifier printed.
//!
//! ## Who moved them
//!
//! `plane_snap::snap_chord_to_plane`, and the mutation says so: stub out its
//! projection and this reproduction passes.
//!
//! The draw's face list held three faces -- the rect, and two pieces of the box
//! wall it had cut. Every face on that list is given the draw's plane and then
//! flattened onto it. For the rect that is a drift correction of a few microns.
//! For a wall standing perpendicular to the draw it is a 100 mm displacement.
//!
//! ADR-168 had a guard for exactly this and never connected it. `PLANE_SNAP_
//! NORMAL` was defined, documented ("a snapped face has normal within 1e-3 of
//! target"), and mentioned nowhere else except a `let _ =` in a test silencing
//! the unused warning. The offset half alone means "project every vertex onto
//! this plane, at any distance" -- and the module's own tests only ever move a
//! vertex 5e-3 mm, four orders of magnitude short of what a draw can do.
//!
//! ## What this asserts, and why not the message
//!
//! The invariant is that drawing does not move a solid that was already there,
//! so that is what is checked -- the eight corners, by position. Asserting the
//! violation string would pass again the day the symptom changes shape while the
//! box is still being crushed.
//!
//! Whether the draw's face list SHOULD contain a piece of the box's wall is a
//! separate question about ownership and is left open. Owning a face is not a
//! licence to flatten it.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use glam::DVec3;

/// `create_box` maps its arguments width→X, height→Z, depth→Y, so this box is
/// x∈[170,230], y∈[-80,-20], z∈[0,120].
fn box_then_rect(draw_rect: bool) -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;

    let _ = s
        .mesh
        .create_box(DVec3::new(200.0, -50.0, 60.0), 60.0, 120.0, 60.0, FORM_MATERIAL);

    if draw_rect {
        // Plane z=100 cuts the box; the edge at y=-20 lies in the wall's plane.
        s.execute(Command::DrawRectAsShape {
            center: DVec3::new(200.0, 50.0, 100.0),
            normal: DVec3::Z,
            up: DVec3::X,
            width: 140.0,
            height: 105.0,
        });
    }
    s
}

/// The eight corners the box is made of.
const CORNERS: &[(f64, f64, f64)] = &[
    (170.0, -80.0, 0.0),
    (230.0, -80.0, 0.0),
    (230.0, -20.0, 0.0),
    (170.0, -20.0, 0.0),
    (170.0, -80.0, 120.0),
    (230.0, -80.0, 120.0),
    (230.0, -20.0, 120.0),
    (170.0, -20.0, 120.0),
];

fn missing_corners(s: &Scene) -> Vec<String> {
    let live: Vec<DVec3> = s
        .mesh
        .verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .map(|(_, v)| v.pos())
        .collect();
    CORNERS
        .iter()
        .filter(|(x, y, z)| {
            let want = DVec3::new(*x, *y, *z);
            !live.iter().any(|p| p.distance(want) < 1e-6)
        })
        .map(|(x, y, z)| format!("({x},{y},{z})"))
        .collect()
}

/// The control. If the box does not stand up on its own, the test below is
/// measuring the wrong thing and its pass means nothing.
#[test]
fn the_box_alone_has_its_eight_corners() {
    let s = box_then_rect(false);
    let gone = missing_corners(&s);
    assert!(gone.is_empty(), "the box is wrong before anything is drawn: {gone:?}");
}

/// ⚠ This is the guard. Before the fix, four of the eight were gone.
#[test]
fn drawing_across_the_box_does_not_move_its_corners() {
    let s = box_then_rect(true);
    let gone = missing_corners(&s);
    assert!(
        gone.is_empty(),
        "drawing a rect moved {} of the box's corners: {gone:?}",
        gone.len()
    );
}

/// The wall was not merely moved -- it was flattened onto a line. A face whose
/// vertices land on fewer than three distinct points bounds nothing.
#[test]
fn no_face_is_crushed_to_a_line() {
    let s = box_then_rect(true);
    let mut crushed = Vec::new();
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        let pts = s.mesh.face_outline_points(f).unwrap_or_default();
        if pts.len() < 3 {
            continue;
        }
        let mut distinct: Vec<DVec3> = Vec::new();
        for p in &pts {
            if !distinct.iter().any(|q| q.distance(*p) < 1e-9) {
                distinct.push(*p);
            }
        }
        if distinct.len() < 3 {
            crushed.push(format!("{f:?} has {} verts on {} points", pts.len(), distinct.len()));
        }
    }
    assert!(crushed.is_empty(), "faces collapsed to a line: {crushed:?}");
}

/// And the verifier agrees -- this is the reading the fuzz reported, kept as a
/// secondary so a future change of wording cannot quietly retire the case.
#[test]
fn the_verifier_reports_nothing() {
    let s = box_then_rect(true);
    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "invariants after the draw: {:?}", inv.violations);
}
