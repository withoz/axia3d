//! Drawing a rect on a solid's face gives the piece to the solid. Does the
//! drawn Shape still point at it?
//!
//! The rule is 사용자 결재 2026-08-11: a region cut out of somebody's face
//! stays theirs, so a closed solid keeps its shell whole. The cost recorded at
//! the time was that `drawRectAsShape → getShapeFaceIds → createSolidExtrude`
//! stops working, because the Shape no longer owns the region and so is no
//! longer a handle to it.
//!
//! ⚠ That cost was reported here as "does not reproduce", on a measurement
//! taken against a WASM binary that had not been rebuilt — `npm run build`
//! rebuilds the TypeScript and leaves `axia_wasm_bg.wasm` alone, so the engine
//! change under test was not in the bundle. With `npm run build:wasm` first,
//! `adr-264-embedded-boss` goes red exactly as the parking note said. The
//! claim was wrong and so was the reason given for it: the WASM
//! `create_box` calls `create_xia_with_faces`, so a box made through the
//! bridge IS owned — it is `Mesh::create_box`, one layer down and used by
//! Rust tests, that leaves its six faces to nobody.
//!
//! The fix is below the tests: the solid owns the region and the Shape
//! REMEMBERS it (`Scene::shape_drawn_faces`), which is what makes it a handle
//! again without making it an owner.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult};
use axia_geo::CreateSolidMode;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// A solid the app built: a drawn rect, extruded. Its faces have an owner.
fn drawn_solid() -> (Scene, f64) {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });
    let base = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .next()
        .expect("the drawn rect");
    s.execute(Command::CreateSolid {
        face_id: base,
        mode: CreateSolidMode::Extrude { distance: 200.0 },
    });
    (s, 200.0)
}

#[test]
fn the_solid_the_app_builds_owns_its_faces() {
    let (s, _) = drawn_solid();
    let unowned = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active() && s.get_xia_for_face(*fid).is_none() && !s.face_to_shape.contains_key(fid)
        })
        .count();
    let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("DRAWN SOLID {live} faces, {unowned} unowned");
    assert_eq!(live, 6, "a box");
    assert_eq!(
        unowned, 0,
        "unlike `create_box`, the app's own build route leaves nothing unowned \
         — which is why the boss E2E's primitive box does not exercise the rule"
    );
}

/// The question. A rect drawn on that solid's top: who owns it, and can the
/// drawn Shape still find it?
#[test]
fn a_rect_drawn_on_an_owned_solid_can_still_be_found_by_its_shape() {
    let (mut s, top_z) = drawn_solid();
    let before: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();

    let r = s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, top_z),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 120.0,
        height: 120.0,
    });
    let sid = match r {
        CommandResult::ShapeCreated(id) => axia_core::ShapeId::new(id),
        other => panic!("the draw must make a Shape: {other:?}"),
    };

    let fresh: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && !before.contains(fid))
        .map(|(fid, _)| fid)
        .collect();
    assert!(!fresh.is_empty(), "the draw divided the top");

    let owns: Vec<_> = s
        .get_shape(sid)
        .map(|sh| sh.face_ids.clone())
        .unwrap_or_default();
    let remembers = s.shape_drawn_faces(sid);
    println!(
        "SHAPE {sid:?} owns {:?}, remembers {:?}; fresh faces {:?}",
        owns, remembers, fresh
    );

    // This is the whole of it: whatever the ownership answer, the Shape has to
    // remain a handle. `drawRectAsShape → getShapeFaceIds → createSolidExtrude`
    // is how DrawWallTool builds a wall, how SliceTool finds its faces, and how
    // ADR-264's boss tests find the face to push.
    let handle: Vec<_> = if owns.is_empty() { remembers } else { owns };
    assert!(
        !handle.is_empty(),
        "the drawn Shape must still point at the region it drew — owning it or \
         remembering it, but not neither"
    );
    for f in &handle {
        assert!(
            s.mesh.faces.get(*f).is_some_and(|x| x.is_active()),
            "the handle names {f:?}, which is not a live face"
        );
    }

    // And the solid is still whole, which is what the rule was for.
    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "invariants: {:?}", inv.violations);
}
