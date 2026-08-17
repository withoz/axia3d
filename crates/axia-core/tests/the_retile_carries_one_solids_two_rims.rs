//! Teaching the coplanar re-tile to carry a solid whose on-plane face has a
//! hole — two rims, one owner.
//!
//! The re-tile declines any plane that reaches more than one perimeter. The
//! reason it gives is about two SOLIDS ("a face belongs to one owner"), and that
//! reason does not apply to one solid with a hole in its top. The count rule
//! catches both because it counts components, not owners.
//!
//! Session 10 of the fuzz is the cost: a circle drawn over a ring-topped solid
//! is never divided against that top, so it lies on it. This file measures what
//! the re-tile does to a two-rim solid when it is allowed to carry one, which is
//! the thing that has to work before the rule can be relaxed.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
use axia_geo::FaceId;
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
    let faces: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .collect();
    let info = s.mesh.face_set_manifold_info(&faces);
    Health {
        faces: faces.len(),
        closed: info.is_closed_solid,
        boundary: info.boundary_edge_count,
        non_manifold: info.non_manifold_edge_count,
        violations: s.mesh.verify_face_invariants().violations.len(),
    }
}

/// A drilled box, then a rectangle straddling the hole's rim on its top.
fn drilled_box_with_a_rect_across_the_rim() -> Scene {
    let mut s = prod();
    let f = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 60.0), 200.0, 120.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("box".into(), DVec3::ZERO, f);
    s.drill_rect_through_hole(
        DVec3::new(-30.0, -30.0, 120.0),
        DVec3::new(30.0, 30.0, 120.0),
        DVec3::Z,
    )
    .expect("the drill");
    let r = s.execute(Command::DrawRectAsShape {
        center: DVec3::new(30.0, 0.0, 120.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 60.0,
        height: 30.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    s
}

/// What the re-tile does to it, whichever way the rule is set.
///
/// Prints rather than asserts the counts, and asserts only the thing that must
/// hold either way — so it can be read with the decline in place and with it
/// removed, which is how the damage was found.
#[test]
fn what_a_two_rim_solid_looks_like_after_a_draw() {
    let s = drilled_box_with_a_rect_across_the_rim();
    let h = health(&s);
    println!(
        "\n  drilled box + rect across the rim: 면 {}  closed {}  경계 {}  비-manifold {}  위반 {}",
        h.faces, h.closed, h.boundary, h.non_manifold, h.violations
    );

    let mut shown = 0;
    for (eid, edge) in s.mesh.edges.iter() {
        if !edge.is_active() {
            continue;
        }
        let (adj, _) = s.mesh.get_faces_sharing_edge(eid);
        let live: Vec<FaceId> = adj
            .iter()
            .copied()
            .filter(|f| s.mesh.faces.get(*f).map_or(false, |x| x.is_active()))
            .collect();
        if live.len() == 2 || shown >= 8 {
            continue;
        }
        let d: Vec<String> = live
            .iter()
            .map(|f| {
                format!(
                    "{f:?}({}, area={:.0}, holes={})",
                    if s.mesh.is_face_in_volume(*f) { "solid" } else { "sheet" },
                    s.mesh.face_outer_area(*f),
                    s.mesh.faces[*f].inners().len()
                )
            })
            .collect();
        println!("    {eid:?} [{}면] → {}", live.len(), d.join(" · "));
        shown += 1;
    }

    assert!(h.faces >= 11, "the draw has to land: {} faces", h.faces);
}

/// Two boxes overlapping on the ground plane — genuinely two owners.
fn two_boxes_sharing_the_ground() -> Scene {
    let mut s = prod();
    let a = s.mesh.create_box(DVec3::new(0.0, 0.0, 60.0), 120.0, 120.0, 120.0, FORM_MATERIAL).unwrap();
    s.create_xia_with_faces("A".into(), DVec3::ZERO, a);
    let b = s.mesh.create_box(DVec3::new(80.0, 80.0, 60.0), 120.0, 120.0, 120.0, FORM_MATERIAL).unwrap();
    s.create_xia_with_faces("B".into(), DVec3::ZERO, b.clone());
    s.intersect_faces_with_scene(&b).expect("the scene pass");
    s
}

/// Both scenes, reported as health rather than as a face count.
///
/// The two tests that guard the decline assert face COUNTS — 11 and 12 — and
/// those are the first assertions in each, so when the count moves they fire
/// and the health assertions behind them never run. This asks the health
/// directly, which is what "the re-tile cannot carry this" was a claim about.
#[test]
fn both_guarded_scenes_are_sound_whatever_the_count() {
    for (name, s) in [
        ("구멍 뚫린 상자 + 림 걸친 rect", drilled_box_with_a_rect_across_the_rim()),
        ("겹친 상자 둘", two_boxes_sharing_the_ground()),
    ] {
        let h = health(&s);
        println!(
            "\n  {name}: 면 {}  closed {}  경계 {}  비-manifold {}  위반 {}",
            h.faces, h.closed, h.boundary, h.non_manifold, h.violations
        );
        assert_eq!(h.boundary, 0, "{name}: nothing may be left open");
        assert_eq!(h.non_manifold, 0, "{name}: no edge may bear three faces");
        assert_eq!(h.violations, 0, "{name}: and the invariants hold");
    }
}
