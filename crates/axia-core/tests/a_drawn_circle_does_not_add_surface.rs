//! C-2's precondition: make the curved matrix judgeable.
//!
//! C-0 measured that two overlapping circles on a curved host do not resolve —
//! the second splits its own face and leaves the first alone, so the lens
//! belongs to two faces at once. That is C-2's subject. But it also measured
//! that nothing can currently JUDGE such a fix:
//!
//! ```text
//!   face_outer_area   integrates the surface's whole u/v range, and a split
//!                     hands both children the parent's range (trap #7)
//!   point_in_face     tests the face PLANE first — every point on a curved
//!                     face reads outside
//!   export_buffers    sound before a draw (1882.477 vs a true 1884.956) and
//!                     inflated after one (2292.511, +21.6%)
//! ```
//!
//! The export is the closest to usable, and its error has a known cause: the
//! remainder is earcut in (u, v) and mapped back, and u is an ANGLE, so a
//! triangle spanning a wide u is a flat slab through the solid rather than a
//! patch of its skin. Fix that and the buffers become the instrument C-2 needs.
//!
//! The claim under test is one every host must satisfy and none of them can
//! argue with: **dividing a face does not change how much surface there is.**

use axia_core::scene::Scene;
use axia_geo::surfaces::{cone, cylinder, sphere, torus, AnalyticSurface};
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Host {
    Cylinder,
    Sphere,
    Cone,
    Torus,
}

impl Host {
    const ALL: [Host; 4] = [Host::Cylinder, Host::Sphere, Host::Cone, Host::Torus];
    fn name(self) -> &'static str {
        match self {
            Host::Cylinder => "cylinder",
            Host::Sphere => "sphere",
            Host::Cone => "cone",
            Host::Torus => "torus",
        }
    }
}

fn build(host: Host) -> (Scene, FaceId) {
    let mut s = Scene::new();
    let m = MaterialId::new(0);
    s.mesh.set_cylinder_path_b_default(true);
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.set_cone_path_b_default(true);
    s.mesh.set_torus_path_b_default(true);
    match host {
        Host::Cylinder => {
            s.mesh.create_cylinder(DVec3::ZERO, 10.0, 20.0, 16, m).unwrap();
        }
        Host::Sphere => {
            s.mesh.create_sphere(DVec3::ZERO, 10.0, 16, 12, m).unwrap();
        }
        Host::Cone => {
            s.mesh.create_cone(DVec3::ZERO, 10.0, 20.0, 16, m).unwrap();
        }
        Host::Torus => {
            s.mesh
                .create_torus_kernel_native(DVec3::ZERO, 10.0, 3.0, m)
                .unwrap();
        }
    }
    let curved = s
        .mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface()
                    .is_some_and(|su| !matches!(su, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .unwrap_or_else(|| panic!("{}: no curved face", host.name()));
    (s, curved)
}

fn at(surf: &AnalyticSurface, u: f64, v: f64) -> DVec3 {
    match surf {
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } => {
            cylinder::evaluate(*axis_origin, *axis_dir, *radius, *ref_dir, u, v)
        }
        AnalyticSurface::Sphere { center, radius, axis_dir, ref_dir, .. } => {
            sphere::evaluate(*center, *radius, *axis_dir, *ref_dir, u, v)
        }
        AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, .. } => {
            cone::evaluate(*apex, *axis_dir, *half_angle, *ref_dir, u, v)
        }
        AnalyticSurface::Torus {
            center, axis_dir, ref_dir, major_radius, minor_radius, ..
        } => torus::evaluate(*center, *axis_dir, *ref_dir, *major_radius, *minor_radius, u, v),
        _ => panic!("not curved"),
    }
}

fn ranges(surf: &AnalyticSurface) -> ((f64, f64), (f64, f64)) {
    match surf {
        AnalyticSurface::Cylinder { u_range, v_range, .. }
        | AnalyticSurface::Sphere { u_range, v_range, .. }
        | AnalyticSurface::Cone { u_range, v_range, .. }
        | AnalyticSurface::Torus { u_range, v_range, .. } => (*u_range, *v_range),
        _ => panic!("not curved"),
    }
}

fn draw_circle(s: &mut Scene, host: Host, f: FaceId, c: DVec3, r: DVec3) -> Option<(FaceId, FaceId)> {
    match host {
        Host::Cylinder => s.draw_circle_on_cylinder(f, c, r),
        Host::Sphere => s.draw_circle_on_sphere(f, c, r),
        Host::Cone => s.draw_circle_on_cone(f, c, r),
        Host::Torus => s.draw_circle_on_torus(f, c, r),
    }
}

/// The area of the triangles the viewport is handed.
fn drawn_area(s: &mut Scene) -> f64 {
    let (pos, _n, idx, _fm, _d) = s.mesh.export_buffers().expect("export");
    idx.chunks_exact(3)
        .map(|t| {
            let p = |i: u32| {
                DVec3::new(
                    pos[3 * i as usize] as f64,
                    pos[3 * i as usize + 1] as f64,
                    pos[3 * i as usize + 2] as f64,
                )
            };
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            0.5 * (b - a).cross(c - a).length()
        })
        .sum()
}

/// How far off the skin the worst triangle sits, as a fraction of the radius.
///
/// A patch of a curved surface has all three corners on it, so its centroid
/// sags inward by the sagitta — tiny for a fine tessellation. A slab through
/// the solid sags most of the way to the axis. One number, no interpretation.
fn worst_sag(s: &mut Scene, centre: DVec3, radius: f64) -> f64 {
    let (pos, _n, idx, _fm, _d) = s.mesh.export_buffers().expect("export");
    let mut worst = 0.0_f64;
    for t in idx.chunks_exact(3) {
        let p = |i: u32| {
            DVec3::new(
                pos[3 * i as usize] as f64,
                pos[3 * i as usize + 1] as f64,
                pos[3 * i as usize + 2] as f64,
            )
        };
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        // Only triangles whose corners are all on the surface can be judged
        // this way; a cap's corners are too, so use the corner distance as the
        // reference rather than assuming the radius.
        let d = |q: DVec3| (q - centre).length();
        let corner = (d(a) + d(b) + d(c)) / 3.0;
        if (corner - radius).abs() > 0.05 * radius {
            continue; // not on this sphere/cylinder of radius `radius`
        }
        let ctr = (a + b + c) / 3.0;
        let sag = (corner - d(ctr)) / radius;
        if sag > worst {
            worst = sag;
        }
    }
    worst
}

/// What the fix COSTS, measured in the same breath as what it buys.
///
/// A render fix that trades a 22% area error for a 240x triangle count is not
/// a fix — LOCKED #45/#46 are both about the syncMesh budget — and the first
/// two attempts here were exactly that. A global N-grid over every coarse
/// triangle (ADR-197 #6) bought the area back at 300 → 39,049 triangles on the
/// cylinder. Per-edge splitting with a centroid fan was cheaper and did not
/// work at all: the wedges still spanned the full width and the cone got
/// WORSE, ×4.78. What shipped grids the band and rings the cap, so the cost is
/// a small multiple of what an undrawn primitive already pays:
///
/// ```text
///   sphere      2500 → 2712   ×1.1     (untouched — the control)
///   torus       3192 → 2434   ×0.8     fewer, and now correct
///   cylinder     300 → 1502   ×5.0
///   cone         238 → 3266   ×13.7    largest ratio, smallest base
/// ```
///
/// The ratio is pinned per host AND the absolute count is capped, because a
/// ratio alone would let the cone's low base hide a real number, and a cap
/// alone would let a primitive that starts heavy get heavier.
#[test]
fn the_triangle_count_stays_within_budget() {
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let n0 = { let (_p, _n, i, _f, _d) = s.mesh.export_buffers().unwrap(); i.len() / 3 };
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let du = 0.15 * (u1 - u0);
        draw_circle(&mut s, host, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .unwrap_or_else(|| panic!("{}: circle must draw", host.name()));
        let n1 = { let (_p, _n, i, _f, _d) = s.mesh.export_buffers().unwrap(); i.len() / 3 };
        let ratio = n1 as f64 / n0 as f64;
        println!("TRIS {:<9} {:>7} -> {:>7}   x{:.1}", host.name(), n0, n1, ratio);
        assert!(
            ratio < 20.0,
            "{}: drawing one circle multiplied the triangle count by {ratio:.1}",
            host.name()
        );
        assert!(
            n1 < 6000,
            "{}: {n1} triangles for one drawn circle is past the budget",
            host.name()
        );
    }
}

/// Drawing a circle must not change how much surface the viewport draws.
///
/// Where this file started, and where it ended:
///
/// ```text
///                 before     after
///   sphere       x1.0001   x1.0001   was already right - the control
///   cone         x1.1033   x1.0002   drew 10% too much
///   cylinder     x1.2178   x1.0000   drew 22% too much
///   torus        x0.5418   x1.0055   drew 46% too LITTLE
/// ```
///
/// The sphere was never wrong, and that is what said this was not "curved
/// hosts are hard": its clipper walks the boundary and refines as it goes
/// (marching triangles, ADR-202 beta-3b). The other three earcut a band in
/// (u, v) and map it back, and u is an ANGLE - so earcut, which adds no
/// interior points, produced triangles spanning up to 177 degrees of the turn.
/// Flat slabs through the solid on the cylinder and cone; on the torus, where
/// BOTH parameters are angles, a collapse to 85 triangles and a hole a user
/// could see.
///
/// Fixed by gridding the band and ringing the cap instead of earcutting
/// either - see `band_minus_hole_uv` and `disc_from_boundary_uv`.
#[test]
fn drawing_a_circle_must_not_change_the_drawn_area() {
    let mut ratios = Vec::new();
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let before = drawn_area(&mut s);
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let du = 0.15 * (u1 - u0);

        draw_circle(&mut s, host, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .unwrap_or_else(|| panic!("{}: circle must draw", host.name()));
        let after = drawn_area(&mut s);
        let ratio = after / before;
        println!(
            "AREA {:<9} {:>9.3} → {:>9.3}   ×{:.4}",
            host.name(),
            before,
            after,
            ratio
        );
        ratios.push((host, ratio));
    }
    // Every host, one claim, no exceptions: dividing a face is not a change of
    // area. 1% covers the inscribed-polygon deficit a chord tolerance leaves
    // and nothing else - the errors this replaced were 10%, 22% and 46%.
    for (host, r) in &ratios {
        assert!(
            (r - 1.0).abs() < 0.01,
            "{}: drawing a circle changed the drawn area by x{r:.4}",
            host.name()
        );
    }
}

/// The mechanism, in one number: how far the worst triangle sags off the skin.
///
/// A patch of a curved surface has its corners on it and its centroid barely
/// inside; a chord slab sags most of the way to the axis. Measured on the
/// CYLINDER — the sphere does not have this problem, which is exactly why it
/// is the wrong host to measure it on.
#[test]
fn a_drawn_cylinder_emits_triangles_that_sag_off_the_skin() {
    let radial = |p: DVec3| (p.x * p.x + p.y * p.y).sqrt();
    let sag = |s: &mut Scene| -> f64 {
        let (pos, _n, idx, _fm, _d) = s.mesh.export_buffers().expect("export");
        let mut worst = 0.0_f64;
        for t in idx.chunks_exact(3) {
            let p = |i: u32| {
                DVec3::new(
                    pos[3 * i as usize] as f64,
                    pos[3 * i as usize + 1] as f64,
                    pos[3 * i as usize + 2] as f64,
                )
            };
            let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
            // Only triangles whose corners all lie on the wall can be judged
            // this way — a cap's corners do not.
            if [a, b, c].iter().any(|&q| (radial(q) - 10.0).abs() > 1e-6) {
                continue;
            }
            let s = (10.0 - radial((a + b + c) / 3.0)) / 10.0;
            if s > worst {
                worst = s;
            }
        }
        worst
    };

    let (mut s, face) = build(Host::Cylinder);
    let plain = sag(&mut s);
    let surf = s.mesh.face_surface(face).unwrap().clone();
    let ((u0, u1), (v0, v1)) = ranges(&surf);
    let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let du = 0.15 * (u1 - u0);
    draw_circle(&mut s, Host::Cylinder, face, at(&surf, um, vm), at(&surf, um + du, vm))
        .expect("circle draws");
    let drawn = sag(&mut s);
    println!("SAG cylinder plain {plain:.4} → after a circle {drawn:.4} (of the radius)");

    assert!(
        plain < 0.02,
        "an undrawn cylinder's wall triangles hug the skin ({plain:.4}) — so the \
         number below is the draw's doing, not the tessellator's"
    );
    // The mechanism, gone: nothing cuts through the solid any more. 5% is ten
    // times the plain cylinder's own sag and a twentieth of what a slab
    // measured (0.6639), so it fails on a slab and passes on a skin.
    assert!(
        drawn < 0.05,
        "a wall triangle sags {drawn:.4} of the radius inward — a slab through \
         the solid, not a patch of its skin (plain reads {plain:.4})"
    );
}
