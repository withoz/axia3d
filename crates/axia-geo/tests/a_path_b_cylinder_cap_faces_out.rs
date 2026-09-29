//! A PATH B CYLINDER'S CAPS LOOK OUT OF THE SOLID.
//!
//! `extrude_cylinder_kernel_native` keeps the profile as one cap and builds the
//! other with `add_face_closed_curve`, and a closed-curve face looks along its
//! circle's normal. Nothing turned either round, so whichever cap ended up on the
//! back of the extrusion looked INTO the solid — for `+dist` the profile, for
//! `−dist` the new cap (measured 2026-09-15, both caps of a disk at z = 100):
//!
//! ```text
//!   extrude +300   bottom = profile   looks +Z   flux  +502 654.8
//!   extrude −300   bottom = new cap   looks +Z   flux −1 005 309.6
//! ```
//!
//! Volume is flux over three, so the error is `2·A·z_bottom / 3` — nothing at all
//! when the bottom sits on z = 0, which is where every volume test stood its
//! cylinder. The Inspector reads the same flux (`face_set_volume`), so the volume
//! and the weight of every Path B cylinder moved with the height it stood at
//! (r = 40, h = 300):
//!
//! ```text
//!   base z = 0       1.0000
//!   base z = 100     1.2222
//!   base z = −150    0.6667
//! ```
//!
//! The renderer drew the bottom facing in as well: 100 of 100 triangles, winding
//! and vertex normals both, so from below the cap showed its back.
//!
//! ADR-183 turned the bottom cap of a POLYGON extrude outward; the closed-curve
//! branch returns before that code runs. The two-sided extrude (ADR-261) reaches
//! the same builder for a circle, which is why its doc comment's "outward
//! bottom cap" was not true for one: 100/200 about z = 0 read 0.5556.

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

/// Path B on, as the app has it.
fn path_b() -> Mesh {
    let mut m = Mesh::new();
    m.set_cylinder_path_b_default(true);
    m
}

/// A closed-curve disk the way a circle tool leaves one: an anchor on the rim, the
/// circle on a self-loop edge, and the Plane `add_face_closed_curve` attaches.
fn disk(m: &mut Mesh, center: DVec3, normal: DVec3, basis_u: DVec3) -> FaceId {
    let anchor = m.add_vertex(center + basis_u * R);
    m.add_face_closed_curve(
        anchor,
        AnalyticCurve::Circle { center, radius: R, normal, basis_u },
        mat(),
    )
    .expect("disk")
}

#[test]
fn a_path_b_cylinder_measures_the_same_wherever_it_stands() {
    let truth = PI * R * R * H;
    let mut read: Vec<(String, f64)> = Vec::new();

    for z in [0.0, 100.0, -150.0] {
        let mut m = path_b();
        m.create_cylinder(DVec3::new(0.0, 0.0, z), R, H, 32, mat())
            .expect("cylinder");
        read.push((format!("create_cylinder, base z = {z}"), m.mesh_volume()));
    }
    for dist in [H, -H] {
        let mut m = path_b();
        let f = disk(&mut m, DVec3::new(0.0, 0.0, 100.0), DVec3::Z, DVec3::X);
        m.create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat())
            .expect("extrude");
        read.push((format!("extrude {dist:+} from z = 100"), m.mesh_volume()));
    }
    // A circle on a wall, pushed: the axis is X.
    {
        let mut m = path_b();
        let f = disk(&mut m, DVec3::new(-150.0, 0.0, 0.0), DVec3::X, DVec3::Y);
        m.create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
            .expect("extrude along X");
        read.push(("extrude along X from x = -150".into(), m.mesh_volume()));
    }
    // Two-sided: the profile moves to the far side first, so its cap is never on
    // the origin even when the circle was drawn there.
    {
        let mut m = path_b();
        let f = disk(&mut m, DVec3::ZERO, DVec3::Z, DVec3::X);
        m.create_solid(
            f,
            CreateSolidMode::ExtrudeBidirectional { dist_pos: 100.0, dist_neg: 200.0 },
            mat(),
        )
        .expect("two-sided");
        read.push(("two-sided 100/200 about z = 0".into(), m.mesh_volume()));
    }

    let wrong: Vec<String> = read
        .iter()
        .filter(|(_, v)| (v / truth - 1.0).abs() > 1e-6)
        .map(|(label, v)| format!("{label}: {:.4}", v / truth))
        .collect();
    assert!(
        wrong.is_empty(),
        "πr²h = {truth:.1}; these read otherwise (fraction of the truth):\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn each_cap_looks_away_from_the_solid() {
    for dist in [H, -H] {
        let mut m = path_b();
        let f = disk(&mut m, DVec3::new(0.0, 0.0, 100.0), DVec3::Z, DVec3::X);
        let r = m
            .create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat())
            .expect("extrude");
        // The profile stays where it was drawn; the new cap lands `dist` along +Z.
        let (bottom, top) = if dist > 0.0 {
            (r.profile_face, r.top_face)
        } else {
            (r.top_face, r.profile_face)
        };
        for (label, cap, out) in [("bottom", bottom, DVec3::NEG_Z), ("top", top, DVec3::Z)] {
            let looks = m.faces[cap].normal();
            assert!(
                looks.dot(out) > 0.999,
                "extrude {dist:+}: the {label} cap looks {looks:?}, it should look {out:?}"
            );
            // The draw-plane lookup and every coplanar test read the Plane, not the
            // face — they have to say the same thing.
            match m.face_surface(cap) {
                Some(AnalyticSurface::Plane { normal, .. }) => assert!(
                    normal.dot(out) > 0.999,
                    "extrude {dist:+}: the {label} cap looks {looks:?} but its Plane says {normal:?}"
                ),
                other => panic!("extrude {dist:+}: the {label} cap carries {other:?}, not a Plane"),
            }
        }
    }
}

#[test]
fn the_renderer_draws_both_caps_facing_out() {
    for dist in [H, -H] {
        let mut m = path_b();
        let f = disk(&mut m, DVec3::new(0.0, 0.0, 100.0), DVec3::Z, DVec3::X);
        m.create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat())
            .expect("extrude");
        let (z_lo, z_hi) = if dist > 0.0 { (100.0, 100.0 + dist) } else { (100.0 + dist, 100.0) };

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
                "extrude {dist:+}: no triangle lies on the {label} cap at z = {z} — the question never arrived"
            );
            assert_eq!(
                (wound_in, normal_in),
                (0, 0),
                "extrude {dist:+}: of {total} {label} cap triangles, {wound_in} are wound inward and \
                 {normal_in} carry an inward normal"
            );
        }
    }
}

/// A cap's rim circle is shared with the side face, so the circle cannot say which
/// way the cap looks: once the bottom cap is turned out it looks −Z while the rim
/// still runs about +Z. An extrude that starts from such a face has to go the way
/// the FACE looks — before, it followed the rim, and pushing a bottom cap away
/// from the solid would have driven it back up into it.
#[test]
fn an_extrude_goes_the_way_its_face_looks_not_its_rim() {
    let center = DVec3::new(0.0, 0.0, 100.0);
    let mut m = path_b();
    let f = disk(&mut m, center, DVec3::Z, DVec3::X);
    // Turn the disk round without touching its rim.
    m.faces[f].set_normal(DVec3::NEG_Z);
    m.set_face_surface(
        f,
        Some(AnalyticSurface::Plane {
            origin: center,
            normal: DVec3::NEG_Z,
            basis_u: DVec3::X,
            u_range: (-1.5 * R, 1.5 * R),
            v_range: (-1.5 * R, 1.5 * R),
        }),
    );

    m.create_solid(f, CreateSolidMode::Extrude { distance: 50.0 }, mat())
        .expect("extrude");

    let (lo, hi) = m
        .verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .fold((f64::MAX, f64::MIN), |(lo, hi), (_, v)| (lo.min(v.pos().z), hi.max(v.pos().z)));
    assert_eq!(
        (lo, hi),
        (50.0, 100.0),
        "a disk at z = 100 looking −Z, pushed 50, should reach down to z = 50"
    );
    let v = m.mesh_volume();
    let truth = PI * R * R * 50.0;
    assert!((v / truth - 1.0).abs() < 1e-6, "volume {v:.1}, πr²h = {truth:.1}");
}
