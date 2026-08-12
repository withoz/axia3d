//! A bore's walls belong to the solid they were cut into.
//!
//! ADR-203 §36 called missing face ownership "측정으로 밝혀진 진짜 블로커" and fixed
//! it for exactly three ops — the door, the rect drill and the rect punch. The
//! circular, polygon and crossing drills were never promoted to the Scene layer,
//! so the same gesture gave a different result. Measured 2026-08-11 in the
//! running app on a 200³ box with a Ø80 bore:
//!
//! ```text
//!   rect drill      Xia owns every face   IFC → 1 element  'Box'
//!   circular drill  Xia owns 6 of 38      IFC → 2 elements 'Box' + orphan 'Model'
//!                                         Xia volume 5,333,333 (true 6,994,690)
//! ```
//!
//! Nothing anywhere stated that drilled faces should be unowned; the machinery
//! (`capture_host_owner` + `adopt_new_active_faces`) already existed and was
//! already wired into the rect siblings. These tests hold all five to it.

use axia_core::scene::Scene;
use axia_core::xia::XiaId;
use glam::DVec3;

const R: f64 = 40.0;
const SEG: u32 = 32;

/// A 200³ box owned by one Xia, and that Xia's id.
fn boxed_scene() -> (Scene, XiaId) {
    let mut scene = Scene::new();
    let faces = scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let xia = scene.create_xia_with_faces("Box".to_string(), DVec3::ZERO, faces);
    (scene, xia)
}

/// Every active face, and how many of them no Xia claims.
fn census(scene: &Scene) -> (usize, usize) {
    let active: Vec<_> = scene
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    let unowned = active.iter().filter(|f| scene.get_xia_for_face(**f).is_none()).count();
    (active.len(), unowned)
}

#[test]
fn a_circular_bore_leaves_no_face_unowned() {
    let (mut scene, xia) = boxed_scene();
    let tube = scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");
    assert!(tube > 0, "the drill has to actually cut something");

    let (active, unowned) = census(&scene);
    assert_eq!(unowned, 0, "{active} active faces, {unowned} owned by nobody");
    // And the owner really lists them — not just the reverse index.
    let owned = &scene.xias[&xia].face_ids;
    for (fid, f) in scene.mesh.faces.iter() {
        if f.is_active() {
            assert!(owned.contains(&fid), "face {fid:?} is active but not in the Xia's list");
        }
    }
}

/// The number the Inspector shows. With the tube walls missing it read
/// 5,333,333 for a solid whose true volume is 6,994,690 — an OPEN surface, so
/// the divergence sum is meaningless rather than merely wrong.
#[test]
fn the_owner_volume_matches_the_whole_mesh() {
    let (mut scene, xia) = boxed_scene();
    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");

    let ids: Vec<_> = scene.xias[&xia]
        .face_ids
        .iter()
        .copied()
        .filter(|f| scene.mesh.faces.contains(*f))
        .collect();
    let owner_volume = axia_core::promote::face_set_volume(&scene.mesh, &ids);
    let whole = scene.mesh.mesh_volume();
    let truth = 200.0_f64.powi(3) - std::f64::consts::PI * R * R * 200.0;

    assert!((whole - truth).abs() < 1.0, "whole mesh {whole} vs {truth}");
    assert!(
        (owner_volume - whole).abs() < 1.0,
        "the owner sees {owner_volume}, the mesh {whole} — the Xia is missing faces",
    );
}

#[test]
fn a_polygon_bore_leaves_no_face_unowned() {
    let (mut scene, _) = boxed_scene();
    let loop_pts = [
        DVec3::new(100.0, -40.0, -40.0),
        DVec3::new(100.0, 40.0, -40.0),
        DVec3::new(100.0, 40.0, 40.0),
        DVec3::new(100.0, -40.0, 40.0),
    ];
    let tube = scene.drill_polygon_through_hole(&loop_pts, DVec3::X).expect("drill");
    assert!(tube > 0);
    let (active, unowned) = census(&scene);
    assert_eq!(unowned, 0, "{active} active, {unowned} unowned");
}

#[test]
fn the_punches_leave_no_face_unowned() {
    let (mut scene, _) = boxed_scene();
    scene
        .punch_circular_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("punch");
    assert_eq!(census(&scene).1, 0, "circular punch orphaned a face");

    let (mut scene, _) = boxed_scene();
    let loop_pts = [
        DVec3::new(100.0, -40.0, -40.0),
        DVec3::new(100.0, 40.0, -40.0),
        DVec3::new(100.0, 40.0, 40.0),
        DVec3::new(100.0, -40.0, 40.0),
    ];
    scene.punch_polygon_hole(&loop_pts, DVec3::X).expect("punch");
    assert_eq!(census(&scene).1, 0, "polygon punch orphaned a face");
}

/// The crossing bore is the biggest of them — 128 walls on top of the first
/// bore's 32, so an ownership gap here is the most visible of all.
#[test]
fn a_crossing_bore_leaves_no_face_unowned() {
    let (mut scene, _) = boxed_scene();
    scene
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, SEG)
        .expect("first bore");
    let walls = scene
        .drill_crossing_bore(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("crossing");
    assert!(walls > 0);

    let (active, unowned) = census(&scene);
    assert_eq!(unowned, 0, "{active} active, {unowned} unowned");
    assert!(scene.mesh.verify_face_invariants().is_valid());
}

/// A host nobody owns must not acquire an owner — the reconcile adopts INTO an
/// existing element, it does not invent one.
#[test]
fn an_unowned_host_stays_unowned() {
    let mut scene = Scene::new();
    scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");
    let (active, unowned) = census(&scene);
    assert_eq!(unowned, active, "no Xia existed, so nothing should be owned");
    assert!(scene.xias.is_empty());
}
