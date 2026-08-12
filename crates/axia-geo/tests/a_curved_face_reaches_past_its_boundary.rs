//! How big a thing is, when the thing is curved.
//!
//! Area and volume already read a curved face from its SURFACE and a planar one
//! from its POLYGON. The bounding box — the third reader, the one the Inspector
//! quotes as length/width/height — still read the boundary vertices, and a Path B
//! primitive barely has any: a sphere is two faces sharing one equator anchor.
//! Measured 2026-08-11, with area and volume already exact beside them:
//!
//! ```text
//!   sphere   r=10        20 x 0 x 0     should be 20 x 20 x 20
//!   cylinder r=10 h=20   20 x 0 x 0     should be 20 x 20 x 20
//!   cone, torus           0 x 0 x 0
//! ```
//!
//! Sampling the surface fixes the collapse but a uv grid rarely lands ON the
//! extreme — that left a cone at 19.89 and a torus at 49.901. So the extremes
//! are walked out from the best sample. Every candidate is `evaluate`d, so every
//! one lies on the surface: the walk can tighten the box toward the truth and
//! cannot push it past.

use axia_geo::mesh::Mesh;
use glam::DVec3;

/// Extent of a whole primitive: the union of its faces' bounds.
fn extent(mesh: &Mesh) -> (DVec3, DVec3) {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        if let Some((a, b)) = mesh.face_bounds(fid) {
            lo = lo.min(a);
            hi = hi.max(b);
        }
    }
    (lo, hi)
}

fn assert_size(mesh: &Mesh, want: DVec3, what: &str) {
    let (lo, hi) = extent(mesh);
    let got = hi - lo;
    for (axis, g, w) in [("x", got.x, want.x), ("y", got.y, want.y), ("z", got.z, want.z)] {
        assert!(
            (g - w).abs() < 1e-3,
            "{what}: {axis} extent {g}, expected {w} (full box {got:?})",
        );
    }
}

#[test]
fn a_path_b_sphere_is_as_wide_as_it_is_tall() {
    let mut mesh = Mesh::new();
    mesh.create_sphere_kernel_native(DVec3::ZERO, 10.0, Default::default()).expect("sphere");
    assert_size(&mesh, DVec3::splat(20.0), "sphere r=10");
}

#[test]
fn a_path_b_cylinder_fills_its_own_diameter() {
    let mut mesh = Mesh::new();
    mesh.create_cylinder_kernel_native_clean(DVec3::ZERO, 10.0, 20.0, Default::default())
        .expect("cylinder");
    assert_size(&mesh, DVec3::new(20.0, 20.0, 20.0), "cylinder r=10 h=20");
}

#[test]
fn a_path_b_cone_reaches_its_apex_and_its_rim() {
    let mut mesh = Mesh::new();
    mesh.create_cone_kernel_native(DVec3::ZERO, 10.0, 20.0, Default::default())
        .expect("cone");
    assert_size(&mesh, DVec3::new(20.0, 20.0, 20.0), "cone r=10 h=20");
}

#[test]
fn a_path_b_torus_is_wider_than_it_is_thick() {
    let mut mesh = Mesh::new();
    mesh.create_torus_kernel_native(DVec3::ZERO, 20.0, 5.0, Default::default())
        .expect("torus");
    assert_size(&mesh, DVec3::new(50.0, 50.0, 10.0), "torus R=20 r=5");
}

/// The control. A planar face has always been read from its boundary and must
/// still be — this whole change must be invisible to a box.
#[test]
fn a_box_is_unchanged() {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 20.0, 30.0, 40.0, Default::default()).expect("box");
    let (lo, hi) = extent(&mesh);
    // create_box maps w→X, h→Z, d→Y (see CLAUDE.md).
    assert!((hi.x - lo.x - 20.0).abs() < 1e-9);
    assert!((hi.y - lo.y - 40.0).abs() < 1e-9);
    assert!((hi.z - lo.z - 30.0).abs() < 1e-9);
}

/// The safety property the walk rests on: it only ever reports points that are
/// genuinely on the surface, so a box built from them is contained by the true
/// one. Checked where "on the surface" has a closed form.
#[test]
fn every_reported_point_lies_on_the_sphere() {
    let mut mesh = Mesh::new();
    let centre = DVec3::new(3.0, -7.0, 11.0);
    mesh.create_sphere_kernel_native(centre, 10.0, Default::default()).expect("sphere");

    let (lo, hi) = extent(&mesh);
    // Every corner the box touches is reached by a real surface point, so each
    // face of the box is exactly one radius from the centre.
    for (v, expect) in [
        (lo.x, centre.x - 10.0), (hi.x, centre.x + 10.0),
        (lo.y, centre.y - 10.0), (hi.y, centre.y + 10.0),
        (lo.z, centre.z - 10.0), (hi.z, centre.z + 10.0),
    ] {
        assert!((v - expect).abs() < 1e-3, "{v} vs {expect}");
    }
}

/// A hemisphere is not a ball. The walk is clamped to the surface's own
/// parameter range, so a half must report half.
#[test]
fn half_a_sphere_reports_half_its_height() {
    let mut mesh = Mesh::new();
    mesh.create_sphere_kernel_native(DVec3::ZERO, 10.0, Default::default()).expect("sphere");

    // The two hemispheres are separate faces; each one alone spans the full
    // diameter across, but only a radius up or down.
    let mut seen = 0;
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let (lo, hi) = mesh.face_bounds(fid).expect("bounds");
        assert!((hi.x - lo.x - 20.0).abs() < 1e-3, "a hemisphere still spans the equator");
        assert!(
            (hi.z - lo.z - 10.0).abs() < 1e-3,
            "a hemisphere should be one radius tall, got {}",
            hi.z - lo.z,
        );
        seen += 1;
    }
    assert_eq!(seen, 2, "a Path B sphere is two faces");
}

#[test]
fn an_inactive_face_has_no_bounds() {
    let mut mesh = Mesh::new();
    let faces = mesh.create_box(DVec3::ZERO, 20.0, 20.0, 20.0, Default::default()).expect("box");
    let victim = faces[0];
    assert!(mesh.face_bounds(victim).is_some());
    mesh.remove_face(victim).expect("remove");
    assert!(mesh.face_bounds(victim).is_none());
}
