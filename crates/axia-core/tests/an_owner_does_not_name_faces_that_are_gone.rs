//! What an owner still claims after a carve re-derives its faces.
//!
//! `adopt_new_active_faces` is the prune-then-add pattern of `scene.rs:3650` with
//! the `retain` half missing: it adds what the op created and never drops what
//! the op killed. Each drill re-derives its two host caps, so each drill leaves
//! exactly two dead ids behind. Measured 2026-08-11 over three parallel bores:
//!
//! ```text
//!   listed/active   6/6 → 24/22 → 42/38 → 60/54
//! ```
//!
//! That is not merely untidy. Slice and Trim take the owner's list verbatim and
//! `slice.rs` aborts on the first id it cannot find, so **a drilled solid could
//! not be cut at all**:
//!
//! ```text
//!   clean box     sliceVolumeByPlane → {"ok":true,"newXia":2}
//!   drilled box   sliceVolumeByPlane → {"ok":false,"error":"slice: face FaceId(4) not found"}
//! ```
//!
//! Dropping by liveness is safe because a face never comes back — nothing calls
//! `set_active(true)` on one — and undo restores the whole snapshot, list
//! included.

use axia_core::scene::Scene;
use axia_core::xia::XiaId;
use glam::DVec3;

const R: f64 = 15.0;
const SEG: u32 = 16;

fn boxed_scene() -> (Scene, XiaId) {
    let mut scene = Scene::new();
    let faces = scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let xia = scene.create_xia_with_faces("Box".to_string(), DVec3::ZERO, faces);
    (scene, xia)
}

/// (what the owner lists, how many of those the mesh still has and draws)
fn listed_vs_live(scene: &Scene, xia: XiaId) -> (usize, usize) {
    let listed = &scene.xias[&xia].face_ids;
    let live = listed
        .iter()
        .filter(|f| scene.mesh.faces.get(**f).is_some_and(|fc| fc.is_active()))
        .count();
    (listed.len(), live)
}

#[test]
fn a_drill_leaves_nothing_dead_behind() {
    let (mut scene, xia) = boxed_scene();
    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");
    let (listed, live) = listed_vs_live(&scene, xia);
    assert_eq!(listed, live, "the owner lists {listed} faces but only {live} exist");
}

/// The accumulation is what made it grow past noticing. Three bores that do not
/// cross each other, so all three actually cut.
#[test]
fn bore_after_bore_does_not_accumulate() {
    let (mut scene, xia) = boxed_scene();
    for z in [-60.0_f64, 0.0, 60.0] {
        scene
            .drill_circular_through_hole(DVec3::new(100.0, 0.0, z), DVec3::X, R, SEG)
            .expect("drill");
        let (listed, live) = listed_vs_live(&scene, xia);
        assert_eq!(listed, live, "after the bore at z={z}: {listed} listed, {live} live");
    }
}

/// The failure a user actually hit.
#[test]
fn a_drilled_solid_can_still_be_cut() {
    let (mut scene, xia) = boxed_scene();
    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");

    // Exactly what SliceTool sends: the owner's list, unfiltered.
    let ids = scene.xias[&xia].face_ids.clone();
    // Clear of the bore, which spans z in [-15, 15].
    let plane = axia_geo::operations::slice::SlicePlane::new(
        DVec3::new(0.0, 0.0, 70.0),
        DVec3::Z,
    )
    .expect("plane");
    let res = scene.mesh.slice_volume_by_plane(&ids, plane, Default::default());
    assert!(res.is_ok(), "slicing a drilled solid failed: {:?}", res.err());
}

/// The control: the same cut on a solid nobody drilled must be unaffected.
#[test]
fn a_clean_solid_is_unaffected() {
    let (scene, xia) = boxed_scene();
    let (listed, live) = listed_vs_live(&scene, xia);
    assert_eq!((listed, live), (6, 6));

    let mut scene = scene;
    let ids = scene.xias[&xia].face_ids.clone();
    let plane =
        axia_geo::operations::slice::SlicePlane::new(DVec3::ZERO, DVec3::Z).expect("plane");
    assert!(scene.mesh.slice_volume_by_plane(&ids, plane, Default::default()).is_ok());
}

/// A second solid, untouched by the drill, keeps its list exactly.
///
/// What this really guards is a prune that takes LIVE faces — inverting the
/// liveness test empties B and fails here. It would NOT catch a prune widened to
/// every owner, because a correct prune over an untouched owner removes nothing;
/// that widening is harmless, which is why nothing tests against it.
#[test]
fn another_solids_list_is_left_alone() {
    let mut scene = Scene::new();
    let a_faces = scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box a");
    let a = scene.create_xia_with_faces("A".to_string(), DVec3::ZERO, a_faces);
    let b_faces = scene
        .mesh
        .create_box(DVec3::new(600.0, 0.0, 0.0), 200.0, 200.0, 200.0, Default::default())
        .expect("box b");
    let b = scene.create_xia_with_faces("B".to_string(), DVec3::new(600.0, 0.0, 0.0), b_faces);
    let b_before = scene.xias[&b].face_ids.clone();

    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill into A");

    assert_eq!(scene.xias[&b].face_ids, b_before, "B's list changed");
    let (listed, live) = listed_vs_live(&scene, a);
    assert_eq!(listed, live, "A: {listed} listed, {live} live");
}

/// The reverse index has to lose the dead ids too, or `get_xia_for_face` keeps
/// answering for a face that is gone — which is how `solid_owner_of` failed to
/// catch the slice before it reached the engine.
#[test]
fn the_reverse_index_forgets_them_too() {
    let (mut scene, xia) = boxed_scene();
    let before: Vec<_> = scene.xias[&xia].face_ids.clone();
    scene
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, SEG)
        .expect("drill");

    for f in before {
        if scene.mesh.faces.get(f).is_some_and(|fc| fc.is_active()) {
            continue;
        }
        assert!(
            scene.get_xia_for_face(f).is_none(),
            "face {f:?} is gone but the index still names an owner",
        );
    }
}
