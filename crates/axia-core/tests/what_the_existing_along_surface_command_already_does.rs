//! Before claiming the imprint added a capability: what does the command that
//! ALREADY exists do?
//!
//! `Command::DrawLineAlongSurface` routes with `path_along_surface` and draws
//! each leg with `exec_draw_line_as_shape`, which goes through the whole draw
//! pipeline (crossing-split, re-derive, arrangement).
//!
//! ⚠ **MEASURED, AND THE PLAN'S PREMISE IS WRONG FOR TWO FACES.** Track B is
//! specified as "각인(면 분할)은 없다". It exists:
//!
//! ```text
//!   endpoints INSIDE their faces   6 -> 6   draws a wire, divides nothing
//!   endpoints ON A RIM (legs cross) 6 -> 8   DIVIDES BOTH FACES
//!   a three-face route              refused  "사이 면을 지나는 경로는 아직 없습니다"
//! ```
//!
//! The first row is not a failure of the command — a leg that stops inside its
//! face has no second boundary to cut to, the same limit the imprint reports as
//! `skipped_runs`. Give it a route it CAN divide and it divides.
//!
//! So what `Mesh::imprint_surface_path` actually adds is narrower than the plan
//! says, and worth stating plainly:
//!
//! 1. **more than two faces** — the routing gives up past two
//!    (`NoPath::LeavesThroughAnotherEdge`), so a band round a box (4) or a
//!    polygonal cylinder (24) cannot be expressed at all here;
//! 2. **an already-resolved path as input** — points and faces supplied, no
//!    routing, which is what lets a caller imprint a shape it computed itself;
//! 3. **a report** — `skipped_runs` + `first_refusal` instead of a silent
//!    no-op, so "nothing divided" is distinguishable from "nothing to divide".
//!
//! For the two-face case the two routes overlap. That is a fact about the
//! engine, not a defect in either, and it is pinned here so nobody re-derives
//! it — this is the fourth time this session that a plan row turned out to
//! describe a capability the engine already had (D5, D7, D8 were the others).

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

fn production_box() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    // Same solid the existing along-surface sim uses: centre (100,100,50),
    // w=200 h=100 d=200 → x,y ∈ [0,200], z ∈ [0,100].
    s.mesh
        .create_box(
            DVec3::new(100.0, 100.0, 50.0),
            200.0,
            100.0,
            200.0,
            Default::default(),
        )
        .expect("box");
    s
}

/// CASE 1 — endpoints in the faces' INTERIORS, which is what a user click
/// usually gives. Neither leg reaches a second boundary, so neither can divide
/// anything — the same limit the imprint reports as `skipped_runs`.
#[test]
fn a_line_between_interior_points_draws_but_divides_nothing() {
    let mut s = production_box();
    let before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(before, 6);

    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(100.0, 100.0, 100.0), // the top, interior
        end: DVec3::new(200.0, 100.0, 50.0),    // a wall, interior
    });
    let after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let v = s.mesh.verify_face_invariants();
    println!("CASE1 result={r:?} faces {before} -> {after} valid={}", v.is_valid());

    assert!(matches!(r, axia_core::CommandResult::ShapeCreated(_)), "it succeeds: {r:?}");
    assert_eq!(
        after, before,
        "but nothing divides — both legs stop inside their face"
    );
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);
}

/// CASE 2 — the same command with both endpoints ON A RIM, so each leg crosses
/// its face boundary to boundary. This is the fair question: given a route it
/// COULD divide, does the existing command?
#[test]
fn a_line_between_rim_points_divides_or_does_not_and_this_says_which() {
    let mut s = production_box();
    let before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();

    // top: z = 100, x,y in [0,200]. +X wall: x = 200, z in [0,100].
    // Their shared edge is x = 200, z = 100.
    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(0.0, 100.0, 100.0), // the top's far rim
        end: DVec3::new(200.0, 100.0, 0.0),   // the wall's bottom rim
    });
    let after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let v = s.mesh.verify_face_invariants();
    println!("CASE2 result={r:?} faces {before} -> {after} valid={}", v.is_valid());
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);

    assert_eq!(
        after,
        before + 2,
        "PIN: given a route it CAN divide, the existing command divides both          faces ({before} -> {after}). If this drops to {before}, the draw          pipeline stopped splitting and the imprint is the only route left"
    );
}

/// And the limit that makes the imprint worth having: three faces.
#[test]
fn a_route_across_three_faces_is_refused_by_the_existing_command() {
    let mut s = production_box();
    // +X to −Y: not adjacent through one edge — the route would need +Y or the
    // bottom in between.
    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(0.0, 100.0, 50.0),   // the −X wall
        end: DVec3::new(200.0, 100.0, 50.0),   // the +X wall — the top in between
    });
    match r {
        axia_core::CommandResult::Error(msg) => {
            println!("THREE-FACE refusal: {msg}");
            assert!(
                msg.contains("면") ,
                "and it says so in the user's language: {msg}"
            );
        }
        other => panic!(
            "PIN: a three-face route is refused today. If this now succeeds, \
             `path_along_surface` grew past two faces — retire this pin. Got {other:?}"
        ),
    }
}
