//! A MATERIAL PICK MOVES ITS OWNER — only when it leaves no doubt (ADR-313 D5).
//!
//! The Inspector promised *"재질을 부여하면 이 객체는 XIA (특성)로 승격됩니다"* and
//! nothing in the app ever promoted: `promoteShapeToXia` had no caller in the
//! browser (ADR-313 §2.5). So ADR-091's demotion had never run on anything a
//! person drew either — and when a XIA was made by hand to try it, removing its
//! material in the Inspector was refused every time ("재질 제거 시 1건 강등
//! 실패"): the removal cleared the faces, and the demotion is triggered by the
//! XIA's own material, which nothing cleared.
//!
//! The rule the engine now follows, inside the pick's one undo step:
//!
//! ```text
//!   every face of a Shape on the picked material   promote (ADR-050 decides)
//!   every face of a XIA on the picked material     it is the XIA's primary
//!   every face of a XIA on no material             demote (ADR-091)
//!   anything less                                  face-level; no owner moves
//! ```
//!
//! ⚠ Mutation-checked: skip the promotion, and "every face ... makes it a XIA"
//! fails; drop the XIA's material from the removal, and "taking the material off
//! a XIA" fails with the XIA still there; drop the "already promoted" skip, and
//! "picking again" counts two XIAs.

use axia_core::{Command, PromoteError, Scene, ShapeId, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId, MaterialId};
use glam::DVec3;

fn sheet() -> (Scene, Vec<FaceId>) {
    let mut s = Scene::new();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 1000.0,
        height: 1000.0,
    });
    let faces = active_faces(&s);
    assert_eq!(faces.len(), 1, "a sheet");
    (s, faces)
}

fn a_box() -> (Scene, Vec<FaceId>) {
    let (mut s, faces) = sheet();
    s.execute(Command::CreateSolid {
        face_id: faces[0],
        mode: CreateSolidMode::Extrude { distance: 500.0 },
    });
    let faces = active_faces(&s);
    assert_eq!(faces.len(), 6, "a box");
    (s, faces)
}

fn active_faces(s: &Scene) -> Vec<FaceId> {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect()
}

fn shape_of(s: &Scene, f: FaceId) -> ShapeId {
    *s.face_to_shape.get(&f).expect("a drawn face belongs to a Shape")
}

fn id(s: &Scene, name: &str) -> MaterialId {
    s.material_library.find_by_name(name).expect("a built-in")
}

#[test]
fn every_face_of_a_box_on_one_material_makes_it_a_xia() {
    let (mut s, faces) = a_box();
    let shape = shape_of(&s, faces[0]);
    let concrete = id(&s, "Concrete");

    let pick = s.assign_material_to_faces(faces.clone(), concrete).expect("concrete");
    assert_eq!(pick.promoted.len(), 1, "one Shape, promoted: {:?}", pick);
    let (from, xia) = pick.promoted[0];
    assert_eq!(from, shape);
    assert_eq!(s.xias.get(&xia).map(|x| x.material), Some(concrete), "with the pick as primary");
    for &f in &faces {
        assert_eq!(s.get_xia_for_face(f), Some(xia), "face {:?} belongs to the XIA", f);
    }
}

#[test]
fn one_undo_takes_back_the_pick_and_the_promotion() {
    let (mut s, faces) = a_box();
    let concrete = id(&s, "Concrete");
    s.assign_material_to_faces(faces.clone(), concrete).expect("concrete");
    assert_eq!(s.xias.len(), 1);

    s.execute(Command::Undo);
    assert!(s.xias.is_empty(), "the XIA goes with the pick");
    assert_eq!(active_faces(&s).len(), 6, "and the box stays");
    assert!(faces.iter().all(|&f| s.get_xia_for_face(f).is_none()));
}

#[test]
fn painting_one_face_leaves_the_box_a_shape() {
    let (mut s, faces) = a_box();
    let pick = s.assign_material_to_faces(vec![faces[0]], id(&s, "Brick")).expect("brick");
    assert!(pick.promoted.is_empty() && pick.refused.is_empty(), "{:?}", pick);
    assert!(s.xias.is_empty());
}

#[test]
fn a_box_on_two_materials_is_not_guessed() {
    let (mut s, faces) = a_box();
    s.assign_material_to_faces(vec![faces[0]], id(&s, "Brick")).expect("brick");
    let pick = s.assign_material_to_faces(faces[1..].to_vec(), id(&s, "Concrete")).expect("concrete");
    assert!(pick.promoted.is_empty() && pick.refused.is_empty(), "{:?}", pick);
    assert!(s.xias.is_empty(), "which of two materials is the box's is not the engine's to guess");
}

#[test]
fn the_last_face_to_match_promotes() {
    let (mut s, faces) = a_box();
    let concrete = id(&s, "Concrete");
    let first = s.assign_material_to_faces(faces[1..].to_vec(), concrete).expect("five");
    assert!(first.promoted.is_empty(), "five of six is not the whole box");
    let last = s.assign_material_to_faces(vec![faces[0]], concrete).expect("the sixth");
    assert_eq!(last.promoted.len(), 1, "the sixth makes it the whole box: {:?}", last);
}

#[test]
fn a_sheet_is_refused_and_says_why() {
    let (mut s, faces) = sheet();
    let concrete = id(&s, "Concrete");
    let pick = s.assign_material_to_faces(faces.clone(), concrete).expect("concrete");
    assert!(pick.promoted.is_empty());
    assert_eq!(pick.refused.len(), 1, "{:?}", pick);
    assert!(
        matches!(pick.refused[0].1, PromoteError::NotWatertight { .. }),
        "a sheet encloses nothing: {:?}",
        pick.refused[0].1
    );
    assert_eq!(
        s.mesh.faces.get(faces[0]).map(|f| f.material()),
        Some(concrete),
        "the face keeps the material"
    );
    assert!(s.xias.is_empty());
}

#[test]
fn repicking_a_whole_xia_changes_its_primary() {
    let (mut s, faces) = a_box();
    s.assign_material_to_faces(faces.clone(), id(&s, "Concrete")).expect("concrete");
    let xia = *s.xias.keys().next().expect("a XIA");
    let brick = id(&s, "Brick");

    let pick = s.assign_material_to_faces(faces.clone(), brick).expect("brick");
    assert_eq!(pick.primary, vec![xia]);
    assert_eq!(s.xias.get(&xia).map(|x| x.material), Some(brick));
}

#[test]
fn picking_again_does_not_make_a_second_xia() {
    let (mut s, faces) = a_box();
    let concrete = id(&s, "Concrete");
    s.assign_material_to_faces(faces.clone(), concrete).expect("once");
    let again = s.assign_material_to_faces(faces.clone(), concrete).expect("twice");
    assert!(again.promoted.is_empty(), "{:?}", again);
    assert_eq!(s.xias.len(), 1, "one member, not two over the same faces");
}

#[test]
fn taking_the_material_off_a_xia_demotes_it() {
    let (mut s, faces) = a_box();
    let shape = shape_of(&s, faces[0]);
    s.assign_material_to_faces(faces.clone(), id(&s, "Concrete")).expect("concrete");
    let xia = *s.xias.keys().next().expect("a XIA");

    let pick = s.remove_material_from_faces(faces.clone());
    assert_eq!(pick.demoted, vec![(xia, shape)], "{:?}", pick);
    assert!(s.xias.is_empty(), "the XIA is gone");
    assert!(faces.iter().all(|&f| s.face_to_shape.get(&f) == Some(&shape)), "back to its Shape");
}

#[test]
fn one_undo_after_taking_it_off_brings_back_the_xia_and_its_material() {
    let (mut s, faces) = a_box();
    let concrete = id(&s, "Concrete");
    s.assign_material_to_faces(faces.clone(), concrete).expect("concrete");
    let xia = *s.xias.keys().next().expect("a XIA");
    s.remove_material_from_faces(faces.clone());

    s.execute(Command::Undo);
    assert_eq!(s.xias.get(&xia).map(|x| x.material), Some(concrete), "the XIA is back, with its material");
    assert!(faces
        .iter()
        .all(|&f| s.mesh.faces.get(f).map(|x| x.material()) == Some(concrete)));
}

#[test]
fn taking_it_off_one_face_keeps_the_xia() {
    let (mut s, faces) = a_box();
    let concrete = id(&s, "Concrete");
    s.assign_material_to_faces(faces.clone(), concrete).expect("concrete");
    let xia = *s.xias.keys().next().expect("a XIA");

    let pick = s.remove_material_from_faces(vec![faces[0]]);
    assert!(pick.demoted.is_empty(), "{:?}", pick);
    assert_eq!(s.xias.get(&xia).map(|x| x.material), Some(concrete), "five faces still carry it");
    assert_eq!(s.mesh.faces.get(faces[0]).map(|f| f.material()), Some(FORM_MATERIAL));
}
