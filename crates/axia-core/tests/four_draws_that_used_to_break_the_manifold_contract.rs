//! THE SMALLEST SCENE THAT USED TO LEAVE THE MESH NON-MANIFOLD.
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
//! It was. This scene failed on main, with production flags, in four
//! operations, and dropping any one of the four cleared it:
//!
//! ```text
//!   rect, box, c110, c70   19 faces   4 violations
//!         box, c110, c70    9 faces   0
//!   rect,      c110, c70   13 faces   0
//!   rect, box,       c70    8 faces   0
//!   rect, box, c110        18 faces   0
//! ```
//!
//! ## What it turned out to be
//!
//! Two of the four violations named a one-vertex closed-curve face, which sent
//! the first day of looking towards `can_punch_contained` and the containment
//! branch that refuses to polygonise a circle. That was the wrong end. The
//! punch was not being refused — it had already happened, and it was the thing
//! that broke the mesh:
//!
//! ```text
//!   EdgeId(47)   FaceId(14) sliver outer + FaceId(19) outer + FaceId(2)  HOLE
//!   EdgeId(50)   FaceId(16) sliver outer + FaceId(18) outer + FaceId(24) HOLE
//! ```
//!
//! The re-derive lays a fine arrangement on z=100, and where the rectangle's
//! corner pokes outside the r=110 circle it leaves a 6.58 mm² sliver. That
//! sliver is geometrically inside the box's bottom face and inside the r=70
//! circle, so the containment pass offered it to them as a hole — and the
//! reparent took it, because it asked whether the particular twin half-edge it
//! wanted was free rather than whether the EDGE still had a side to give. It
//! did not: the sliver's arc edges already carried the sliver and its
//! neighbouring arrangement cell.
//!
//! The fix is in `axia-geo/src/operations/annulus.rs::reject_used_boundary`,
//! and the rule itself is pinned next to it in
//! `axia-geo/tests/a_hole_needs_a_free_side.rs`. This file guards the
//! consequence: the scene that found it is sound, and sound in the particular
//! way the fix produces — nothing merged, nothing deleted, one reparent
//! declined.
//!
//! ## What the fix does NOT do, measured
//!
//! It does not make the overlap go away, and it should not: the box bottom and
//! the sliver really do lie on the same ground, and two things standing in the
//! same place is a legal model (LOCKED #105). Before, the fictional hole hid
//! one of those overlaps from the report while paying for it with a
//! non-manifold edge. Now it is reported:
//!
//! ```text
//!                     violations   non-manifold edges   damaging contacts
//!   before                 4               4                   10
//!   after                  0               0                   11
//! ```
//!
//! The scene also leaves `is_closed_solid` false either way — a box with a
//! sheet drawn across its bottom face. That is untouched by this, and is not
//! what the four violations were.

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
fn four_operations_leave_the_mesh_sound() {
    let s = the_scene();
    let report = s.mesh.verify_face_invariants();

    assert!(
        report.is_valid(),
        "the four draws are non-manifold again: {:?}",
        report.violations
    );
    assert!(
        s.mesh.collect_non_manifold_edges().is_empty(),
        "an edge is carrying three faces again: {:?}",
        s.mesh.collect_non_manifold_edges()
    );
    assert_eq!(active_faces(&s), 19, "the face count moved");
}

/// Sound the same way it was before the last draw, which is the point: the
/// fourth operation adds a face and adds no damage.
#[test]
fn the_final_draw_adds_a_face_and_no_damage() {
    let mut s = production();
    rect(&mut s);
    solid(&mut s);
    circle(&mut s, C110, 110.0);

    assert_eq!(violations(&s), 0, "already broken before the last draw");
    assert_eq!(active_faces(&s), 18);

    // And that draw does not touch the rect. ⚠ Measured by clamping the
    // circle's centre into the rectangle, not by comparing the centre distance
    // to a radius plus a half-width — that was the first version of this line
    // and it fired, because 158.1 mm of centre distance says nothing about two
    // shapes of different kinds. The rect spans x ∈ [−90, 90], y ∈ [32.5,
    // 167.5]; the circle sits at (50, −50) with r = 70, so the nearest rect
    // point is (50, 32.5) and the gap is 12.5 mm of clear ground. It touches
    // the BOX, and the rect is 12.5 mm away and was still necessary.
    let nearest = DVec3::new(
        C70.x.clamp(RECT.x - 90.0, RECT.x + 90.0),
        C70.y.clamp(RECT.y - 67.5, RECT.y + 67.5),
        C70.z,
    );
    let clearance = (nearest - C70).length() - 70.0;
    assert!(clearance > 0.0, "the last circle overlaps the rect after all ({clearance})");

    circle(&mut s, C70, 70.0);
    assert_eq!(violations(&s), 0, "the last draw broke it again");
    assert_eq!(active_faces(&s), 19, "it stopped adding a face");
}

/// The reduction, kept as an assertion: the four-operation scene was the odd
/// one out and now it is not. Every variant is sound.
///
/// ⚠ This no longer establishes minimality — with the full scene sound too,
/// nothing here distinguishes it, and the numbers that did are in the header,
/// from the run that found them. What it still buys is breadth: five
/// neighbouring scenes through the same arrangement code, every one checked.
#[test]
fn the_whole_family_is_sound() {
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

    for (name, s, faces) in [
        ("without the rect", &without_rect, 9),
        ("without the solid", &without_solid, 13),
        ("without the 110 circle", &without_c110, 8),
        ("without the 70 circle", &without_c70, 18),
        ("all four", &the_scene(), 19),
    ] {
        assert_eq!(violations(s), 0, "{name}: {} violations", violations(s));
        assert_eq!(active_faces(s), faces, "{name}: the face count moved");
    }
}

// ── What the fix actually changed, so a reader is not left guessing ──────

/// The arrangement is untouched. The re-derive still lays down the same
/// fourteen faces on z=100 including the four 6.58 mm² slivers; what stopped is
/// one reparent.
///
/// ```text
///                       faces   slivers   inner loops   violations
///   re-derive on          19       4           0             0
///   re-derive off         10       0           0             0
/// ```
///
/// The zero in the third column is the fix seen from the scene: before it, the
/// box's bottom face and the r=70 circle each held a sliver as a hole, and each
/// of those holes was a third face on an edge that already had two.
#[test]
fn nothing_was_merged_or_deleted_and_no_container_holds_a_sliver() {
    let mut with = production();
    let mut without = production();
    without.face_rederive_on_draw = false;
    for s in [&mut with, &mut without] {
        rect(s);
        solid(s);
        circle(s, C110, 110.0);
        circle(s, C70, 70.0);
    }

    assert_eq!(violations(&with), 0);
    assert_eq!(violations(&without), 0);
    assert!(
        active_faces(&with) > active_faces(&without),
        "the re-derive stopped adding faces: {} against {}",
        active_faces(&with),
        active_faces(&without)
    );

    // The slivers are still there. ⚠ Their existence was never the defect — a
    // fine arrangement may well produce small regions.
    let tiny = with
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && with.mesh.face_outer_area(*fid) < 50.0)
        .count();
    assert_eq!(tiny, 4, "the 7 mm² slivers moved: {tiny} of them");

    // And not one of them is anybody's hole.
    let inner_loops: usize = with
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(_, f)| f.inners().len())
        .sum();
    assert_eq!(
        inner_loops, 0,
        "a container took a hole on this scene again — that is the reparent the \
         gate declines, and it is what put a third face on the sliver's arc edge"
    );
}

/// Why no container may take one, stated at the sliver rather than at the
/// container: two of each sliver's four edges are already full.
///
/// ```text
///   each 6.58 mm² sliver, faces per boundary edge:   [1, 1, 2, 2]
/// ```
///
/// The two carrying two are its arc edges, shared with the neighbouring
/// arrangement cell. A hole loop needs the other side of every edge, and on
/// those two there is no other side left.
#[test]
fn each_sliver_has_two_edges_that_are_already_full() {
    let s = the_scene();
    let m = &s.mesh;

    let mut seen = 0usize;
    for (fid, f) in m.faces.iter() {
        if !f.is_active() || m.face_outer_area(fid) >= 50.0 {
            continue;
        }
        seen += 1;
        let hes = m.collect_loop_hes(f.outer().start).expect("the sliver's loop");
        let mut counts: Vec<usize> = hes
            .iter()
            .map(|&h| m.get_faces_sharing_edge(m.hes[h].edge()).0.len())
            .collect();
        counts.sort_unstable();
        assert_eq!(
            counts,
            vec![1, 1, 2, 2],
            "sliver {fid:?} no longer has exactly two full edges, so the reason \
             the gate refuses it has changed"
        );
    }
    assert_eq!(seen, 4, "the slivers are gone; nothing here was measured");
}

/// The overlap is left alone, deliberately.
///
/// Both faces of every stacked pair are in a volume, and two solids standing in
/// the same place is a legal model (LOCKED #105) — so `subtract_double_covered_
/// faces` changing nothing is the right answer, not a second defect. ⚠ Do not
/// "fix" this by deleting one of them; what was illegal was the non-manifold
/// edge, and that is gone.
#[test]
fn the_overlap_is_reported_and_not_repaired_away() {
    let mut s = the_scene();

    let reported = s.mesh.damaging_contacts().len();
    assert!(
        reported > 0,
        "the overlap stopped being reported — the box bottom and the sliver do \
         lie on the same ground, and hiding that was what the fictional hole did"
    );

    let nothing_was_already_broken = std::collections::HashSet::new();
    let changed = s.subtract_double_covered_faces(&nothing_was_already_broken);
    assert_eq!(changed, 0, "the repair now changes {changed} thing(s) on this scene");
    assert_eq!(violations(&s), 0, "the repair left the mesh non-manifold");
    assert_eq!(s.mesh.damaging_contacts().len(), reported, "it moved the overlap instead");
}
