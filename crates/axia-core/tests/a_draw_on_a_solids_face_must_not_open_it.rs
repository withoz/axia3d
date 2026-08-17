//! A circle drawn where a box's bottom already is, and the box comes apart.
//!
//! Session 9 of the fuzz reduced to six operations, and the measurement in
//! `pushing_in_leaves_faces_on_top_of_each_other.rs` said the push was not the
//! cause: the box was already open before it. This reduces that further — to
//! **two** operations, with every automatic behaviour off, which is as small as
//! the defect gets.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use glam::DVec3;

/// Engine default: no auto-intersect, no face synthesis, no re-derive, no
/// freeform overlap. Whatever happens here is the draw's own doing.
fn bare() -> Scene {
    Scene::new()
}

fn a_box(s: &mut Scene) {
    // z from 200 to 320, x from -120 to 20, y from -210 to -90.
    s.mesh
        .create_box(DVec3::new(-50.0, -150.0, 260.0), 140.0, 120.0, 140.0, FORM_MATERIAL)
        .expect("a box");
}

fn faces(s: &Scene) -> Vec<axia_geo::FaceId> {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect()
}

fn solid_count(s: &Scene) -> usize {
    faces(s).iter().filter(|f| s.mesh.is_face_in_volume(**f)).count()
}

#[test]
fn a_box_alone_is_closed() {
    let mut s = bare();
    a_box(&mut s);
    println!("\n  상자만: 면 {}개, 그중 솔리드 {}", faces(&s).len(), solid_count(&s));
    assert_eq!(faces(&s).len(), 6);
    assert_eq!(solid_count(&s), 6, "a fresh box is six solid faces");
}

/// The two operations.
///
/// The circle sits on z=200 — the box's bottom — and overlaps part of it. It is
/// drawn as a kernel-native curve, so it is one anchor and one self-loop edge.
#[test]
fn drawing_a_circle_on_the_bottom_leaves_the_box_closed() {
    let mut s = bare();
    a_box(&mut s);
    let before = faces(&s);

    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(-100.0, -100.0, 200.0),
        normal: DVec3::Z,
        radius: 30.0,
    });

    let after = faces(&s);
    println!("\n  그린 뒤: 면 {}개 (전 {}개), 그중 솔리드 {}", after.len(), before.len(), solid_count(&s));
    for f in &after {
        let Some(face) = s.mesh.faces.get(*f) else { continue };
        let n = face.normal().normalize_or_zero();
        let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
        println!(
            "    {f:?}  {}  n=({:+.0},{:+.0},{:+.0})  verts={}  {}",
            if s.mesh.is_face_in_volume(*f) { "solid" } else { "sheet" },
            n.x, n.y, n.z, vv.len(),
            if before.contains(f) { "old" } else { "new" }
        );
    }

    // The box's six faces have to still be six faces of a solid. The drawn
    // circle may add faces of its own — that is not what this is about.
    let surviving_box_faces = before.iter().filter(|f| after.contains(f)).count();
    assert_eq!(
        surviving_box_faces, 6,
        "drawing on a plane a solid's face already occupies must not consume \
         that face — one of the box's six is gone"
    );
    assert_eq!(
        before.iter().filter(|f| s.mesh.is_face_in_volume(**f)).count(),
        6,
        "and the six have to still bound a solid: a face that no longer has a \
         neighbour on every edge reads as a sheet, which is how the box opens"
    );
}

/// Which layer eats the face: the kernel call, or the scene around it?
///
/// `Scene::execute` does more than build the curve — with every automatic
/// behaviour off it is supposed to do almost nothing else, so this calls the
/// kernel directly with the same anchor and circle and compares. Whichever side
/// loses the box's bottom is the side to fix.
#[test]
fn the_kernel_call_alone_leaves_the_box_closed() {
    let mut s = bare();
    a_box(&mut s);
    let before = faces(&s);

    let center = DVec3::new(-100.0, -100.0, 200.0);
    let r = 30.0;
    let anchor_pos = center + DVec3::X * r;
    let anchor = s.mesh.add_vertex(anchor_pos);
    let circle = axia_geo::AnalyticCurve::Circle {
        center,
        normal: DVec3::Z,
        radius: r,
        basis_u: DVec3::X,
    };
    let made = s.mesh.add_face_closed_curve(anchor, circle, axia_core::FORM_MATERIAL);

    let after = faces(&s);
    let kept = before.iter().filter(|f| after.contains(f)).count();
    println!(
        "\n  커널만: 면 {} → {}, 상자 면 {kept}/6 남음, 결과 {:?}",
        before.len(),
        after.len(),
        made.is_ok()
    );
    for f in &after {
        if before.contains(f) {
            continue;
        }
        let Some(face) = s.mesh.faces.get(*f) else { continue };
        let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
        println!("    새 면 {f:?}  verts={}", vv.len());
    }
    assert!(made.is_ok(), "the kernel call has to succeed: {made:?}");
    assert_eq!(
        kept, 6,
        "the kernel call on its own must not consume the box's bottom — if this \
         passes while the Command does not, the fault is in the scene layer"
    );
}

/// The case that the double-cover repair was written for, asked the other
/// question.
///
/// `scene::tests::the_same_draw_over_a_one_shot_face_now_resolves_too` draws a
/// rectangle straddling a box's ground face and pins that nothing is left
/// double-covered. It does not ask whether the box survives, and that is the
/// question this defect is about — so it is asked here, on the same scene.
#[test]
fn the_rect_over_a_box_ground_case_keeps_its_box() {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;

    let box_faces = s
        .mesh
        .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("box".into(), DVec3::ZERO, box_faces.clone());

    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(200.0, 200.0, 0.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 200.0,
        height: 200.0,
    });

    let kept = box_faces
        .iter()
        .filter(|f| s.mesh.faces.get(**f).map_or(false, |x| x.is_active()))
        .count();
    let still_solid = box_faces.iter().filter(|f| s.mesh.is_face_in_volume(**f)).count();
    let overlaps = s.mesh.detect_self_intersections().count();
    println!(
        "\n  상자 면 {}/6 남음, 그중 솔리드 {}, 겹침 {}",
        kept, still_solid, overlaps
    );

    // NOT "all six survive": the coplanar re-tile legitimately replaces the
    // ground face (ADR-281 β-1) and hands the walls back the edges they stand
    // on. What must hold is that whatever survives is still part of a solid —
    // a wall that has become a sheet is the box coming apart, which is the
    // defect, and a re-tiled ground is not.
    assert_eq!(
        still_solid, kept,
        "every surviving face of the box has to still bound a solid — {} of {}          do, so the draw took the box apart",
        still_solid, kept
    );
}
