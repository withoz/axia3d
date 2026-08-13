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
use std::f64::consts::{PI, TAU};

/// The area of the regular n-gon a drill of `radius` actually cuts.
fn ngon_area(radius: f64, segments: u32) -> f64 {
    0.5 * (segments as f64) * radius * radius * (2.0 * PI / segments as f64).sin()
}

/// What one circular bore takes out of a 200³ box: the cylinder, exactly.
///
/// It was not always. The barrel has read its `Cylinder` as an arc since #105
/// while the caps still deducted the n-gon the drill's vertices traced, so the
/// two ends of the same bore answered differently and the removed volume landed
/// between the two models — 1,003,160 against an ideal 1,005,310 at r=40, h=200,
/// 32 segments. A cap now asks the wall across its hole what surface it carries;
/// a drilled bore says `Cylinder`, so the cap deducts πr² and both ends agree.
///
/// `segments` is deliberately still a parameter: the answer must NOT depend on
/// it any more, and a test that stopped passing it would stop saying so.
fn bore_removes(radius: f64, _segments: u32, _half_height: f64, depth: f64) -> f64 {
    PI * radius * radius * depth
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
        // The n-gon the drill traced is now beside the point — the answer is the
        // same at every density, which is the whole claim.
        let prism = 8.0e6 - ngon_area(40.0, segments) * 200.0;
        assert!(
            got < prism,
            "segments={segments}: {got:.4} must remove MORE than the prism {prism:.4}"
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

#[test]
fn a_lying_wall_is_refused_and_the_rim_answers_for_itself() {
    // The refusals matter more than the acceptance: this reads a NEIGHBOUR's
    // declared surface to change a number on THIS face, so every way that
    // reading can be wrong has to end at the polygon instead.

    // 1. A rectangular bore's walls carry no surface at all.
    let mut mesh = boxed();
    mesh.drill_rect_through_hole(
        DVec3::new(-30.0, -30.0, 100.0),
        DVec3::new(30.0, 30.0, 100.0),
        DVec3::Z,
    )
    .expect("rect bore");
    let truth = 8.0e6 - 60.0 * 60.0 * 200.0;
    assert!(
        (mesh.mesh_volume() - truth).abs() < 1e-6,
        "a square hole is a square hole"
    );

    // 2. A cylinder whose radius does not match the loop it is attached to.
    //    Nothing stops someone attaching one; the loop's own vertices do.
    let mut mesh = boxed();
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, 32)
        .expect("bore");
    let honest = mesh.mesh_volume();
    {
        // the acceptance, stated before the refusal
        let d = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
            .map(|(fid, _)| mesh.face_outer_area(fid) - mesh.face_area(fid))
            .fold(0.0_f64, f64::max);
        assert!(
            (d - PI * 1600.0).abs() / (PI * 1600.0) < 1e-9,
            "an honest drilled cap deducts πr² = {:.4}, got {d:.4}",
            PI * 1600.0
        );
    }
    for &fid in &res.tube_faces {
        let Some(AnalyticSurface::Cylinder { axis_origin, axis_dir, ref_dir, u_range, v_range, .. }) =
            mesh.face_surface(fid).cloned()
        else {
            panic!("the drill attaches a cylinder")
        };
        assert!(
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Cylinder {
                    axis_origin,
                    axis_dir,
                    radius: 41.0, // a lie
                    ref_dir,
                    u_range,
                    v_range,
                })
            ),
            "re-attach"
        );
    }
    // Look at the CAP's deduction, not at the whole volume: inflating the
    // radius also inflates the barrel's own flux, and the two move opposite
    // ways. My first version of this asserted on `mesh_volume` and read the
    // barrel's change as the cap refusing.
    let deducted = |m: &Mesh| -> f64 {
        m.faces
            .iter()
            .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
            .map(|(fid, _)| m.face_outer_area(fid) - m.face_area(fid))
            .fold(f64::NAN, |acc, d| if acc.is_nan() { d } else { acc.max(d) })
    };
    let with_the_lie = deducted(&mesh);
    // The lie is refused — the loop's own vertices do not sit at 41. What
    // answers instead is the rim itself: `punch_circular_hole` puts an `Arc`
    // on every rim edge, and since the hole deduction reads arc bulge
    // (step 0, FACE-PIPELINE-PLAN-2026-08-13) that testimony is the TRUE
    // πr², not the 32-gon this test pinned while the instrument was
    // polygon-blind.
    let truth = PI * 1600.0;
    assert!(
        (with_the_lie - truth).abs() / truth < 1e-9,
        "the lie is refused and the rim's own arcs answer: expected {truth:.4}, \
         got {with_the_lie:.4} (πr² for the lie would be {:.4})",
        PI * 41.0 * 41.0
    );
    assert!(
        (with_the_lie - PI * 41.0 * 41.0).abs() > 100.0,
        "the lying wall's radius must never be adopted"
    );
    assert!(honest > 0.0, "the honest bore measured");
}

#[test]
fn a_bare_wall_yields_to_the_rim_arcs_then_to_the_polygon() {
    // A loop can border more than one thing. Strip the surface off a SINGLE
    // tube quad and the WALL route must stop claiming to know what its hole
    // is — 31 cylinders and one unknown is not a circle. The rim's own arcs
    // are a second, closer witness and still answer πr²; only when they are
    // stripped too does the deduction fall all the way back to the polygon,
    // which is what this test pinned while the instrument was polygon-blind.
    let mut mesh = boxed();
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, 32)
        .expect("bore");
    assert!(mesh.set_face_surface(res.tube_faces[0], None), "strip one wall");

    let deducted = |m: &Mesh| -> f64 {
        m.faces
            .iter()
            .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
            .map(|(fid, _)| m.face_outer_area(fid) - m.face_area(fid))
            .fold(0.0_f64, f64::max)
    };
    let truth = PI * 1600.0;
    let with_arcs = deducted(&mesh);
    assert!(
        (with_arcs - truth).abs() / truth < 1e-9,
        "one bare wall stops the wall route, but the rim's arcs still answer \
         {truth:.4} — got {with_arcs:.4}"
    );

    // Now silence the rim too: no wall testimony, no curve testimony —
    // nothing left says "circle", and the polygon is all there is.
    let rims: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
        .flat_map(|(_, f)| f.inners().iter().map(|lr| lr.start).collect::<Vec<_>>())
        .collect();
    for start in rims {
        for he in mesh.collect_loop_hes(start).expect("rim loop") {
            let eid = mesh.hes[he].edge();
            mesh.edges[eid].set_curve(None);
        }
    }
    let bare = deducted(&mesh);
    let polygon = ngon_area(40.0, 32);
    assert!(
        (bare - polygon).abs() / polygon < 1e-9,
        "with no witness at all the cap keeps the polygon {polygon:.4} — got {bare:.4} \
         (πr² would be {:.4})",
        PI * 1600.0
    );
}

#[test]
fn an_oblique_section_keeps_its_polygon() {
    // An oblique cut of a cylinder is an ELLIPSE, and πr² is not its area.
    //
    // ⚠ The first version of this asked the drill for an oblique bore and
    // returned early when it refused — so it asserted NOTHING, and dropping the
    // perpendicularity check left it passing. Built by hand instead: a face in a
    // 45° plane whose hole is a genuine section of a Z-axis cylinder, with walls
    // across that loop carrying the cylinder. Every vertex of the loop really is
    // at radius r from the axis, so only the perpendicularity test can refuse.
    let r = 40.0;
    let n: u32 = 32;
    let mut mesh = Mesh::new();
    // the 45° plane z = y, with normal (0,-1,1)/√2
    let normal = DVec3::new(0.0, -1.0, 1.0).normalize();
    let ellipse: Vec<_> = (0..n)
        .map(|i| {
            let a = TAU * (i as f64) / (n as f64);
            let (x, y) = (r * a.cos(), r * a.sin());
            mesh.add_vertex(DVec3::new(x, y, y)) // on the cylinder AND on z = y
        })
        .collect();
    let outer: Vec<_> = [(-150.0, -150.0), (150.0, -150.0), (150.0, 150.0), (-150.0, 150.0)]
        .iter()
        .map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, y)))
        .collect();
    let host = mesh
        .add_face_with_holes(&outer, &[&ellipse], Default::default())
        .expect("oblique face with an elliptical hole");

    // Walls across the loop, each carrying the cylinder the loop sits on.
    let surface = AnalyticSurface::Cylinder {
        axis_origin: DVec3::ZERO,
        axis_dir: DVec3::Z,
        radius: r,
        ref_dir: DVec3::X,
        u_range: (0.0, TAU),
        v_range: (-200.0, 200.0),
    };
    for i in 0..n as usize {
        let j = (i + 1) % n as usize;
        let a = mesh.vertex_pos(ellipse[i]).unwrap();
        let b = mesh.vertex_pos(ellipse[j]).unwrap();
        let down_a = mesh.add_vertex(a - DVec3::Z * 200.0);
        let down_b = mesh.add_vertex(b - DVec3::Z * 200.0);
        let wall = mesh
            .add_face(&[ellipse[i], ellipse[j], down_b, down_a], Default::default())
            .expect("wall");
        assert!(mesh.set_face_surface(wall, Some(surface.clone())), "attach");
    }

    let deducted = mesh.face_outer_area(host) - mesh.face_area(host);
    assert!(
        (deducted - PI * r * r).abs() > 1.0,
        "an oblique section is an ellipse — πr² = {:.4} must NOT be what is          deducted, got {deducted:.4}",
        PI * r * r
    );
    // What it should be: the loop's own polygon, which for a 45° cut is the
    // circle's polygon stretched by √2 in one direction.
    let polygon = ngon_area(r, n) * 2f64.sqrt();
    assert!(
        (deducted - polygon).abs() / polygon < 1e-6,
        "got {deducted:.4}, the loop's polygon is {polygon:.4}"
    );
}

#[test]
fn walls_that_disagree_are_refused_and_the_rim_answers() {
    // The walls must agree they are ONE cylinder. Give a single tube quad a
    // different radius and the cap must stop claiming to know its hole, even
    // though every other wall still says 40 and the loop still sits at 40.
    let mut mesh = boxed();
    let res = mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, 32)
        .expect("bore");
    let odd = res.tube_faces[7];
    let Some(AnalyticSurface::Cylinder { axis_origin, axis_dir, ref_dir, u_range, v_range, .. }) =
        mesh.face_surface(odd).cloned()
    else {
        panic!("the drill attaches a cylinder")
    };
    assert!(
        mesh.set_face_surface(
            odd,
            Some(AnalyticSurface::Cylinder {
                axis_origin,
                axis_dir,
                radius: 45.0, // one wall out of thirty-two disagrees
                ref_dir,
                u_range,
                v_range,
            })
        ),
        "re-attach"
    );

    let deducted = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && !f.inners().is_empty())
        .map(|(fid, _)| mesh.face_outer_area(fid) - mesh.face_area(fid))
        .fold(0.0_f64, f64::max);
    // The wall route refuses — thirty-one 40s and one 45 is not one cylinder
    // — and the rim's own arcs answer the true πr² instead (they were pinned
    // to the polygon while the instrument was polygon-blind; step 0,
    // FACE-PIPELINE-PLAN-2026-08-13). The 45 must never be adopted.
    let truth = PI * 1600.0;
    assert!(
        (deducted - truth).abs() / truth < 1e-9,
        "disagreeing walls are refused and the rim's arcs answer {truth:.4} — \
         got {deducted:.4}"
    );
    assert!(
        (deducted - PI * 45.0 * 45.0).abs() > 100.0,
        "the disagreeing wall's radius must never be adopted"
    );
}
