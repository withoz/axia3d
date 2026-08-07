//! BUILDING A SECOND SOLID ON A PLANE THE FIRST ONE ALREADY SITS ON.
//!
//! Two 120³ boxes, the second overlapping the first, both bottoms on z=0. Each
//! must come out whole — that pair is what Boolean is meant to receive.
//!
//! The coplanar re-tile used to divide the shared z=0 ground and leave the halves
//! belonging to nobody. Measured 2026-08-07 in the browser with the auto flags
//! toggled one at a time:
//!
//! ```text
//!   re-tile off, autoIntersect on   24 faces  14 violations  si 4
//!   autoIntersect off, re-tile on   13 faces   8 boundary     si 7
//!   both off                        12 faces   0, closed ✓    si 12
//!   both on (control)               13 faces   8 boundary     si 7
//! ```
//!
//! Only with the re-tile out of the way is each solid whole. The 12
//! self-intersections there are CORRECT: the boxes really do interpenetrate.
//! Asserting `si == 0` here would be asserting that two overlapping solids may
//! not exist.
//!
//! The fix asks whether the perimeters being fed belong to two different SHELLS
//! (`Mesh::face_shell_ids`), not whether there are two of them — a solid with a
//! hole in its top has two perimeter components and must keep working. That side
//! is guarded in `axia-geo/tests/two_solids_on_one_plane_are_two_shells.rs` and
//! by the ring case below.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

struct Health {
    faces: usize,
    closed: bool,
    boundary: usize,
    non_manifold: usize,
    violations: usize,
}

fn health(s: &Scene) -> Health {
    let all: Vec<axia_geo::FaceId> =
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let mi = s.mesh.face_set_manifold_info(&all);
    Health {
        faces: all.len(),
        closed: mi.is_closed_solid,
        boundary: mi.boundary_edge_count,
        non_manifold: mi.non_manifold_edge_count,
        violations: s.mesh.verify_face_invariants().violations.len(),
    }
}

#[test]
fn two_boxes_sharing_the_ground_stay_twelve_whole_faces() {
    let mut s = prod();
    let a = s.mesh.create_box(DVec3::new(0.0, 0.0, 60.0), 120.0, 120.0, 120.0, FORM_MATERIAL).unwrap();
    s.create_xia_with_faces("A".into(), DVec3::ZERO, a);
    let first = health(&s);
    assert_eq!(first.faces, 6);
    assert!(first.closed, "the first box is a closed solid");

    let b = s.mesh.create_box(DVec3::new(80.0, 80.0, 60.0), 120.0, 120.0, 120.0, FORM_MATERIAL).unwrap();
    s.create_xia_with_faces("B".into(), DVec3::ZERO, b.clone());
    // the scene-level pass the draw path runs after new geometry appears
    s.intersect_faces_with_scene(&b).expect("the pass itself must not fail");

    let h = health(&s);
    assert_eq!(h.faces, 12, "six faces each — the shared ground is left alone");
    assert!(h.closed, "both solids are still closed");
    assert_eq!(h.boundary, 0, "nothing is left open");
    assert_eq!(h.non_manifold, 0);
    assert_eq!(h.violations, 0);
    // and they do overlap, which is the point: this is Boolean's input
    assert!(
        s.mesh.detect_self_intersections().count() > 0,
        "the fixture must actually interpenetrate, or it proves nothing"
    );
}

/// The other side of the same judgement: one solid whose top has a hole has two
/// perimeter components too, and drawing on it must be untouched by the fix.
#[test]
fn drawing_on_a_solid_top_with_a_hole_is_unchanged() {
    let mut s = prod();
    let f = s.mesh.create_box(DVec3::new(0.0, 0.0, 60.0), 200.0, 120.0, 200.0, FORM_MATERIAL).unwrap();
    s.create_xia_with_faces("box".into(), DVec3::ZERO, f);
    s.drill_rect_through_hole(
        DVec3::new(-30.0, -30.0, 120.0),
        DVec3::new(30.0, 30.0, 120.0),
        DVec3::Z,
    )
    .expect("the drill must succeed for this fixture to mean anything");
    let drilled = health(&s);
    assert_eq!(drilled.faces, 10, "box 6 − top + ring + 4 tube walls");
    assert!(drilled.closed);

    // a rect on the ring, clear of the hole — this is the case a component count
    // would have skipped, and it still divides the ring and stays closed
    let r = s.execute(Command::DrawRectAsShape {
        center: DVec3::new(65.0, 0.0, 120.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 40.0,
        height: 40.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    let h = health(&s);
    assert_eq!(h.faces, 11, "the rect becomes its own face on the ring");
    assert!(h.closed, "the solid stays closed");
    assert_eq!(h.boundary, 0);
    assert_eq!(h.violations, 0);
}
