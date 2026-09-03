//! DRAWING A CIRCLE OVER A BOX'S RIM IS NORMAL, AND THE OVERLAY SAID IT WAS NOT.
//!
//! Reported from the running app 2026-09-03: a circle drawn half on a box's top
//! face and half off it came up with an orange line along the rim. Measured in
//! that scene — the mesh was clean:
//!
//! ```text
//!   faces                   6 -> 8
//!   verify_face_invariants  valid, 0 violations
//!   overlay segments        1     (200,-109.1,200) - (200,109.1,200)
//! ```
//!
//! The two disagreed because they asked different questions. The overlay's
//! source (`getNonManifoldEdgeSegments`) returned every edge with ≥3 active
//! faces; `verify_face_invariants`'s I5 asks whether two of those faces COVER
//! THE SAME GROUND, and by that measure nothing was wrong. I5's own comment
//! records the 사용자 결정 of 2026-08-06:
//!
//! > Three faces on one edge is what a sheet hanging off a solid looks like:
//! > the wall, the cap, and the plate — the solid is still closed, there is
//! > simply something attached to it. SketchUp builds exactly that.
//!
//! So the overlay was warning about the normal case. It now asks the same
//! judgement (`edge_stacked_face_pair`).
//!
//! ⚠ The overlay is NOT dead: two faces covering one patch of ground are
//! invisible except as z-fighting, which users read as "면이 사라졌다", and
//! that is exactly what it exists to mark. This file holds both halves — the
//! straddling draw stays quiet, and an edge that IS stacked still speaks.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

fn production() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// What the overlay draws: edges the engine judges STACKED, not merely shared.
fn overlay_edges(s: &Scene) -> Vec<axia_geo::EdgeId> {
    s.mesh
        .collect_non_manifold_edges()
        .into_iter()
        .filter(|&e| s.mesh.edge_stacked_face_pair(e).is_some())
        .collect()
}

#[test]
fn the_straddling_circle_leaves_nothing_to_warn_about() {
    let mut s = production();
    // The reported scene: a 400x400x200 box, top at z = 200, and a circle
    // centred 150 out along +X with r = 120, so it crosses the rim at x = 200.
    s.mesh
        .create_box(DVec3::new(0.0, 0.0, 100.0), 400.0, 200.0, 400.0, FORM_MATERIAL);
    let r = s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(150.0, 0.0, 200.0),
        normal: DVec3::Z,
        radius: 120.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "the draw itself must land: {r:?}");

    let shared = s.mesh.collect_non_manifold_edges();
    let stacked = overlay_edges(&s);
    let inv = s.mesh.verify_face_invariants();
    println!(
        "  edges with >=3 faces {}   stacked {}   invariant violations {}",
        shared.len(),
        stacked.len(),
        inv.violations.len()
    );

    // The mesh is sound — that is the premise, and if it ever stops being true
    // this file is measuring something else.
    assert!(inv.violations.is_empty(), "the scene stopped being clean: {:?}", inv.violations);

    // ⚠ The rim edge IS shared by three faces (wall, the circle's piece of the
    // top, and the piece hanging off). That is the T-junction, and it must not
    // reach the overlay.
    assert!(
        !shared.is_empty(),
        "the straddling draw no longer makes a 3-face edge — the scene changed, \
         so re-measure before trusting the assertion below"
    );
    assert!(
        stacked.is_empty(),
        "a straddling draw raised {} warning edge(s) on a mesh the invariant \
         checker calls clean: {stacked:?}",
        stacked.len()
    );
}

#[test]
fn an_edge_that_really_is_stacked_still_speaks() {
    // The control. Without it, "quiet" would also pass on an overlay that was
    // simply turned off — which is the failure mode this change risks.
    //
    // A sheet on the ground with a box set down exactly on top of it: the box's
    // bottom and the sheet cover one patch, and their rims meet edge-on.
    // Measured: 4 edges with ≥3 faces, all 4 stacked, 4 invariant violations.
    let mut s = Scene::new(); // engine defaults — nothing divides them
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 200.0,
        height: 200.0,
    });
    s.mesh
        .create_box(DVec3::new(0.0, 0.0, 100.0), 200.0, 200.0, 200.0, FORM_MATERIAL);

    let stacked = overlay_edges(&s);
    let viol = s.mesh.verify_face_invariants().violations.len();
    println!("  box set on a sheet -> stacked {}  violations {viol}", stacked.len());
    assert!(
        viol > 0,
        "the control stopped being a damaged scene — re-measure before trusting \
         the assertion below"
    );
    assert!(
        !stacked.is_empty(),
        "two faces covering the same ground must still reach the overlay — \
         otherwise the filter removed the warning instead of narrowing it"
    );
}

/// ⚠ And what the overlay does NOT cover, said out loud so it is not mistaken
/// for a regression of this change.
///
/// Two exactly coincident sheets are damage — `damaging_contacts` reports 1 —
/// and they share no edge with three faces, so neither the old count nor the
/// new judgement marks them. That was already true; narrowing the filter did
/// not lose it. Marking coplanar overlap without an edge to hang it on is a
/// different overlay.
#[test]
fn coincident_sheets_are_damage_the_overlay_has_never_shown() {
    let mut s = Scene::new();
    for _ in 0..2 {
        s.execute(Command::DrawRectAsShape {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            up: DVec3::X,
            width: 200.0,
            height: 200.0,
        });
    }
    let dmg = s.mesh.damaging_contacts().len();
    let shared = s.mesh.collect_non_manifold_edges().len();
    println!("  coincident sheets -> damaging {dmg}  edges with >=3 faces {shared}");
    assert_eq!(dmg, 1, "the coincident pair stopped being damage: {dmg}");
    assert_eq!(
        shared, 0,
        "there is now an edge with 3+ faces here, so the overlay's reach changed \
         — re-measure what it does and does not show"
    );
}
