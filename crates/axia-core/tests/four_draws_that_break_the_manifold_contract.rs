//! The smallest scene that leaves the mesh non-manifold, and how it was found.
//!
//! ## Where it came from
//!
//! Not from looking. The coplanar work has FOUR readers of a face's boundary
//! and three of them follow arc edges — `mesh.rs:14872`, `face_split.rs:166`,
//! `self_intersect.rs:351`, each asking for it by name with `.following_arcs()`.
//! `coplanar.rs::collect_face_boundary` is the fourth and reads the chord, which
//! is a 29 mm blind strip along a quarter arc of r=100 (measured 2026-08-04,
//! `the_detector_reads_the_rim_not_the_chord.rs`).
//!
//! Unifying it was tried on 2026-09-05 as a MEASUREMENT, not a commit. One test
//! broke: the fuzz, session 10, at operation 16 — a stacked pair. The repo's own
//! rule for that is to reduce it and re-run the reduction on main before blaming
//! the change, because four times running the "new" break was a pre-existing
//! defect the shifted op stream happened to reach.
//!
//! It was. This scene fails on main, with production flags, in four operations.
//!
//! ## What it is, and what it is not
//!
//! ⚠ This is NOT the standing-solid overlap already pinned in
//! `only_one_mechanism_runs_on_a_draw.rs`. That one leaves the manifold contract
//! INTACT and only reports damage. Measured side by side:
//!
//! ```text
//!                          invariant violations   damaging contacts
//!   the 3-op pin                     0                   1
//!   this, 4 operations               4                  10
//! ```
//!
//! Four half-edges bearing faces on one edge is not a picture of two solids
//! standing in the same place — it is a mesh no walker can traverse. The fuzz
//! gate does not reach it today at 12 × 20.
//!
//! ## Minimal
//!
//! Every one of the four operations is load-bearing. Dropping any single one
//! gives zero violations:
//!
//! ```text
//!   rect, box, c110, c70   19 faces   4 violations
//!         box, c110, c70    9 faces   0
//!   rect,      c110, c70   13 faces   0
//!   rect, box,       c70    8 faces   0
//!   rect, box, c110        18 faces   0
//! ```
//!
//! ⚠ The last row is the one to notice: the scene is SOUND right up to the final
//! draw. That draw does not overlap the rect at all — nearest approach 12.5 mm
//! of clear ground, measured by clamping the circle's centre into the rectangle.
//! It touches the BOX, and the rect is 12.5 mm away and still necessary.
//!
//! ## What this file asserts
//!
//! Today's behaviour, so a fix fires it. When someone makes this scene sound,
//! this test fails and should be rewritten to guard the soundness — the same
//! contract `only_one_mechanism_runs_on_a_draw.rs` carries.

use axia_core::{Command, Scene, FORM_MATERIAL};
use glam::DVec3;

fn production() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

const RECT: DVec3 = DVec3::new(0.0, 100.0, 100.0);
/// `create_box` takes the CENTRE; the fuzz builds it at `c + 60·ẑ`, so a
/// `box(-50, 50, 100)` line in a fuzz log is this point.
const BOX: DVec3 = DVec3::new(-50.0, 50.0, 160.0);
const C110: DVec3 = DVec3::new(0.0, 100.0, 100.0);
const C70: DVec3 = DVec3::new(50.0, -50.0, 100.0);

fn rect(s: &mut Scene) {
    s.execute(Command::DrawRectAsShape {
        center: RECT,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 180.0,
        height: 135.0,
    });
}

fn solid(s: &mut Scene) {
    s.mesh
        .create_box(BOX, 180.0, 120.0, 180.0, FORM_MATERIAL)
        .expect("a box");
}

fn circle(s: &mut Scene, center: DVec3, radius: f64) {
    s.execute(Command::DrawCircleAsCurve { center, normal: DVec3::Z, radius });
}

fn violations(s: &Scene) -> usize {
    s.mesh.verify_face_invariants().violations.len()
}

fn active_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// The scene, in the order the fuzz found it.
fn the_scene() -> Scene {
    let mut s = production();
    rect(&mut s);
    solid(&mut s);
    circle(&mut s, C110, 110.0);
    circle(&mut s, C70, 70.0);
    s
}

#[test]
fn four_operations_leave_the_mesh_non_manifold() {
    let s = the_scene();
    let report = s.mesh.verify_face_invariants();

    assert!(
        !report.is_valid(),
        "the scene is sound now. If that is a fix rather than a coincidence, \
         rewrite this file to guard it — see the header."
    );
    assert_eq!(
        report.violations.len(),
        4,
        "the count moved: {:?}",
        report.violations
    );
    assert_eq!(active_faces(&s), 19, "the face count moved");

    // Every one of them is the same shape: an edge carrying more faces than a
    // manifold allows, because two of them cover the same ground.
    for v in &report.violations {
        let text = format!("{v}");
        assert!(
            text.contains("non-manifold") && text.contains("stacked"),
            "a violation of a different kind appeared: {text}"
        );
    }
}

/// The scene is sound right up to the last draw — which is what makes the last
/// draw, and not the pile before it, the thing to look at.
#[test]
fn it_is_sound_until_the_final_draw() {
    let mut s = production();
    rect(&mut s);
    solid(&mut s);
    circle(&mut s, C110, 110.0);

    assert_eq!(violations(&s), 0, "the scene was already broken before the last draw");
    assert_eq!(active_faces(&s), 18);

    // And that draw does not touch the rect. ⚠ Measured by clamping the circle's
    // centre into the rectangle, not by comparing the centre distance to a
    // radius plus a half-width — that was the first version of this line and it
    // fired, because 158.1 mm of centre distance says nothing about two shapes
    // of different kinds. The rect spans x ∈ [−90, 90], y ∈ [32.5, 167.5]; the
    // circle sits at (50, −50) with r = 70, so the nearest rect point is
    // (50, 32.5) and the gap is 12.5 mm of clear ground.
    let nearest = DVec3::new(
        C70.x.clamp(RECT.x - 90.0, RECT.x + 90.0),
        C70.y.clamp(RECT.y - 67.5, RECT.y + 67.5),
        C70.z,
    );
    let clearance = (nearest - C70).length() - 70.0;
    assert!(
        clearance > 0.0,
        "the last circle overlaps the rect after all (clearance {clearance}), so          `whatever couples them is not proximity` in the header is wrong"
    );

    circle(&mut s, C70, 70.0);
    assert_eq!(violations(&s), 4, "the last draw stopped breaking it");
}

/// The reduction, kept as an assertion rather than as prose: drop any one of the
/// four and the mesh is sound.
///
/// ⚠ Without this, someone could "simplify" the scene above and quietly be
/// measuring something else. Each of these is a real run, not a claim.
#[test]
fn every_one_of_the_four_operations_is_needed() {
    let without_rect = {
        let mut s = production();
        solid(&mut s);
        circle(&mut s, C110, 110.0);
        circle(&mut s, C70, 70.0);
        s
    };
    let without_solid = {
        let mut s = production();
        rect(&mut s);
        circle(&mut s, C110, 110.0);
        circle(&mut s, C70, 70.0);
        s
    };
    let without_c110 = {
        let mut s = production();
        rect(&mut s);
        solid(&mut s);
        circle(&mut s, C70, 70.0);
        s
    };
    let without_c70 = {
        let mut s = production();
        rect(&mut s);
        solid(&mut s);
        circle(&mut s, C110, 110.0);
        s
    };

    for (name, s) in [
        ("without the rect", &without_rect),
        ("without the solid", &without_solid),
        ("without the 110 circle", &without_c110),
        ("without the 70 circle", &without_c70),
    ] {
        assert_eq!(
            violations(s),
            0,
            "{name}: still {} violations, so the scene above is not minimal",
            violations(s)
        );
    }

    // The control. Without it every line above would also pass on an engine
    // that never violates anything.
    assert_eq!(violations(&the_scene()), 4, "the full scene stopped failing");
}
