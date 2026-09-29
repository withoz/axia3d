//! A FILE SAVED BEFORE THE RENUMBERING OPENS WITH THE MATERIALS IT MEANT
//! (ADR-313 D2, L-313-3).
//!
//! Until 2026-09-29 the engine numbered its built-in materials from 0 while the
//! app numbered them from 1, so a saved file holds material ids in TWO
//! numberings at once:
//!
//! * a face the app assigned carries the APP's number — 1 meant 콘크리트, and
//!   now that the engine counts from 1 too, it already means what it meant;
//! * an id the engine's own paths wrote — promotion by the IFC importer or by
//!   MCP `create_xia` — is in the ENGINE's old numbering, where 4 was 벽돌, and
//!   has to move up by one.
//!
//! The fixture is not constructed here; it is a snapshot the engine wrote
//! BEFORE the change (generated on 2026-09-29 from `9357dd0`'s axia-core), so
//! what this reads is what an old file holds. It contains:
//!
//! | shape | how its material got there              | ids in the file        |
//! |-------|-----------------------------------------|------------------------|
//! | 1     | app picked 콘크리트                     | face 0 → 1             |
//! | 2     | IFC-import of a 벽돌 member             | faces 1..=6 → 4, XIA 3 → 4 |
//! | 3     | MCP `create_xia` with the engine's 강철 | faces 7..=12 → 0, XIA 5 → 1 |
//! | 4     | a Project material (Asset Library)      | face 13 → 100          |
//! | 5     | app picked 토양                         | face 14 → 11           |
//!
//! ⚠ Mutation-checked: without the migration the 벽돌 member reads as 유리 and
//! the 강철 member as 콘크리트; migrating every face instead of only the ones
//! the engine wrote turns the app's 콘크리트 into 강철.

use axia_core::material::{MaterialLibrary, MaterialTier};
use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use axia_geo::{FaceId, MaterialId};

const OLD_FILE: &[u8] = include_bytes!("fixtures/materials_numbered_from_zero.snap");

fn open(bytes: &[u8]) -> Scene {
    let mut scene = Scene::default();
    scene.import_versioned_snapshot(bytes).expect("the old file must still open");
    scene
}

fn material_of(scene: &Scene, f: u32) -> u32 {
    scene.mesh.faces.get(FaceId::new(f)).expect("face").material().raw()
}

fn name_en(lib: &MaterialLibrary, id: u32) -> Option<String> {
    lib.get(MaterialId::new(id)).map(|m| m.name_en.clone())
}

#[test]
fn every_face_keeps_the_material_it_was_given() {
    let scene = open(OLD_FILE);
    let lib = &scene.material_library;

    // The app's own assignments were already in the numbering the engine now uses.
    assert_eq!(material_of(&scene, 0), 1, "the app's 콘크리트 must stay 1");
    assert_eq!(name_en(lib, 1).as_deref(), Some("Concrete"));
    assert_eq!(material_of(&scene, 14), 11, "the app's 토양 must stay 11");
    assert_eq!(name_en(lib, 11).as_deref(), Some("Soil"));

    // The IFC importer wrote the engine's old 4 (벽돌) on the member and its faces.
    for f in 1..=6 {
        assert_eq!(
            name_en(lib, material_of(&scene, f)).as_deref(),
            Some("Brick"),
            "face {f} of the imported brick member must still be brick"
        );
    }
    let brick_xia = scene.xias.get(&3).expect("the brick member is an XIA");
    assert_eq!(name_en(lib, brick_xia.material.raw()).as_deref(), Some("Brick"));

    // MCP create_xia wrote only the member's primary (the engine's old 1, 강철).
    let steel_xia = scene.xias.get(&5).expect("the steel member is an XIA");
    assert_eq!(name_en(lib, steel_xia.material.raw()).as_deref(), Some("Steel"));
    for f in 7..=12 {
        assert_eq!(material_of(&scene, f), FORM_MATERIAL.raw(), "face {f} had no material and keeps none");
    }

    // A custom material is not a built-in and does not move.
    assert_eq!(material_of(&scene, 13), 100);
    assert_eq!(lib.get(MaterialId::new(100)).map(|m| m.name.as_str()), Some("프로젝트 재질"));
    assert_eq!(lib.tier_of(MaterialId::new(100)), Some(MaterialTier::Project));
}

#[test]
fn the_library_comes_back_in_the_new_numbering() {
    let scene = open(OLD_FILE);
    let lib = &scene.material_library;
    assert!(lib.get(FORM_MATERIAL).is_none(), "no material is not a material");
    let fresh = MaterialLibrary::new();
    for id in 1..=12u32 {
        assert_eq!(
            name_en(lib, id),
            name_en(&fresh, id),
            "built-in {id} must be the same material a new library holds there"
        );
        assert_eq!(lib.tier_of(MaterialId::new(id)), Some(MaterialTier::System), "built-in {id} tier");
    }
}

#[test]
fn a_file_saved_after_the_renumbering_is_not_moved_again() {
    let once = open(OLD_FILE);
    let again = open(&once.export_versioned_snapshot().expect("re-save"));
    for f in 0..=14u32 {
        assert_eq!(material_of(&again, f), material_of(&once, f), "face {f} moved on a second load");
    }
    for (k, x) in &once.xias {
        assert_eq!(again.xias[k].material, x.material, "XIA {k} moved on a second load");
    }
}
