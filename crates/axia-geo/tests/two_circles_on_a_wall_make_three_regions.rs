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

    let regions = arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("they cross, so they arrange");
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
    let regions = arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("arrange");
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
        arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("middle")
    };
    let on_seam = {
        // Centred on u = 0, where the chart would be cut by default.
        let a = geodesic_circle(0.0, 10.0, r, 96);
        let b = geodesic_circle(d / R, 10.0, r, 96);
        arrange_two_loops_on_cylinder(&wall(), &a, &b).expect("on the seam")
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
