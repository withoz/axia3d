//! A ring and the face filling its hole tile their plane once — not twice.
//!
//! `edge_stacked_face_pair` decides whether an edge carrying three or more faces
//! is damage or just a sheet attached to a solid, and it decides it by which SIDE
//! of the edge each face occupies: `normal × direction`, because a face lies to
//! the left of its own half-edge.
//!
//! That "to the left" only holds on an OUTER loop. A hole loop runs the other way
//! round its face, so the same expression points INTO the hole — away from the
//! face that owns it. Nothing distinguished the two, so a ring and the face in its
//! hole came out on the same side and were called stacked.
//!
//! Measured 2026-08-13, a 400×400 rect drawn over a 200³ box's top face:
//!
//! ```text
//!   ring face 7   hole loop    side (0,-1,0)  ┐ +1.000  → "stacked"
//!   top  face 8   outer loop   side (0,-1,0)  ┘
//!   areas: ring 160000-40000 = 120000, top 40000 → 400×400 covered ONCE
//! ```
//!
//! The verdict was not cosmetic. `face_set_manifold_info` asks the same question,
//! so the box reported `is_closed_solid = false` and `face_set_volume` returned
//! 12,000,000 against a true 8,000,000 — a correct solid reading as broken.
//!
//! The second test is the control: two faces genuinely on top of each other must
//! still be caught, so that "no violations" cannot be reached by switching the
//! check off.

use axia_core::scene::Scene;
use axia_core::Command;
use axia_geo::entities::MaterialId;
use axia_geo::mesh::Mesh;
use glam::DVec3;

#[test]
fn a_ring_around_a_box_top_is_not_stacked_on_it() {
    // Exactly what production does: draw a 400x400 rect over a 200-cube's top.
    // The arrangement turns the drawn rect into a RING whose hole is the box top,
    // so the four rim edges each carry wall + top + ring.
    let mut scene = Scene::new();
    scene.auto_intersect_on_draw = true;
    scene.auto_face_synthesis_on_draw = true;
    scene.face_rederive_on_draw = true;
    let box_faces = scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, MaterialId::default())
        .expect("box");
    assert!(scene.mesh.verify_face_invariants().is_valid(), "the box alone");

    scene.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, 100.0),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });

    // The ring is the face that came out with a hole in it.
    let ring = scene
        .mesh
        .faces
        .iter()
        .find(|(_, f)| f.is_active() && !f.inners().is_empty())
        .map(|(id, _)| id)
        .expect("the drawn rect should have become a ring around the box top");

    // Geometry first: ring plus the face in its hole tile 400x400 exactly once.
    let plane: Vec<_> = scene
        .mesh
        .faces
        .iter()
        .filter(|(id, f)| {
            f.is_active()
                && scene
                    .mesh
                    .face_bounds(*id)
                    .is_some_and(|(lo, hi)| lo.z > 99.0 && hi.z > 99.0 && (hi.z - lo.z).abs() < 1.0)
        })
        .map(|(id, _)| id)
        .collect();
    let covered: f64 = plane.iter().map(|&f| scene.mesh.face_area(f)).sum();
    assert!(
        (covered - 400.0 * 400.0).abs() < 1.0,
        "the plane at z=100 should be covered once (160000), got {covered} over {plane:?}",
    );
    assert!(
        !scene.mesh.faces[ring].inners().is_empty(),
        "the ring kept its hole",
    );

    // So no edge may report two of those faces as covering the same ground.
    let report = scene.mesh.verify_face_invariants();
    assert!(
        report.is_valid(),
        "a ring and the face in its hole are adjacent, not stacked: {:?}",
        report.violations,
    );

    // And the box must still read as the closed solid it is. Name its LIVE faces:
    // the arrangement destroys and rebuilds the top, so `create_box`'s original
    // ids no longer describe the solid (that identity gap is ADR-312, not this).
    let solid: Vec<_> = scene
        .mesh
        .faces
        .iter()
        .filter(|(id, f)| f.is_active() && *id != ring)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(solid.len(), 6, "five original faces plus the rebuilt top");
    let info = scene.mesh.face_set_manifold_info(&solid);
    assert!(
        info.is_closed_solid,
        "the box stayed closed; boundary={} non_manifold={}",
        info.boundary_edge_count, info.non_manifold_edge_count,
    );
    let vol = axia_core::promote::face_set_volume(&scene.mesh, &solid);
    assert!(
        (vol - 8_000_000.0).abs() < 1.0,
        "the box still measures 8,000,000, got {vol}",
    );
}

#[test]
fn two_faces_on_the_same_ground_are_still_caught() {
    // Control. A square, a second square laid over half of it, and a wall
    // standing on their shared edge — three faces on one edge, two of them
    // covering the same ground. Without this, the test above could pass by
    // deleting the check rather than correcting it.
    let mut mesh = Mesh::new();
    let a = mesh.add_vertex(DVec3::new(-100.0, 0.0, 0.0));
    let b = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = mesh.add_vertex(DVec3::new(100.0, 100.0, 0.0));
    let d = mesh.add_vertex(DVec3::new(-100.0, 100.0, 0.0));
    let c2 = mesh.add_vertex(DVec3::new(100.0, 50.0, 0.0));
    let d2 = mesh.add_vertex(DVec3::new(-100.0, 50.0, 0.0));
    let up1 = mesh.add_vertex(DVec3::new(-100.0, 0.0, 100.0));
    let up2 = mesh.add_vertex(DVec3::new(100.0, 0.0, 100.0));

    mesh.add_face(&[a, b, c, d], MaterialId::default()).expect("plate");
    mesh.add_face(&[a, b, c2, d2], MaterialId::default()).expect("plate over it");
    mesh.add_face(&[b, a, up1, up2], MaterialId::default()).expect("wall");

    let shared = mesh.find_edge(a, b).expect("the edge all three share");
    assert_eq!(
        mesh.get_faces_sharing_edge(shared).0.len(),
        3,
        "the control needs three faces on one edge to reach the check",
    );
    assert!(
        mesh.edge_stacked_face_pair(shared).is_some(),
        "two plates on the same side of one edge do cover the same ground",
    );
}
