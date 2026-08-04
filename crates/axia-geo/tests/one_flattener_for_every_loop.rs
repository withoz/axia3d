//! ONE PLACE TURNS A LOOP INTO A POLYGON.
//!
//! A closed curve is one self-loop half-edge (ADR-089 Phase 2), so every
//! consumer that wants a polygon has to know to tessellate it. Four of them knew
//! it separately — the render fan, the render hole branch, the self-intersection
//! detector, the carve outline, plus the area measure — and they disagreed on
//! how finely, at three different circle tolerances that nothing put side by
//! side. That is how a ring came to be drawn over its own hole: the fan had no
//! idea the hole branch existed.
//!
//! They now share `Mesh::loop_polygon`, and each states its tolerance as a named
//! policy rather than open-coding the arithmetic. The policies still DIFFER —
//! that is not the bug, and flattening them into one number would be. What they
//! must not do is differ invisibly.
use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::ChordTol;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

fn disk(mesh: &mut Mesh, radius: f64) -> axia_geo::FaceId {
    let anchor = mesh.add_vertex(DVec3::new(radius, 0.0, 0.0));
    mesh.add_face_closed_curve(
        anchor,
        AnalyticCurve::Circle {
            center: DVec3::ZERO,
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        },
        MaterialId::new(0),
    )
    .expect("a circle face")
}

/// The three policies, said out loud.
#[test]
fn each_policy_says_what_it_is() {
    // Measurement: the same however big the shape, however far the camera.
    let measure = ChordTol::fixed(0.1);
    assert_eq!(measure.for_radius(2.0), 0.1);
    assert_eq!(measure.for_radius(2000.0), 0.1);

    // Render: the caller's LOD tolerance, but a small circle still stays round.
    let render = ChordTol::scaled(0.02, 0.002, 1e-6);
    assert_eq!(render.for_radius(1000.0), 0.02, "a big circle takes the base");
    assert_eq!(render.for_radius(5.0), 0.01, "a small one is capped by its radius");
    assert_eq!(render.for_radius(0.0), 1e-6, "and never reaches zero");

    // Cutting: fixed base, radius-capped — geometry may not follow the camera.
    let cut = ChordTol::scaled(0.1, 0.02, 1e-4);
    assert_eq!(cut.for_radius(1000.0), 0.1);
    assert_eq!(cut.for_radius(2.0), 0.04);
    assert_eq!(cut.for_radius(0.0), 1e-4);

    // And they are genuinely different questions — same circle, different answers.
    assert_ne!(measure.for_radius(5.0), render.for_radius(5.0));
    assert_ne!(render.for_radius(5.0), cut.for_radius(5.0));
}

/// A finer tolerance gives more corners, and the corners are on the circle.
#[test]
fn a_circle_flattens_to_its_own_rim() {
    let mut mesh = Mesh::new();
    let f = disk(&mut mesh, 70.0);
    let start = mesh.faces[f].outer().start;

    let coarse = mesh.loop_polygon(start, ChordTol::fixed(1.0)).expect("coarse");
    let fine = mesh.loop_polygon(start, ChordTol::fixed(0.001)).expect("fine");
    assert!(fine.len() > coarse.len(), "{} vs {}", fine.len(), coarse.len());

    for p in &fine {
        let r = (p.x * p.x + p.y * p.y).sqrt();
        assert!((r - 70.0).abs() < 1e-9, "every corner sits on the circle, got r={r}");
        assert!(p.z.abs() < 1e-9);
    }
    // The closing duplicate is dropped — these are the unique corners.
    assert!(
        (fine[0] - fine[fine.len() - 1]).length() > 1e-6,
        "first and last must not be the same point"
    );
}

/// A polygon loop is simply its vertices — no tolerance involved.
#[test]
fn a_polygon_loop_is_its_own_corners() {
    let mut mesh = Mesh::new();
    let vids: Vec<_> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
        DVec3::new(0.0, 10.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    let f = mesh.add_face(&vids, MaterialId::new(0)).unwrap();
    let start = mesh.faces[f].outer().start;
    let a = mesh.loop_polygon(start, ChordTol::fixed(0.1)).unwrap();
    let b = mesh.loop_polygon(start, ChordTol::fixed(100.0)).unwrap();
    assert_eq!(a.len(), 4);
    assert_eq!(a, b, "tolerance cannot move a corner that was given, not sampled");
}

/// An Arc self-loop does not bound a region, and saying so beats returning a
/// polygon nobody should trust. (`add_face_closed_curve` refuses to build one —
/// ADR-089 defers Arc — so this asks the flattener directly.)
#[test]
fn an_open_curve_is_not_a_loop() {
    let mut mesh = Mesh::new();
    let a = mesh.add_vertex(DVec3::new(70.0, 0.0, 0.0));
    let b = mesh.add_vertex(DVec3::new(0.0, 70.0, 0.0));
    let e = mesh
        .add_edge_with_curve(
            a,
            b,
            AnalyticCurve::Arc {
                center: DVec3::ZERO,
                radius: 70.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
                start_angle: 0.0,
                end_angle: std::f64::consts::FRAC_PI_2,
            },
        )
        .expect("an arc edge");
    let he = mesh.edges[e].any_he();
    assert!(
        mesh.loop_polygon(he, ChordTol::fixed(0.1)).is_none(),
        "an arc bounds nothing on its own"
    );
}
