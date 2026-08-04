//! WHAT THE OVERLAP DETECTOR CAN SEE ALONG A CURVED BOUNDARY.
//!
//! It reads a face's boundary as a polygon. A closed curve has no vertices to
//! read, so it was always sampled — but an ARC along a POLYGON edge does have
//! endpoints, and the detector took the straight chord between them.
//!
//! Measured 2026-08-04, which is the only reason the size is known: a quarter
//! arc of r=100 bows **29 mm** past its chord, and a 6 mm face sitting in that
//! crescent — inside the real face, outside its chord — was invisible. That is
//! not a rounding difference. The gate exists to see faces lying on top of each
//! other, and there was a 29 mm strip along every arc where it could not.
//!
//! Following arcs is a POLICY, not a refinement, and it is off elsewhere:
//! switching it on for the carve outline was measured to stop the cross-drilling
//! guard rejecting a drill whose axis crosses an existing hole. So the detector
//! asks for it by name and nobody else inherits it.
use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::ChordTol;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

const R: f64 = 100.0;

/// A quarter sector: two straight edges and one arc.
fn sector(mesh: &mut Mesh) -> axia_geo::FaceId {
    let c = mesh.add_vertex(DVec3::ZERO);
    let a = mesh.add_vertex(DVec3::new(R, 0.0, 0.0));
    let b = mesh.add_vertex(DVec3::new(0.0, R, 0.0));
    mesh.add_edge_with_curve(
        a,
        b,
        AnalyticCurve::Arc {
            center: DVec3::ZERO,
            radius: R,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::FRAC_PI_2,
        },
    )
    .expect("an arc edge");
    mesh.add_face(&[c, a, b], MaterialId::new(0)).expect("a sector")
}

/// How far the true rim bulges past its chord.
fn sagitta() -> f64 {
    R * (1.0 - std::f64::consts::FRAC_PI_4.cos())
}

/// A small square sitting in the crescent — inside the real face, outside the
/// chord that used to stand in for it.
fn blob_in_the_crescent(mesh: &mut Mesh, half: f64) -> axia_geo::FaceId {
    let on_rim = DVec3::new(R * 0.7071, R * 0.7071, 0.0);
    let p = on_rim - on_rim.normalize() * (sagitta() * 0.5);
    let v: Vec<_> = [
        p + DVec3::new(-half, -half, 0.0),
        p + DVec3::new(half, -half, 0.0),
        p + DVec3::new(half, half, 0.0),
        p + DVec3::new(-half, half, 0.0),
    ]
    .iter()
    .map(|&q| mesh.add_vertex(q))
    .collect();
    mesh.add_face(&v, MaterialId::new(0)).expect("a blob")
}

#[test]
fn a_face_lying_in_the_crescent_of_an_arc_is_seen() {
    assert!(sagitta() > 29.0, "the fixture only means something if the bulge is big: {}", sagitta());
    let mut mesh = Mesh::new();
    let s = sector(&mut mesh);
    let b = blob_in_the_crescent(&mut mesh, 3.0);
    let pairs = mesh.detect_self_intersections().intersecting_pairs;
    assert!(
        pairs.iter().any(|&(x, y)| (x, y) == (s, b) || (x, y) == (b, s)),
        "the sector and the blob overlap and must be reported: {pairs:?}"
    );
}

/// And the boundary the detector reads really does reach the rim.
#[test]
fn the_sampled_boundary_reaches_the_rim() {
    let mut mesh = Mesh::new();
    let s = sector(&mut mesh);
    let start = mesh.faces[s].outer().start;

    let chord = mesh.loop_polygon(start, ChordTol::fixed(0.02)).expect("chord form");
    assert_eq!(chord.len(), 3, "cutting across, a sector is a triangle");

    let rim = mesh
        .loop_polygon(start, ChordTol::fixed(0.02).following_arcs())
        .expect("rim form");
    assert!(rim.len() > 20, "following the arc takes many more points: {}", rim.len());
    // Every added point sits on the circle, and the farthest one is a full
    // sagitta beyond where the chord ran.
    let farthest = rim
        .iter()
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(0.0_f64, f64::max);
    assert!((farthest - R).abs() < 0.05, "the rim is the circle: {farthest}");
}

/// Off by default, because switching it on changes what other readers see —
/// measured, it stops the cross-drilling guard rejecting.
#[test]
fn following_arcs_is_asked_for_not_assumed() {
    assert!(!ChordTol::fixed(0.02).follow_arc_edges);
    assert!(!ChordTol::scaled(0.1, 0.02, 1e-4).follow_arc_edges);
    assert!(ChordTol::fixed(0.02).following_arcs().follow_arc_edges);
    // And it is orthogonal to how finely a curve is sampled.
    let t = ChordTol::scaled(0.02, 0.002, 1e-6).following_arcs();
    assert_eq!(t.for_radius(5.0), 0.01);
}

/// A boundary with no arc on it reads the same either way, so turning the
/// policy on costs nothing where there is nothing to follow.
#[test]
fn a_straight_boundary_is_unchanged_by_the_policy() {
    let mut mesh = Mesh::new();
    let v: Vec<_> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(0.0, 10.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    let f = mesh.add_face(&v, MaterialId::new(0)).unwrap();
    let start = mesh.faces[f].outer().start;
    let plain = mesh.loop_polygon(start, ChordTol::fixed(0.02)).unwrap();
    let arcs = mesh.loop_polygon(start, ChordTol::fixed(0.02).following_arcs()).unwrap();
    assert_eq!(plain, arcs);
}

/// The closed-curve case was never the problem — it has no vertices to read
/// instead, so it was always sampled. Kept as the control.
#[test]
fn a_closed_curve_was_already_sampled() {
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
        .unwrap();
    let start = mesh.faces[disk].outer().start;
    let plain = mesh.loop_polygon(start, ChordTol::fixed(0.02)).unwrap();
    let arcs = mesh.loop_polygon(start, ChordTol::fixed(0.02).following_arcs()).unwrap();
    assert_eq!(plain, arcs, "the policy has nothing to add to a closed curve");
    assert!(plain.len() > 50);
}
