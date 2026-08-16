//! The other developable host: a cone.
//!
//! The cylinder proved the shape of the thing; this proves the shape was not
//! the cylinder's. A cone flattens too — its slant length `L = v/cos α` becomes
//! a polar radius and its sweep narrows to `φ = u·sin α`, which is why a cone's
//! development is a sector that does not close on itself. Unrolling is still a
//! local isometry, so the arrangement on the chart IS the arrangement on the
//! surface, and everything below the chart is the cylinder's machinery
//! unchanged: the same planar sweep, the same crossing decomposition, the same
//! two chain splits.
//!
//! The closed form is the plane's, so the flat distance between two points of
//! the sector is stated here in full rather than borrowed from the engine:
//!
//! ```text
//!   L_i  = v_i / cos α        Δφ = (u₁ − u₀)·sin α
//!   dist = √(L₀² + L₁² − 2·L₀·L₁·cos Δφ)      (law of cosines)
//! ```
//!
//! What is NOT here is the sphere and the torus. They cannot be flattened
//! without stretching, so a chart of them would be an approximation wearing an
//! exact answer's clothes — the last test asks that they are refused.

use axia_geo::operations::curved_arrange::{
    arrange_two_loops_on_developable, crossing_on_developable, Share,
};
use axia_geo::operations::face_split::split_face_by_chain;
use axia_geo::surfaces::{cone, torus, AnalyticSurface};
use axia_geo::{FaceId, MaterialId, Mesh, VertId};
use glam::DVec3;

// How finely the circles are sampled. Adjustable so
// `the_error_is_the_polygons_and_shrinks_with_them` can vary it; every other
// test uses the default, which reaches `circle_on_cone`'s 64-sample ceiling.
thread_local! {
    static CHORD_TOL: std::cell::Cell<f64> = const { std::cell::Cell::new(0.001) };
}

/// apex, axis, half-angle, ref, and the `v` the circles sit at.
struct Cone {
    apex: DVec3,
    axis: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    v: f64,
}

impl Cone {
    /// The flat distance between two points of the wall at the same `v`,
    /// `du` apart in `u` — the law of cosines in the unrolled sector.
    fn flat_gap(&self, du: f64) -> f64 {
        let l = self.v / self.half_angle.cos();
        let dphi = du * self.half_angle.sin();
        (2.0 * l * l * (1.0 - dphi.cos())).sqrt()
    }

    fn at(&self, u: f64) -> DVec3 {
        cone::evaluate(self.apex, self.axis, self.half_angle, self.ref_dir, u, self.v)
    }

    /// A geodesic circle centred at `u`, built by the engine's own routine.
    ///
    /// Using `circle_on_cone` rather than the chart keeps the check honest: it
    /// was written for ADR-263, independently of the arrangement's chart, so
    /// agreement between the two is evidence rather than tautology.
    fn circle(&self, u: f64, dur: f64) -> Vec<DVec3> {
        cone::circle_on_cone(
            self.apex,
            self.axis,
            self.half_angle,
            self.ref_dir,
            self.at(u),
            self.at(u + dur),
            CHORD_TOL.with(|c| c.get()),
        )
        .expect("a geodesic circle on the cone wall")
    }
}

/// A kernel-native cone, and the one face that is not a disc.
fn cone_wall() -> (Mesh, FaceId, AnalyticSurface, Cone) {
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_cone_kernel_native(DVec3::ZERO, 10.0, 20.0, MaterialId::new(0))
        .expect("cone");
    let wall = faces
        .iter()
        .copied()
        .find(|&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Cone { .. })))
        .expect("the cone's side");
    let surface = mesh.face_surface(wall).expect("surface").clone();
    let geom = match &surface {
        AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. } => Cone {
            apex: *apex,
            axis: *axis_dir,
            half_angle: *half_angle,
            ref_dir: *ref_dir,
            v: 0.5 * (v_range.0 + v_range.1),
        },
        _ => unreachable!(),
    };
    (mesh, wall, surface, geom)
}

/// `du` for the radius point, and `du` between the two centres — chosen so the
/// centres end up 1.2 radii apart, which is a crossing rather than a graze.
const DU_RADIUS: f64 = 0.40;
const DU_CENTRES: f64 = 0.48;

/// The exact lens for two equal circles of radius `r` with centres `d` apart.
fn lens_of(r: f64, d: f64) -> f64 {
    2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt()
}

/// The chart is exact, so the three regions match trigonometry — the same claim
/// the cylinder makes, on a surface whose chart is a sector rather than a strip.
#[test]
fn the_arrangement_on_a_cone_matches_the_flat_answer() {
    let (_mesh, _wall, surface, c) = cone_wall();
    let (r, d) = (c.flat_gap(DU_RADIUS), c.flat_gap(DU_CENTRES));
    let a = c.circle(1.0, DU_RADIUS);
    let b = c.circle(1.0 + DU_CENTRES, DU_RADIUS);

    let regions = arrange_two_loops_on_developable(&surface, &a, &b)
        .expect("two circles that cross on a cone arrange")
        .regions;
    assert_eq!(regions.len(), 3, "A-only, the lens, B-only");

    let lens_exact = lens_of(r, d);
    let disc = std::f64::consts::PI * r * r;
    let of = |s: Share| regions.iter().find(|x| x.share == s).expect("region").area;
    let (a_only, lens, b_only) = (of(Share::FirstOnly), of(Share::Both), of(Share::SecondOnly));
    println!(
        "CONE r {r:.4} d {d:.4} n {} | a-only {a_only:.4} (exact {:.4})  \
         lens {lens:.4} (exact {lens_exact:.4})  b-only {b_only:.4}",
        a.len(),
        disc - lens_exact
    );

    // 1% each. The loops are 64-gons, so what is left is the inscribed
    // polygon's deficit and nothing else — measured at 0.33% for the lens,
    // which is the bulgiest of the three, and shown to shrink with the
    // sampling by `the_error_is_the_polygons_and_shrinks_with_them`. A wrong
    // chart is out by tens of percent: a cylinder's chart applied to a cone
    // misses by the whole `sin α`.
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
    let union_exact = 2.0 * disc - lens_exact;
    assert!(
        (a_only + lens + b_only - union_exact).abs() < 0.01 * union_exact,
        "the three tile the union: {:.4} vs {union_exact:.4}",
        a_only + lens + b_only
    );
}

/// What is left over is the polygons, not the chart.
///
/// The distinction matters and cannot be read off a single number: a 2.3% miss
/// looks the same whether the sampling is coarse or the flattening is wrong.
/// Refining the sampling separates them — a polygon deficit shrinks with it, a
/// bad chart does not move. Measured: 24-gon −2.34%, 45-gon −0.68%, 64-gon
/// −0.33%, and no further because `circle_on_cone` caps its samples at 64.
#[test]
fn the_error_is_the_polygons_and_shrinks_with_them() {
    let (_mesh, _wall, surface, c) = cone_wall();
    let (r, d) = (c.flat_gap(DU_RADIUS), c.flat_gap(DU_CENTRES));
    let lens_exact = lens_of(r, d);

    let mut seen: Vec<(usize, f64)> = Vec::new();
    for tol in [0.02, 0.005, 0.001] {
        CHORD_TOL.with(|x| x.set(tol));
        let a = c.circle(1.0, DU_RADIUS);
        let b = c.circle(1.0 + DU_CENTRES, DU_RADIUS);
        let regions = arrange_two_loops_on_developable(&surface, &a, &b)
            .expect("arrange")
            .regions;
        let lens = regions
            .iter()
            .find(|x| x.share == Share::Both)
            .expect("lens")
            .area;
        let err = (lens_exact - lens) / lens_exact;
        println!("CONVERGE tol {tol:<7} n {:>3}  lens {lens:.5}  short by {:.4}%", a.len(), 100.0 * err);
        seen.push((a.len(), err));
    }
    CHORD_TOL.with(|x| x.set(0.001)); // leave the default as found

    for w in seen.windows(2) {
        assert!(w[1].0 > w[0].0, "the sampling really did refine");
        assert!(
            w[1].1 < w[0].1,
            "a {}-gon is short by {:.4}% and a {}-gon by {:.4}% — refining did not \
             help, so the miss is the chart's rather than the polygons'",
            w[0].0,
            100.0 * w[0].1,
            w[1].0,
            100.0 * w[1].1
        );
        assert!(w[1].1 > 0.0, "an inscribed polygon is short, never long");
    }
    assert!(
        seen.last().unwrap().1 < 0.005,
        "at the finest sampling the lens is still short by {:.4}%",
        100.0 * seen.last().unwrap().1
    );
}

/// Every boundary point comes back ONTO the cone.
///
/// A point is on the cone when its axial and radial distances are in the cone's
/// own ratio, `radial = v·tan α`. Note what this can and cannot see: it catches
/// an inverse that leaves the surface, and nothing else. Both chart mutations
/// tried against it — dropping the `sin α` narrowing, and scaling the polar
/// radius one-way — leave every point on some part of the cone, and it passed
/// for both. The metric is the area test's business, not this one's.
#[test]
fn the_regions_come_back_onto_the_cone() {
    let (_mesh, _wall, surface, c) = cone_wall();
    let a = c.circle(1.0, DU_RADIUS);
    let b = c.circle(1.0 + DU_CENTRES, DU_RADIUS);
    let regions = arrange_two_loops_on_developable(&surface, &a, &b)
        .expect("arrange")
        .regions;
    for reg in &regions {
        assert!(reg.boundary.len() >= 3, "a region has a boundary");
        for p in &reg.boundary {
            let rel = *p - c.apex;
            let v = rel.dot(c.axis);
            let radial = (rel - c.axis * v).length();
            assert!(
                (radial - v * c.half_angle.tan()).abs() < 1e-6,
                "{p:?} is {radial:.6} out at v={v:.6}, the cone wants {:.6}",
                v * c.half_angle.tan()
            );
        }
    }
}

/// And into the mesh: the same two chain splits, on a cone.
#[test]
fn two_overlapping_circles_resolve_on_a_cone() {
    let (mut mesh, wall, _surface, c) = cone_wall();
    let a = c.circle(1.0, DU_RADIUS);
    let b = c.circle(1.0 + DU_CENTRES, DU_RADIUS);

    let (cap_a, host) = mesh
        .split_cone_face_by_circle(wall, &a)
        .expect("the first circle divides the cone wall");
    let host_surface = mesh.face_surface(host).expect("surface").clone();
    let rim_pts: Vec<DVec3> = mesh
        .collect_loop_verts(mesh.faces.get(cap_a).unwrap().outer().start)
        .expect("rim")
        .iter()
        .map(|&v| mesh.vertex_pos(v).unwrap())
        .collect();
    let x = crossing_on_developable(&host_surface, &rim_pts, &b).expect("the circles cross");

    // The crossings become vertices on the rim, which is shared, so the cap and
    // the host gain them at once.
    let rim: Vec<VertId> = mesh
        .collect_loop_verts(mesh.faces.get(cap_a).unwrap().outer().start)
        .expect("rim");
    let m = rim.len();
    let split_at = |mesh: &mut Mesh, k: usize| -> VertId {
        let (va, vb) = (rim[x.seg[k] % m], rim[(x.seg[k] + 1) % m]);
        let e = mesh.find_edge(va, vb).expect("rim edge");
        mesh.split_edge(e, x.points[k]).expect("split").0
    };
    let p0 = split_at(&mut mesh, 0);
    let p1 = split_at(&mut mesh, 1);
    let chain_of = |mesh: &mut Mesh, pts: &[DVec3]| -> Vec<VertId> {
        let mut vs = vec![p0];
        for &p in &pts[1..pts.len() - 1] {
            vs.push(mesh.add_vertex(p));
        }
        vs.push(p1);
        vs.dedup();
        for w in vs.windows(2) {
            if w[0] != w[1] {
                let _ = mesh.add_edge(w[0], w[1]);
            }
        }
        vs
    };
    let outside = chain_of(&mut mesh, &x.outside);
    let inside = chain_of(&mut mesh, &x.inside);

    let mat = MaterialId::new(0);
    let cut_host = split_face_by_chain(&mut mesh, host, &outside, mat)
        .expect("the host is cut by B's arc outside A");
    let cut_cap = split_face_by_chain(&mut mesh, cap_a, &inside, mat)
        .expect("cap A is cut by B's arc inside A");

    let mut out = vec![host, cap_a];
    out.extend(cut_host.new_faces.iter().copied());
    out.extend(cut_cap.new_faces.iter().copied());
    out.retain(|&f| mesh.faces.get(f).is_some_and(|x| x.is_active()));
    println!("CONE RESOLVED faces out: {out:?}");
    assert_eq!(out.len(), 4, "the host, and the three regions");

    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);
    assert_eq!(
        mesh.detect_self_intersections().count(),
        0,
        "and nothing pierces anything"
    );
    // Every piece is still the same wall (ADR-089 A-χ).
    for f in &out {
        assert!(
            mesh.face_surface(*f)
                .is_some_and(|s| matches!(s, AnalyticSurface::Cone { .. })),
            "face {f:?} kept the Cone"
        );
    }
}

/// The hosts that CANNOT be flattened are refused, not approximated.
///
/// The pairs here genuinely cross on their own host, which is the whole point:
/// the first version of this test passed the SAME ring twice, so `None` came
/// back for want of a crossing and the refusal was never exercised. Letting a
/// sphere through as if it were a cylinder did not fail it. It does now.
#[test]
fn a_sphere_and_a_torus_are_refused() {
    let sphere = AnalyticSurface::Sphere {
        center: DVec3::ZERO,
        radius: 10.0,
        axis_dir: DVec3::Z,
        ref_dir: DVec3::X,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
    };
    let torus = AnalyticSurface::Torus {
        center: DVec3::ZERO,
        axis_dir: DVec3::Z,
        ref_dir: DVec3::X,
        major_radius: 10.0,
        minor_radius: 3.0,
        u_range: (0.0, std::f64::consts::TAU),
        v_range: (0.0, std::f64::consts::TAU),
    };

    // A circle on a sphere: the cone of directions at angle `alpha` about a
    // centre direction, which is a planar circle lying on the sphere.
    let sphere_circle = |about: f64, alpha: f64| -> Vec<DVec3> {
        let c = DVec3::new(about.cos(), about.sin(), 0.0);
        let (e1, e2) = (DVec3::Z, c.cross(DVec3::Z).normalize());
        (0..48)
            .map(|i| {
                let t = std::f64::consts::TAU * (i as f64) / 48.0;
                10.0 * (c * alpha.cos() + (e1 * t.cos() + e2 * t.sin()) * alpha.sin())
            })
            .collect()
    };
    // Centres 0.36 rad apart with geodesic radius 0.30 rad: they cross.
    let sphere_pair = (sphere_circle(0.0, 0.30), sphere_circle(0.36, 0.30));

    let tp = |u: f64, v: f64| torus::evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 10.0, 3.0, u, v);
    let torus_circle = |u: f64| {
        torus::circle_on_torus(
            DVec3::ZERO,
            DVec3::Z,
            DVec3::X,
            10.0,
            3.0,
            tp(u, 0.0),
            tp(u + 0.25, 0.0),
            0.02,
        )
        .expect("a circle on the torus")
    };
    let torus_pair = (torus_circle(0.0), torus_circle(0.30));

    for (name, s, (a, b)) in [
        ("sphere", sphere, sphere_pair),
        ("torus", torus, torus_pair),
    ] {
        // The pair really does cross — checked on the plane through the three
        // radii, so a refusal below cannot be "there was nothing to find".
        let near = a
            .iter()
            .map(|p| b.iter().map(|q| (*p - *q).length()).fold(f64::MAX, f64::min))
            .fold(f64::MAX, f64::min);
        println!("{name}: the two loops come within {near:.4}");
        assert!(near < 1.0, "{name}: the pair must actually meet");

        assert!(
            arrange_two_loops_on_developable(&s, &a, &b).is_none(),
            "{name}: not developable, so it is refused rather than approximated"
        );
        assert!(
            crossing_on_developable(&s, &a, &b).is_none(),
            "{name}: and so is the crossing"
        );
    }
}
