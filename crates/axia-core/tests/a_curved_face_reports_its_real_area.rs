//! A CURVED FACE'S AREA COMES FROM ITS SURFACE, NOT FROM ITS BOUNDARY POLYGON.
//!
//! `face_outer_area` tried the polygon first and only reached the analytic
//! branch when the boundary was a self-loop. That held exactly as long as a
//! curved face kept its 1-vertex rim. Draw a circle inside a sphere — the disc
//! floats within it, nothing is cut — and the sphere face gains four boundary
//! vertices whose Newell sum is ~0:
//!
//! ```text
//!   sphere r=5, alone            face_area 314.159  = 4πr²  correct
//!   after a circle is drawn      face_area   0.000          ← Inspector: 면적 0.00 m²
//!   analytic_face_area said      314.159                    the whole time
//! ```
//!
//! The Inspector shows that number: `getXiaInfo`'s `surfaceArea` is the sum of
//! `face_area` over the owned faces, and XiaInspector divides it by 1e6 for m².
//! The same wrong reading had already been fixed once for self-loop faces (the
//! comment at that summation says so) — this is the next shape of it, and the
//! same correction `face_outward_flux` (volume) got on the same day.
//!
//! A PLANE keeps the polygon: its `u_range × v_range` is an AABB rectangle,
//! which over-reports a disk (ADR-253 P1). So the rule is curved ⇒ surface,
//! planar ⇒ polygon — not "analytic as a fallback".
use axia_core::{Command, Scene, FORM_MATERIAL};
use glam::DVec3;

const PI: f64 = std::f64::consts::PI;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.set_cylinder_path_b_default(true);
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn sphere_face(s: &Scene) -> axia_geo::FaceId {
    s.mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && matches!(
                    f.surface(),
                    Some(axia_geo::surfaces::AnalyticSurface::Sphere { .. })
                )
        })
        .map(|(f, _)| f)
        .expect("the sphere face")
}

#[test]
fn a_circle_drawn_inside_a_sphere_does_not_erase_its_area() {
    let r: f64 = 5.0;
    let truth = 4.0 * PI * r * r;
    let mut s = prod();
    s.mesh.create_sphere(DVec3::ZERO, r, 16, 12, FORM_MATERIAL).unwrap();
    let before = s.mesh.face_area(sphere_face(&s));
    assert!((before - truth).abs() / truth < 1e-9, "baseline {before} vs {truth}");

    let drawn = s.execute(Command::DrawCircleAsCurve {
        center: DVec3::ZERO,
        normal: DVec3::Y,
        radius: 3.0,
    });
    assert!(!matches!(drawn, axia_core::CommandResult::Error(_)), "{drawn:?}");

    // The trigger: the sphere face now has a polygon boundary.
    let sf = sphere_face(&s);
    let bverts = s
        .mesh
        .collect_loop_verts(s.mesh.faces[sf].outer().start)
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(
        bverts >= 3,
        "the circle gave it a polygon boundary ({bverts} verts) — without that \
         this test would pass on the old reader too"
    );

    let after = s.mesh.face_area(sf);
    assert!(
        (after - truth).abs() / truth < 1e-9,
        "nothing was cut, so the area is unchanged: {after} vs {truth}"
    );
}

/// The controls. A plane must NOT switch to its surface (the AABB rectangle
/// over-reports), and a curved face that was already right must stay right.
#[test]
fn planar_and_path_b_areas_are_unchanged() {
    let mut s = prod();
    s.mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, FORM_MATERIAL)
        .unwrap();
    let total: f64 = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(f, _)| s.mesh.face_area(f))
        .sum();
    assert_eq!(total, 60000.0, "a 100mm box is exactly 6 * 10000");

    let mut s = prod();
    s.mesh.create_cylinder(DVec3::ZERO, 5.0, 10.0, 24, FORM_MATERIAL).unwrap();
    let total: f64 = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(f, _)| s.mesh.face_area(f))
        .sum();
    let truth = 2.0 * PI * 5.0 * 10.0 + 2.0 * PI * 25.0;
    assert!(
        (total - truth).abs() / truth < 1e-9,
        "cylinder side + two caps: {total} vs {truth}"
    );
}
