//! Drawing inside a face: whose is the piece you cut out?
//!
//! 사용자 결재 2026-08-11 — **(a) 솔리드 것.** A solid's shell is its identity;
//! drawing a line or a rectangle on one of its faces does not remove material,
//! so the piece stays the solid's and the drawn entity keeps only its identity.
//!
//! Before, measured on a 200³ box:
//!
//! ```text
//!   rect on the top face   box owns 6 of 7 faces   isSolid false   volume 7,666,666
//! ```
//!
//! ⚠ **Only when the host is a CLOSED SOLID.** On a flat sheet there is no shell
//! to keep whole, and a rect drawn inside a bigger rect is its own object — that
//! is LOCKED #1 P7. Applying the rule to every owned face broke six of its
//! regressions (`test_two_stacked_inner_rects_both_faced` and friends), which is
//! how the condition got narrowed.

use axia_core::promote::face_set_volume;
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

fn rect_on_top(scene: &mut Scene, side: f64) {
    scene.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, 100.0),
        normal: DVec3::Z,
        up: DVec3::X,
        width: side,
        height: side,
    });
}

#[test]
fn the_solid_keeps_the_piece_and_stays_a_solid() {
    let (mut scene, xia) = boxed_scene();
    rect_on_top(&mut scene, 100.0);

    let active = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(active, 7, "the top should have become a ring plus the rect");

    let owned = &scene.xias[&xia].face_ids;
    assert_eq!(owned.len(), active, "the box should own every one of them");
    assert!(
        scene.mesh.face_set_manifold_info(owned).is_closed_solid,
        "the box stopped being a closed solid",
    );
    let vol = face_set_volume(&scene.mesh, owned);
    assert!((vol - 8_000_000.0).abs() < 1.0, "volume became {vol}");
}

/// The drawn entity still exists — it just does not take the face with it.
#[test]
fn the_drawn_rect_keeps_its_identity_and_no_face() {
    let (mut scene, _) = boxed_scene();
    rect_on_top(&mut scene, 100.0);

    let rects: Vec<_> = scene.shapes.values().filter(|s| s.name == "Rectangle").collect();
    assert_eq!(rects.len(), 1, "the gesture should still produce its Shape");
    assert!(rects[0].face_ids.is_empty(), "it must not own the solid's face");
}

/// The other half of the rule, and the reason it is conditional: a sheet has no
/// shell, so the piece belongs to what was drawn.
#[test]
fn on_a_sheet_the_drawn_rect_keeps_its_own_face() {
    let mut scene = Scene::new();
    scene.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });
    scene.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 100.0,
        height: 100.0,
    });

    for shape in scene.shapes.values() {
        assert!(
            !shape.face_ids.is_empty(),
            "on a sheet every drawn rect owns its own face — {:?} owns none",
            shape.name,
        );
    }
}

/// ⚠ **Records today's limit, not the desired behaviour.** A SECOND rect, drawn
/// inside the first one's face, still takes that piece away — the box drops to
/// 7 of 8 faces, opens, and reads 7,880,000.
///
/// The rule itself is fine; it never runs. Measured with the guard instrumented:
/// the gesture does reach the interior fast-path
/// (`edge_interaction=false, face_interaction=true`), but
/// `single_face_containing_corners` returns **None** once the host face has a
/// hole — the corners sit inside both the ring's outer boundary and the inner
/// face, so nothing is "single" — and the draw falls through to the unified
/// pipeline, which is one of the paths ADR-312 §4 lists as unwired.
///
/// **When containment learns about holes, this test goes red. That is the fix
/// landing, not a regression — replace it with the assertions above.**
#[test]
fn a_second_rect_nested_in_the_first_still_takes_its_piece() {
    let (mut scene, xia) = boxed_scene();
    rect_on_top(&mut scene, 120.0);
    assert!(
        scene.mesh.face_set_manifold_info(&scene.xias[&xia].face_ids).is_closed_solid,
        "the first rect already broke the rule — the rest of this test is moot",
    );

    rect_on_top(&mut scene, 60.0);
    let vol = face_set_volume(&scene.mesh, &scene.xias[&xia].face_ids);
    assert!(
        (vol - 8_000_000.0).abs() > 1.0,
        "the nested case now holds too ({vol}) — good; promote this test",
    );
}
