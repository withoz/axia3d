//! A RING IS NOT DRAWN OVER ITS OWN HOLE.
//!
//! 사용자 (2026-08-04, screenshot): `작은원과 큰원의 면은 분리가 안되어있네` —
//! selecting the ring painted the whole big disk, over the small circle inside
//! it.
//!
//! The DCEL was right: the ring holds the small circle as a hole, the small
//! circle is its own face, invariants clean, solid closed. What was wrong was
//! the DRAWING. A closed-curve face (ADR-089 Phase 2 — one self-loop half-edge
//! carrying a Circle) had a render fast-path that fans from the centre to the
//! rim, and a fan is the whole disk. It never looked at `face.inners()`, so the
//! moment such a face gained a hole it was drawn straight over it.
//!
//! Measured in the running app before the fix: the ring's triangles summed to
//! π·70² (the full disk) instead of π·(70²−30²), and 50 of them covered the
//! centre — the same 50 the small disk had. Two faces, one on top of the other.
//!
//! The area is what this pins, not the triangle count: a tolerance change moves
//! the count and means nothing, while the area is the shape itself.
use axia_geo::curves::AnalyticCurve;
use axia_geo::operations::annulus::split_face_by_inner_circle;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

fn disk(mesh: &mut Mesh, radius: f64) -> axia_geo::FaceId {
    let anchor = mesh.add_vertex(DVec3::new(radius, 0.0, 0.0));
    mesh.add_face_closed_curve(
        anchor,
        AnalyticCurve::Circle {
            center: DVec3::ZERO,
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        },
        MaterialId::new(0),
    )
    .expect("a circle face")
}

/// Triangle area drawn for each face, and how many of its triangles cover a
/// point — read out of the buffers the viewport actually receives.
fn drawn(mesh: &mut Mesh, at: (f64, f64)) -> Vec<(u32, f64, usize)> {
    let (_pos32, _n, indices, face_map, pos) = mesh.export_buffers().expect("export");
    let mut area: std::collections::BTreeMap<u32, f64> = Default::default();
    let mut cover: std::collections::BTreeMap<u32, usize> = Default::default();
    for t in 0..indices.len() / 3 {
        let p = |k: usize| {
            let i = indices[3 * t + k] as usize * 3;
            (pos[i], pos[i + 1])
        };
        let (a, b, c) = (p(0), p(1), p(2));
        let f = face_map[t];
        *area.entry(f).or_default() +=
            ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() / 2.0;
        let side = |p: (f64, f64), q: (f64, f64)| {
            (q.0 - p.0) * (at.1 - p.1) - (at.0 - p.0) * (q.1 - p.1)
        };
        let d = [side(a, b), side(b, c), side(c, a)];
        if !(d.iter().any(|&x| x < 0.0) && d.iter().any(|&x| x > 0.0)) {
            *cover.entry(f).or_default() += 1;
        }
    }
    area.into_iter().map(|(f, a)| (f, a, cover.get(&f).copied().unwrap_or(0))).collect()
}

#[test]
fn a_ring_is_drawn_as_a_ring_and_only_the_disk_covers_the_centre() {
    let mut mesh = Mesh::new();
    let ring = disk(&mut mesh, 70.0);
    let inner = disk(&mut mesh, 30.0);
    split_face_by_inner_circle(&mut mesh, ring, inner).expect("the disk becomes the ring's hole");

    let drawn = drawn(&mut mesh, (0.0, 0.0));
    let get = |f: axia_geo::FaceId| {
        drawn.iter().find(|(id, _, _)| *id == f.raw()).copied().expect("face was drawn")
    };
    let (_, ring_area, ring_cover) = get(ring);
    let (_, disk_area, disk_cover) = get(inner);

    let annulus = std::f64::consts::PI * (70.0f64.powi(2) - 30.0f64.powi(2));
    let full = std::f64::consts::PI * 70.0f64.powi(2);
    assert!(
        (ring_area - annulus).abs() < annulus * 0.01,
        "the ring is drawn as a ring, not as the whole disk \
         (got {ring_area:.0}, ring {annulus:.0}, full disk {full:.0})"
    );
    assert!(
        (disk_area - std::f64::consts::PI * 900.0).abs() < 30.0,
        "and the disk is drawn whole (got {disk_area:.0})"
    );
    assert_eq!(ring_cover, 0, "nothing of the ring lies over its hole");
    assert!(disk_cover > 0, "the disk does cover the centre");
}

/// The same face without a hole still takes the cheap fan — the fix must not
/// cost every circle its fast-path.
#[test]
fn a_plain_disk_is_still_drawn_whole() {
    let mut mesh = Mesh::new();
    let d = disk(&mut mesh, 70.0);
    let drawn = drawn(&mut mesh, (0.0, 0.0));
    let (_, area, cover) = drawn.iter().find(|(id, _, _)| *id == d.raw()).copied().unwrap();
    let full = std::f64::consts::PI * 70.0f64.powi(2);
    assert!((area - full).abs() < full * 0.01, "got {area:.0}, expected {full:.0}");
    assert!(cover > 0, "a plain disk covers its own centre");
}
