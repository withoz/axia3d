//! A drilled bore IS a cylinder — the mesh should say so.
//!
//! `drill_circular_through_hole` bridged its two hole loops with N quads and
//! left them bare: `face_surface` was `None` on every one of them, so the engine
//! knew the bore only as a chord net. A Path A cylinder's side facets have
//! carried their `Cylinder` since ADR-032 P17; this is the same thing for a
//! hole.
//!
//! It is also the prerequisite for crossing bores: the closed form that turns
//! two crossing cylinders into two ellipses (#103) takes `AnalyticSurface::
//! Cylinder` parameters, and until now the mesh had none to give it.

use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::FaceId;
use glam::DVec3;
use std::f64::consts::TAU;

const R: f64 = 40.0;
const DEPTH: f64 = 200.0;

fn bored(segments: u32) -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, segments)
        .expect("bore");
    (mesh, res.tube_faces)
}

#[test]
fn every_quad_of_a_bore_carries_the_cylinder_it_stands_on() {
    let (mesh, tube) = bored(32);
    assert_eq!(tube.len(), 32);
    for &fid in &tube {
        match mesh.face_surface(fid) {
            Some(AnalyticSurface::Cylinder { radius, axis_dir, v_range, .. }) => {
                assert!((radius - R).abs() < 1e-9, "radius {radius}");
                assert!(axis_dir.cross(DVec3::Z).length() < 1e-9, "axis {axis_dir:?}");
                assert!(
                    (v_range.1 - v_range.0 - DEPTH).abs() < 1e-6,
                    "a tube quad spans the whole bore: {v_range:?}"
                );
            }
            other => panic!("tube quad has no cylinder: {other:?}"),
        }
    }
}

#[test]
fn the_slices_tile_the_whole_turn() {
    // The sharp one. Each quad gets its OWN angular slice, read off its own
    // corners because `bridge_through_loops` decides the quad order. If the
    // seam-straddling quad were left wrapped it would claim nearly a full turn;
    // if a slice were dropped or doubled the total would miss. Exactly 2π says
    // they tile: no gap, no overlap, no seam special case gone wrong.
    for segments in [3u32, 8, 16, 32, 64, 97] {
        let (mesh, tube) = bored(segments);
        let mut total = 0.0;
        for &fid in &tube {
            let Some(AnalyticSurface::Cylinder { u_range, .. }) = mesh.face_surface(fid) else {
                panic!("segments={segments}: a quad without a cylinder");
            };
            let span = u_range.1 - u_range.0;
            assert!(span > 0.0, "segments={segments}: empty slice");
            assert!(
                span < std::f64::consts::PI,
                "segments={segments}: a slice claiming half a turn means the seam \
                 quad stayed wrapped ({span})"
            );
            total += span;
        }
        assert!(
            (total - TAU).abs() < 1e-9,
            "segments={segments}: slices sum to {total}, not 2π"
        );
    }
}

#[test]
fn the_bore_now_reads_as_a_cylinder_rather_than_a_prism() {
    // What the surface buys: the barrel's area is the arc's, not the chords'.
    let segments = 32;
    let (mesh, tube) = bored(segments);
    let barrel: f64 = tube.iter().map(|&f| mesh.face_outer_area(f)).sum();
    let arc = TAU * R * DEPTH;
    let chords = (segments as f64) * (2.0 * R * (std::f64::consts::PI / segments as f64).sin()) * DEPTH;
    assert!(
        (barrel - arc).abs() / arc < 1e-6,
        "barrel reads {barrel:.4}; the arc is {arc:.4} and the chord net {chords:.4}"
    );
}

#[test]
fn a_bore_is_still_a_valid_closed_solid() {
    let (mesh, _) = bored(32);
    let inv = mesh.verify_face_invariants();
    assert!(inv.is_valid(), "invariants: {:?}", inv.violations);
    assert!(mesh.detect_self_intersections().is_clean(), "self-intersections");
    assert!(mesh.verify_outward_normals().is_closed_solid, "closed");
}

#[test]
fn a_rect_bore_is_left_bare() {
    // The control. `bridge_through_loops` is shared with the rect and polygon
    // drills, and neither of those is a cylinder — the surface belongs to the
    // circular drill that knows a radius, not to the tube builder.
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let res = mesh
        .drill_rect_through_hole(
            DVec3::new(-30.0, -30.0, 100.0),
            DVec3::new(30.0, 30.0, 100.0),
            DVec3::Z,
        )
        .expect("rect bore");
    assert_eq!(res.tube_faces.len(), 4);
    for &fid in &res.tube_faces {
        assert!(
            mesh.face_surface(fid).is_none(),
            "a rect tube wall is not a cylinder"
        );
    }
}

#[test]
fn a_crossing_drill_is_still_refused() {
    // The step-1 guard rays against `carve_face_plane`, which rejects a face
    // that is curved AND bounded by a closed curve. A tube quad is now curved
    // but is still bounded by a polygon, so it must stay a legitimate ray
    // target — otherwise the guard would stop seeing the wall it exists to find.
    for segments in [16u32, 32] {
        let (mut mesh, _) = bored(segments);
        let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let crossing =
            mesh.drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, segments);
        assert!(crossing.is_err(), "segments={segments}: a crossing drill must be refused");
        let after = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert_eq!(before, after, "segments={segments}: the mesh was touched anyway");
        assert!(mesh.detect_self_intersections().is_clean(), "segments={segments}");
    }
}
