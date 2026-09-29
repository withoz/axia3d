//! A CURVED FACE IS READ OVER THE PATCH IT STANDS ON, NOT ITS WHOLE SURFACE.
//!
//! A builder hands every quad of a band the WHOLE band — `extrude_planar_cylinder`'s
//! ≥ 3-vertex path, `extrude_planar_mixed`'s arc walls, the polygonal cone paths —
//! and a split keeps its parent's full range on purpose (ADR-089 A-χ), leaving the
//! slice to whoever reads it. The renderer has sliced since A-ρ/A-φ; the three
//! readers never did, so each quad counted a whole band. Measured 2026-09-23 on one
//! quad of a 16-gon band (r = 40, h = 300, so its own share is a sixteenth):
//!
//! ```text
//!             the quad read     its own share
//!   area          75398.2           4712.4
//!   flux        3015928.9         188495.6
//!   bounds    the whole band     a wedge of it
//! ```
//!
//! A solid built that way then measured ×11 to ×16 of its volume, and the one a
//! user reaches — a circle cut by a line with one half pushed — ×2.88.
//!
//! The controls: the drill attaches a slice per quad and has always measured exact,
//! and a Path B band's one-vertex rims cannot say anything, so its stored surface
//! IS its own patch.

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

/// A circle of `n` vertices with an Arc on every edge, at height `z` — the shape a
/// circle takes once anything has split it, and what the legacy paths build.
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
fn a_band_quad_reads_its_own_slice() {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, 0.0);
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
        .expect("extrude");
    let quad = r.side_faces[0];

    // The quad's own share, read off its own corners rather than assumed.
    let corners: Vec<DVec3> = m
        .collect_loop_verts(m.faces[quad].outer().start)
        .expect("quad")
        .iter()
        .map(|&v| m.vertex_pos(v).expect("corner"))
        .collect();
    assert_eq!(corners.len(), 4, "a band quad has four corners");
    let angles: Vec<f64> = corners.iter().map(|p| p.y.atan2(p.x)).collect();
    let (u_lo, u_hi) = (
        angles.iter().cloned().fold(f64::MAX, f64::min),
        angles.iter().cloned().fold(f64::MIN, f64::max),
    );
    let du = u_hi - u_lo;
    assert!(du > 0.1 && du < 1.0, "one sixteenth of a turn, got {du}");

    let area = m.face_area(quad);
    assert!(
        (area / (R * du * H) - 1.0).abs() < 1e-9,
        "the quad's area is r·Δu·h = {:.1}, it read {area:.1}",
        R * du * H
    );

    let flux = m.face_outward_flux(quad).expect("flux");
    assert!(
        (flux / (R * R * du * H) - 1.0).abs() < 1e-9,
        "the quad's flux is r²·Δu·h = {:.1}, it read {flux:.1}",
        R * R * du * H
    );

    let (lo, hi) = m.face_bounds(quad).expect("bounds");
    let wedge_lo = DVec3::new(R * u_hi.cos() - 1e-6, R * u_lo.sin() - 1e-6, -1e-6);
    let wedge_hi = DVec3::new(R + 1e-6, R * u_hi.sin() + 1e-6, H + 1e-6);
    assert!(
        lo.x >= wedge_lo.x && lo.y >= wedge_lo.y && lo.z >= wedge_lo.z,
        "the quad's box starts at {lo:?}; its wedge starts at {wedge_lo:?}"
    );
    assert!(
        hi.x <= wedge_hi.x && hi.y <= wedge_hi.y && hi.z <= wedge_hi.z,
        "the quad's box ends at {hi:?}; its wedge ends at {wedge_hi:?}"
    );
}

/// The quads of a band divide one cylinder between them, so their fluxes add up to
/// that whole cylinder — no more (each used to count all of it) and no less (a
/// slice must not lose a wedge at the seam, where u wraps).
#[test]
fn a_band_of_quads_adds_up_to_the_whole_cylinder() {
    let band = std::f64::consts::TAU * R * R * H; // 2πr²h
    let mut wrong = Vec::new();
    for z in [0.0, 100.0] {
        let mut m = Mesh::new();
        let f = arc_circle(&mut m, 16, z);
        let r = m
            .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
            .expect("extrude");
        let sum: f64 = r.side_faces.iter().filter_map(|&s| m.face_outward_flux(s)).sum();
        if (sum / band - 1.0).abs() > 1e-9 {
            wrong.push(format!("16-arc circle extrude, base z = {z}: {:.4}", sum / band));
        }

        // The legacy tessellate-then-extrude path lands on the same builder.
        let mut m = Mesh::new();
        m.set_cylinder_path_b_default(false);
        let f = disk(&mut m, z);
        let r = m
            .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
            .expect("extrude");
        let sum: f64 = r.side_faces.iter().filter_map(|&s| m.face_outward_flux(s)).sum();
        if (sum / band - 1.0).abs() > 1e-9 {
            wrong.push(format!("Path A disk extrude, base z = {z}: {:.4}", sum / band));
        }
    }
    assert!(
        wrong.is_empty(),
        "the band is 2πr²h = {band:.1}; these read otherwise (fraction of it):\n  {}",
        wrong.join("\n  ")
    );
}

/// And the solid such a band belongs to measures its volume, where it used to read
/// eleven times it.
///
/// To the last digit, but only once two other readings were put right alongside
/// this one: a cap bounded by arcs had its flux read from the CHORD polygon while
/// its area read the arcs, and this solid's top rim carried each arc one edge
/// along (its cap read 4770.1 where the circle is 5026.5). Each has its own guard
/// in this crate.
#[test]
fn a_solid_whose_band_is_one_surface_measures_its_volume() {
    let truth = PI * R * R * H;
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, 0.0);
    m.create_solid(f, CreateSolidMode::Extrude { distance: H }, mat()).expect("extrude");
    let v = m.mesh_volume();
    assert!(
        (v / truth - 1.0).abs() < 1e-9,
        "πr²h = {truth:.1}, it read {v:.1} ({:.4} of it) — it used to read about eleven times it",
        v / truth
    );
}

/// The controls: the two shapes that always measured right must not move.
#[test]
fn a_slice_per_quad_and_a_one_vertex_rim_are_unchanged() {
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, mat()).expect("box");
    m.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, 32).expect("bore");
    let bored = 8_000_000.0 - PI * R * R * 200.0;
    let v = m.mesh_volume();
    assert!((v / bored - 1.0).abs() < 1e-9, "a bored box is {bored:.1}, it read {v:.1}");

    // A Path B band has two one-vertex rims, so its stored surface IS its own
    // patch and must read exactly as before. Its solid's VOLUME is not the control
    // here: on this branch the cap on the back of an extrusion still looks in,
    // which is PR #263's subject rather than this one's.
    let mut m = Mesh::new();
    m.set_cylinder_path_b_default(true);
    let f = disk(&mut m, 100.0);
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: H }, mat())
        .expect("extrude");
    let band = r.side_faces[0];
    let whole = std::f64::consts::TAU;
    let area = m.face_area(band);
    assert!(
        (area / (R * whole * H) - 1.0).abs() < 1e-9,
        "the band is 2πrh = {:.1}, it read {area:.1}",
        R * whole * H
    );
    let flux = m.face_outward_flux(band).expect("flux");
    assert!(
        (flux / (R * R * whole * H) - 1.0).abs() < 1e-9,
        "its flux is 2πr²h = {:.1}, it read {flux:.1}",
        R * R * whole * H
    );
    let (lo, hi) = m.face_bounds(band).expect("bounds");
    assert!(
        (lo - DVec3::new(-R, -R, 100.0)).length() < 1e-3
            && (hi - DVec3::new(R, R, 100.0 + H)).length() < 1e-3,
        "the band's box is the whole band, it read {lo:?}..{hi:?}"
    );
}

/// A slice still wants four corners, and that is the right place to stop: a
/// polygonal cone's apex fan has three, and the apex inverts to u = 0 whatever
/// sector the triangle covers, so a range read off those corners comes out twice
/// as wide rather than narrow. Nothing needs the reader to try: every 3-vertex
/// curved face that exists carries its own range from its BUILDER —
/// `create_cone`'s 16 fan triangles, `create_sphere`'s 32 polar triangles, and
/// since 2026-09-27 `extrude_planar_cone`'s fan too, which used to share one
/// full-turn surface and read 15.9946x its volume. Full treatment in
/// `a_cone_fan_counts_its_own_sector`.
#[test]
fn a_cone_fan_triangle_is_narrowed_by_its_builder() {
    let mut m = Mesh::new();
    let f = arc_circle(&mut m, 16, 0.0);
    let r = m
        .create_solid(f, CreateSolidMode::ExtrudeCone { distance: H, top_scale: 0.0 }, mat())
        .expect("cone");

    // Narrow at the source, so no reader has to narrow it.
    let sector = std::f64::consts::TAU / 16.0;
    for (i, &s) in r.side_faces.iter().enumerate() {
        let Some(AnalyticSurface::Cone { u_range, .. }) = m.face_surface(s) else {
            panic!("fan triangle {i} carries no Cone surface");
        };
        assert!(
            ((u_range.1 - u_range.0) / sector - 1.0).abs() < 1e-9,
            "fan triangle {i} should carry one sixteenth of the turn ({sector:.4}); it carries {:.4}",
            u_range.1 - u_range.0
        );
    }

    // A cone band has no closed-form flux, so it is read from its tessellation —
    // the same 1e-3 the primitive cone reads at.
    let truth = PI * R * R * H / 3.0;
    let ratio = m.mesh_volume() / truth;
    assert!(
        (ratio - 1.0).abs() < 1e-3,
        "the apex cone reads its own volume; it read {ratio:.4} of it (it used to read ~16x)"
    );
}
