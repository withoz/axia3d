//! A CONTAINER CAN ONLY TAKE A BOUNDARY THAT STILL HAS A FREE SIDE.
//!
//! Every containment split ends the same way: the inner face keeps its own
//! half-edges and its container is handed the TWINS as a hole loop. That needs
//! the other side of every boundary edge to be unclaimed, and until 2026-09-05
//! the code asked a narrower question — does the chosen twin already bear a
//! face? A radial ring can hold more than two half-edges, so "this twin is
//! free" and "this edge is free" are not the same sentence, and a coplanar
//! arrangement produces exactly the case where they differ.
//!
//! Measured on the four-draw scene
//! (`axia-core/tests/four_draws_that_used_to_break_the_manifold_contract.rs`):
//! the sliver where a rectangle's corner pokes outside a circle kept its arc edge
//! with three half-edges — two already bearing the sliver and its neighbouring
//! arrangement cell, one free. The punch took the free one and the edge ended
//! up with three faces.
//!
//! ⚠ These two cases are the rule at its own level, so a reader does not have
//! to build a four-operation scene to see what the gate does. The scene-level
//! consequence is pinned separately, in the file named above.

use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

fn poly(mesh: &mut Mesh, corners: &[(f64, f64)]) -> axia_geo::FaceId {
    let vids: Vec<_> = corners
        .iter()
        .map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, 0.0)))
        .collect();
    mesh.add_face(&vids, MaterialId::new(0)).expect("a face")
}

/// Two small rectangles side by side, both inside a big one. The shared edge
/// between them already has its two faces, so neither can become the big one's
/// hole — the container would be the third face on that edge.
#[test]
fn a_boundary_a_neighbour_already_holds_is_refused() {
    let mut mesh = Mesh::new();
    let big = poly(&mut mesh, &[(-100.0, -100.0), (100.0, -100.0), (100.0, 100.0), (-100.0, 100.0)]);
    let left = poly(&mut mesh, &[(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0)]);
    let right = poly(&mut mesh, &[(30.0, 10.0), (50.0, 10.0), (50.0, 30.0), (30.0, 30.0)]);

    // The premise: they really do share an edge, and it really is full.
    let shared = mesh
        .collect_loop_hes(mesh.faces[left].outer().start)
        .expect("left's loop")
        .into_iter()
        .map(|h| mesh.hes[h].edge())
        .find(|&e| {
            let (faces, _) = mesh.get_faces_sharing_edge(e);
            faces.len() == 2 && faces.contains(&right)
        })
        .expect("the two small rects were built without sharing an edge — the premise is gone");
    assert_eq!(mesh.get_faces_sharing_edge(shared).0.len(), 2);

    let before = mesh.clone();
    let refusal = axia_geo::operations::annulus::split_face_by_inner_polygon(&mut mesh, big, left)
        .expect_err(
            "the big rect took a boundary whose other side belongs to its neighbour; \
             that edge now has three faces",
        );
    assert!(
        format!("{refusal}").contains("no side is free"),
        "refused for a different reason: {refusal}",
    );

    // And it refused without touching anything.
    assert!(mesh.collect_non_manifold_edges().is_empty(), "the refusal still left damage");
    assert_eq!(
        mesh.faces[big].inners().len(),
        before.faces[big].inners().len(),
        "the container gained a hole loop on the way out",
    );
}

/// The control, and the reason the gate is not simply "never reparent": a
/// polygon whose boundary nobody else uses still becomes a hole.
#[test]
fn an_unshared_boundary_is_still_taken() {
    let mut mesh = Mesh::new();
    let big = poly(&mut mesh, &[(-100.0, -100.0), (100.0, -100.0), (100.0, 100.0), (-100.0, 100.0)]);
    let alone = poly(&mut mesh, &[(-50.0, -50.0), (-30.0, -50.0), (-30.0, -30.0), (-50.0, -30.0)]);

    axia_geo::operations::annulus::split_face_by_inner_polygon(&mut mesh, big, alone)
        .expect("an unshared boundary is exactly what the split is for");

    assert_eq!(mesh.faces[big].inners().len(), 1, "the hole did not land");
    assert!((mesh.face_area(big) - (40000.0 - 400.0)).abs() < 1e-9, "{}", mesh.face_area(big));
    assert!(mesh.collect_non_manifold_edges().is_empty());
}

/// The circle path has the same rule, and needed it more: it never asked about
/// the twin at all, it just took `next_rad` and set a face on it. A second
/// container would have STOLEN the hole from the first, which stays listed as
/// its owner — an inner loop whose half-edges belong to someone else.
#[test]
fn a_disk_already_held_as_a_hole_is_not_taken_twice() {
    use axia_geo::curves::AnalyticCurve;

    let mut mesh = Mesh::new();
    let plate = poly(&mut mesh, &[(-100.0, -100.0), (100.0, -100.0), (100.0, 100.0), (-100.0, 100.0)]);
    let ring_anchor = mesh.add_vertex(DVec3::new(70.0, 0.0, 0.0));
    let ring = mesh
        .add_face_closed_curve(
            ring_anchor,
            AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: 70.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("a disk");
    let disk_anchor = mesh.add_vertex(DVec3::new(30.0, 0.0, 0.0));
    let disk = mesh
        .add_face_closed_curve(
            disk_anchor,
            AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: 30.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("a smaller disk");

    axia_geo::operations::annulus::split_face_by_inner_circle_generic(&mut mesh, plate, disk)
        .expect("the first container takes it");
    let held_by_plate = mesh.faces[plate].inners()[0].start;

    let refusal =
        axia_geo::operations::annulus::split_face_by_inner_circle(&mut mesh, ring, disk)
            .expect_err("a second container took a hole that already had an owner");
    assert!(
        format!("{refusal}").contains("no side is free"),
        "refused for a different reason: {refusal}",
    );

    assert_eq!(
        mesh.hes[held_by_plate].face(),
        plate,
        "the plate's hole loop was reassigned under it",
    );
    assert!(mesh.faces[ring].inners().is_empty(), "the ring took it anyway");
}

/// The freeform closed-curve split is the fourth entry, and the only one that
/// does not take `reject_non_nestable` (a freeform curve has no comparable
/// "size"), so it asks the boundary question itself. Without this the gate on
/// that path would be decoration.
#[test]
fn the_freeform_entry_refuses_the_same_boundary() {
    use axia_geo::curves::AnalyticCurve;

    let mut mesh = Mesh::new();
    let plate = poly(&mut mesh, &[(-100.0, -100.0), (100.0, -100.0), (100.0, 100.0), (-100.0, 100.0)]);
    let other = poly(&mut mesh, &[(-100.0, -100.0), (100.0, -100.0), (100.0, 100.0), (-100.0, 100.0)]);
    let disk_anchor = mesh.add_vertex(DVec3::new(30.0, 0.0, 0.0));
    let disk = mesh
        .add_face_closed_curve(
            disk_anchor,
            AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: 30.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("a disk");

    axia_geo::operations::annulus::split_face_by_inner_closed_curve_generic(&mut mesh, plate, disk)
        .expect("the first container takes it");
    let refusal = axia_geo::operations::annulus::split_face_by_inner_closed_curve_generic(
        &mut mesh, other, disk,
    )
    .expect_err("a second container took a hole that already had an owner");
    assert!(
        format!("{refusal}").contains("no side is free"),
        "refused for a different reason: {refusal}",
    );
    assert!(mesh.faces[other].inners().is_empty(), "it took it anyway");
}
