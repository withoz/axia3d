//! FACES THAT MEET AT AN ANGLE, measured.
//!
//! 사용자 (2026-08-03): `면 교차시 분할이 안되서 그런가?` → `교차분할까지 기능을 확장`
//!
//! Two faces on the same plane get divided where they overlap — that is the
//! coplanar re-derive, and it has been there a long time. Two faces meeting at
//! an ANGLE were not divided at all: they simply passed through each other. A
//! shape drawn across another looked whole and cut nothing.
//!
//! This file pins what that split does now, including where it stops. The
//! numbers are measured, not intended — when one improves, lower it here and
//! say which case changed.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

fn rect(s: &mut Scene, c: DVec3, n: DVec3, up: DVec3, w: f64, h: f64) -> CommandResult {
    s.execute(Command::DrawRectAsShape { center: c, normal: n, up, width: w, height: h })
}

/// The case the user reported: a shape drawn across another face.
#[test]
fn a_face_drawn_across_another_divides_it_and_itself() {
    let mut s = prod();
    rect(&mut s, DVec3::ZERO, DVec3::Z, DVec3::Y, 200.0, 200.0);
    assert_eq!(faces(&s), 1);

    let r = rect(&mut s, DVec3::ZERO, DVec3::X, DVec3::Z, 200.0, 200.0);
    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");

    assert_eq!(
        faces(&s), 4,
        "each face is cut in two where the other passes through it"
    );
    assert_eq!(
        s.mesh.detect_self_intersections().count(), 0,
        "once divided, nothing is passing through anything"
    );
    assert!(s.mesh.verify_face_invariants().is_valid());
}

/// A draw elsewhere in the scene must not carve up geometry the user did not
/// touch. Two solids may sit inside each other quite legitimately.
#[test]
fn an_unrelated_draw_leaves_existing_crossings_alone() {
    let mut s = prod();
    s.mesh.create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL).unwrap();
    s.mesh.create_box(DVec3::new(150.0, 150.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL).unwrap();
    let before = faces(&s);
    assert!(s.mesh.detect_self_intersections().count() > 0, "fixture must already cross");

    let r = rect(&mut s, DVec3::new(9000.0, 9000.0, 0.0), DVec3::Z, DVec3::Y, 100.0, 100.0);
    assert!(!matches!(r, CommandResult::Error(_)));
    assert_eq!(
        faces(&s), before + 1,
        "only the drawn sheet is added — the two boxes are none of its business"
    );
}

/// Where it stops. One pass divides what it can see; a face cut in two can
/// reach a crossing its whole self did not, and chasing those made things worse
/// (on a tilted plane the follow-up rounds damaged the mesh and the whole draw
/// was rolled back). So these end up partly divided, and that is recorded here
/// rather than hidden.
#[test]
fn more_than_one_partner_is_only_partly_divided() {
    let mut s = prod();
    rect(&mut s, DVec3::new(-80.0, 0.0, 0.0), DVec3::Z, DVec3::Y, 100.0, 100.0);
    rect(&mut s, DVec3::new(80.0, 0.0, 0.0), DVec3::Z, DVec3::Y, 100.0, 100.0);
    let r = rect(&mut s, DVec3::ZERO, DVec3::Y, DVec3::Z, 500.0, 400.0);
    assert!(!matches!(r, CommandResult::Error(_)));
    assert_eq!(faces(&s), 5, "3 sheets, one of them cut in two");
    assert_eq!(
        s.mesh.detect_self_intersections().count(), 4,
        "the rest still pass through — one pass does not reach them"
    );
    assert!(s.mesh.verify_face_invariants().is_valid(), "but nothing is damaged");
}

#[test]
fn a_tilted_crossing_is_partly_divided_too() {
    let mut s = prod();
    rect(&mut s, DVec3::ZERO, DVec3::Z, DVec3::Y, 200.0, 200.0);
    let n = DVec3::new(1.0, 0.0, 1.0).normalize();
    let up = DVec3::new(-1.0, 0.0, 1.0).normalize();
    let r = rect(&mut s, DVec3::ZERO, n, up, 300.0, 300.0);
    assert!(!matches!(r, CommandResult::Error(_)));
    assert_eq!(faces(&s), 3);
    assert_eq!(s.mesh.detect_self_intersections().count(), 2);
    assert!(s.mesh.verify_face_invariants().is_valid());
}

/// The crossing split leaves curved primitives alone (ADR-197): a sphere's
/// faces carry an analytic surface and are skipped, so nothing here carves a
/// sheet by a silhouette.
///
/// A sheet drawn through a sphere still comes out in two pieces — measured both
/// with and without the crossing split, unchanged either way, so that division
/// comes from somewhere else and is recorded here so a later change to it is
/// not mistaken for this one. Both kinds of sphere are shown because the
/// polygonal one genuinely IS a pile of flat faces and would take part.
#[test]
fn a_sheet_through_a_sphere_splits_the_same_as_it_always_did() {
    let mut kernel = prod();
    kernel.mesh.create_sphere_kernel_native(DVec3::ZERO, 100.0, FORM_MATERIAL).unwrap();
    let k_before = faces(&kernel);
    let r = rect(&mut kernel, DVec3::ZERO, DVec3::Z, DVec3::Y, 400.0, 400.0);
    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    assert_eq!(
        faces(&kernel), k_before + 2,
        "the sheet comes out in two pieces — not the crossing split's doing"
    );

    let mut poly = prod();
    poly.mesh.create_sphere(DVec3::ZERO, 100.0, 16, 16, FORM_MATERIAL).unwrap();
    let p_before = faces(&poly);
    let r = rect(&mut poly, DVec3::ZERO, DVec3::Z, DVec3::Y, 400.0, 400.0);
    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    assert_eq!(faces(&poly), p_before + 2, "the same two pieces");
}
