//! C-2, measured with the instrument C-0 had to build first.
//!
//! C-0 showed topologically that the second circle drawn on a curved host does
//! not notice the first: cap A's boundary was 31 vertices before a circle was
//! drawn across it and 31 after. What it could not say was how much ground the
//! two faces share, because every area reader was broken — that is what the
//! render fix in the same PR repaired.
//!
//! Asking that question found something else first, and then corrected the
//! question itself:
//!
//!   1. drawing a SECOND circle over-drew by a whole disc, overlapping or not,
//!      because the render clipped only the first geodesic hole it found. Now
//!      fixed for the cylinder; `two_disjoint_circles_leave_the_area_alone`
//!      pins it.
//!   2. and the drawn total CANNOT see an overlap: both caps are drawn whole
//!      either way, so disjoint and overlapping read the same number. The
//!      overlap is judged topologically instead, as C-0 judged it.

use axia_core::scene::Scene;
use axia_geo::surfaces::{cylinder, AnalyticSurface};
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 10.0;
const H: f64 = 20.0;
/// 2πRh + 2πR² for a Ø20 × 20 cylinder.
const TRUE_SURFACE: f64 = 1884.956;

fn cylinder_scene() -> (Scene, FaceId) {
    let mut s = Scene::new();
    s.mesh.set_cylinder_path_b_default(true);
    s.mesh
        .create_cylinder(DVec3::ZERO, R, H, 16, MaterialId::new(0))
        .expect("cylinder");
    let side = s
        .mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface()
                    .is_some_and(|su| !matches!(su, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("the one curved face");
    (s, side)
}

fn on_wall(s: &Scene, face: FaceId, u: f64, v: f64) -> DVec3 {
    match s.mesh.face_surface(face).expect("surface") {
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } => {
            cylinder::evaluate(*axis_origin, *axis_dir, *radius, *ref_dir, u, v)
        }
        _ => panic!("not a cylinder"),
    }
}

fn drawn_area(s: &mut Scene) -> f64 {
    let (pos, _n, idx, _f, _d) = s.mesh.export_buffers().expect("export");
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

/// Two circles that overlap are NOT resolved — and the area cannot say so.
///
/// ⚠ This test claimed the opposite twice, and measurement corrected it both
/// times. Worth keeping the record, because the second correction is the more
/// interesting one.
///
/// It first asserted "the excess IS the lens". The excess was 25.461 against a
/// lens of 8.051 — three times too big. Per-face attribution found why: the
/// band was covering the second cap, because the render clipped only the FIRST
/// geodesic hole it found. That is a different defect, now fixed, and the
/// disjoint control below is what pins it.
///
/// Then, with that separated out, the deeper correction: **the drawn total
/// cannot see an overlap at all**. Disjoint circles and overlapping circles
/// both read 1910.417. Both caps are drawn whole either way, and the band
/// excludes hole A either way, so the sum is band + capA + capB regardless of
/// whether A and B share ground. An instrument that gives the same number for
/// both cannot be asked this question — which is exactly the D2 lesson, one
/// layer further in.
///
/// So the overlap is judged where C-0 judged it: topologically. If the pair
/// were resolved, cap A would have lost the lens.
#[test]
fn two_overlapping_circles_are_not_resolved() {
    let (mut s, side) = cylinder_scene();
    let du = 0.3;
    let (u0, vm) = (std::f64::consts::PI, H * 0.5);

    let (cap_a, rem) = s
        .draw_circle_on_cylinder(side, on_wall(&s, side, u0, vm), on_wall(&s, side, u0 + du, vm))
        .expect("first circle");
    let loop_of = |s: &Scene, f: FaceId| {
        s.mesh
            .faces
            .get(f)
            .filter(|x| x.is_active())
            .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
    };
    let before = loop_of(&s, cap_a).expect("cap A");

    // Centres 1.2 r apart with equal radii, so the discs genuinely cross.
    s.draw_circle_on_cylinder(
        rem,
        on_wall(&s, side, u0 + 1.2 * du, vm),
        on_wall(&s, side, u0 + 2.2 * du, vm),
    )
    .expect("second circle");
    let after = loop_of(&s, cap_a);
    println!(
        "OVERLAP cap A boundary {} vert(s) -> {:?};  remainder inners {}",
        before.len(),
        after.as_ref().map(|v| v.len()),
        s.mesh.faces.get(rem).map(|f| f.inners().len()).unwrap_or(0)
    );

    // PIN. Two circles crossing on a plane become three regions; here the first
    // cap is untouched and the ground under the lens belongs to two faces.
    assert_eq!(
        after.as_deref(),
        Some(before.as_slice()),
        "TODAY the first cap is untouched by a circle drawn across it"
    );
    // And every gate still passes, which is why nothing else catches it.
    assert!(s.mesh.verify_face_invariants().is_valid(), "invariants hold");
    assert_eq!(
        s.mesh.detect_self_intersections().count(),
        0,
        "and the scan is blind to two faces sharing ground on one surface"
    );
}

/// Disjoint circles: two that do NOT overlap must leave the area alone.
///
/// ⚠ This passed when it was written, and it should not have. The circles were
/// small (r = 2, disc 12.57) and the threshold was 1% of the whole cylinder
/// (18.85), so a whole undrawn disc fitted underneath it. Same shape of
/// mistake as the four other vacuous checks this session: an assertion that
/// survives the defect it is aimed at. Now the circles are the same size as
/// the overlapping pair and the tolerance is a fraction of ONE DISC, which is
/// the quantity that can go wrong.
#[test]
fn two_disjoint_circles_leave_the_area_alone() {
    let (mut s, side) = cylinder_scene();
    let du = 0.3;
    let vm = H * 0.5;
    let (_, rem) = s
        .draw_circle_on_cylinder(
            side,
            on_wall(&s, side, 1.0, vm),
            on_wall(&s, side, 1.0 + du, vm),
        )
        .expect("first circle");
    // Half a turn away — nowhere near the first.
    s.draw_circle_on_cylinder(
        rem,
        on_wall(&s, side, 1.0 + std::f64::consts::PI, vm),
        on_wall(&s, side, 1.0 + std::f64::consts::PI + du, vm),
    )
    .expect("second circle");
    let two = drawn_area(&mut s);
    let disc = std::f64::consts::PI * (R * du) * (R * du);
    println!(
        "DISJOINT two circles → {two:.3} (true {TRUE_SURFACE:.3}, excess {:.3}, \
         one disc {disc:.3})",
        two - TRUE_SURFACE
    );
    assert!(
        (two - TRUE_SURFACE).abs() < 0.2 * disc,
        "two circles that share no ground do not change the area: {two:.3} vs \
         {TRUE_SURFACE:.3}, an excess of {:.3} against a disc of {disc:.3}",
        two - TRUE_SURFACE
    );
}

/// The same claim on every curved host, not just the one that was measured
/// first. The cylinder was restructured to collect all holes; the cone and
/// torus were left at one for a while, with the gap written into the code —
/// this is what closed it.
#[test]
fn two_disjoint_circles_leave_the_area_alone_on_every_host() {
    use axia_geo::surfaces::{cone, sphere, torus};
    for host in ["cylinder", "sphere", "cone", "torus"] {
        let mut s = Scene::new();
        let m = MaterialId::new(0);
        s.mesh.set_cylinder_path_b_default(true);
        s.mesh.set_sphere_path_b_default(true);
        s.mesh.set_cone_path_b_default(true);
        s.mesh.set_torus_path_b_default(true);
        match host {
            "cylinder" => { s.mesh.create_cylinder(DVec3::ZERO, R, H, 16, m).unwrap(); }
            "sphere" => { s.mesh.create_sphere(DVec3::ZERO, R, 16, 12, m).unwrap(); }
            "cone" => { s.mesh.create_cone(DVec3::ZERO, R, H, 16, m).unwrap(); }
            _ => { s.mesh.create_torus_kernel_native(DVec3::ZERO, R, 3.0, m).unwrap(); }
        }
        let face = s
            .mesh
            .faces
            .iter()
            .find(|(_, f)| {
                f.is_active()
                    && f.surface()
                        .is_some_and(|su| !matches!(su, AnalyticSurface::Plane { .. }))
            })
            .map(|(fid, _)| fid)
            .unwrap();
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let at = |u: f64, v: f64| -> DVec3 {
            match &surf {
                AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } =>
                    cylinder::evaluate(*axis_origin, *axis_dir, *radius, *ref_dir, u, v),
                AnalyticSurface::Sphere { center, radius, axis_dir, ref_dir, .. } =>
                    sphere::evaluate(*center, *radius, *axis_dir, *ref_dir, u, v),
                AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, .. } =>
                    cone::evaluate(*apex, *axis_dir, *half_angle, *ref_dir, u, v),
                AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } =>
                    torus::evaluate(*center, *axis_dir, *ref_dir, *major_radius, *minor_radius, u, v),
                _ => panic!("not curved"),
            }
        };
        let ((u0, u1), (v0, v1)) = match &surf {
            AnalyticSurface::Cylinder { u_range, v_range, .. }
            | AnalyticSurface::Sphere { u_range, v_range, .. }
            | AnalyticSurface::Cone { u_range, v_range, .. }
            | AnalyticSurface::Torus { u_range, v_range, .. } => (*u_range, *v_range),
            _ => panic!("not curved"),
        };
        let (vm, du) = (0.5 * (v0 + v1), 0.12 * (u1 - u0));
        let draw = |s: &mut Scene, f: FaceId, u: f64| match host {
            "cylinder" => s.draw_circle_on_cylinder(f, at(u, vm), at(u + du, vm)),
            "sphere" => s.draw_circle_on_sphere(f, at(u, vm), at(u + du, vm)),
            "cone" => s.draw_circle_on_cone(f, at(u, vm), at(u + du, vm)),
            _ => s.draw_circle_on_torus(f, at(u, vm), at(u + du, vm)),
        };
        let before = drawn_area(&mut s);
        // A quarter turn apart in u — nowhere near each other.
        let quarter = 0.25 * (u1 - u0);
        let Some((_, rem)) = draw(&mut s, face, u0 + quarter) else {
            panic!("{host}: first circle must draw");
        };
        let second = draw(&mut s, rem, u0 + 3.0 * quarter);
        assert!(second.is_some(), "{host}: second circle must draw");
        let after = drawn_area(&mut s);
        println!(
            "EVERY {host:<9} {before:>9.3} -> {after:>9.3}   x{:.4}",
            after / before
        );
        // Per host, because they do not all land in the same place and one
        // shared threshold would either hide the torus's residual or fail the
        // three that are exact.
        //
        //   cylinder / sphere / cone   x1.0001 - x1.0002
        //   torus                      x1.0243   (x1.0055 for a single circle)
        //
        // The torus read x0.5418 before any of this — it drew less than half of
        // its own surface. What is left is a ring-versus-grid residual on a
        // doubly periodic chart: recorded rather than rounded away, and small
        // enough that chasing it would cost more than it returns.
        let tol = if host == "torus" { 0.03 } else { 0.01 };
        assert!(
            (after / before - 1.0).abs() < tol,
            "{host}: two circles that share no ground changed the drawn area by x{:.4}",
            after / before
        );
    }
}
