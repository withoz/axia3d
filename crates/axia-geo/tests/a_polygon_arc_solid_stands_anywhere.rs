//! A SOLID BUILT FROM A POLYGON-ARC CIRCLE MEASURES THE SAME WHEREVER IT STANDS.
//!
//! ADR-183 turns the cap on the back of an extrusion outward: `extrude_planar_box`
//! does it, `extrude_planar_box_tapered` does it, `extrude_planar_mixed` does it.
//! `extrude_planar_cylinder`'s ≥ 3-vertex path never did, and nor did the polygonal
//! paths of `extrude_planar_cone` — so their back cap looked INTO the solid.
//!
//! Volume is flux over three and a cap's flux is `(centroid·n)·A`, so a cap that
//! looks in reads `+z·A` where it should read `−z·A` and the solid measures
//! `2·A·z / 3` too much. That is exactly nothing on z = 0, which is where every
//! test of it stood. Measured 2026-09-23, a 16-arc circle of r = 40 extruded 300,
//! as a fraction of its πr²h:
//!
//! ```text
//!                       up 300     down 300
//!   standing at z = 0    1.0000       0.3333
//!   standing at z = 100  1.2222       0.5556
//!   standing at z = −150 0.6667       0.0000
//! ```
//!
//! The last one is a cylinder that measures nothing at all: extruded down from
//! z = −150 its two caps sit at −150 and −450, both looking up, and their fluxes
//! cancel the band exactly. A user reads that as a weight of zero.
//!
//! These are the paths a legacy circle reaches: a circle that anything has split
//! carries an Arc per edge, and Path A — the tessellate-then-extrude route — lands
//! on the same builder.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::CreateSolidMode;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 40.0;
const H: f64 = 300.0;
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

/// The closed-curve disk a circle tool leaves, for the legacy Path A route.
fn disk(m: &mut Mesh, z: f64) -> FaceId {
    let c = DVec3::new(0.0, 0.0, z);
    let a = m.add_vertex(c + DVec3::X * R);
    m.add_face_closed_curve(
        a,
        AnalyticCurve::Circle { center: c, radius: R, normal: DVec3::Z, basis_u: DVec3::X },
        mat(),
    )
    .unwrap()
}

#[test]
fn a_polygon_arc_extrusion_measures_its_cylinder_wherever_it_stands() {
    let truth = PI * R * R * H;
    let mut wrong = Vec::new();
    for dist in [H, -H] {
        for z in [0.0, 100.0, -150.0] {
            let mut m = Mesh::new();
            let f = arc_circle(&mut m, 16, z);
            m.create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat()).expect("extrude");
            let v = m.mesh_volume();
            if (v / truth - 1.0).abs() > 1e-9 {
                wrong.push(format!("16-arc circle {dist:+}, base z = {z}: {:.4}", v / truth));
            }

            // Path A (`cylinder_path_b_default = false`) tessellates the circle
            // and recurses into the same builder.
            let mut m = Mesh::new();
            m.set_cylinder_path_b_default(false);
            let f = disk(&mut m, z);
            m.create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat()).expect("extrude");
            let v = m.mesh_volume();
            if (v / truth - 1.0).abs() > 1e-9 {
                wrong.push(format!("Path A disk {dist:+}, base z = {z}: {:.4}", v / truth));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "πr²h = {truth:.1}; these read otherwise (fraction of it):\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn each_cap_of_a_polygon_arc_extrusion_looks_away_from_the_solid() {
    for dist in [H, -H] {
        let mut m = Mesh::new();
        let f = arc_circle(&mut m, 16, 0.0);
        let r = m
            .create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat())
            .expect("extrude");
        let (bottom, top) = if dist > 0.0 {
            (r.profile_face, r.top_face)
        } else {
            (r.top_face, r.profile_face)
        };
        for (label, cap, out) in [("bottom", bottom, DVec3::NEG_Z), ("top", top, DVec3::Z)] {
            let looks = m.faces[cap].normal();
            assert!(
                looks.dot(out) > 0.999,
                "{dist:+}: the {label} cap looks {looks:?}, it should look {out:?}"
            );
            match m.face_surface(cap) {
                Some(AnalyticSurface::Plane { normal, .. }) => assert!(
                    normal.dot(out) > 0.999,
                    "{dist:+}: the {label} cap looks {looks:?} but its Plane says {normal:?}"
                ),
                other => panic!("{dist:+}: the {label} cap carries {other:?}, not a Plane"),
            }
        }
    }
}

/// The same turn on the polygonal cone paths. An apex cone has no top cap, so its
/// base is the back cap only when the apex stands above it.
#[test]
fn each_cap_of_a_polygonal_cone_looks_away_from_the_solid() {
    for dist in [H, -H] {
        // Apex: one cap, which is the bottom when the apex is above it.
        let mut m = Mesh::new();
        let f = arc_circle(&mut m, 16, 0.0);
        let r = m
            .create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale: 0.0 }, mat())
            .expect("apex cone");
        let out = if dist > 0.0 { DVec3::NEG_Z } else { DVec3::Z };
        let looks = m.faces[r.profile_face].normal();
        assert!(
            looks.dot(out) > 0.999,
            "apex {dist:+}: the base cap looks {looks:?}, it should look {out:?}"
        );

        // Frustum: two real caps.
        let mut m = Mesh::new();
        let f = arc_circle(&mut m, 16, 0.0);
        let r = m
            .create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale: 0.5 }, mat())
            .expect("frustum");
        let (lower, upper) = if dist > 0.0 {
            (r.profile_face, r.top_face)
        } else {
            (r.top_face, r.profile_face)
        };
        for (label, cap, out) in [("lower", lower, DVec3::NEG_Z), ("upper", upper, DVec3::Z)] {
            let looks = m.faces[cap].normal();
            assert!(
                looks.dot(out) > 0.999,
                "frustum {dist:+}: the {label} cap looks {looks:?}, it should look {out:?}"
            );
        }
    }
}

/// Left as it is, and pinned so it is not a surprise: turning the caps outward
/// takes most of the cone's placement dependence away but not all of it, and for
/// two different reasons.
///
/// An apex cone's fan triangles have three corners each, so every one of them
/// still carries the WHOLE cone (`a_cone_fan_triangle_still_counts_the_whole_cone`),
/// and a flux read sixteen times over does not stay put when the solid moves:
/// measured 2026-09-23, +300 standing at z = 100 reads 1.3125 of its z = 0 reading
/// where it read 1.3542 before the caps were turned.
///
/// A frustum's quads do read their own wedge, and what is left is its top rim:
/// the base carries the circle's arcs and the scaled top rim carries none, so the
/// band is read as a cone while the cap it closes is read as a 16-gon. That
/// mismatch is ~0.2% of the solid and moves with height — 0.9987 at z = 100.
#[test]
fn a_polygonal_cone_still_moves_a_little_with_its_height() {
    let mut at = |top_scale: f64, dist: f64, z: f64| {
        let mut m = Mesh::new();
        let f = arc_circle(&mut m, 16, z);
        m.create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale }, mat())
            .expect("cone");
        m.mesh_volume()
    };
    let apex = at(0.0, H, 100.0) / at(0.0, H, 0.0);
    assert!(
        apex > 1.2,
        "an apex cone's fan still counts the whole cone sixteen times, so it still          moves with height; it read {apex:.4} — if this is near 1.0 the fan has learned          about triangles and this pin should become a guard"
    );
    let frustum = at(0.5, H, 100.0) / at(0.5, H, 0.0);
    assert!(
        (frustum - 1.0).abs() > 1e-4 && (frustum - 1.0).abs() < 5e-3,
        "a frustum's top rim carries no arcs, so its cap and its band disagree by ~0.2%          and that disagreement moves with height; it read {frustum:.4} — if this is exact          the rim has its arcs and this pin should become a guard"
    );
}
