//! A curved face that looks INWARD must contribute its volume inward.
//!
//! `analytic_face_flux`'s Plane arm has always read the sign off the face's own
//! winding — "the face's own winding decides the sign, not the stored surface
//! normal". Its Cylinder and Sphere arms did not, so the first inward-facing
//! curved face the engine ever grew would have had its volume counted the wrong
//! way round.
//!
//! There is no such face today: every curved surface in the engine looks
//! outward, and all five primitives measure true volume. The one that is coming
//! is a bore wall — the wall of a drilled hole faces the void, i.e. radially
//! INWARD — which is what these tests build.
//!
//! Measured on the quad below (200³ box, r = 40 bore, 32 segments) before the
//! fix: polygon −62428.90, surface **+**62831.85. Same magnitude to within the
//! arc-vs-chord gain of 1.0065; opposite sign.

use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::FaceId;
use glam::DVec3;
use std::f64::consts::{PI, TAU};

const R: f64 = 40.0;
const SEGMENTS: u32 = 32;
/// Bore axis: the Z axis. The box spans z ∈ [−100, 100], so the axis origin sits
/// at the entry plane's far side and `v` runs 0..200 along it.
const AXIS_ORIGIN: DVec3 = DVec3::new(0.0, 0.0, -100.0);

/// A bored box whose tube walls are BARE.
///
/// The drill attaches a Cylinder to each tube quad itself now — that is the
/// point of `a_bore_carries_the_cylinder_it_stands_on`. This file is about what
/// happens to a face when a curved surface arrives on it, so it strips them off
/// again and puts one back deliberately, keeping the before/after comparison
/// the tests below are actually making.
fn box_with_a_bore() -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, SEGMENTS)
        .expect("bore");
    for &fid in &res.tube_faces {
        assert!(mesh.set_face_surface(fid, None), "strip");
    }
    (mesh, res.tube_faces)
}

/// The Cylinder patch that one tube quad stands on: its own angular slice and
/// its own axial span, read off its own corners.
fn cylinder_patch_for(mesh: &Mesh, fid: FaceId) -> AnalyticSurface {
    let pts = mesh.face_outline_points(fid).expect("quad outline");
    let mut us: Vec<f64> = pts
        .iter()
        .map(|p| {
            let d = *p - AXIS_ORIGIN;
            let a = d.y.atan2(d.x);
            if a < 0.0 { a + TAU } else { a }
        })
        .collect();
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (mut lo, mut hi) = (us[0], us[us.len() - 1]);
    if hi - lo > PI {
        // the quad straddling the seam: unwrap it rather than claim a near-full turn
        let shifted: Vec<f64> = us.iter().map(|&u| if u > PI { u - TAU } else { u }).collect();
        lo = shifted.iter().cloned().fold(f64::MAX, f64::min);
        hi = shifted.iter().cloned().fold(f64::MIN, f64::max);
    }
    let vs: Vec<f64> = pts.iter().map(|p| (*p - AXIS_ORIGIN).dot(DVec3::Z)).collect();
    AnalyticSurface::Cylinder {
        axis_origin: AXIS_ORIGIN,
        axis_dir: DVec3::Z,
        radius: R,
        ref_dir: DVec3::X,
        u_range: (lo, hi),
        v_range: (
            vs.iter().cloned().fold(f64::MAX, f64::min),
            vs.iter().cloned().fold(f64::MIN, f64::max),
        ),
    }
}

#[test]
fn a_bore_wall_looks_inward() {
    // The premise the rest of the file rests on, stated rather than assumed.
    let (mesh, tube) = box_with_a_bore();
    for &fid in &tube {
        let pts = mesh.face_outline_points(fid).expect("outline");
        let c: DVec3 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let face_n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize();
        let radial_out = {
            let d = c - AXIS_ORIGIN;
            (d - DVec3::Z * d.dot(DVec3::Z)).normalize()
        };
        assert!(
            face_n.dot(radial_out) < -0.99,
            "a bore wall faces the void, i.e. away from the axis: {}",
            face_n.dot(radial_out)
        );
    }
}

#[test]
fn attaching_a_cylinder_to_a_bore_wall_does_not_flip_its_flux() {
    let (mut mesh, tube) = box_with_a_bore();
    let fid = tube[0];
    let from_polygon = mesh.face_outward_flux(fid).expect("polygon flux");
    assert!(from_polygon < 0.0, "the polygon already knows: {from_polygon}");

    let patch = cylinder_patch_for(&mesh, fid);
    assert!(mesh.set_face_surface(fid, Some(patch)), "attach");
    let from_surface = mesh.face_outward_flux(fid).expect("surface flux");

    assert!(
        from_surface < 0.0,
        "the surface must agree with the winding, not with its own outward \
         normal (got {from_surface}, polygon said {from_polygon})"
    );
    // Same patch, read two ways: the only difference should be arc vs chord.
    let gain = from_surface / from_polygon;
    assert!(
        (gain - 1.0065).abs() < 0.002,
        "magnitude should differ only by the arc-vs-chord gain, got {gain}"
    );
}

#[test]
fn a_bored_solid_keeps_its_volume_when_the_bore_gains_a_surface() {
    let (mut mesh, tube) = box_with_a_bore();
    let before = mesh.mesh_volume();
    for &fid in &tube {
        let patch = cylinder_patch_for(&mesh, fid);
        assert!(mesh.set_face_surface(fid, Some(patch)), "attach");
    }
    let after = mesh.mesh_volume();
    let drift = (after - before) / before;
    // Reading the bore as a true cylinder rather than a 32-gon removes a sliver
    // more material, so the solid gets very slightly SMALLER. A flipped sign
    // would instead ADD twice the barrel's share — measured at +18.2%.
    assert!(
        (-0.01..=0.0).contains(&drift),
        "volume moved {:+.3}% ({before:.1} -> {after:.1}); a sign flip shows up \
         here as roughly +18%",
        100.0 * drift
    );
}

#[test]
fn an_inside_out_sphere_encloses_its_volume_the_other_way() {
    // The Sphere arm needs its own inward-facing face, and the engine grows no
    // spherical cavity yet — so turn a sphere inside out. Flipping every face
    // leaves the same surfaces on the same geometry with the opposite winding,
    // which is exactly the situation a cavity would present.
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_sphere(DVec3::ZERO, 50.0, 32, 32, Default::default())
        .expect("sphere");
    let truth = 4.0 / 3.0 * PI * 50f64.powi(3);
    let outward = mesh.mesh_volume();
    assert!((outward / truth - 1.0).abs() < 0.002, "outward {outward:.4}");

    let flipped = mesh.flip_faces(&faces);
    assert_eq!(flipped, faces.len(), "every face should flip");
    let inward = mesh.mesh_volume();
    assert!(
        (inward / -truth - 1.0).abs() < 0.002,
        "an inside-out sphere encloses -V, not +V (got {inward:.4}, want {:.4})",
        -truth
    );
}

#[test]
fn a_face_whose_polygon_says_nothing_is_left_alone() {
    // The decisive-vote guard, pinned. A curved face can carry a polygon that
    // tells you nothing about which way it looks: four points spread around a
    // sphere's equator have a chord plane through the poles and a centroid AT
    // the centre, where there is no radial direction at all. The vote lands at
    // ~0, and an ambiguous face must keep the answer it already had rather than
    // be guessed at — guessing wrong on a sphere flips its whole volume.
    let radius = 50.0;
    let mut mesh = Mesh::new();
    let ring: Vec<_> = (0..4)
        .map(|i| {
            let a = TAU * (i as f64) / 4.0;
            mesh.add_vertex(DVec3::new(radius * a.cos(), radius * a.sin(), 0.0))
        })
        .collect();
    let fid = mesh.add_face(&ring, Default::default()).expect("great-circle face");
    assert!(
        mesh.set_face_surface(
            fid,
            Some(AnalyticSurface::Sphere {
                center: DVec3::ZERO,
                radius,
                axis_dir: DVec3::Z,
                ref_dir: DVec3::X,
                u_range: (0.0, TAU),
                v_range: (0.0, PI / 2.0),
            })
        ),
        "attach"
    );
    let flux = mesh.analytic_face_flux(fid).expect("analytic flux");
    // Unflipped, the northern cap's flux is +r^3 * u_extent = 50^3 * 2pi.
    let unflipped = radius.powi(3) * TAU;
    assert!(
        (flux / unflipped - 1.0).abs() < 1e-9,
        "an ambiguous face must keep its unflipped value: got {flux:.4}, want {unflipped:.4}"
    );
}

#[test]
fn an_outward_curved_face_is_left_alone() {
    // The control. If the sign were applied blindly — or inverted — these would
    // move; they are the faces that were always right.
    let truth = |v: f64, t: f64| {
        assert!(
            (v / t - 1.0).abs() < 0.002,
            "volume {v:.4} is not truth {t:.4} (ratio {:.4})",
            v / t
        );
    };

    let mut mesh = Mesh::new();
    mesh.create_sphere(DVec3::ZERO, 50.0, 32, 32, Default::default()).expect("sphere");
    truth(mesh.mesh_volume(), 4.0 / 3.0 * PI * 50f64.powi(3));

    let mut mesh = Mesh::new();
    mesh.create_cylinder(DVec3::ZERO, R, 100.0, 64, Default::default()).expect("cylinder");
    truth(mesh.mesh_volume(), PI * R * R * 100.0);

    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default()).expect("box");
    truth(mesh.mesh_volume(), 8.0e6);
}
