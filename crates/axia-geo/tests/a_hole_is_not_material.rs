//! A hole is not material — the volume must take it out.
//!
//! `face_outward_flux` fanned the OUTER loop and stopped there, so every face
//! with a window through it handed over the flux of a face that was never
//! there. The hole-aware reader (`analytic_face_flux`, via `face_area`) sits
//! after that branch, so a planar face with a polygon never reached it.
//!
//! Measured on a 200³ box with a 60×60 through-hole: **7,520,000** reported
//! against a truth of 7,280,000. The excess was 240,000.0 against a prediction
//! of 240,000.0 — two caps × |p·n| × hole area ÷ 3 — matching to the last
//! digit, which is what identified the cause.

use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use glam::DVec3;
use std::f64::consts::PI;

/// The area of the regular n-gon a drill of `radius` actually cuts.
fn ngon_area(radius: f64, segments: u32) -> f64 {
    0.5 * (segments as f64) * radius * radius * (2.0 * PI / segments as f64).sin()
}

/// What one circular bore takes out of a 200³ box — which is NOT the prism the
/// drill's vertices trace, and not the ideal cylinder either.
///
/// The two ends of the same bore are read by different rules, and both rules are
/// the engine's own. The caps are PLANAR, so their hole is the n-gon the drill
/// actually cut. The barrel is CURVED, so since #105 it reads its `Cylinder` and
/// measures as an arc. The removed volume is therefore
///
///     [ 2·|p·n|_cap·A_ngon  +  2πr²h ] / 3
///
/// ⚠ This is a seam, not a law. Measured at r=40, h=200, 32 segments: the ideal
/// cylinder removes 1,005,310, this removes 1,003,160 (−0.21%), and the pure
/// prism removed 998,862 (−0.64%) — so it is three times closer than before and
/// still not exact. Closing it means letting a planar cap deduct the CIRCLE when
/// its hole loop is the rim of a cylinder standing perpendicular to it, at which
/// point this helper becomes `PI * r * r * h` and these tests say so by failing.
fn bore_removes(radius: f64, segments: u32, half_height: f64, depth: f64) -> f64 {
    let caps = 2.0 * half_height * ngon_area(radius, segments);
    let barrel = 2.0 * PI * radius * radius * depth;
    (caps + barrel) / 3.0
}

fn boxed() -> Mesh {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    mesh
}

#[test]
fn a_rectangular_through_hole_comes_out_of_the_volume() {
    let mut mesh = boxed();
    mesh.drill_rect_through_hole(
        DVec3::new(-30.0, -30.0, 100.0),
        DVec3::new(30.0, 30.0, 100.0),
        DVec3::Z,
    )
    .expect("bore");
    let truth = 8.0e6 - 60.0 * 60.0 * 200.0;
    let got = mesh.mesh_volume();
    assert!(
        (got - truth).abs() < 1e-6,
        "got {got:.4}, truth {truth:.4} (the old answer was 7_520_000)"
    );
}

#[test]
fn a_circular_through_hole_comes_out_at_every_density() {
    // The drill cuts an n-gon, not a circle, so the truth follows the segment
    // count — which also keeps this from passing on a coincidence.
    for segments in [8u32, 16, 32, 64] {
        let mut mesh = boxed();
        mesh.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, segments)
            .expect("bore");
        let want = 8.0e6 - bore_removes(40.0, segments, 100.0, 200.0);
        let got = mesh.mesh_volume();
        assert!(
            (got / want - 1.0).abs() < 1e-9,
            "segments={segments}: got {got:.4}, want {want:.4}"
        );
        // And it must sit between the two models it is made of.
        let prism = 8.0e6 - ngon_area(40.0, segments) * 200.0;
        let ideal = 8.0e6 - PI * 40.0 * 40.0 * 200.0;
        assert!(
            got < prism && got > ideal,
            "segments={segments}: {got:.4} should lie between the prism {prism:.4}              and the ideal cylinder {ideal:.4}"
        );
    }
}

#[test]
fn two_holes_in_one_face_both_come_out() {
    // Both bores punch the SAME two caps, so each cap ends up carrying two
    // inner loops — the case a fix that only ever looked at `inners()[0]`
    // would get half right.
    let mut mesh = boxed();
    for x in [-60.0, 60.0] {
        mesh.drill_circular_through_hole(DVec3::new(x, 0.0, 100.0), DVec3::Z, 20.0, 32)
            .expect("bore");
    }
    let caps_with_two: usize = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && f.inners().len() == 2)
        .count();
    assert_eq!(caps_with_two, 2, "both caps should carry two holes");

    let want = 8.0e6 - 2.0 * bore_removes(20.0, 32, 100.0, 200.0);
    let got = mesh.mesh_volume();
    assert!(
        (got / want - 1.0).abs() < 1e-9,
        "got {got:.4}, want {want:.4}"
    );
}

#[test]
fn a_solid_without_holes_is_untouched() {
    // The control: the fan is the answer whenever there is nothing to take out,
    // and it must stay bit-for-bit the answer it always was.
    let mesh = boxed();
    assert!((mesh.mesh_volume() - 8.0e6).abs() < 1e-6);

    let mut mesh = Mesh::new();
    mesh.create_sphere(DVec3::ZERO, 50.0, 32, 32, Default::default()).expect("sphere");
    let truth = 4.0 / 3.0 * PI * 50f64.powi(3);
    assert!((mesh.mesh_volume() / truth - 1.0).abs() < 0.002);

    let mut mesh = Mesh::new();
    mesh.create_cylinder(DVec3::ZERO, 40.0, 100.0, 64, Default::default()).expect("cylinder");
    let truth = PI * 40.0 * 40.0 * 100.0;
    assert!((mesh.mesh_volume() / truth - 1.0).abs() < 0.002);
}

#[test]
fn a_path_b_cylinder_never_reaches_this_branch_at_all() {
    // A Path B cylinder is ONE side face whose two loops are the tube's RIMS
    // (ADR-094), not holes — deducting them would take 2πrh down to 2πrh − πr².
    //
    // What keeps it safe is NOT the curved check, which is what I first claimed
    // and mutation refuted: forcing every face down the fan left this passing.
    // It is the outer loop. A rim-bounded face has ONE anchor vertex, and the
    // fan needs three, so it never gets there. Both facts are asserted, because
    // the volume alone would not have told me which one was doing the work.
    let mut mesh = Mesh::new();
    mesh.set_cylinder_path_b_default(true);
    mesh.create_cylinder(DVec3::ZERO, 40.0, 100.0, 64, Default::default()).expect("cylinder");

    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| f.is_active() && !f.inners().is_empty())
        .map(|(fid, _)| fid)
        .expect("the Path B side face carries its rims as inner loops");
    let outer_verts = mesh
        .collect_loop_verts(mesh.faces.get(side).unwrap().outer().start)
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(
        outer_verts < 3,
        "a rim is an anchor, not a polygon — {outer_verts} vertices would put          this face into the fan, where its rims would read as holes"
    );

    let truth = PI * 40.0 * 40.0 * 100.0;
    let got = mesh.mesh_volume();
    assert!(
        (got / truth - 1.0).abs() < 0.002,
        "got {got:.4}, truth {truth:.4} — a rim was treated as a hole"
    );
}

#[test]
fn a_window_in_a_curved_wall_still_over_reports() {
    // A KNOWN limitation, pinned so it is noticed rather than rediscovered.
    // Punching a hole in a Path A cylinder facet makes the one shape that has a
    // curved surface, a real polygon AND a hole. It takes the surface branch,
    // whose Cylinder arm integrates u_range × v_range and never hears about the
    // hole — so the wall still measures as if the window were not there.
    //
    // Telling that inner loop from a Path B rim cannot be done from the loops
    // alone (`face_area` says so and paid for it), so this is left alone rather
    // than guessed at. The assert records today's answer: if a later change
    // makes it right, this test is what says so out loud.
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_cylinder(DVec3::ZERO, 40.0, 100.0, 16, Default::default())
        .expect("cylinder");
    let side = faces
        .iter()
        .copied()
        .find(|&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Cylinder { .. })))
        .expect("a side facet");
    let pts = mesh.face_outline_points(side).expect("facet outline");
    let c: DVec3 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
    let n = (pts[1] - pts[0]).cross(pts[2] - pts[0]).normalize();

    let before = mesh.mesh_volume();
    let host = mesh.punch_circular_hole(c, n, 3.0, 12).expect("window");
    assert_eq!(mesh.faces.get(host).unwrap().inners().len(), 1, "the window is a hole loop");
    assert!(
        (mesh.mesh_volume() - before).abs() < 1e-9,
        "unchanged today — the curved arm does not see the hole"
    );
}
