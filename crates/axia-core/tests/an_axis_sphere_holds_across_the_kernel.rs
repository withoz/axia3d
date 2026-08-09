//! THE AXIS-DEFINED SPHERE, PUT THROUGH THE KERNEL.
//!
//! 사용자 2026-08-07 asked for the wiring and the kernel to be checked for
//! coherence before anything else was changed. This is that check, kept as a
//! regression: a two-vertex seam boundary meets rules written for polygons in
//! places that have nothing to do with vertex counts, and six such places have
//! turned up so far —
//!
//! ```text
//!   verify_face_invariants I1   outer loop >= 3 verts      fixed (seam exemption)
//!   face_set_manifold_info      active_faces >= 4          fixed (floor removed)
//!   face_set_volume             verts.len() < 3 → skip     fixed (outward flux)
//!   Mesh::mesh_volume           the same, separately       fixed (one reader now)
//!   polygonalize_curved_operand asked for a SELF-LOOP      fixed (asks <3 verts)
//!   translate_verts all-or-none partial move drops surface fixed at the caller
//!   push_pull_create_face       boundary.len() >= 3        OPEN (a circle drawn
//!                                                         inside a sphere still
//!                                                         cannot be extruded)
//! ```
//!
//! Anything else that reads a boundary as a polygon will show up here as a wrong
//! number rather than as a panic, which is why the numbers are checked against
//! closed forms instead of against each other.
use axia_core::{Scene, FORM_MATERIAL};
use glam::DVec3;

const PI: f64 = std::f64::consts::PI;
const R: f64 = 50.0;

fn sphere() -> (Scene, Vec<axia_geo::FaceId>) {
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    let f = s.mesh.create_sphere(DVec3::ZERO, R, 16, 12, FORM_MATERIAL).unwrap();
    (s, f)
}

#[test]
fn it_is_one_face_bounded_by_a_seam_between_the_poles() {
    let (s, f) = sphere();
    assert_eq!(f.len(), 1, "one face carries the whole sphere");
    let verts: Vec<_> = s.mesh.verts.iter().filter(|(_, v)| v.is_active()).map(|(_, v)| v.pos()).collect();
    assert_eq!(verts.len(), 2, "the poles, and nothing on the equator");
    for p in &verts {
        assert!(p.x.abs() < 1e-9 && p.y.abs() < 1e-9, "a pole is on the axis: {p:?}");
        assert!((p.z.abs() - R).abs() < 1e-9, "at ±radius along it: {p:?}");
    }
    assert!(s.mesh.verify_face_invariants().is_valid());
}

/// Both volume readers, against the closed form. They were fixed separately and
/// disagreed for a while — `mesh_volume` (which the WASM boundary exposes) still
/// said 0 after `face_set_volume` was right.
#[test]
fn every_volume_reader_agrees_with_the_closed_form() {
    let (s, f) = sphere();
    let truth = 4.0 / 3.0 * PI * R.powi(3);
    let via_set = axia_core::promote::face_set_volume(&s.mesh, &f);
    let via_mesh = s.mesh.mesh_volume().abs();
    assert!((via_set - truth).abs() / truth < 1e-9, "face_set_volume {via_set} vs {truth}");
    assert!((via_mesh - truth).abs() / truth < 1e-9, "mesh_volume {via_mesh} vs {truth}");
}

#[test]
fn area_and_closure_agree_with_the_closed_form() {
    let (s, f) = sphere();
    let truth = 4.0 * PI * R * R;
    assert!((s.mesh.face_area(f[0]) - truth).abs() / truth < 1e-9);
    let mi = s.mesh.face_set_manifold_info(&f);
    assert!(mi.is_closed_solid, "one curved face closes it");
    assert_eq!(mi.boundary_edge_count, 0, "the seam is walked twice");
    let ow = s.mesh.verify_outward_normals();
    assert!(ow.is_closed_solid && ow.inward_count == 0, "outward: {ow:?}");
}

/// A snapshot has to carry the seam and the surface, or a saved sphere comes back
/// as something else. Checked by measuring the restored copy, not by comparing
/// bytes — bytes would pass on two matching wrongs.
#[test]
fn it_survives_a_snapshot_round_trip() {
    let (s, _) = sphere();
    let snap = s.scene_snapshot();
    let mut back = Scene::new();
    back.restore_scene_snapshot(&snap);

    let f: Vec<_> = back.mesh.faces.iter().filter(|(_, x)| x.is_active()).map(|(x, _)| x).collect();
    assert_eq!(f.len(), 1);
    assert!(back.mesh.face_surface(f[0]).is_some(), "the surface came back");
    assert_eq!(back.mesh.verts.iter().filter(|(_, v)| v.is_active()).count(), 2);
    let truth = 4.0 / 3.0 * PI * R.powi(3);
    let v = axia_core::promote::face_set_volume(&back.mesh, &f);
    assert!((v - truth).abs() / truth < 1e-9, "restored volume {v} vs {truth}");
    assert!(back.mesh.verify_face_invariants().is_valid());
}

/// The radius edit moves the poles, and the poles move in OPPOSITE directions —
/// which trips ADR-060's all-or-none rule if done one at a time. That dropped the
/// surface and the update below it skipped in silence.
#[test]
fn a_radius_edit_keeps_the_surface() {
    let (mut s, f) = sphere();
    assert!(s.mesh.set_sphere_radius(f[0], 18.0), "the edit must succeed");
    let surf = s.mesh.face_surface(f[0]).cloned();
    assert!(surf.is_some(), "the surface must survive the pole moves");
    match surf.unwrap() {
        axia_geo::surfaces::AnalyticSurface::Sphere { radius, .. } => {
            assert!((radius - 18.0).abs() < 1e-9, "radius {radius}")
        }
        other => panic!("still a sphere, got {other:?}"),
    }
    for (_, v) in s.mesh.verts.iter().filter(|(_, v)| v.is_active()) {
        assert!((v.pos().z.abs() - 18.0).abs() < 1e-6, "poles moved to ±18: {:?}", v.pos());
    }
    assert!(s.mesh.verify_face_invariants().is_valid());
    let truth = 4.0 / 3.0 * PI * 18f64.powi(3);
    let vol = s.mesh.mesh_volume().abs();
    assert!((vol - truth).abs() / truth < 1e-9, "volume follows the radius: {vol} vs {truth}");
}

/// Boolean has to see it as a curved operand. It asked for a SELF-LOOP boundary,
/// which a seam is not, so subtract declined in silence (6 faces → 6, no cut).
#[test]
fn boolean_still_sees_it_as_a_curved_operand() {
    let (mut s, f) = sphere();
    let box_faces = s.mesh
        .create_box(DVec3::new(R * 0.6, 0.0, 0.0), R, R, R, FORM_MATERIAL)
        .unwrap();
    let before = s.mesh.faces.iter().filter(|(_, x)| x.is_active()).count();
    let r = s.mesh.boolean_solid(
        &box_faces, &f,
        axia_geo::operations::boolean::BoolOp::Subtract,
        FORM_MATERIAL,
    );
    assert!(r.is_ok(), "box − sphere: {:?}", r.err());
    let after = s.mesh.faces.iter().filter(|(_, x)| x.is_active()).count();
    assert_ne!(after, before, "the subtract must actually cut, not decline");
}
