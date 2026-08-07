//! A CLOSED SOLID DOES NOT NEED FOUR FACES.
//!
//! `face_set_manifold_info` required `active_faces >= 4`, noted as "최소 closed
//! solid = tetrahedron". True only while every face is flat — a curved face can
//! close space by itself, which is what the whole Path B family is. Measured
//! 2026-08-07: sphere (2 faces), cone (2) and cylinder (3) each had boundary 0
//! and non-manifold 0 and were still reported open, so `promote_shape_to_xia`
//! refused all three with `NotWatertight{boundary_edges: 0}` — a Path B
//! primitive could not become a XIA at all (사용자: "요건이 잘못된것이 아닐까?").
//!
//! What keeps the wrong things open is the boundary count, not the face count.
//! Both halves are pinned here: the primitives close, and the things that must
//! stay open still do.
use axia_core::{Scene, FORM_MATERIAL};
use axia_geo::entities::MaterialId;
use glam::DVec3;

fn pathb() -> Scene {
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.set_cone_path_b_default(true);
    s.mesh.set_cylinder_path_b_default(true);
    s
}

fn closed(s: &Scene, faces: &[axia_geo::FaceId]) -> bool {
    s.mesh.face_set_manifold_info(faces).is_closed_solid
}

#[test]
fn path_b_primitives_are_closed_solids() {
    for (name, expect_faces) in [("sphere", 2usize), ("cone", 2), ("cylinder", 3)] {
        let mut s = pathb();
        let f = match name {
            "sphere" => s.mesh.create_sphere(DVec3::ZERO, 50.0, 16, 12, FORM_MATERIAL).unwrap(),
            "cone" => s.mesh.create_cone(DVec3::ZERO, 50.0, 100.0, 24, FORM_MATERIAL).unwrap(),
            _ => s.mesh.create_cylinder(DVec3::ZERO, 50.0, 100.0, 24, FORM_MATERIAL).unwrap(),
        };
        assert_eq!(f.len(), expect_faces, "{name}: Path B face count");
        let mi = s.mesh.face_set_manifold_info(&f);
        assert_eq!(mi.boundary_edge_count, 0, "{name}: nothing open");
        assert_eq!(mi.non_manifold_edge_count, 0, "{name}");
        assert!(
            mi.is_closed_solid,
            "{name}: {} curved faces enclose space just as well as {} flat ones",
            f.len(), 4
        );
        assert!(s.mesh.verify_face_invariants().is_valid());
    }
}

/// The other half: the boundary count is what holds the line, so it must still
/// hold it. If this ever passes, the floor removal went too far.
#[test]
fn open_shells_are_still_open() {
    // a single triangle — three boundary edges
    let mut s = Scene::new();
    let a = s.mesh.add_vertex(DVec3::ZERO);
    let b = s.mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = s.mesh.add_vertex(DVec3::new(0.0, 100.0, 0.0));
    let t = s.mesh.add_face(&[a, b, c], FORM_MATERIAL).unwrap();
    assert!(!closed(&s, &[t]), "one triangle encloses nothing");

    // a box with a face taken away
    let mut s = Scene::new();
    let f = s.mesh.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, FORM_MATERIAL).unwrap();
    let lidless: Vec<_> = f.iter().copied().take(5).collect();
    assert!(!closed(&s, &lidless), "a lidless box is open");
    assert!(closed(&s, &f), "and the whole box is not");
}

/// A zero-volume "pillow" (two triangles over the same three edges) DOES close
/// now — every edge has exactly two faces. It is refused where that belongs, by
/// volume, and this pins which check does the refusing.
#[test]
fn a_flat_pillow_closes_but_cannot_become_a_xia() {
    let mut s = Scene::new();
    let a = s.mesh.add_vertex(DVec3::ZERO);
    let b = s.mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = s.mesh.add_vertex(DVec3::new(0.0, 100.0, 0.0));
    let t1 = s.mesh.add_face(&[a, b, c], FORM_MATERIAL).unwrap();
    let t2 = s.mesh.add_face(&[c, b, a], FORM_MATERIAL).unwrap();
    assert!(closed(&s, &[t1, t2]), "every edge has two faces");
    let shape = s.create_shape("pillow".into(), vec![t1, t2]);
    let err = s.promote_shape_to_xia(shape, MaterialId::new(1)).unwrap_err();
    assert!(
        matches!(err, axia_core::promote::PromoteError::ZeroVolume),
        "volume is the check that should refuse a pillow, got {err:?}"
    );
}

/// The axis definition of a sphere — poles, one seam, one face — passes the
/// invariants. Before the seam exemption it reported "outer loop has 2 verts".
#[test]
fn a_seam_bounded_face_satisfies_the_invariants() {
    let mut s = Scene::new();
    let f = s.mesh.sim_create_sphere_axis_native(DVec3::ZERO, 50.0, DVec3::Z, FORM_MATERIAL).unwrap();
    let r = s.mesh.verify_face_invariants();
    assert!(r.is_valid(), "a seam is a boundary: {:?}", r.violations);
    let mi = s.mesh.face_set_manifold_info(&[f]);
    assert_eq!(mi.boundary_edge_count, 0, "the seam is walked twice, so nothing is open");
    assert!(mi.is_closed_solid, "one curved face closes the sphere");
}
