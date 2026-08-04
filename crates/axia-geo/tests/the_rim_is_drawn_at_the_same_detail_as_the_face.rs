//! THE RIM AND THE FACE ARE SAMPLED ALIKE, NEAR AND FAR.
//!
//! The face export has taken an LOD tolerance since ADR-135; the wireframe never
//! did. So a circle's rim was sampled at the near-view tolerance however far away
//! it sat. Measured on twenty circles of r=100: pulling the camera back took the
//! faces from 3160 triangles to 1000 while the wireframe stayed at 3160 segments
//! — full price for detail nothing can resolve, and every one of those segments
//! is a quad in the line shader.
//!
//! It also settles what LOCKED #40 §L2 claims. Near the camera the two formulas
//! already agreed exactly (measured: identical). They drifted only once LOD
//! engaged, by up to ~1 mm at r=5000 — which is about a quarter of a pixel, LOD
//! being chosen to keep the chord error right there. So this was never a visible
//! misalignment; it was the wireframe not taking the discount.
use axia_geo::curves::AnalyticCurve;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

const NEAR: f64 = 0.02; // DEFAULT_ANALYTIC_CHORD_TOL
const FAR: f64 = 1.0; // what ADR-135 gives beyond ~5 m

fn disks(n: usize, r: f64) -> Mesh {
    let mut mesh = Mesh::new();
    for i in 0..n {
        let c = DVec3::new(i as f64 * r * 3.0, 0.0, 0.0);
        let a = mesh.add_vertex(c + DVec3::new(r, 0.0, 0.0));
        mesh.add_face_closed_curve(
            a,
            AnalyticCurve::Circle { center: c, radius: r, normal: DVec3::Z, basis_u: DVec3::X },
            MaterialId::new(0),
        )
        .expect("a circle face");
    }
    mesh
}

fn counts(mesh: &mut Mesh, tol: f64) -> (usize, usize) {
    let (_p, _n, idx, _fm, _pf) = mesh.export_buffers_with_tol(tol).expect("faces");
    let (lines, _em) = mesh.export_edge_lines_with_map_tol(20.1, tol);
    (idx.len() / 3, lines.len() / 6)
}

#[test]
fn the_wireframe_takes_the_lod_discount_too() {
    let mut mesh = disks(20, 100.0);
    let (near_tris, near_segs) = counts(&mut mesh, NEAR);
    let (far_tris, far_segs) = counts(&mut mesh, FAR);

    assert!(far_tris < near_tris / 2, "the faces do get coarser: {near_tris} → {far_tris}");
    assert!(
        far_segs < near_segs / 2,
        "and so does the rim — it used to stay at {near_segs}, got {far_segs}"
    );
}

/// Sampled alike, which is the part LOCKED #40 §L2 asks for.
#[test]
fn the_rim_and_the_boundary_use_the_same_detail() {
    let mut mesh = disks(1, 100.0);
    for tol in [NEAR, 0.1, FAR] {
        let (tris, segs) = counts(&mut mesh, tol);
        // A fan of n rim points is n triangles and n segments.
        assert_eq!(tris, segs, "at tol {tol}: {tris} triangles vs {segs} segments");
    }
}

/// The old entry point is what a caller gets when it says nothing, and it stays
/// at the near tolerance — this is a new choice, not a change forced on
/// everyone who never asked.
#[test]
fn asking_for_nothing_still_gets_the_near_detail() {
    let mut mesh = disks(3, 100.0);
    let (plain, _) = mesh.export_edge_lines_with_map(20.1);
    let (near, _) = mesh.export_edge_lines_with_map_tol(20.1, NEAR);
    assert_eq!(plain.len(), near.len());
}

/// A small circle is capped by its own radius, not by the camera, so it stays
/// round however far away it is — the same rule the face has.
#[test]
fn a_small_circle_stays_round_at_any_distance() {
    let mut mesh = disks(1, 5.0);
    let (_, near) = counts(&mut mesh, NEAR);
    let (_, far) = counts(&mut mesh, FAR);
    assert_eq!(near, far, "r=5 is capped at radius×0.002 either way: {near} vs {far}");
    assert!(near > 40, "and that is genuinely round: {near} segments");
}

/// Straight edges have nothing to sample, so the tolerance cannot move them.
#[test]
fn a_straight_wireframe_is_unaffected() {
    let mut mesh = Mesh::new();
    let v: Vec<_> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(100.0, 0.0, 0.0),
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(0.0, 100.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    mesh.add_face(&v, MaterialId::new(0)).unwrap();
    let (near, _) = mesh.export_edge_lines_with_map_tol(20.1, NEAR);
    let (far, _) = mesh.export_edge_lines_with_map_tol(20.1, FAR);
    assert_eq!(near.len(), far.len());
    assert_eq!(near.len() / 6, 4, "four sides, four segments");
}
