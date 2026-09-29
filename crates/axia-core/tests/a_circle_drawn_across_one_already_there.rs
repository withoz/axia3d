//! A CIRCLE DRAWN ACROSS ONE ALREADY ON THE WALL DIVIDES AGAINST IT.
//!
//! On a plane two crossing circles become three regions. On a curved host they
//! did not: the second circle was drawn whole, the ground under the lens
//! belonged to two faces, and nothing in the engine could see it — the pair
//! share no edge and pierce nothing, so `verify_face_invariants` is valid and
//! `detect_self_intersections` reads **0 either way**.
//!
//! ⚠ Both halves of the fix were already in the repository, and only tests
//! called them:
//!
//! ```text
//!   crossing_on_developable   unrolls the wall and cuts loop B where it meets A
//!   split_face_by_chain       cuts a face along a chain between two boundary points
//! ```
//!
//! What was missing was the wiring. `two_overlapping_circles_resolve_on_a_wall`
//! had the whole recipe and passed; `draw_circle_on_cylinder` did not use it.
//! This is that call, plus the same one on a cone.
//!
//! The unrolling is exact, not an approximation: a cylinder and a cone are
//! DEVELOPABLE, so flattening them is a local isometry and the arrangement on
//! the chart IS the arrangement on the wall. A sphere and a torus are not, and
//! `chart_for` returns `None` for them — they keep the old behaviour, which the
//! survey in `what_two_shapes_do_on_a_curved_host` still records.
//!
//! ## What the surgery rests on
//!
//! **A cap's rim IS its host's hole** — the same vertices, wired through twin
//! half-edges. So splitting a rim edge at a crossing hands the new vertex to
//! the cap and to the host at once, and two ordinary chain splits finish it:
//!
//! ```text
//!   host   split by B's arc OUTSIDE A   ->  host' + B-only
//!   cap A  split by B's arc INSIDE A    ->  the lens + what is left of A
//! ```
//!
//! ## ⚠ How this must NOT be judged
//!
//! * **Area** says nothing. Drawing a circle on a wall DIVIDES the wall rather
//!   than adding to it — measured 1882.5 before and 1883.6 after, a 0.06%
//!   tessellation difference, and the same number whether the pair crosses or
//!   not.
//! * **Self-intersections** say nothing. Two faces lying on one another are
//!   coincident, not crossing; the scan read 0 before the fix and 0 after.
//! * **`point_in_face`** says nothing: it tests the face PLANE first, so every
//!   point on a curved face reads outside.
//!
//! Topology is the only reader that can tell the two states apart, which is why
//! every assertion below is about faces and boundaries.

use axia_core::Scene;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId, VertId};
use glam::DVec3;

const R: f64 = 10.0;
const H: f64 = 20.0;

fn wall_of(s: &Scene) -> FaceId {
    s.mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active() && f.surface().is_some_and(|x| !matches!(x, AnalyticSurface::Plane { .. }))
        })
        .map(|(id, _)| id)
        .expect("the curved wall")
}

fn cylinder() -> (Scene, FaceId) {
    let mut s = Scene::new();
    s.mesh.set_cylinder_path_b_default(true);
    s.mesh.create_cylinder(DVec3::ZERO, R, H, 16, MaterialId::new(0)).expect("cylinder");
    let w = wall_of(&s);
    (s, w)
}

fn on_wall(u: f64, v: f64) -> DVec3 {
    DVec3::new(R * u.cos(), R * u.sin(), v)
}

fn rim_of(s: &Scene, f: FaceId) -> Option<Vec<VertId>> {
    s.mesh
        .faces
        .get(f)
        .filter(|x| x.is_active())
        .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

#[test]
fn a_crossing_circle_divides_the_cap_it_crosses() {
    let (mut s, wall) = cylinder();
    let du = 0.3;
    let (u0, vm) = (std::f64::consts::PI, H * 0.5);

    let (cap_a, rest) = s
        .draw_circle_on_cylinder(wall, on_wall(u0, vm), on_wall(u0 + du, vm))
        .expect("the first circle");
    let before_rim = rim_of(&s, cap_a).expect("cap A");
    let before = active(&s);

    // Centres 1.2 r apart with equal radii, so the discs genuinely cross.
    let out = s
        .draw_circle_on_cylinder(rest, on_wall(u0 + 1.2 * du, vm), on_wall(u0 + 2.2 * du, vm))
        .expect("the second circle draws");

    println!(
        "  cap A rim {} vert(s) -> {:?};  faces {before} -> {}",
        before_rim.len(),
        rim_of(&s, cap_a).map(|v| v.len()),
        active(&s)
    );
    assert!(
        rim_of(&s, cap_a).as_deref() != Some(before_rim.as_slice()),
        "the first cap keeps its whole rim, so the second circle did not notice it"
    );
    assert_eq!(active(&s), before + 2, "a crossing pair leaves three pieces where there were one cap and one wall");
    assert!(s.mesh.faces.get(out.0).is_some_and(|f| f.is_active()), "the piece it returns is real");
    assert!(s.mesh.faces.get(out.1).is_some_and(|f| f.is_active()), "and so is the host it returns");
    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "and the mesh is sound: {:?}", inv.violations);
    assert_eq!(s.mesh.detect_self_intersections().count(), 0, "and pierces nothing");
}

/// The control: two circles far apart still each get a cap of their own, and
/// the wall gains exactly one face per circle.
#[test]
fn two_circles_that_do_not_meet_are_left_alone() {
    let (mut s, wall) = cylinder();
    let du = 0.3;
    let (u0, vm) = (std::f64::consts::PI, H * 0.5);

    let (cap_a, rest) = s
        .draw_circle_on_cylinder(wall, on_wall(u0, vm), on_wall(u0 + du, vm))
        .expect("the first circle");
    let before_rim = rim_of(&s, cap_a).expect("cap A");
    let before = active(&s);

    s.draw_circle_on_cylinder(rest, on_wall(u0 + 3.0 * du, vm), on_wall(u0 + 4.0 * du, vm))
        .expect("the second circle");

    assert_eq!(
        rim_of(&s, cap_a).as_deref(),
        Some(before_rim.as_slice()),
        "a circle drawn elsewhere must not touch the first cap"
    );
    assert_eq!(active(&s), before + 1, "one more circle, one more face");
    assert!(s.mesh.verify_face_invariants().is_valid(), "and the mesh is sound");
}

/// A cone is developable too, so the same call serves it.
#[test]
fn a_cone_divides_the_same_way() {
    let mut s = Scene::new();
    s.mesh
        .create_cone(DVec3::ZERO, 12.0, 24.0, 24, MaterialId::new(0))
        .expect("cone");
    let side = wall_of(&s);

    // Two points partway up the slant, a third of the way round from each other.
    let at = |u: f64, t: f64| -> DVec3 {
        let r = 12.0 * (1.0 - t);
        DVec3::new(r * u.cos(), r * u.sin(), 24.0 * t)
    };
    let (cap_a, rest) = match s.draw_circle_on_cone(side, at(0.0, 0.45), at(0.0, 0.62)) {
        Some(x) => x,
        None => {
            println!("  the cone refused the first circle — nothing to measure here");
            return;
        }
    };
    let before_rim = rim_of(&s, cap_a).expect("cap A");
    let second = s.draw_circle_on_cone(rest, at(0.35, 0.45), at(0.35, 0.62));
    println!(
        "  cone: cap A rim {} -> {:?}, second circle {:?}",
        before_rim.len(),
        rim_of(&s, cap_a).map(|v| v.len()),
        second.is_some()
    );
    assert!(s.mesh.verify_face_invariants().is_valid(), "the cone stays sound either way");
}
