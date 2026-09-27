//! AN EXTRUDED BOX IS THE SHAPE THE PRIMITIVE MAKES.
//!
//! Two routes to the same solid used to leave different meshes. `create_box`
//! gives 24 half-edges over 12 edges; drawing a rect and extruding it gave 32 —
//! two extra on every profile boundary edge, carrying no face.
//!
//! `find_halfedge` Pass 1 only reuses a FREE half-edge pointing the way the new
//! loop needs. A wall asks for its base edge in the direction its own winding
//! needs, and the bottom cap's loop ran that same way until ADR-183 flipped it —
//! which happened AFTER the walls were built. So Pass 1 missed, Pass 2 allocated
//! a second pair, and the first was stranded. Moving the flip before the walls
//! (`extrude_planar_box`, `..._tapered`, 2026-09-28) makes Pass 1 take it.
//!
//! ```text
//!                     before      after
//!   rect extrude      32 he / 8   24 he / 0     = create_box
//!   hexagon extrude   48 he / 12  36 he / 0
//!   tapered           32 he / 8   24 he / 0
//!   bidirectional     32 he / 8   24 he / 0
//! ```
//!
//! ADR-264's fuse path already relied on this and says so: it removes the profile
//! before the walls for the same reason.
//!
//! ⚠ The circle route (`extrude_planar_cylinder`) still carries its 46. The same
//! move works there too and is deliberately NOT taken — `halfedge_litter_audit`
//! and `what_overlapping_draws_leave_on_the_ground` both record why, with the two
//! scenes it breaks.

use axia_core::{Command, Scene, FORM_MATERIAL};
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

/// (half-edges, of which face-less, edges, verts, faces, duplicate edge pairs)
fn shape(s: &Scene) -> (usize, usize, usize, usize, usize, usize) {
    let he = s.mesh.hes.iter().filter(|(_, h)| h.is_active()).count();
    let spare = s
        .mesh
        .hes
        .iter()
        .filter(|(_, h)| h.is_active() && h.face().is_null())
        .count();
    let edges = s.mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
    let verts = s.mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let mut seen = std::collections::HashMap::new();
    for (_, e) in s.mesh.edges.iter() {
        if e.is_active() {
            *seen.entry((e.v_small(), e.v_large())).or_insert(0usize) += 1;
        }
    }
    let dup = seen.values().filter(|&&c| c > 1).count();
    (he, spare, edges, verts, faces, dup)
}

fn extruded(kind: u8, mode: CreateSolidMode) -> Scene {
    let mut s = prod();
    match kind {
        0 => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::ZERO,
                normal: DVec3::Z,
                up: DVec3::Y,
                width: 200.0,
                height: 200.0,
            });
        }
        _ => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::ZERO,
                normal: DVec3::Z,
                radius: 100.0,
                sides: 6,
            });
        }
    }
    let f = s
        .mesh
        .faces
        .iter()
        .filter(|(_, x)| x.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a drawn profile");
    s.execute(Command::CreateSolid { face_id: f, mode });
    s
}

#[test]
fn a_drawn_rect_extrudes_to_the_same_mesh_create_box_builds() {
    let drawn = extruded(0, CreateSolidMode::Extrude { distance: 100.0 });

    let mut primitive = prod();
    primitive
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .expect("box primitive");

    assert_eq!(
        shape(&drawn),
        shape(&primitive),
        "the two routes to one box must leave one mesh \
         (half-edges, face-less, edges, verts, faces, duplicate edge pairs)"
    );
    assert_eq!(shape(&drawn), (24, 0, 12, 8, 6, 0), "and that mesh is 24 half-edges over 12 edges");
}

#[test]
fn every_box_shaped_extrude_leaves_nothing_spare() {
    let cases: [(&str, u8, CreateSolidMode, usize, usize); 4] = [
        ("rect", 0, CreateSolidMode::Extrude { distance: 100.0 }, 24, 12),
        ("hexagon", 2, CreateSolidMode::Extrude { distance: 100.0 }, 36, 18),
        (
            "tapered",
            0,
            CreateSolidMode::ExtrudeTapered { distance: 100.0, taper_deg: 10.0 },
            24,
            12,
        ),
        (
            "bidirectional",
            0,
            CreateSolidMode::ExtrudeBidirectional { dist_pos: 50.0, dist_neg: 50.0 },
            24,
            12,
        ),
    ];
    let mut wrong = Vec::new();
    for (what, kind, mode, he, edges) in cases {
        let s = extruded(kind, mode);
        let got = shape(&s);
        if (got.0, got.1, got.2, got.5) != (he, 0, edges, 0) {
            wrong.push(format!(
                "{what}: {} half-edges ({} spare) over {} edges, {} duplicate pairs — \
                 wanted {he} / 0 / {edges} / 0",
                got.0, got.1, got.2, got.5
            ));
        }
    }
    assert!(wrong.is_empty(), "an extrude left litter behind:\n  {}", wrong.join("\n  "));
}

/// The one that is held back, so its number is not mistaken for an oversight.
/// The same move works in `extrude_planar_cylinder` — the circle goes 46 → 0 —
/// and it is not taken: it makes `a_vertex_whose_outgoing_half_edge_is_gone`
/// close a face on three collinear points, and `the_fifty_operation_inventory`'s
/// MoveOnly push gain a stacked pair. Both ledgers say so in full.
#[test]
fn the_circle_route_still_carries_its_leftovers() {
    let mut s = prod();
    s.execute(Command::DrawCircleAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        radius: 100.0,
        segments: 24,
    });
    let f = s
        .mesh
        .faces
        .iter()
        .filter(|(_, x)| x.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a drawn circle");
    s.execute(Command::CreateSolid {
        face_id: f,
        mode: CreateSolidMode::Extrude { distance: 100.0 },
    });
    let (_, spare, ..) = shape(&s);
    assert_eq!(
        spare, 46,
        "the circle route is held at 46 on purpose — if this is 0, the cylinder \
         move was taken and the two scenes named above need re-measuring"
    );
}
