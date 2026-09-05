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

// ── Where it comes from, and why nothing clears it ───────────────────────
//
// Measured 2026-09-05, after the reduction. Both halves are here because the
// next person's first two questions are "which mechanism" and "why does the
// repair not fix it", and answering them by hand costs an hour.

/// One flag decides it, and it is not the one that draws.
///
/// ```text
///   intersect  synth  rederive  freeform   faces   violations
///     true     true     true      true       19        4
///     false    true     true      true       19        4
///     true     false    true      true       19        4
///     true     true     FALSE     true       10        0
///     true     true     true      false      19        4
/// ```
///
/// The re-derive alone. It also nearly triples the faces on that plane — five
/// without it, fourteen with — and four of those are 7 mm² slivers.
#[test]
fn only_the_re_derive_makes_them() {
    let mut with = production();
    let mut without = production();
    without.face_rederive_on_draw = false;
    for s in [&mut with, &mut without] {
        rect(s);
        solid(s);
        circle(s, C110, 110.0);
        circle(s, C70, 70.0);
    }

    assert_eq!(violations(&with), 4);
    assert_eq!(
        violations(&without),
        0,
        "turning the re-derive off no longer clears it, so something else makes \
         them now and the diagnosis below is stale"
    );
    assert!(
        active_faces(&with) > active_faces(&without),
        "the re-derive stopped adding faces: {} against {}",
        active_faces(&with),
        active_faces(&without)
    );

    // The slivers. ⚠ Their existence is not the defect — a fine arrangement may
    // well produce small regions. Two of them being STACKED on a larger face is.
    let tiny = with
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && with.mesh.face_outer_area(*fid) < 50.0)
        .count();
    assert!(tiny >= 2, "the 7 mm² slivers are gone; only {tiny} tiny faces left");
}

/// The repair declines, and the two pairs decline for different reasons.
///
/// ```text
///   pair    in the detector's list?   verts     crossings / lens
///   24x16          no                 1 / 4        0 / 4
///    2x19          yes                8 / 7        2 / 5
/// ```
///
/// The first is invisible to `detect_self_intersections` at all: its larger side
/// is a one-vertex closed-curve face carrying a surface, which ADR-271 skips on
/// purpose. And `coplanar_intersection_segments` gives it a non-empty lens with
/// ZERO crossings — the shape ADR-128's vertex-on-edge fallback exists for,
/// which does not fire here.
///
/// The second is seen, has an even crossing count, and passes every gate the
/// repair applies — and the repair still changes nothing. ⚠ That may be correct:
/// both faces are IN a volume, and two solids standing in the same place is a
/// legal model (LOCKED #105). What is not legal is the non-manifold edge it
/// leaves, so the answer is not simply "make the repair subtract one of them".
#[test]
fn the_repair_declines_and_the_two_pairs_decline_differently() {
    use axia_geo::operations::coplanar as cop;

    let mut s = the_scene();
    let seen = s.mesh.detect_self_intersections().intersecting_pairs;

    // Recover the stacked pairs from the report rather than naming face ids,
    // which shift the moment anything upstream changes.
    let report = s.mesh.verify_face_invariants();
    let mut invisible = 0usize;
    let mut passes_every_gate = 0usize;
    for v in &report.violations {
        let text = format!("{v}");
        let ids: Vec<u32> = text
            .split("FaceId(")
            .skip(1)
            .filter_map(|t| t.split(')').next().and_then(|n| n.parse().ok()))
            .collect();
        if ids.len() != 2 {
            continue;
        }
        let (fa, fb) = (axia_geo::FaceId::new(ids[0]), axia_geo::FaceId::new(ids[1]));
        let listed = seen
            .iter()
            .any(|(x, y)| (*x == fa && *y == fb) || (*x == fb && *y == fa));
        let vc = |f| s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).map(|v| v.len()).unwrap_or(0);
        let (big, small) = if vc(fa) >= vc(fb) { (fa, fb) } else { (fb, fa) };
        let crossings = cop::coplanar_intersection_segments(&s.mesh, big, small)
            .map(|ci| ci.crossings.len())
            .unwrap_or(usize::MAX);
        if !listed {
            invisible += 1;
        } else if crossings >= 2 && crossings % 2 == 0 {
            passes_every_gate += 1;
        }
    }
    assert!(invisible > 0, "no pair is invisible to the detector any more");
    assert!(passes_every_gate > 0, "no pair reaches the repair's arithmetic any more");

    // And asked to fix everything — an EMPTY "already overlapping" set, which is
    // how you say "nothing here was broken before, repair what you find" — it
    // changes nothing.
    let nothing_was_already_broken = std::collections::HashSet::new();
    let changed = s.subtract_double_covered_faces(&nothing_was_already_broken);
    assert_eq!(
        changed, 0,
        "the repair now changes {changed} thing(s) on this scene. If it clears the \
         violations, rewrite this file to guard that."
    );
    assert_eq!(violations(&s), 4, "the repair moved the violations without being asked");
}
