//! A CONE EXTRUSION'S CAPS LOOK OUT OF THE SOLID.
//!
//! The circle-to-cone extrude (ADR-260) mirrors the Path B cylinder builder, and
//! it mirrored that builder's defect: nothing turned the cap on the back of the
//! extrusion round, so it looked into the solid. Measured 2026-09-16 (r = 40,
//! h = 300, volume over πr²h/3·(1 + s + s²)):
//!
//! ```text
//!                      base z = 0   z = 100   z = −150
//!   apex     +300        0.9997     1.6662    −0.0002
//!   apex     −300        0.9997     0.9998     0.9995    the base is in front
//!   frustum  +300        0.9997     1.3806     0.4284
//!   frustum  −300        0.7140     0.8093     0.5711    the small cap is behind
//! ```
//!
//! ADR-260 settled "no flip needed" with `verify_face_invariants`, which does
//! not look at which way a face points.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::{CreateSolidMode, CreateSolidResult};
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 40.0;
const H: f64 = 300.0;
const PI: f64 = std::f64::consts::PI;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// A circle drawn at height `z`, pushed `dist` with the top scaled by `top_scale`.
fn cone(z: f64, dist: f64, top_scale: f64) -> (Mesh, CreateSolidResult) {
    let mut m = Mesh::new();
    m.set_cylinder_path_b_default(true);
    let c = DVec3::new(0.0, 0.0, z);
    let anchor = m.add_vertex(c + DVec3::X * R);
    let f = m
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle { center: c, radius: R, normal: DVec3::Z, basis_u: DVec3::X },
            mat(),
        )
        .expect("disk");
    let r = m
        .create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale }, mat())
        .expect("cone extrude");
    (m, r)
}

#[test]
fn a_cone_extrusion_measures_the_same_wherever_it_stands() {
    let mut wrong = Vec::new();
    for (label, s) in [("apex", 0.0), ("frustum 50%", 0.5)] {
        let truth = PI * R * R * H / 3.0 * (1.0 + s + s * s);
        for dist in [H, -H] {
            for z in [0.0, 100.0, -150.0] {
                let (m, _) = cone(z, dist, s);
                let ratio = m.mesh_volume() / truth;
                // The side has no closed form and goes through the tessellated
                // fall-back, which lands within 0.05% here.
                if (ratio - 1.0).abs() > 1e-3 {
                    wrong.push(format!("{label} {dist:+}, base z = {z}: {ratio:.4}"));
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "πr²h/3·(1 + s + s²); these read otherwise (fraction of the truth):\n  {}",
        wrong.join("\n  ")
    );
}

fn assert_looks(m: &Mesh, cap: FaceId, out: DVec3, what: &str) {
    let looks = m.faces[cap].normal();
    assert!(looks.dot(out) > 0.999, "{what}: the cap looks {looks:?}, it should look {out:?}");
    match m.face_surface(cap) {
        Some(AnalyticSurface::Plane { normal, .. }) => assert!(
            normal.dot(out) > 0.999,
            "{what}: the cap looks {looks:?} but its Plane says {normal:?}"
        ),
        other => panic!("{what}: the cap carries {other:?}, not a Plane"),
    }
}

#[test]
fn each_cone_cap_looks_away_from_the_solid() {
    for dist in [H, -H] {
        // Frustum: two caps. The profile stays at z = 0; the small cap lands at `dist`.
        let (m, r) = cone(0.0, dist, 0.5);
        let (bottom, top) = if dist > 0.0 {
            (r.profile_face, r.top_face)
        } else {
            (r.top_face, r.profile_face)
        };
        assert_looks(&m, bottom, DVec3::NEG_Z, &format!("frustum {dist:+}, bottom"));
        assert_looks(&m, top, DVec3::Z, &format!("frustum {dist:+}, top"));

        // Apex: one cap, and it is the bottom only when the apex is above it.
        let (m, r) = cone(0.0, dist, 0.0);
        let out = if dist > 0.0 { DVec3::NEG_Z } else { DVec3::Z };
        assert_looks(&m, r.profile_face, out, &format!("apex {dist:+}, base"));
    }
}

#[test]
fn the_renderer_draws_both_frustum_caps_facing_out() {
    for dist in [H, -H] {
        let (mut m, _) = cone(0.0, dist, 0.5);
        let (z_lo, z_hi) = if dist > 0.0 { (0.0, dist) } else { (dist, 0.0) };
        let (pos, nrm, tris, _, _) = m.export_buffers().expect("export");
        let p = |i: u32| {
            let i = i as usize * 3;
            DVec3::new(pos[i] as f64, pos[i + 1] as f64, pos[i + 2] as f64)
        };
        let n = |i: u32| {
            let i = i as usize * 3;
            DVec3::new(nrm[i] as f64, nrm[i + 1] as f64, nrm[i + 2] as f64)
        };
        for (label, z, out) in [("bottom", z_lo, DVec3::NEG_Z), ("top", z_hi, DVec3::Z)] {
            let (mut total, mut wound_in, mut normal_in) = (0, 0, 0);
            for t in tris.chunks(3) {
                if !t.iter().all(|&i| (p(i).z - z).abs() < 1e-3) {
                    continue;
                }
                total += 1;
                if (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).dot(out) <= 0.0 {
                    wound_in += 1;
                }
                if t.iter().any(|&i| n(i).dot(out) <= 0.0) {
                    normal_in += 1;
                }
            }
            assert!(
                total > 0,
                "frustum {dist:+}: no triangle lies on the {label} cap at z = {z} — the question never arrived"
            );
            assert_eq!(
                (wound_in, normal_in),
                (0, 0),
                "frustum {dist:+}: of {total} {label} cap triangles, {wound_in} are wound inward and \
                 {normal_in} carry an inward normal"
            );
        }
    }
}
