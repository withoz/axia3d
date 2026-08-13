//! Cutting somebody's face does not hand it over.
//!
//! Drawing a line across a box's top used to give BOTH halves to the drawn
//! entity. Measured 2026-08-11 in the running app, with the geometry unchanged:
//!
//! ```text
//!   box                isSolid true   volume 8,000,000   list and index agree
//!   + one line on top  isSolid FALSE  volume 7,333,333   they disagree (face 1)
//! ```
//!
//! A user reads that as "the solid lost a face". Nothing was lost — the face
//! stopped being theirs.
//!
//! Two things were wrong and both are fixed here:
//!   1. the split pieces were adopted by the drawn entity even though their host
//!      already had an owner;
//!   2. the reconcile that clears dead links walks the reverse index, so it
//!      cannot see a face that was dropped from the index while staying in its
//!      owner's list — and something does exactly that.
//!
//! ⚠ **The hand-back happens AFTER the arrangement, never during it.** The
//! post-draw arrangement reads the ownership maps — its coplanar reconcile works
//! out `region_shapes` from `face_to_shape` — so giving a face away mid-flight
//! changes what it decides. Measured both ways on the 5-rect chain:
//!
//! ```text
//!   hand back inside the split loop   area 5,260,000   (an inner rect unabsorbed)
//!   hand back after the arrangement   area 4,940,000   correct
//! ```
//!
//! `five_rects_with_small_inner` in `six_rect_chain.rs` is what catches it, so
//! that is where the ordering is really pinned; this file states why.

use axia_core::scene::Scene;
use axia_core::xia::XiaId;
use axia_core::Command;
use glam::DVec3;

fn boxed_scene() -> (Scene, XiaId) {
    let mut scene = Scene::new();
    let faces = scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let xia = scene.create_xia_with_faces("Box".to_string(), DVec3::ZERO, faces);
    (scene, xia)
}

fn draw_line(scene: &mut Scene, a: DVec3, b: DVec3) {
    scene.execute(Command::DrawLineAsShape { start: a, end: b, surface_normal: None });
}

/// Faces the owner lists, and how many of those the index agrees are its.
fn listed_and_agreeing(scene: &Scene, xia: XiaId) -> (usize, usize) {
    let listed = &scene.xias[&xia].face_ids;
    let agreeing = listed.iter().filter(|f| scene.get_xia_for_face(**f) == Some(xia)).count();
    (listed.len(), agreeing)
}

#[test]
fn a_line_across_a_box_top_leaves_the_box_owning_both_halves() {
    let (mut scene, xia) = boxed_scene();
    assert_eq!(listed_and_agreeing(&scene, xia), (6, 6));

    // Edge to edge across the top face — this really does split it.
    draw_line(&mut scene, DVec3::new(0.0, -100.0, 100.0), DVec3::new(0.0, 100.0, 100.0));

    let active = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(active, 7, "the top should have become two faces");

    let (listed, agreeing) = listed_and_agreeing(&scene, xia);
    assert_eq!(listed, 7, "the box should own both halves, not lose its top");
    assert_eq!(agreeing, listed, "the owner's list and the reverse index disagree");
}

/// The number a user sees. An open shell makes the divergence sum meaningless,
/// so this is the assertion that would have caught it.
#[test]
fn the_box_is_still_a_solid_of_the_same_volume() {
    let (mut scene, xia) = boxed_scene();
    let before = axia_core::promote::face_set_volume(&scene.mesh, &scene.xias[&xia].face_ids);
    assert!((before - 8_000_000.0).abs() < 1.0, "got {before}");

    draw_line(&mut scene, DVec3::new(0.0, -100.0, 100.0), DVec3::new(0.0, 100.0, 100.0));

    let after = axia_core::promote::face_set_volume(&scene.mesh, &scene.xias[&xia].face_ids);
    assert!(
        (after - 8_000_000.0).abs() < 1.0,
        "drawing a line changed the volume to {after} — the geometry did not move",
    );
}

/// A line on nobody's face still creates the drawn thing. The rule is "keep the
/// host's owner", not "never create".
#[test]
fn a_line_in_empty_space_still_makes_its_own_shape() {
    let mut scene = Scene::new();
    let before = scene.shapes.len();
    draw_line(&mut scene, DVec3::new(0.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 0.0));
    assert!(scene.shapes.len() > before, "a free line should still make a Shape");
}

/// The second half: a dead id must not survive in an owner's list just because
/// the reverse index already forgot it.
#[test]
fn no_owner_names_a_face_that_is_gone() {
    let (mut scene, xia) = boxed_scene();
    draw_line(&mut scene, DVec3::new(0.0, -100.0, 100.0), DVec3::new(0.0, 100.0, 100.0));

    for f in &scene.xias[&xia].face_ids {
        assert!(
            scene.mesh.faces.get(*f).is_some_and(|fc| fc.is_active()),
            "the Xia still names {f:?}, which the mesh no longer has",
        );
    }
    for shape in scene.shapes.values() {
        for f in &shape.face_ids {
            assert!(
                scene.mesh.faces.get(*f).is_some_and(|fc| fc.is_active()),
                "a Shape still names {f:?}, which the mesh no longer has",
            );
        }
    }
}

/// Two lines meeting exactly on a solid EDGE — the case the complaint named.
/// Both faces split, and the box keeps every piece.
#[test]
fn a_line_bending_over_a_solid_edge_splits_both_faces() {
    let (mut scene, xia) = boxed_scene();
    draw_line(&mut scene, DVec3::new(0.0, -100.0, 100.0), DVec3::new(0.0, 100.0, 100.0));
    draw_line(&mut scene, DVec3::new(0.0, 100.0, 100.0), DVec3::new(0.0, 100.0, -100.0));

    let active = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(active, 8, "top and front should each have split");

    let (listed, agreeing) = listed_and_agreeing(&scene, xia);
    assert_eq!(listed, 8);
    assert_eq!(agreeing, listed);
    let vol = axia_core::promote::face_set_volume(&scene.mesh, &scene.xias[&xia].face_ids);
    assert!((vol - 8_000_000.0).abs() < 1.0, "got {vol}");
}
