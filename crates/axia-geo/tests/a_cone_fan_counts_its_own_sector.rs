//! AN APEX CONE'S FAN TRIANGLE CARRIES THE SECTOR IT COVERS, NOT THE WHOLE CONE.
//!
//! `extrude_planar_cone`'s apex arm gave all N fan triangles ONE Cone surface
//! spanning the full turn, `u_range: (0, 2π)`, and left the narrowing to whoever
//! read it. A reader cannot do it here: a slice is taken from the face's corners,
//! and the apex is a parametric degeneracy — its u is undefined, it inverts to
//! u = 0, and a range taken from all three corners comes out twice as wide as the
//! sector. So nothing narrowed, and every triangle answered for the whole cone.
//!
//! Measured 2026-09-27 on a 16-arc circle (r = 40) coned 300:
//!
//! ```text
//!   volume                  15.9946 × πr²h/3
//!   one fan triangle's flux 1507458.3, where its own sector is 94247.8
//!   its corners invert to   u = −0.7854, 0.0000 (the apex), −0.3927
//!                           → the range spans 0.7854, twice the sector's 0.3927
//!   render                  6462 triangles — sixteen whole cones, overlapping;
//!                           `create_cone` draws the same shape in 526
//! ```
//!
//! The sibling builder never had this: `create_cone` hands each sector face
//! `u_range: (theta_start, theta_end)` (primitives.rs), which is why the cone
//! PRIMITIVE has always measured 0.9997 and the extruded one 15.99. This does the
//! same, except that the angles are read off each triangle's own two base
//! corners rather than synthesised from its index — the loop starts wherever its
//! start half-edge is, which is how the top-rim arcs went one edge along.
//!
//! ⚠ The reader's gap is still there and now has nothing reachable behind it:
//! `compute_uv_slice_for_quad_face` wants four corners, so no 3-vertex curved
//! face is narrowed by a reader. Every one that exists today carries its own
//! range from its builder — `create_cone`'s 16 fan triangles and
//! `create_sphere`'s 32 polar triangles, both measured — so there is nothing to
//! fix and no guard to write. A split that leaves a triangle on a curved face
//! would land in that gap; when one does, the slice will need to learn about
//! triangles, and about which corner it must ignore.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::CreateSolidMode;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 40.0;
const H: f64 = 300.0;
const N: usize = 16;
const PI: f64 = std::f64::consts::PI;
const TAU: f64 = std::f64::consts::TAU;

/// A cone band has no closed-form flux — `analytic_face_flux` has arms for Plane,
/// Sphere and Cylinder only — so it is read from its tessellation, whose chords
/// lie inside the surface. `create_cone`, which has always been narrowed per
/// sector, reads 0.9997 of its truth for the same reason.
const TESSELLATION_SLACK: f64 = 1e-3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

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
        let a0 = i as f64 * TAU / n as f64;
        let a1 = (i + 1) as f64 * TAU / n as f64;
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

fn apex_cone(z: f64, dist: f64) -> (Mesh, Vec<FaceId>, FaceId) {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, N, z);
    let r = m
        .create_solid(f, CreateSolidMode::ExtrudeCone { distance: dist, top_scale: 0.0 }, mat())
        .expect("apex cone");
    (m, r.side_faces, r.profile_face)
}

#[test]
fn an_apex_cone_measures_its_cone() {
    let truth = PI * R * R * H / 3.0;
    for dist in [H, -H] {
        let (m, _, _) = apex_cone(0.0, dist);
        let v = m.mesh_volume();
        assert!(
            (v / truth - 1.0).abs() < TESSELLATION_SLACK,
            "{dist:+}: πr²h/3 = {truth:.1}, it read {v:.1} ({:.4} of it) — it used to \
             read about sixteen times it",
            v / truth
        );
    }
}

/// Each triangle answers for its own sector, and the sixteen of them add up to
/// the cone's lateral surface once — no more (each used to count all of it) and
/// no less (a sector must not be lost where u wraps).
#[test]
fn the_fan_divides_one_cone_between_its_triangles() {
    // 3V = base flux (zero at z = 0) + lateral flux, so the lateral is 3V.
    let lateral = 3.0 * PI * R * R * H / 3.0;
    let (m, sides, _) = apex_cone(0.0, H);
    assert_eq!(sides.len(), N, "a 16-arc circle fans into 16 triangles");

    let each = lateral / N as f64;
    for (i, &s) in sides.iter().enumerate() {
        let flux = m.face_outward_flux(s).expect("flux");
        assert!(
            (flux / each - 1.0).abs() < TESSELLATION_SLACK,
            "triangle {i} covers one sixteenth, which is {each:.1}; it read {flux:.1}"
        );
    }
    let sum: f64 = sides.iter().filter_map(|&s| m.face_outward_flux(s)).sum();
    assert!(
        (sum / lateral - 1.0).abs() < TESSELLATION_SLACK,
        "the lateral surface is {lateral:.1}; the fan summed to {sum:.1}"
    );
}

/// And the sector each triangle carries is the one its own two base corners
/// span — read off the corners, because the boundary loop starts wherever its
/// start half-edge is and not at angle 0.
#[test]
fn each_triangle_carries_the_sector_its_corners_span() {
    let (m, sides, _) = apex_cone(0.0, H);
    let sector = TAU / N as f64;
    for (i, &s) in sides.iter().enumerate() {
        let Some(AnalyticSurface::Cone { apex, axis_dir, ref_dir, u_range, v_range, .. }) =
            m.face_surface(s)
        else {
            panic!("triangle {i} carries no Cone surface");
        };
        assert!(
            ((u_range.1 - u_range.0) / sector - 1.0).abs() < 1e-9,
            "triangle {i}'s sector is {sector:.4} wide; it carries {:.4} ({u_range:?})",
            u_range.1 - u_range.0
        );
        assert!(
            (v_range.0).abs() < 1e-9 && (v_range.1 - H).abs() < 1e-9,
            "triangle {i} runs from the apex to the base, so v is (0, {H}); it carries {v_range:?}"
        );

        // The corners themselves, inverted the way a reader would.
        let axis_n = axis_dir.normalize_or_zero();
        let ref_n = ref_dir.normalize_or_zero();
        let basis_v = axis_n.cross(ref_n);
        let corners = m.collect_loop_verts(m.faces[s].outer().start).expect("triangle");
        assert_eq!(corners.len(), 3, "an apex fan triangle has three corners");
        let mut off_apex = 0;
        for &c in &corners {
            let local = m.vertex_pos(c).expect("corner") - *apex;
            if local.dot(axis_n).abs() < 1e-9 {
                continue; // the apex — its u is undefined
            }
            off_apex += 1;
            let u = local.dot(basis_v).atan2(local.dot(ref_n));
            // The stored range may be shifted a whole turn from atan2's branch.
            let inside = [u, u + TAU, u - TAU]
                .iter()
                .any(|&x| x >= u_range.0 - 1e-9 && x <= u_range.1 + 1e-9);
            assert!(inside, "triangle {i} carries {u_range:?}, which does not hold its corner at u = {u:.4}");
        }
        assert_eq!(off_apex, 2, "two of the three corners sit on the base ring");
    }
}

/// A solid barely changes by standing somewhere else. It used to change a great
/// deal: the whole-cone reading scaled with the placement, so this same cone read
/// 1.3125 of its z = 0 value at z = 100 even after its caps were turned outward.
///
/// Measured 2026-09-27 it is 0.999894 — and not 1.000000, for the reason the
/// frustum is not exact either: the caps are exact and the band is read from its
/// tessellation, so the part that does not cancel moves with the height. The
/// is-signal for that cause lives in `a_polygon_arc_solid_stands_anywhere`.
#[test]
fn an_apex_cone_measures_almost_the_same_wherever_it_stands() {
    let mut wrong = Vec::new();
    for dist in [H, -H] {
        let base = apex_cone(0.0, dist).0.mesh_volume();
        for z in [100.0, -150.0] {
            let v = apex_cone(z, dist).0.mesh_volume();
            if (v / base - 1.0).abs() > 5e-4 {
                wrong.push(format!("{dist:+}, base z = {z}: {:.6} of its z = 0 reading", v / base));
            }
        }
    }
    assert!(wrong.is_empty(), "a solid moved more than the band's tessellation can account for:\n  {}", wrong.join("\n  "));
}

/// And the renderer stops drawing the same cone sixteen times.
#[test]
fn the_fan_is_drawn_once() {
    let (mut m, _, _) = apex_cone(0.0, H);
    let (_pos, _nrm, idx, _fmap, _p64) = m.export_buffers().expect("export");
    let tris = idx.len() / 3;

    let mut control = Mesh::new();
    control.create_cone(DVec3::ZERO, R, H, N as u32, mat()).expect("primitive cone");
    let (_p, _n, cidx, _f, _q) = control.export_buffers().expect("export");
    let control_tris = cidx.len() / 3;

    assert!(
        tris < control_tris * 3,
        "the same shape from `create_cone` takes {control_tris} triangles; this took {tris} \
         (it used to take 6462 — sixteen whole cones laid over each other)"
    );
}

/// The controls: the two builders that were already narrowed per face must not
/// move, and neither must the frustum, whose quads a reader can slice.
#[test]
fn the_primitives_and_the_frustum_are_unchanged() {
    let cone_truth = PI * R * R * H / 3.0;
    let mut m = Mesh::new();
    m.create_cone(DVec3::ZERO, R, H, N as u32, mat()).expect("primitive cone");
    let v = m.mesh_volume();
    assert!(
        (v / cone_truth - 1.0).abs() < TESSELLATION_SLACK,
        "`create_cone` reads its own cone; it read {:.4} of it",
        v / cone_truth
    );

    let sphere_truth = 4.0 / 3.0 * PI * R * R * R;
    let mut m = Mesh::new();
    m.create_sphere(DVec3::ZERO, R, 16, 12, mat()).expect("sphere");
    let v = m.mesh_volume();
    assert!(
        (v / sphere_truth - 1.0).abs() < 1e-9,
        "`create_sphere`, whose 32 polar triangles are narrowed by their builder too, \
         read {:.4} of its sphere",
        v / sphere_truth
    );

    const S: f64 = 0.5;
    let r_top = R * S;
    let frustum_truth = PI * H * (R * R + R * r_top + r_top * r_top) / 3.0;
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, N, 0.0);
    m.create_solid(f, CreateSolidMode::ExtrudeCone { distance: H, top_scale: S }, mat())
        .expect("frustum");
    let v = m.mesh_volume();
    assert!(
        (v / frustum_truth - 1.0).abs() < TESSELLATION_SLACK,
        "the frustum read {:.6} of its truth",
        v / frustum_truth
    );
}
