//! WHERE A POINT LANDS ON A CURVED SURFACE — and why a straight line cannot
//! divide a sphere.
//!
//! The four primitives each already knew how to project a point onto
//! themselves, but only from inside `surfaces::`, having picked the right
//! function and unpacked the right fields. Nothing outside could ask, so a tool
//! that wanted to put a drawn point ON a curved face had no way to.
//!
//! The second half of this file is the reason that door was opened and then not
//! walked through. Measured 2026-08-05, in this order:
//!
//! ```text
//!   seam of 2 points                          -> declined (needs an interior point)
//!   + interior points on the straight CHORD   -> declined (they sit inside the sphere)
//!   + those points PROJECTED onto the sphere  -> STILL declined
//! ```
//!
//! The third line is the finding. The projection of a chord between two points
//! on the equator lands back on the equator — the geodesic between two rim
//! points IS the rim. So a straight line from rim to rim runs along the
//! boundary and separates nothing, and a line whose ends are in the interior
//! never reaches the boundary at all. A two-click straight line cannot divide a
//! hemisphere, whatever it is made of. ADR-284 sends the user to freehand and
//! bezier for this, and that guidance is right.
use axia_geo::curves::AnalyticCurve;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

const R: f64 = 100.0;

fn sphere() -> AnalyticSurface {
    AnalyticSurface::Sphere {
        center: DVec3::ZERO,
        radius: R,
        axis_dir: DVec3::Z,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
    }
}

/// A point on the sphere, by longitude and latitude in degrees.
fn on_sphere(lon: f64, lat: f64) -> DVec3 {
    let (lo, la) = (lon.to_radians(), lat.to_radians());
    DVec3::new(R * lo.cos() * la.cos(), R * lo.sin() * la.cos(), R * la.sin())
}

#[test]
fn a_point_anywhere_lands_on_the_sphere() {
    let s = sphere();
    for p in [
        DVec3::new(10.0, 0.0, 0.0),      // deep inside
        DVec3::new(500.0, 300.0, -20.0), // far outside
        on_sphere(37.0, -12.0),          // already on it
    ] {
        let q = s.project_world_pos(p).expect("a sphere projects");
        assert!((q.length() - R).abs() < 1e-9, "got |q| = {}", q.length());
        // And it is the NEAREST point — same direction from the centre.
        assert!(q.normalize().dot(p.normalize()) > 1.0 - 1e-12);
    }
    assert!(s.project_world_pos(DVec3::ZERO).is_none(), "the centre has no nearest point");
}

#[test]
fn the_other_three_primitives_project_too() {
    let cyl = AnalyticSurface::Cylinder {
        axis_origin: DVec3::ZERO,
        axis_dir: DVec3::Z,
        radius: 50.0,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (0.0, 200.0),
    };
    let q = cyl.project_world_pos(DVec3::new(10.0, 10.0, 80.0)).expect("cylinder");
    assert!((q.truncate().length() - 50.0).abs() < 1e-9, "on the wall: {q:?}");
    assert!((q.z - 80.0).abs() < 1e-9, "the axial position is kept");

    let cone = AnalyticSurface::Cone {
        apex: DVec3::new(0.0, 0.0, 100.0),
        axis_dir: -DVec3::Z,
        half_angle: std::f64::consts::FRAC_PI_4,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (0.0, 100.0),
    };
    let q = cone.project_world_pos(DVec3::new(30.0, 0.0, 50.0)).expect("cone");
    // A 45° cone about -Z from an apex at z=100: radius equals the drop.
    assert!((q.truncate().length() - (100.0 - q.z)).abs() < 1e-6, "on the cone: {q:?}");

    let tor = AnalyticSurface::Torus {
        center: DVec3::ZERO,
        axis_dir: DVec3::Z,
        ref_dir: DVec3::X,
        major_radius: 80.0,
        minor_radius: 20.0,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (0.0, std::f64::consts::TAU),
    };
    let q = tor.project_world_pos(DVec3::new(200.0, 0.0, 0.0)).expect("torus");
    let ring = DVec3::new(q.x, q.y, 0.0).normalize() * 80.0;
    assert!(((q - ring).length() - 20.0).abs() < 1e-9, "on the tube: {q:?}");
}

/// A plane is not a curved target, and the tensor surfaces need an inversion
/// nobody has written — both say so rather than guessing.
#[test]
fn a_plane_and_the_tensor_surfaces_decline() {
    let pl = AnalyticSurface::Plane {
        origin: DVec3::ZERO,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        u_range: (-1.0, 1.0),
        v_range: (-1.0, 1.0),
    };
    assert!(pl.project_world_pos(DVec3::new(1.0, 1.0, 9.0)).is_none());

    let patch = AnalyticSurface::BezierPatch {
        ctrl_grid: vec![
            vec![DVec3::ZERO, DVec3::X],
            vec![DVec3::Y, DVec3::new(1.0, 1.0, 0.0)],
        ],
    };
    assert!(patch.project_world_pos(DVec3::new(0.5, 0.5, 5.0)).is_none());
}

/// THE FINDING. A straight line between two points of a hemisphere's rim runs
/// along the rim, so it divides nothing — even with every point put exactly on
/// the surface first. Pinned here so it is not attempted again.
#[test]
fn a_straight_line_between_rim_points_cannot_divide_a_hemisphere() {
    let s = sphere();
    let (a, b) = (on_sphere(0.0, 0.0), on_sphere(90.0, 0.0));
    for steps in [2usize, 4, 8, 16] {
        for i in 1..steps {
            let chord = a.lerp(b, i as f64 / steps as f64);
            let q = s.project_world_pos(chord).expect("projects");
            assert!((q.length() - R).abs() < 1e-9, "it IS on the sphere");
            assert!(
                q.z.abs() < 1e-9,
                "and still on the equator (z=0) — the geodesic between two rim \
                 points is the rim itself, so there is no interior point to \
                 divide by. got z={}",
                q.z
            );
        }
    }
    // What DOES divide it is a stroke that bulges INTO the hemisphere, which is
    // not a straight line — that is what freehand and bezier give (ADR-284).
    let bulge = on_sphere(45.0, 30.0);
    assert!(bulge.z > 1.0, "an interior point sits off the equator: z={}", bulge.z);
}

/// The projection reaches a face through its surface, which is how a tool gets
/// at it — a face with no surface has nothing to project onto.
#[test]
fn a_face_without_a_surface_has_nothing_to_project_onto() {
    let mut mesh = Mesh::new();
    let anchor = mesh.add_vertex(DVec3::new(R, 0.0, 0.0));
    let disk = mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: R,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("a circle face");
    // Its surface is a Plane, which declines — the caller falls back.
    let su = mesh.face_surface(disk).cloned();
    assert!(su.is_some(), "the fixture only means something if it HAS a surface");
    assert!(su.unwrap().project_world_pos(DVec3::new(1.0, 1.0, 5.0)).is_none());

    mesh.set_face_surface(disk, Some(sphere()));
    let q = mesh
        .face_surface(disk)
        .unwrap()
        .project_world_pos(DVec3::new(1.0, 1.0, 5.0))
        .expect("now it projects");
    assert!((q.length() - R).abs() < 1e-9);
}
