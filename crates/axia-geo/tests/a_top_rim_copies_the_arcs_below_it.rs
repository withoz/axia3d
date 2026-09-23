//! AN EXTRUDED TOP RIM CARRIES THE ARCS OF THE RIM BELOW IT.
//!
//! `extrude_planar_cylinder`'s ≥ 3-vertex path gave each top-rim edge an arc it
//! synthesised from the edge's INDEX — `i·2π/n .. (i+1)·2π/n` — which is the right
//! arc only if the boundary's first vertex sits at angle 0. It sits wherever the
//! loop's start half-edge is: measured 2026-09-23 on a 16-arc circle (r = 40)
//! whose loop began at (37.0, 15.3), one step along, extruded 300:
//!
//! ```text
//!   the edge (37.0, 15.3) → (28.3, 28.3)   carried the arc 0.000..0.393,
//!   whose midpoint (39.2, 7.8) lies on the far side of a different chord
//! ```
//!
//! so the area reader deducted each bulge instead of adding it: the top cap read
//! 4770.1 mm² where its circle is 5026.5 and even its 16-gon is 4898.3. The base
//! cap, whose arcs are the ones that were drawn, read 5026.5 throughout.
//!
//! `extrude_planar_mixed` has always copied each boundary edge's own curve and
//! translated it; this is the same, and the arcs then say what the geometry says.

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

fn arc_circle(m: &mut Mesh, n: usize, z: f64) -> FaceId {
    let center = DVec3::new(0.0, 0.0, z);
    let vs: Vec<_> = (0..n)
        .map(|i| {
            let t = i as f64 * std::f64::consts::TAU / n as f64;
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
        let a0 = i as f64 * std::f64::consts::TAU / n as f64;
        let a1 = (i + 1) as f64 * std::f64::consts::TAU / n as f64;
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

/// Every arc on this loop starts and ends where its own edge does.
fn arcs_match_their_edges(m: &Mesh, cap: FaceId) -> Vec<String> {
    let start = m.faces[cap].outer().start;
    let mut wrong = Vec::new();
    for he in m.collect_loop_hes(start).expect("loop") {
        let (src, dst) = (m.he_src(he).expect("src"), m.hes[he].dst());
        let (p0, p1) = (m.vertex_pos(src).expect("p0"), m.vertex_pos(dst).expect("p1"));
        let Some(AnalyticCurve::Arc { center, radius, normal, basis_u, start_angle, end_angle }) =
            m.edges.get(m.hes[he].edge()).and_then(|e| e.curve().cloned())
        else {
            wrong.push(format!("{p0:.1?} → {p1:.1?} carries no arc"));
            continue;
        };
        let basis_v = normal.cross(basis_u).normalize_or_zero();
        let at = |a: f64| center + basis_u * (radius * a.cos()) + basis_v * (radius * a.sin());
        let (a, b) = (at(start_angle), at(end_angle));
        // Either way round the edge is walked, its ends are the arc's ends.
        let matches = ((a - p0).length() < 1e-9 && (b - p1).length() < 1e-9)
            || ((a - p1).length() < 1e-9 && (b - p0).length() < 1e-9);
        if !matches {
            wrong.push(format!(
                "{p0:.1?} → {p1:.1?} carries the arc {start_angle:.3}..{end_angle:.3}, \
                 which runs {a:.1?} → {b:.1?}"
            ));
        }
    }
    wrong
}

#[test]
fn every_arc_on_an_extruded_top_rim_starts_where_its_edge_does() {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, 0.0);
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
        .expect("extrude");

    let base = arcs_match_their_edges(&m, r.profile_face);
    assert!(base.is_empty(), "the base cap's own arcs:\n  {}", base.join("\n  "));

    let top = arcs_match_their_edges(&m, r.top_face);
    assert!(top.is_empty(), "the top rim's arcs:\n  {}", top.join("\n  "));
}

#[test]
fn both_caps_of_an_extruded_arc_circle_measure_the_circle() {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, 0.0);
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
        .expect("extrude");
    let circle = PI * R * R;
    for (label, cap) in [("base", r.profile_face), ("top", r.top_face)] {
        let area = m.face_area(cap);
        assert!(
            (area / circle - 1.0).abs() < 1e-9,
            "the {label} cap is bounded by the circle's own arcs, so it is πr² = {circle:.1}; \
             it read {area:.1} (its 16-gon is {:.1})",
            0.5 * 16.0 * R * R * (std::f64::consts::TAU / 16.0).sin()
        );
    }
}

/// The same synthesis a second time. `extrude_closed_curve_face_via_tessellation`
/// (the legacy Path A route) tessellates the circle, recurses into the builder
/// above — which now hands the top rim the arcs of the rim below it — and then
/// OVERWRITES them from the index again in its own step 8. Its note says the arc
/// is direction-agnostic, which holds for drawing the ring and not for measuring
/// the cap it bounds: measured 2026-09-23, a 23-segment Path A cylinder read its
/// base cap at 5026.5 mm² (πr², right) and its top at 4902.0 — the 23-gon's
/// 4964.3 with every bulge deducted instead of added.
fn disk(m: &mut Mesh, z: f64) -> FaceId {
    let c = DVec3::new(0.0, 0.0, z);
    let a = m.add_vertex(c + DVec3::X * R);
    m.add_face_closed_curve(
        a,
        AnalyticCurve::Circle { center: c, radius: R, normal: DVec3::Z, basis_u: DVec3::X },
        mat(),
    )
    .unwrap()
}

#[test]
fn a_path_a_cylinder_keeps_the_arcs_its_recursion_gave_it() {
    let mut m = Mesh::new();
    m.set_cylinder_path_b_default(false);
    let f = disk(&mut m, 0.0);
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
        .expect("extrude");

    let top = arcs_match_their_edges(&m, r.top_face);
    assert!(top.is_empty(), "the top rim's arcs:\n  {}", top.join("\n  "));

    let circle = PI * R * R;
    for (label, cap) in [("base", r.profile_face), ("top", r.top_face)] {
        let area = m.face_area(cap);
        assert!(
            (area / circle - 1.0).abs() < 1e-9,
            "the {label} cap is bounded by the circle's own arcs, so it is πr² = {circle:.1}; \
             it read {area:.1}"
        );
    }
}
