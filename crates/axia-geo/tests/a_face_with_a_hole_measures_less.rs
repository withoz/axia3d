//! WHAT "AREA" MEANS WHEN THE FACE HAS A HOLE.
//!
//! Measured in the running app (2026-08-04): a box top with a Ø140 circle cut in
//! it reported 40000 — the whole 200×200 — and the ring around a Ø60 circle
//! reported π·70² instead of π·(70²−30²). `face_area` read the outer loop and
//! stopped. That number reaches the user through the XIA Inspector's 면적 and
//! 표면적, so a wall with a window was billed for the window.
//!
//! But the deduction cannot simply be pushed into the one function, because
//! nearly every OTHER caller is asking a different question. Measured, all of
//! them: which coplanar face contains this point, which of two containers is the
//! inner one, is this boundary degenerate. For those, holes must NOT come off —
//! a ring with a wide hole would sort as smaller than the face it contains, and
//! a thin frame would fall under the degeneracy threshold and be deleted.
//!
//! So there are two questions and now two names. This pins both, and the third
//! thing measurement forced: on a CURVED surface an inner loop need not be a
//! hole at all.
use axia_geo::curves::AnalyticCurve;
use axia_geo::operations::annulus::split_face_by_inner_circle;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

fn disk(mesh: &mut Mesh, centre: DVec3, radius: f64) -> axia_geo::FaceId {
    let anchor = mesh.add_vertex(centre + DVec3::new(radius, 0.0, 0.0));
    mesh.add_face_closed_curve(
        anchor,
        AnalyticCurve::Circle { center: centre, radius, normal: DVec3::Z, basis_u: DVec3::X },
        MaterialId::new(0),
    )
    .expect("a circle face")
}

fn rect(mesh: &mut Mesh, half: f64) -> axia_geo::FaceId {
    let vids: Vec<_> = [
        DVec3::new(-half, -half, 0.0),
        DVec3::new(half, -half, 0.0),
        DVec3::new(half, half, 0.0),
        DVec3::new(-half, half, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    mesh.add_face(&vids, MaterialId::new(0)).expect("a rect face")
}

#[test]
fn a_ring_is_measured_as_a_ring_and_still_sorts_as_the_bigger_region() {
    let mut mesh = Mesh::new();
    let ring = disk(&mut mesh, DVec3::ZERO, 70.0);
    let inner = disk(&mut mesh, DVec3::ZERO, 30.0);
    split_face_by_inner_circle(&mut mesh, ring, inner).expect("the disk becomes the ring's hole");

    let annulus = std::f64::consts::PI * (70.0f64.powi(2) - 30.0f64.powi(2));
    let full = std::f64::consts::PI * 70.0f64.powi(2);
    assert!(
        (mesh.face_area(ring) - annulus).abs() < 1e-6,
        "the material left: got {}, want {annulus}",
        mesh.face_area(ring)
    );
    assert!(
        (mesh.face_outer_area(ring) - full).abs() < 1e-6,
        "the region it encloses: got {}, want {full}",
        mesh.face_outer_area(ring)
    );
    // The one that orders containment must keep the ring ABOVE the disk it holds.
    assert!(
        mesh.face_outer_area(ring) > mesh.face_outer_area(inner),
        "a ring still encloses more than the disk inside it"
    );
    // And by material alone it would not: that is why the two names exist.
    assert!(mesh.face_area(inner) < mesh.face_area(ring), "not the trap here, but pin it");
}

/// The case that reaches the user: a box top with a circle cut out of it.
#[test]
fn a_face_with_a_circular_hole_does_not_bill_for_the_hole() {
    let mut mesh = Mesh::new();
    let top = rect(&mut mesh, 100.0);
    let hole = disk(&mut mesh, DVec3::ZERO, 70.0);
    axia_geo::operations::annulus::split_face_by_inner_circle_generic(&mut mesh, top, hole)
        .expect("the circle becomes the top's hole");

    let want = 200.0 * 200.0 - std::f64::consts::PI * 70.0f64.powi(2);
    assert!(
        (mesh.face_area(top) - want).abs() < 1e-6,
        "got {}, want {want}",
        mesh.face_area(top)
    );
    assert!((mesh.face_outer_area(top) - 40000.0).abs() < 1e-9, "the region is still 200×200");
}

/// A polygon hole in a polygon face — the same, without a curve anywhere.
#[test]
fn a_polygon_hole_comes_off_too() {
    let mut mesh = Mesh::new();
    let big = rect(&mut mesh, 100.0);
    let small = rect(&mut mesh, 30.0);
    axia_geo::operations::annulus::split_face_by_inner_polygon(&mut mesh, big, small)
        .expect("the small rect becomes the big one's hole");
    assert!((mesh.face_area(big) - (40000.0 - 3600.0)).abs() < 1e-9, "{}", mesh.face_area(big));
    assert!((mesh.face_outer_area(big) - 40000.0).abs() < 1e-9);
    assert!((mesh.face_area(small) - 3600.0).abs() < 1e-9, "the hole-filler is whole");
}

/// On a CURVED surface an inner loop is not necessarily a hole.
///
/// A Path B cylinder is one side face whose two loops are the two RIMS of the
/// tube (ADR-094). Its area comes from the surface formula, not the boundary, so
/// deducting the top rim turned 2πrh into 2πrh − πr² — measured, and it is why
/// the deduction is planar-only.
#[test]
fn a_tubes_far_rim_is_not_a_hole() {
    let mut mesh = Mesh::new();
    mesh.set_cylinder_path_b_default(true); // Path B: 3 faces, side is the annulus
    let faces = mesh
        .create_cylinder(DVec3::ZERO, 5.0, 10.0, 16, MaterialId::new(0))
        .expect("a cylinder");
    let side = faces[2];
    assert!(
        !mesh.faces[side].inners().is_empty(),
        "the fixture only says anything if the side really has a second loop"
    );
    let lateral = 2.0 * std::f64::consts::PI * 5.0 * 10.0;
    let got = mesh.face_area(side);
    assert!(
        (got - lateral).abs() < lateral * 0.01,
        "the tube's whole wall: got {got}, want {lateral} \
         (deducting the far rim would give {})",
        lateral - std::f64::consts::PI * 25.0
    );
}
