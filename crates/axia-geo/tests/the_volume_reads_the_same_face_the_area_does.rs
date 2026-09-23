//! A PLANAR FACE BRINGS ITS ARCS TO THE VOLUME, NOT JUST TO ITS AREA.
//!
//! On a plane `p·n` is constant, so a face's flux is that times the area its
//! boundary encloses — and the fan in `face_outward_flux` walks CHORDS. An edge
//! carrying an `Arc` bulges past its chord, which the AREA reader has counted
//! since 2026-08-13 (a Ø100 circle cut across a square measures 3,927 per half,
//! not the 2,500 of the triangle its three vertices trace) and the volume reader
//! never did. The two read the same face and disagreed.
//!
//! Measured 2026-09-23 on the case a user reaches — a circle cut by a line with
//! one half pushed (r = 300, h = 120):
//!
//! ```text
//!   the top cap's area       141371.7     πr²/2, arcs and all
//!   what it fluxed with       90000.0     the chord triangle
//!   the solid                0.8789       of πr²h/2
//! ```
//!
//! The fan already scaled itself by `face_area / face_outer_area` to take holes
//! out — the same shape of correction, against the wrong denominator. Scaling by
//! `face_area / chord area` takes out the holes AND brings in the arcs, and is
//! exactly 1.0 for a face whose edges are all straight.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 40.0;
const PI: f64 = std::f64::consts::PI;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// A circle of `n` vertices with an Arc on every edge, at height `z`.
fn arc_circle(m: &mut Mesh, n: usize, z: f64) -> FaceId {
    let center = DVec3::new(0.0, 0.0, z);
    let vs: Vec<_> = (0..n)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / n as f64;
            m.add_vertex(center + DVec3::new(R * t.cos(), R * t.sin(), 0.0))
        })
        .collect();
    let f = m.add_face(&vs, mat()).unwrap();
    m.set_face_surface(
        f,
        Some(AnalyticSurface::Plane {
            origin: center,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-R, R),
            v_range: (-R, R),
        }),
    );
    let edges = m.face_outer_edges(f).unwrap();
    for (i, &e) in edges.iter().enumerate() {
        let a0 = i as f64 * std::f64::consts::TAU / n as f64;
        let a1 = (i + 1) as f64 * std::f64::consts::TAU / n as f64;
        m.edges[e].set_curve(Some(AnalyticCurve::Arc {
            center,
            radius: R,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: a0,
            end_angle: a1,
        }));
    }
    f
}

#[test]
fn a_face_bounded_by_arcs_fluxes_the_area_it_reports() {
    let z = 100.0;
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, z);

    let area = m.face_area(f);
    assert!(
        (area / (PI * R * R) - 1.0).abs() < 1e-9,
        "the area reader already sees the circle: πr² = {:.1}, it read {area:.1}",
        PI * R * R
    );

    let flux = m.face_outward_flux(f).expect("flux");
    assert!(
        (flux.abs() / (z * area) - 1.0).abs() < 1e-9,
        "on a plane the flux is p·n times the face's own area = {:.1}, it read {:.1} \
         (the chord polygon would give {:.1})",
        z * area,
        flux.abs(),
        z * 0.5 * 16.0 * R * R * (std::f64::consts::TAU / 16.0).sin()
    );
}

/// The control, and it has to stay exact to the last bit: every edge of a box is
/// straight, so the correction is 1.0 and the fan is untouched.
#[test]
fn a_box_and_a_bore_are_unchanged() {
    let mut m = Mesh::new();
    m.create_box(DVec3::new(0.0, 0.0, 100.0), 200.0, 200.0, 200.0, mat()).expect("box");
    assert_eq!(m.mesh_volume(), 8_000_000.0, "a 200mm box is exactly 8e6 mm³");

    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, mat()).expect("box");
    m.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, 32).expect("bore");
    let bored = 8_000_000.0 - PI * R * R * 200.0;
    let v = m.mesh_volume();
    assert!((v / bored - 1.0).abs() < 1e-9, "a bored box is {bored:.1}, it read {v:.1}");
}
