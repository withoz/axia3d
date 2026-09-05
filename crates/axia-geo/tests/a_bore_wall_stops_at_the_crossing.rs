//! Cutting one bore's wall where another bore passes through it.
//!
//! The first mutation of step 2b(3). A bore's wall runs the whole depth, so a
//! crossing bore leaves a band of it standing INSIDE the other hole — a wall in
//! the middle of a hole, which is the thing the whole track exists to remove
//! (사용자, 2026-08-10: 교차부에 면이 있으면 안 된다).
//!
//! After this, that band is gone and the wall stops exactly on the crossing
//! curve. The solid is deliberately OPEN along that curve: closing it is the
//! other wall's job, and it can only do that because the vertices left here sit
//! on BOTH cylinders.

use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::FaceId;
use glam::DVec3;

const R: f64 = 40.0;

fn bored(segments: u32) -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, segments)
        .expect("bore along Z");
    (mesh, res.tube_faces)
}

/// Distance from the X axis — bore B's axis.
fn to_b_axis(p: DVec3) -> f64 {
    (p.y * p.y + p.z * p.z).sqrt()
}

#[test]
fn nothing_is_left_standing_inside_the_crossing_bore() {
    for segments in [8u32, 16, 32, 64] {
        let (mut mesh, tube) = bored(segments);
        let survivors = mesh
            .trim_bore_wall_for_crossing(&tube, DVec3::ZERO, DVec3::Z, DVec3::X, R, R)
            .expect("trim");

        // Each wall quad spans the bore, so each one is cut in two.
        assert_eq!(
            survivors.len(),
            tube.len() * 2,
            "segments={segments}: every quad crosses the band"
        );

        let closest = survivors
            .iter()
            .flat_map(|&f| mesh.face_outline_points(f).expect("outline"))
            .map(to_b_axis)
            .fold(f64::MAX, f64::min);
        assert!(
            closest >= R - 1e-9,
            "segments={segments}: a wall reaches {closest:.6} from the crossing \
             bore's axis, i.e. {:.6} inside it",
            R - closest
        );
        assert!(
            (closest - R).abs() < 1e-9,
            "segments={segments}: and it must actually REACH the curve, not stop \
             short — closest {closest:.6}"
        );
    }
}

#[test]
fn the_cut_lands_on_both_cylinders() {
    // What makes the seam weldable later: `add_vertex` dedups by position, so
    // the other wall shares these vertices without being told to — but only if
    // they are genuinely on its cylinder too.
    let (mut mesh, tube) = bored(32);
    let survivors = mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::ZERO, DVec3::Z, DVec3::X, R, R)
        .expect("trim");

    let mut cuts = 0;
    for &f in &survivors {
        for p in mesh.face_outline_points(f).expect("outline") {
            if p.z.abs() > 99.0 {
                continue; // an original end corner, on the box's cap
            }
            cuts += 1;
            let on_a = ((p.x * p.x + p.y * p.y).sqrt() - R).abs();
            let on_b = (to_b_axis(p) - R).abs();
            assert!(on_a < 1e-9 && on_b < 1e-9, "cut {p:?}: off A {on_a:.2e}, off B {on_b:.2e}");
        }
    }
    assert!(cuts >= 64, "the cut should leave two curves' worth of points, saw {cuts}");
}

#[test]
fn the_wall_keeps_its_winding_its_surface_and_its_soundness() {
    let (mut mesh, tube) = bored(32);
    let survivors = mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::ZERO, DVec3::Z, DVec3::X, R, R)
        .expect("trim");

    for &f in &survivors {
        assert!(
            matches!(mesh.face_surface(f), Some(AnalyticSurface::Cylinder { .. })),
            "a piece of a bore wall is still on the bore's cylinder"
        );
        // A bore wall faces the void: its normal points at the axis, not away.
        let pts = mesh.face_outline_points(f).expect("outline");
        let c: DVec3 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize();
        let radial_out = DVec3::new(c.x, c.y, 0.0).normalize();
        assert!(n.dot(radial_out) < -0.9, "winding flipped: {:.3}", n.dot(radial_out));
    }

    let inv = mesh.verify_face_invariants();
    assert!(inv.is_valid(), "invariants: {:?}", inv.violations);
    assert!(mesh.detect_self_intersections().is_clean(), "self-intersections");
    // Open on purpose — the crossing bore's own wall is what closes it.
    assert!(
        !mesh.verify_outward_normals().is_closed_solid,
        "the solid is open along the crossing curve until the other wall arrives"
    );
}

#[test]
fn a_bore_that_does_not_cross_is_left_alone() {
    // The control: a crossing bore that passes 300 mm above the solid, so its
    // band never reaches this wall's axial range. Every quad must come back
    // untouched, by identity.
    let (mut mesh, tube) = bored(32);
    let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let survivors = mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::new(0.0, 0.0, 300.0), DVec3::Z, DVec3::X, R, R)
        .expect("trim");
    let after = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(before, after, "no face should have been rebuilt");
    assert_eq!(survivors, tube, "the same faces, in the same order");
    assert!(mesh.verify_outward_normals().is_closed_solid, "and still a closed solid");
}

#[test]
fn a_meeting_point_off_the_walls_axis_is_refused() {
    // ⚠ This was written as a "does not cross" control first, and it is not
    // one: moving the meeting point sideways does not move the crossing bore,
    // it moves the frame every radial direction is measured in. The band then
    // comes out plausible and wrong, and the wall gets cut in the wrong place.
    // The wall itself settles it — every corner of a bore wall stands at exactly
    // its radius from its own axis.
    let (mut mesh, tube) = bored(32);
    let err = mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::new(0.0, 200.0, 0.0), DVec3::Z, DVec3::X, R, R)
        .expect_err("an off-axis meeting point must be refused, not guessed at");
    assert!(
        err.to_string().contains("not on this wall's axis"),
        "and it should say why: {err}"
    );
}

#[test]
fn it_refuses_what_it_cannot_cut() {
    let (mut mesh, tube) = bored(32);
    assert!(mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::ZERO, DVec3::ZERO, DVec3::X, R, R)
        .is_err());
    assert!(mesh
        .trim_bore_wall_for_crossing(&tube, DVec3::ZERO, DVec3::Z, DVec3::X, 0.0, R)
        .is_err());
    // A cap is not a tube wall — four corners, but it does not span the bore.
    let cap = mesh
        .faces
        .iter()
        .find(|(fid, f)| {
            f.is_active()
                && !f.inners().is_empty()
                && mesh.face_surface(*fid).is_none()
        })
        .map(|(fid, _)| fid);
    if let Some(cap) = cap {
        assert!(mesh
            .trim_bore_wall_for_crossing(&[cap], DVec3::ZERO, DVec3::Z, DVec3::X, R, R)
            .is_err());
    }
}
