//! Two bores that cross, with nothing standing in between.
//!
//! 사용자, 2026-08-10: 교차부에 면이 있으면 안 된다. The ordinary drill refuses
//! this because its straight tube cannot bridge the void the first bore left.
//! `drill_crossing_bore` builds that tube anyway and then cuts BOTH walls back
//! to the curve where they cross, so neither is left standing inside the other
//! and the two cuts weld to each other.

use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use glam::DVec3;
use std::f64::consts::PI;

const R: f64 = 40.0;

/// ⚠ Self-intersection must be asked with the surfaces OFF.
///
/// `detect_self_intersections` deliberately skips any pair involving a curved
/// analytic face (ADR-271: its flat-chord tessellation bows inside the true
/// surface, so overlaps there are artifacts). Since #105 a bore's wall carries
/// its `Cylinder`, so asking it directly about two bores returns 0 whether or
/// not they run through each other — measured: the same crossing reads si 0 with
/// the surfaces on and **184** with them off.
fn self_intersections_of(mesh: &Mesh) -> usize {
    let mut bare = mesh.clone();
    let ids: Vec<_> = bare
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    for fid in ids {
        bare.set_face_surface(fid, None);
    }
    bare.detect_self_intersections().count()
}

fn box_bored_along_z(segments: u32) -> Mesh {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    mesh.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, segments)
        .expect("first bore");
    mesh
}

#[test]
fn the_two_bores_open_into_each_other() {
    for segments in [8u32, 16, 32] {
        let mut mesh = box_bored_along_z(segments);
        let kept = mesh
            .drill_crossing_bore(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, segments)
            .unwrap_or_else(|e| panic!("segments={segments}: {e}"));

        // Four walls now — each bore's, cut in two.
        assert_eq!(kept.len(), (segments as usize) * 4, "segments={segments}");
        for &f in &kept {
            assert!(
                matches!(mesh.face_surface(f), Some(AnalyticSurface::Cylinder { .. })),
                "segments={segments}: a wall lost its cylinder"
            );
        }

        // Nothing left standing in the crossing: every wall point is outside
        // BOTH bores (on their surface at worst, never within).
        for &f in &kept {
            for p in mesh.face_outline_points(f).expect("outline") {
                let from_z = (p.x * p.x + p.y * p.y).sqrt();
                let from_x = (p.y * p.y + p.z * p.z).sqrt();
                assert!(
                    from_z >= R - 1e-9 && from_x >= R - 1e-9,
                    "segments={segments}: a wall stands inside a bore at {p:?}"
                );
            }
        }

        let inv = mesh.verify_face_invariants();
        assert!(inv.is_valid(), "segments={segments}: {:?}", inv.violations);
        assert!(
            mesh.verify_outward_normals().is_closed_solid,
            "segments={segments}: the void must be closed again"
        );
        assert_eq!(
            self_intersections_of(&mesh),
            0,
            "segments={segments}: the two walls still run through each other"
        );
    }
}

#[test]
fn the_crossing_region_is_removed_once_not_twice() {
    // The guard that catches a wall claiming more surface than it kept. Two
    // crossing cylinders take out their UNION — two cylinders minus the
    // Steinmetz solid they share — and a piece that kept the whole barrel's
    // `v_range` gets counted as the whole barrel, removing the crossing twice.
    //
    // Measured before the narrowing: 3,351,032 removed where the union is
    // 1,669,286, and the answer stopped depending on the segment count at all —
    // which is the tell, since a chord approximation must.
    let ideal_removed = 2.0 * PI * R * R * 200.0 - (16.0 / 3.0) * R.powi(3);
    let mut previous_error = f64::MAX;
    for segments in [8u32, 16, 32] {
        let mut mesh = box_bored_along_z(segments);
        mesh.drill_crossing_bore(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, segments)
            .expect("crossing bore");
        let removed = 8.0e6 - mesh.mesh_volume();
        let error = (removed - ideal_removed).abs();
        assert!(
            error / ideal_removed < 0.12,
            "segments={segments}: removed {removed:.1} against a union of {ideal_removed:.1}"
        );
        assert!(
            error < previous_error,
            "segments={segments}: a chord approximation must get CLOSER as it \
             refines — error {error:.1} did not improve on {previous_error:.1}"
        );
        previous_error = error;
    }
}

#[test]
fn it_refuses_what_it_cannot_sew_without_touching_the_mesh() {
    // Every refusal happens before anything is cut, so the caller can fall back
    // to the ordinary refusal or to Boolean with the solid intact.
    let cases: Vec<(&str, Box<dyn Fn() -> Mesh>, DVec3, f64, u32)> = vec![
        (
            "nothing to cross",
            Box::new(|| {
                let mut m = Mesh::new();
                m.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default()).unwrap();
                m
            }),
            DVec3::X,
            R,
            32,
        ),
        (
            "unequal radii",
            Box::new(|| box_bored_along_z(32)),
            DVec3::X,
            25.0,
            32,
        ),
        (
            "a segment count whose stations miss the curve",
            Box::new(|| box_bored_along_z(30)),
            DVec3::X,
            R,
            30,
        ),
    ];
    for (name, build, axis, radius, segments) in cases {
        let mut mesh = build();
        let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let volume = mesh.mesh_volume();
        let err = mesh
            .drill_crossing_bore(axis * 100.0, axis, radius, segments)
            .expect_err(&format!("{name}: must be refused"));
        assert!(err.to_string().contains("crossing bore"), "{name}: {err}");
        assert_eq!(
            before,
            mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
            "{name}: the mesh was touched anyway"
        );
        assert!((mesh.mesh_volume() - volume).abs() < 1e-6, "{name}: volume moved");
    }
}

#[test]
fn the_ordinary_drill_still_refuses_a_crossing() {
    // The explicit op is the only way in; the plain drill keeps its refusal,
    // because a straight tube still cannot bridge a void.
    let mut mesh = box_bored_along_z(32);
    let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert!(mesh
        .drill_circular_through_hole(DVec3::new(100.0, 0.0, 0.0), DVec3::X, R, 32)
        .is_err());
    assert_eq!(before, mesh.faces.iter().filter(|(_, f)| f.is_active()).count());
}
