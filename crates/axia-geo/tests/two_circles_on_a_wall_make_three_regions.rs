//! C-2's geometry half, checked against closed form.
//!
//! Two geodesic circles that cross on a cylinder wall divide it into three
//! regions. The cylinder is developable, so unrolling it is a local isometry —
//! lengths, angles and areas all survive — and a geodesic circle on the wall is
//! a Euclidean circle on the chart. That means the answer is not "close
//! enough": it is the same answer trigonometry gives for two circles in a
//! plane, and this file asks for exactly that.
//!
//! For equal radii `r` with centres `d` apart:
//!
//! ```text
//!   lens   = 2r²·acos(d/2r) − (d/2)·√(4r² − d²)
//!   A-only = B-only = πr² − lens
//!   union  = 2πr² − lens
//! ```
//!
//! Nothing here touches the mesh. The engine still leaves two overlapping caps
//! covering the same ground (pinned in
//! `two_circles_on_a_curved_host_share_ground.rs`); this is what the fix will
//! be built on, verified before it is relied on.

use axia_geo::operations::curved_arrange::{arrange_two_loops_on_cylinder, Share};
use axia_geo::surfaces::{cylinder, AnalyticSurface};
use glam::DVec3;

const R: f64 = 10.0;

fn wall() -> AnalyticSurface {
    AnalyticSurface::Cylinder {
        axis_origin: DVec3::ZERO,
        axis_dir: DVec3::Z,
        radius: R,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (0.0, 20.0),
    }
}

/// A geodesic circle of arc-length radius `r`, centred at `(u0, v0)`.
///
/// On the unrolled chart this is a Euclidean circle; mapping each of its points
/// back to the wall gives the geodesic circle, which is what the draw tools
/// produce.
fn geodesic_circle(u0: f64, v0: f64, r: f64, n: usize) -> Vec<DVec3> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * (i as f64) / (n as f64);
            let du = (r * t.cos()) / R;
            let dv = r * t.sin();
            cylinder::evaluate(DVec3::ZERO, DVec3::Z, R, DVec3::X, u0 + du, v0 + dv)
        })
        .collect()
}

#[test]
fn two_crossing_circles_give_three_regions_of_the_right_size() {
    let (r, d) = (3.0_f64, 3.6_f64); // centres 1.2 r apart, so they cross
    let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 96);
    let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 96);

    let regions = arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("they cross, so they arrange").regions;
    assert_eq!(regions.len(), 3, "A-only, the lens, B-only");

    let lens_exact =
        2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt();
    let disc = std::f64::consts::PI * r * r;
    let of = |s: Share| regions.iter().find(|x| x.share == s).expect("region").area;
    let (a_only, lens, b_only) = (of(Share::FirstOnly), of(Share::Both), of(Share::SecondOnly));
    println!(
        "REGIONS a-only {a_only:.4} (exact {:.4})  lens {lens:.4} (exact {lens_exact:.4})  \
         b-only {b_only:.4}",
        disc - lens_exact
    );

    // 1% each — the loops are 96-gons, so the deficit is the inscribed polygon's
    // and nothing else. A wrong arrangement is out by tens of percent, not one.
    assert!(
        (lens - lens_exact).abs() < 0.01 * lens_exact,
        "the lens is {lens:.4}, trigonometry says {lens_exact:.4}"
    );
    for (name, got) in [("A-only", a_only), ("B-only", b_only)] {
        assert!(
            (got - (disc - lens_exact)).abs() < 0.01 * disc,
            "{name} is {got:.4}, should be a disc minus the lens ({:.4})",
            disc - lens_exact
        );
    }
    // And they tile the union rather than merely each being right.
    let union_exact = 2.0 * disc - lens_exact;
    assert!(
        (a_only + lens + b_only - union_exact).abs() < 0.01 * union_exact,
        "the three sum to the union: {:.4} vs {union_exact:.4}",
        a_only + lens + b_only
    );
}

/// Every boundary point comes back ON the wall — the chart is a round trip, not
/// a projection that loses the surface.
#[test]
fn the_regions_come_back_onto_the_wall() {
    let (r, d) = (3.0_f64, 3.6_f64);
    let a = geodesic_circle(1.0, 10.0, r, 64);
    let b = geodesic_circle(1.0 + d / R, 10.0, r, 64);
    let regions = arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("arrange").regions;
    for reg in &regions {
        assert!(reg.boundary.len() >= 3, "a region has a boundary");
        for p in &reg.boundary {
            let radial = (p.x * p.x + p.y * p.y).sqrt();
            assert!(
                (radial - R).abs() < 1e-6,
                "{:?} is {radial:.6} from the axis, not {R}",
                p
            );
        }
    }
}

/// Circles that do NOT cross are refused rather than rearranged into
/// something. Disjoint and nested are cases the caller already handles, and a
/// function that returned three regions for them would be lying.
#[test]
fn circles_that_do_not_cross_are_refused() {
    let far = (
        geodesic_circle(1.0, 10.0, 3.0, 64),
        geodesic_circle(1.0 + std::f64::consts::PI, 10.0, 3.0, 64),
    );
    assert!(
        arrange_two_loops_on_cylinder(&wall(), &far.0, &far.1).is_none(),
        "half a turn apart: nothing to arrange"
    );
    let nested = (
        geodesic_circle(1.0, 10.0, 3.0, 64),
        geodesic_circle(1.0, 10.0, 1.0, 64),
    );
    assert!(
        arrange_two_loops_on_cylinder(&wall(), &nested.0, &nested.1).is_none(),
        "one inside the other: containment, not a crossing"
    );
}

/// The seam is chosen to miss both loops, so a pair sitting ON the chart's
/// natural cut arranges exactly as one in the middle does.
///
/// This is the trap the render's band clipper fell into twice, and the reason
/// the seam here is picked by asking which angle is furthest from every loop
/// point rather than by any cleverer-looking rule.
#[test]
fn a_pair_straddling_the_seam_arranges_the_same() {
    let (r, d) = (3.0_f64, 3.6_f64);
    let middle = {
        let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 96);
        let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 96);
        arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("middle").regions
    };
    let on_seam = {
        // Centred on u = 0, where the chart would be cut by default.
        let a = geodesic_circle(0.0, 10.0, r, 96);
        let b = geodesic_circle(d / R, 10.0, r, 96);
        arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("on the seam").regions
    };
    let area_of = |v: &[axia_geo::operations::curved_arrange::CurvedRegion], s: Share| {
        v.iter().find(|x| x.share == s).expect("region").area
    };
    for s in [Share::FirstOnly, Share::Both, Share::SecondOnly] {
        let (m, o) = (area_of(&middle, s), area_of(&on_seam, s));
        assert!(
            (m - o).abs() < 0.01 * m,
            "{s:?}: {m:.4} in the middle, {o:.4} on the seam"
        );
    }
}

/// The fact the materializer stands on: after a circle splits the wall, the
/// cap's rim IS part of the host's boundary — same vertices, because the two
/// faces are wired through twin half-edges.
///
/// Written because the materializer kept being told "chain start vert 50 not on
/// any loop of face 4", and that claim had to be checked rather than assumed.
#[test]
fn the_cap_rim_is_also_the_hosts_hole() {
    use axia_geo::MaterialId;
    let mut mesh = axia_geo::Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    let a = geodesic_circle(std::f64::consts::PI, 10.0, 3.0, 48);
    let (cap, host) = mesh.split_cylinder_face_by_circle(side, &a).expect("split");

    let cap_rim: Vec<_> = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("cap rim");
    let host_f = mesh.faces.get(host).expect("host");
    let mut host_loops: Vec<Vec<_>> = vec![mesh
        .collect_loop_verts(host_f.outer().start)
        .expect("host outer")];
    for l in host_f.inners() {
        host_loops.push(mesh.collect_loop_verts(l.start).expect("host inner"));
    }
    println!(
        "TWIN cap rim {} verts; host loops {:?}; host id {host:?}, cap id {cap:?}",
        cap_rim.len(),
        host_loops.iter().map(|l| l.len()).collect::<Vec<_>>()
    );
    let shared = host_loops
        .iter()
        .any(|l| l.len() == cap_rim.len() && cap_rim.iter().all(|v| l.contains(v)));
    assert!(
        shared,
        "the cap's rim must be one of the host's loops — otherwise a chain that \
         starts on the rim cannot split the host"
    );
}

/// Cutting B where it meets A: two crossings, and the two arcs between them.
///
/// The crossing is taken ON A's edge, not on the surface, and those are not the
/// same place — a rim edge is a 3D chord and the cylinder stands off it by the
/// sagitta. For a 48-gon of radius 3 on R = 10 that gap is 1.9 µm against a
/// 1.5 µm dedup floor (LOCKED #5): just far enough that a crossing computed on
/// the surface lands beside the rim instead of on it. This asserts which of the
/// two it is.
#[test]
fn the_second_circle_is_cut_where_it_meets_the_first() {
    use axia_geo::operations::curved_arrange::crossing_on_cylinder;
    let (r, d) = (3.0_f64, 3.6_f64);
    let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 48);
    let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 48);
    let x = crossing_on_cylinder(&wall(), &a, &b).expect("they cross");

    // Each crossing sits on the segment of A it says it does.
    for k in 0..2 {
        let (q0, q1) = (a[x.seg[k]], a[(x.seg[k] + 1) % a.len()]);
        let span = q1 - q0;
        let t = (x.points[k] - q0).dot(span) / span.length_squared();
        let off = (q0 + span * t - x.points[k]).length();
        println!("CUT crossing {k} on segment {} at t={t:.4}, off by {off:.3e}", x.seg[k]);
        assert!((0.0..=1.0).contains(&t), "t={t} is not within the segment");
        assert!(off < 1e-12, "the point is {off:.3e} off the chord, not on it");
    }
    // Both arcs start and end at the crossings, and between them they are the
    // whole of B plus the two endpoints counted twice.
    for arc in [&x.inside, &x.outside] {
        assert!((arc[0] - x.points[0]).length() < 1e-9, "an arc starts at a crossing");
        assert!(
            (arc[arc.len() - 1] - x.points[1]).length() < 1e-9,
            "and ends at the other"
        );
    }
    assert_eq!(
        x.inside.len() + x.outside.len(),
        b.len() + 4,
        "every point of B is on one arc or the other, with both crossings on both"
    );
    // The inside arc really is inside: it is the shorter way for circles this
    // close, and every one of its interior points is nearer A's centre.
    let centre_a = geodesic_circle(std::f64::consts::PI, 10.0, 0.0, 1)[0];
    let far = x.inside[1..x.inside.len() - 1]
        .iter()
        .map(|p| (*p - centre_a).length())
        .fold(0.0_f64, f64::max);
    assert!(far < r * 1.02, "the inside arc stays within A ({far:.4} vs {r})");
}
