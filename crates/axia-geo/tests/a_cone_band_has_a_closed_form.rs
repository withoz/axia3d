//! A CONE BAND IS INTEGRATED, NOT SAMPLED.
//!
//! `analytic_face_flux` has closed forms for Plane, Sphere and Cylinder and had
//! none for Cone, so every cone patch fell back to its tessellation, whose chords
//! lie inside the surface. That under-read was the last named cause in this
//! family, and it had two symptoms wherever a cone appeared: the solid read a
//! little under its volume, and — because the caps are exact and the band is not —
//! the reading drifted with the height the solid stood at.
//!
//! Measured 2026-09-27, before this:
//!
//! ```text
//!   16-arc frustum (s = 0.5)   0.999728 of πh(R² + Rr + r²)/3, spread 1.1e-4
//!   16-arc apex cone           0.9997   of πr²h/3,             spread 1.6e-4
//!   the band alone             0.99968  of πh(R² + Rr)
//! ```
//!
//! The form. With `P(u,v) = apex + axis·v + radial(u)·v·tanα` and
//! `n̂(u) = radial(u)·cosα − axis·sinα` (cone.rs), the v terms of `P·n̂` cancel:
//!
//! ```text
//!   P·n̂ = apex·n̂(u),          dA = v·tanα·secα du dv
//!   ∬ = tanα·secα · (v₁² − v₀²)/2
//!       · [ cosα·(aᵣ·(sin u₁ − sin u₀) + aₚ·(cos u₀ − cos u₁)) − sinα·aₐ·(u₁ − u₀) ]
//! ```
//!
//! where (aᵣ, aₚ, aₐ) is the apex in the surface's own basis. On a whole cone
//! standing on z = 0 that comes to πr²h, which is 3V — the base cap contributes
//! nothing there, so the identity is a check on the form and not on the code.
//!
//! The second instrument in this file is a numeric integration of the same patch
//! through `SurfaceOps`, which knows nothing about the derivation.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::CreateSolidMode;
use axia_geo::surfaces::{AnalyticSurface, SurfaceOps};
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 40.0;
const H: f64 = 300.0;
const N: usize = 16;
const PI: f64 = std::f64::consts::PI;
const TAU: f64 = std::f64::consts::TAU;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// A circle whose every edge carries its arc — the shape a circle takes once
/// anything has split it, and what the legacy builders work on.
fn arc_circle(m: &mut Mesh, n: usize, z: f64) -> FaceId {
    let center = DVec3::new(0.0, 0.0, z);
    let vs: Vec<_> = (0..n)
        .map(|i| {
            let t = i as f64 * TAU / n as f64;
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
        m.edges[e].set_curve(Some(AnalyticCurve::Arc {
            center,
            radius: R,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: i as f64 * TAU / n as f64,
            end_angle: (i + 1) as f64 * TAU / n as f64,
        }));
    }
    f
}

fn coned(z: f64, dist: f64, top_scale: f64) -> (Mesh, Vec<FaceId>) {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, N, z);
    let r = m
        .create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale }, mat())
        .expect("cone");
    (m, r.side_faces)
}

/// The derivation, checked against an instrument that does not share it: sample
/// the patch on a fine grid, and integrate `(P·n̂) dA` the way the definition says.
#[test]
fn the_closed_form_agrees_with_the_integral_it_stands_for() {
    let (m, sides) = coned(0.0, H, 0.0);
    let face = sides[0];
    let Some(surface @ AnalyticSurface::Cone { u_range, v_range, .. }) = m.face_surface(face)
    else {
        panic!("a fan triangle carries a Cone surface");
    };
    let (u0, u1) = *u_range;
    let (v0, v1) = *v_range;

    let steps = 400;
    let du = (u1 - u0) / steps as f64;
    let dv = (v1 - v0) / steps as f64;
    let mut numeric = 0.0;
    for i in 0..steps {
        let u = u0 + (i as f64 + 0.5) * du;
        for j in 0..steps {
            let v = v0 + (j as f64 + 0.5) * dv;
            let p = surface.evaluate(u, v);
            let n = surface.normal(u, v);
            // |∂P/∂u × ∂P/∂v| for a cone: v·tanα·secα.
            let e_u = surface.derivative_u(u, v);
            let e_v = surface.derivative_v(u, v);
            let da = e_u.cross(e_v).length() * du * dv;
            numeric += p.dot(n) * da;
        }
    }

    let closed = m.face_outward_flux(face).expect("flux");
    assert!(
        (closed / numeric - 1.0).abs() < 1e-4,
        "the closed form reads {closed:.3} where integrating the same patch gives \
         {numeric:.3} ({:.6} of it)",
        closed / numeric
    );
}

/// An apex cone measures its cone, to the last digit, wherever it stands and
/// whichever way it goes. It used to read 0.9997 and drift 1.6e-4.
#[test]
fn an_apex_cone_measures_its_cone_exactly() {
    let truth = PI * R * R * H / 3.0;
    let mut wrong = Vec::new();
    for dist in [H, -H] {
        for z in [0.0, 100.0, -150.0] {
            let v = coned(z, dist, 0.0).0.mesh_volume();
            if (v / truth - 1.0).abs() > 1e-9 {
                wrong.push(format!("{dist:+}, base z = {z}: {:.9}", v / truth));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "πr²h/3 = {truth:.1}; these read otherwise (fraction of it):\n  {}",
        wrong.join("\n  ")
    );
}

/// And so does a frustum, whose top rim carries the scaled arcs. It used to read
/// 0.999728 and drift 1.1e-4.
#[test]
fn a_frustum_measures_its_frustum_exactly() {
    const S: f64 = 0.5;
    let r_top = R * S;
    let truth = PI * H * (R * R + R * r_top + r_top * r_top) / 3.0;
    let mut wrong = Vec::new();
    for dist in [H, -H] {
        for z in [0.0, 100.0, -150.0] {
            let v = coned(z, dist, S).0.mesh_volume();
            if (v / truth - 1.0).abs() > 1e-9 {
                wrong.push(format!("{dist:+}, base z = {z}: {:.9}", v / truth));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "πh(R² + Rr + r²)/3 = {truth:.1}; these read otherwise (fraction of it):\n  {}",
        wrong.join("\n  ")
    );
}

/// The band divides exactly, not nearly: sixteen sectors, and each one's share.
#[test]
fn the_band_divides_exactly_between_its_sectors() {
    let lateral = PI * R * R * H; // = 3V for a cone standing on z = 0
    let (m, sides) = coned(0.0, H, 0.0);
    let each = lateral / N as f64;
    for (i, &s) in sides.iter().enumerate() {
        let flux = m.face_outward_flux(s).expect("flux");
        assert!(
            (flux / each - 1.0).abs() < 1e-9,
            "sector {i} is {each:.4}; it read {flux:.4}"
        );
    }
    let sum: f64 = sides.iter().filter_map(|&s| m.face_outward_flux(s)).sum();
    assert!(
        (sum / lateral - 1.0).abs() < 1e-9,
        "the lateral surface is {lateral:.4}; the fan summed to {sum:.4}"
    );
}

/// The primitive cone standing on the ground, whose base cap contributes nothing
/// there, so its whole volume is its band: it read 0.9997 and now reads its cone.
#[test]
fn the_primitive_cone_is_exact_on_the_ground() {
    let truth = PI * R * R * H / 3.0;
    let mut m = Mesh::new();
    m.create_cone(DVec3::ZERO, R, H, N as u32, mat()).expect("primitive cone");
    let v = m.mesh_volume();
    assert!(
        (v / truth - 1.0).abs() < 1e-9,
        "πr²h/3 = {truth:.1}, `create_cone` read {v:.1} ({:.9} of it)",
        v / truth
    );
}

/// The controls: the three surfaces that already had a closed form must not move.
#[test]
fn the_other_surfaces_are_unchanged() {
    // Plane + Cylinder, through a bored box.
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, mat()).expect("box");
    m.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, 32).expect("bore");
    let bored = 8_000_000.0 - PI * R * R * 200.0;
    let v = m.mesh_volume();
    assert!((v / bored - 1.0).abs() < 1e-9, "a bored box is {bored:.1}, it read {v:.1}");

    // Sphere.
    let sphere = 4.0 / 3.0 * PI * R * R * R;
    let mut m = Mesh::new();
    m.create_sphere(DVec3::ZERO, R, 16, 12, mat()).expect("sphere");
    let v = m.mesh_volume();
    assert!((v / sphere - 1.0).abs() < 1e-9, "a sphere is {sphere:.1}, it read {v:.1}");

    // The 16-arc cylinder, which the closed-form Cylinder arm already read exactly.
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, N, 100.0);
    m.create_solid(f, CreateSolidMode::Extrude { distance: H }, mat()).expect("extrude");
    let cyl = PI * R * R * H;
    let v = m.mesh_volume();
    assert!((v / cyl - 1.0).abs() < 1e-9, "πr²h = {cyl:.1}, it read {v:.1}");
}

/// The instrument that already knew. `verify_outward_normals` reported
/// `inward_count = 16` for every cone `create_cone` made — all sixteen side
/// triangles wound into the solid — and nothing asserted it, because no reader
/// followed a cone's winding until the band got a closed form. Now something
/// does, for every curved primitive.
#[test]
fn every_curved_primitive_winds_outward() {
    let mut wrong = Vec::new();

    let mut m = Mesh::new();
    m.create_cone(DVec3::ZERO, R, H, N as u32, mat()).expect("cone");
    let rep = m.verify_outward_normals();
    if !rep.is_closed_solid || rep.inward_count != 0 {
        wrong.push(format!("create_cone: closed {} inward {}", rep.is_closed_solid, rep.inward_count));
    }

    let mut m = Mesh::new();
    m.create_cone_kernel_native(DVec3::ZERO, R, H, mat()).expect("path B cone");
    let rep = m.verify_outward_normals();
    if !rep.is_closed_solid || rep.inward_count != 0 {
        wrong.push(format!("create_cone_kernel_native: closed {} inward {}", rep.is_closed_solid, rep.inward_count));
    }

    let mut m = Mesh::new();
    m.create_sphere(DVec3::ZERO, R, 16, 12, mat()).expect("sphere");
    let rep = m.verify_outward_normals();
    if !rep.is_closed_solid || rep.inward_count != 0 {
        wrong.push(format!("create_sphere: closed {} inward {}", rep.is_closed_solid, rep.inward_count));
    }

    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, R, H, N as u32, mat()).expect("cylinder");
    let rep = m.verify_outward_normals();
    if !rep.is_closed_solid || rep.inward_count != 0 {
        wrong.push(format!("create_cylinder: closed {} inward {}", rep.is_closed_solid, rep.inward_count));
    }

    let (m, _) = coned(0.0, H, 0.0);
    let rep = m.verify_outward_normals();
    if !rep.is_closed_solid || rep.inward_count != 0 {
        wrong.push(format!("extruded apex cone: closed {} inward {}", rep.is_closed_solid, rep.inward_count));
    }

    assert!(wrong.is_empty(), "a solid's faces look out of it:\n  {}", wrong.join("\n  "));
}
