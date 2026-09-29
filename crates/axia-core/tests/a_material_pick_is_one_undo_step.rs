//! A MATERIAL PICK IS ONE UNDO STEP (ADR-313 D5).
//!
//! `Command::AssignMaterial` sets face materials and records nothing, so the
//! undo right after a pick went back to the step BEFORE it. Measured in the
//! app, in a real browser against the real engine:
//!
//! ```text
//!   an extruded box               6 faces
//!   pick 콘크리트 in the Inspector   6 faces, material 1
//!   undo once                     1 face,  material 0     ← the extrude went
//! ```
//!
//! The app's pick now comes through `assign_material_to_faces` /
//! `remove_material_from_faces`, which record one transaction each and run the
//! command inside it. The command is left as it was; the plain WASM exports
//! that call it directly are kept (tests use them).
//!
//! ⚠ Mutation-checked: drop the transaction from `assign_material_to_faces`
//! and "one undo takes back the pick" fails with the extrude gone; drop it
//! from `remove_material_from_faces` and "taking the material off" fails.

use axia_core::{Command, Scene, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId, MaterialId};
use glam::DVec3;

/// An extruded box, and its faces.
fn a_box() -> (Scene, Vec<FaceId>) {
    let mut s = Scene::new();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 1000.0,
        height: 1000.0,
    });
    let profile = active_faces(&s)[0];
    s.execute(Command::CreateSolid {
        face_id: profile,
        mode: CreateSolidMode::Extrude { distance: 500.0 },
    });
    let faces = active_faces(&s);
    // PREMISE: a box, or "the extrude stayed" below measures nothing.
    assert_eq!(faces.len(), 6, "the extrude must have made a box");
    (s, faces)
}

fn active_faces(s: &Scene) -> Vec<FaceId> {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
}

fn materials(s: &Scene, faces: &[FaceId]) -> Vec<u32> {
    faces
        .iter()
        .map(|&f| s.mesh.faces.get(f).map(|x| x.material().raw()).unwrap_or(u32::MAX))
        .collect()
}

fn concrete(s: &Scene) -> MaterialId {
    s.material_library.find_by_name("Concrete").expect("concrete is a built-in")
}

#[test]
fn one_undo_takes_back_the_pick_and_nothing_else() {
    let (mut s, faces) = a_box();
    let c = concrete(&s);

    let pick = s.assign_material_to_faces(faces.clone(), c).expect("the library holds concrete");
    assert_eq!(pick.faces, 6);
    assert_eq!(materials(&s, &faces), vec![c.raw(); 6]);

    s.execute(Command::Undo);
    assert_eq!(active_faces(&s).len(), 6, "the extrude stays — it was not this step");
    assert_eq!(materials(&s, &faces), vec![FORM_MATERIAL.raw(); 6], "the pick is taken back");

    s.execute(Command::Redo);
    assert_eq!(materials(&s, &faces), vec![c.raw(); 6], "and redo puts it back");
}

#[test]
fn taking_the_material_off_is_one_undo_step_too() {
    let (mut s, faces) = a_box();
    let c = concrete(&s);
    s.assign_material_to_faces(faces.clone(), c).expect("concrete");

    let pick = s.remove_material_from_faces(faces.clone());
    assert_eq!(pick.faces, 6);
    assert_eq!(materials(&s, &faces), vec![FORM_MATERIAL.raw(); 6]);

    s.execute(Command::Undo);
    assert_eq!(materials(&s, &faces), vec![c.raw(); 6], "one undo brings the material back");
    assert_eq!(active_faces(&s).len(), 6);
}

#[test]
fn a_material_the_library_does_not_hold_is_refused_and_records_nothing() {
    let (mut s, faces) = a_box();
    // 0 is FORM_MATERIAL — "no material", not an entry (ADR-313 D1).
    assert!(s.assign_material_to_faces(faces.clone(), FORM_MATERIAL).is_err());
    assert!(s.assign_material_to_faces(faces.clone(), MaterialId::new(99_999)).is_err());
    assert_eq!(materials(&s, &faces), vec![FORM_MATERIAL.raw(); 6], "nothing changed");

    // Nothing was recorded, so the undo is the extrude's.
    s.execute(Command::Undo);
    assert_eq!(active_faces(&s).len(), 1, "the undo went to the extrude: no frame was left behind");
}
