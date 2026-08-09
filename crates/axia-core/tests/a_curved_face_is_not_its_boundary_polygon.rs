//! A CURVED FACE'S BOUNDARY POLYGON IS NOT ITS SHAPE.
//!
//! Drawing a circle inside a sphere does not cut the sphere — the disc floats
//! within it — but it does add vertices to the sphere face's boundary. That was
//! enough to collapse the volume, because the flux reader tried the boundary
//! polygon first and four points on a sphere give a Newell sum of ~0:
//!
//! ```text
//!   before the circle   mesh_volume 523.60   (= 4/3·π·5³, correct)
//!   after the circle    mesh_volume   0.00
//!   the surface said    1570.796 (= 4πr³)    the whole time
//! ```
//!
//! So the order is not a fall-back preference, it is about what a boundary
//! MEANS: on a flat face the polygon is exact and cheaper, on a curved one it is
//! a chord net that says nothing about the bulge. Curved ⇒ read the surface.
//!
//! Note what is NOT needed here: the sphere's `v_range` stays the full -π/2..π/2
//! and that is correct — nothing was cut, so the whole sphere is still the
//! surface. The two discs sit on planes through the origin, so their own flux is
//! 0 and the sum comes out exactly right.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

const PI: f64 = std::f64::consts::PI;

fn scene_with_sphere(r: f64) -> Scene {
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s.mesh.create_sphere(DVec3::ZERO, r, 16, 12, FORM_MATERIAL).unwrap();
    s
}

#[test]
fn a_circle_drawn_inside_a_sphere_does_not_erase_its_volume() {
    let r: f64 = 5.0;
    let truth = 4.0 / 3.0 * PI * r.powi(3);
    let mut s = scene_with_sphere(r);
    let before = s.mesh.mesh_volume().abs();
    assert!((before - truth).abs() / truth < 1e-9, "baseline {before} vs {truth}");

    let drawn = s.execute(Command::DrawCircleAsCurve {
        center: DVec3::ZERO, normal: DVec3::Y, radius: 3.0,
    });
    assert!(!matches!(drawn, CommandResult::Error(_)), "{drawn:?}");

    // The sphere face now has a polygon boundary — that is the trigger.
    let sphere_face = s.mesh.faces.iter()
        .find(|(_, f)| f.is_active() && matches!(
            f.surface(), Some(axia_geo::surfaces::AnalyticSurface::Sphere { .. })))
        .map(|(f, _)| f)
        .expect("the sphere face is still there");
    let bverts = s.mesh.collect_loop_verts(s.mesh.faces[sphere_face].outer().start)
        .map(|v| v.len()).unwrap_or(0);
    assert!(bverts >= 3, "the circle gave it a polygon boundary ({bverts} verts) — \
        without that this test would pass on the old reader too");

    let after = s.mesh.mesh_volume().abs();
    assert!((after - truth).abs() / truth < 1e-9,
        "the disc floats inside; the volume is unchanged: {after} vs {truth}");
    let via_set: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let set = axia_core::promote::face_set_volume(&s.mesh, &via_set);
    assert!((set - truth).abs() / truth < 1e-9, "and both readers agree: {set} vs {truth}");
}

/// The control: a FLAT face still goes through its boundary polygon, which is
/// exact for it. If the curved-first rule leaked into planar faces this would
/// drift — the box is asserted bit-exact.
#[test]
fn a_flat_solid_is_unchanged_by_the_rule() {
    let mut s = Scene::new();
    let f = s.mesh.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, FORM_MATERIAL).unwrap();
    assert_eq!(axia_core::promote::face_set_volume(&s.mesh, &f), 1_000_000.0);
    assert_eq!(s.mesh.mesh_volume().abs(), 1_000_000.0);
}
