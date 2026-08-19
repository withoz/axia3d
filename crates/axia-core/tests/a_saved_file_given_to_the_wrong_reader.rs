//! A saved snapshot handed to the headerless reader restored nothing, silently.
//!
//! `Scene` has two readers. `restore_scene_snapshot` is the internal one used
//! for undo frames — no header, the first field is a u64 mesh length.
//! `import_versioned_snapshot` is what a saved file goes through: `"AXIA"`, a
//! version, then the same sections.
//!
//! Give a versioned snapshot to the headerless one and the magic is read as a
//! length. `0x41495841` is 1,095,455,297, so the bounds test fails, the legacy
//! branch hands the whole buffer to `Mesh::restore_snapshot`, and the call
//! returns — with an EMPTY scene and no error at all.
//!
//! Measured 2026-08-19 on a user's 500 KB file: 704 faces became 0, and the
//! first report written from that reading said the model was small and fast.
//!
//! ⚠ Both readers are public and the mistake is one character of API surface
//! apart, so the fix belongs in the engine rather than in the caller.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use glam::DVec3;

fn a_scene_with_something_in_it() -> Scene {
    let mut s = Scene::new();
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, FORM_MATERIAL)
        .expect("a box");
    s
}

fn live_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// The headerless reader accepts a versioned snapshot rather than emptying the
/// scene.
///
/// Mutation-checked: remove the magic check at the top of
/// `restore_scene_snapshot` and this reports 0 faces.
#[test]
fn a_versioned_snapshot_survives_the_headerless_reader() {
    let src = a_scene_with_something_in_it();
    let before = live_faces(&src);
    assert!(before > 0, "the fixture has to contain something");
    let saved = src.export_versioned_snapshot().expect("export");
    assert_eq!(&saved[0..4], b"AXIA", "a saved snapshot carries the magic");

    let mut dst = Scene::new();
    dst.restore_scene_snapshot(&saved);
    let after = live_faces(&dst);
    println!("\n  저장된 스냅샷을 헤더 없는 판독기에 — 면 {before} -> {after}\n");
    assert_eq!(
        after, before,
        "a saved snapshot handed to `restore_scene_snapshot` must not come back \
         as an empty scene without a word"
    );
}

/// The headerless form still works, which is the whole reason the branch is a
/// check on the magic rather than a change of reader.
#[test]
fn the_headerless_form_still_round_trips() {
    let src = a_scene_with_something_in_it();
    let before = live_faces(&src);
    let raw = src.scene_snapshot();
    assert_ne!(&raw[0..4], b"AXIA", "the internal form carries no magic");

    let mut dst = Scene::new();
    dst.restore_scene_snapshot(&raw);
    assert_eq!(live_faces(&dst), before, "undo frames must keep working");
}

/// And a truncated buffer is still refused rather than half-read.
#[test]
fn a_cut_off_snapshot_is_not_mistaken_for_a_scene() {
    let src = a_scene_with_something_in_it();
    let saved = src.export_versioned_snapshot().expect("export");
    let mut dst = Scene::new();
    dst.restore_scene_snapshot(&saved[..saved.len() / 3]);
    // Nothing is asserted about WHAT it holds — only that reading half a file
    // does not leave a scene claiming to be the whole one.
    let n = live_faces(&dst);
    println!("\n  잘린 스냅샷 — 면 {n}\n");
    assert!(n < live_faces(&src), "half a file is not the whole model");
}
