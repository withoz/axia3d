//! THE THREE GROUP EDITS THE OUTLINER NEVER OFFERED.
//!
//! `addFacesToGroup`, `removeFacesFromGroup` and `setGroupParent` are engine
//! ops with WASM exports and bridge wrappers, and until 2026-10-01 **none had
//! a caller**. The UI could create a group (Ctrl+G) and dissolve one
//! (Ctrl+Shift+G) and nothing in between: no way to change what a group holds,
//! and no way to put one group inside another.
//!
//! ⚠ `ComponentPanel` renders a NESTED tree — `childMap`, per-depth indent —
//! and `create_group` always sets `parent: None`, with `set_parent` called from
//! nowhere in the engine either. **The Outliner drew a tree the user could not
//! build.**
//!
//! That is a weaker finding than the rename this pass also fixed: the panel's
//! header promised 이름 편집 and did not do it, while nesting was never
//! promised. This is a capability the engine had and the UI did not offer.
//!
//! What this file pins is the ENGINE side, measured before the UI was touched,
//! including the one behaviour a caller must not rely on (see the last test).
//! The UI side is held by `theOutlinerEditsAGroup` in vitest and
//! `the-outliner-edits-a-group` in Playwright.

use axia_core::Scene;
use axia_geo::MaterialId;
use glam::DVec3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// A scene with one box, and its six faces.
fn box_scene() -> (Scene, Vec<axia_geo::FaceId>) {
    let mut s = Scene::new();
    let faces = s
        .mesh
        .create_box(DVec3::ZERO, 100.0, 100.0, 100.0, mat())
        .expect("box");
    (s, faces)
}

fn face_count(s: &Scene, g: u32) -> usize {
    s.groups.groups.get(&g).map(|x| x.face_ids.len()).unwrap_or(0)
}

#[test]
fn faces_go_into_a_group_and_come_back_out() {
    let (mut s, faces) = box_scene();
    let g = s.groups.create_group("벽체".into(), vec![faces[0], faces[1]]);
    // PREMISE: the group was made with what we gave it, or the deltas below
    // are measured against nothing.
    assert_eq!(face_count(&s, g), 2, "the group starts with two faces");

    assert!(
        s.groups.add_faces_to_group(g, &[faces[2], faces[3]]),
        "adding faces must report success"
    );
    assert_eq!(face_count(&s, g), 4, "two more went in");

    assert!(
        s.groups.remove_faces_from_group(g, &[faces[0], faces[2]]),
        "removing faces must report success"
    );
    assert_eq!(face_count(&s, g), 2, "two came out");
}

/// Adding the same face twice must not double-count it — the panel shows this
/// number, and a user clicking "add" twice is ordinary.
#[test]
fn adding_a_face_already_in_the_group_is_not_counted_twice() {
    let (mut s, faces) = box_scene();
    let g = s.groups.create_group("g".into(), vec![faces[0]]);
    s.groups.add_faces_to_group(g, &[faces[0], faces[1]]);
    assert_eq!(face_count(&s, g), 2, "face 0 was already in: 0 and 1, not 0, 0 and 1");
}

#[test]
fn one_group_goes_inside_another() {
    let (mut s, faces) = box_scene();
    let outer = s.groups.create_group("outer".into(), vec![faces[0]]);
    let inner = s.groups.create_group("inner".into(), vec![faces[1]]);
    // PREMISE: a fresh group has no parent — this is why the Outliner's tree
    // had nothing to draw.
    assert!(
        s.groups.groups.get(&inner).and_then(|g| g.parent).is_none(),
        "create_group must leave the parent unset"
    );

    assert!(s.groups.set_parent(inner, Some(outer)), "nesting must succeed");
    assert_eq!(
        s.groups.groups.get(&inner).and_then(|g| g.parent),
        Some(outer),
        "the child knows its parent"
    );
    assert!(
        s.groups.groups.get(&outer).map(|g| g.children.contains(&inner)).unwrap_or(false),
        "and the parent knows its child — the panel reads this side"
    );

    // Out again.
    assert!(s.groups.set_parent(inner, None), "un-nesting must succeed");
    assert!(s.groups.groups.get(&inner).and_then(|g| g.parent).is_none());
    assert!(
        s.groups.groups.get(&outer).map(|g| g.children.is_empty()).unwrap_or(false),
        "and the parent lets go"
    );
}

/// The safety the UI rests on: a group cannot be its own ancestor. Without this
/// the panel's recursive render would not terminate.
#[test]
fn a_group_cannot_be_put_inside_itself_or_its_own_child() {
    let (mut s, faces) = box_scene();
    let a = s.groups.create_group("a".into(), vec![faces[0]]);
    let b = s.groups.create_group("b".into(), vec![faces[1]]);
    let c = s.groups.create_group("c".into(), vec![faces[2]]);

    assert!(!s.groups.set_parent(a, Some(a)), "a group is not its own parent");

    assert!(s.groups.set_parent(b, Some(a)), "b under a");
    assert!(s.groups.set_parent(c, Some(b)), "c under b");
    assert!(!s.groups.set_parent(a, Some(c)), "and a must not go under its own grandchild");
    assert!(!s.groups.set_parent(a, Some(b)), "nor under its own child");
    // The chain is intact after the refusals.
    assert_eq!(s.groups.groups.get(&b).and_then(|g| g.parent), Some(a));
    assert_eq!(s.groups.groups.get(&c).and_then(|g| g.parent), Some(b));
    assert!(s.groups.groups.get(&a).and_then(|g| g.parent).is_none());
}

/// ⚠ Measured, not endorsed: a parent id that names no group is accepted and
/// leaves the child pointing at nothing. The UI must only ever pass ids it read
/// from the engine, which is what `theOutlinerEditsAGroup` holds it to. If this
/// ever starts returning false, tighten that test rather than loosen this one.
#[test]
fn an_unknown_parent_id_is_accepted_which_is_why_the_ui_must_not_invent_one() {
    let (mut s, faces) = box_scene();
    let g = s.groups.create_group("g".into(), vec![faces[0]]);
    let ok = s.groups.set_parent(g, Some(9999));
    println!("  set_parent(g, 9999) -> {ok}, parent now {:?}",
        s.groups.groups.get(&g).and_then(|x| x.parent));
    assert!(ok, "today it reports success");
    assert_eq!(
        s.groups.groups.get(&g).and_then(|x| x.parent),
        Some(9999),
        "and records the dangling id"
    );
}
