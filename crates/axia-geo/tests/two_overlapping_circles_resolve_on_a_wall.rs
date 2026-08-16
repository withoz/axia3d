//! C-2's whole point: two circles that cross on a curved wall become three
//! regions, and the ground under the lens stops belonging to two faces.
//!
//! Before this, the second circle did not notice the first — cap A's boundary
//! was 31 vertices before a circle was drawn across it and 31 after, with
//! invariants valid and self-intersections 0, because the pair share no edge
//! and pierce nothing. Nothing in the engine could see it.
//!
//! The cut is two ordinary chain splits, once the pieces exist to make them
//! with:
//!
//! ```text
//!   host   split by B's arc OUTSIDE A   ->  host' + B-only
//!   cap A  split by B's arc INSIDE A    ->  A-only + the lens
//! ```
//!
//! Both are possible because a cap's rim IS its host's hole — the same
//! vertices, wired through twin half-edges.

use axia_geo::operations::curved_arrange::crossing_on_cylinder;
use axia_geo::operations::face_split::split_face_by_chain;
use axia_geo::surfaces::{cylinder, AnalyticSurface};
use axia_geo::{FaceId, MaterialId, Mesh, VertId};
use glam::DVec3;

const R: f64 = 10.0;

fn geodesic_circle(u0: f64, v0: f64, r: f64, n: usize) -> Vec<DVec3> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * (i as f64) / (n as f64);
            cylinder::evaluate(
                DVec3::ZERO,
                DVec3::Z,
                R,
                DVec3::X,
                u0 + (r * t.cos()) / R,
                v0 + r * t.sin(),
            )
        })
        .collect()
}

/// Draw circle A on a Path B cylinder wall, then resolve circle B against it.
///
/// Returns the mesh and the faces that came out, so each test can ask its own
/// question of the same construction.
fn resolved() -> (Mesh, FaceId, FaceId, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0))
        .expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface()
                    .is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("the wall");

    let (r, d) = (3.0_f64, 3.6_f64); // centres 1.2 r apart, so they cross
    let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 48);
    let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 48);
    let (cap_a, host) = mesh
        .split_cylinder_face_by_circle(side, &a)
        .expect("the first circle divides the wall");

    let surface = mesh.face_surface(host).expect("surface").clone();
    let rim_pts: Vec<DVec3> = mesh
        .collect_loop_verts(mesh.faces.get(cap_a).unwrap().outer().start)
        .expect("rim")
        .iter()
        .map(|&v| mesh.vertex_pos(v).unwrap())
        .collect();
    let x = crossing_on_cylinder(&surface, &rim_pts, &b).expect("the circles cross");

    // The crossings become vertices on the rim, which is shared, so both the
    // cap and the host gain them at once.
    let rim: Vec<VertId> = mesh
        .collect_loop_verts(mesh.faces.get(cap_a).unwrap().outer().start)
        .expect("rim");
    let m = rim.len();
    let mut split_at = |mesh: &mut Mesh, k: usize| -> VertId {
        let (va, vb) = (rim[x.seg[k] % m], rim[(x.seg[k] + 1) % m]);
        let e = mesh.find_edge(va, vb).expect("rim edge");
        mesh.split_edge(e, x.points[k]).expect("split").0
    };
    let p0 = split_at(&mut mesh, 0);
    let p1 = split_at(&mut mesh, 1);

    let mut chain_of = |mesh: &mut Mesh, pts: &[DVec3]| -> Vec<VertId> {
        let mut vs = vec![p0];
        for &p in &pts[1..pts.len() - 1] {
            vs.push(mesh.add_vertex(p));
        }
        vs.push(p1);
        vs.dedup();
        for w in vs.windows(2) {
            if w[0] != w[1] {
                mesh.add_edge(w[0], w[1]);
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
    out.extend(cut_host.new_faces);
    out.extend(cut_cap.new_faces);
    out.retain(|&f| mesh.faces.get(f).is_some_and(|x| x.is_active()));
    (mesh, host, cap_a, out)
}

#[test]
fn the_pair_becomes_three_regions_and_the_mesh_is_sound() {
    let (mesh, _host, _cap, faces) = resolved();
    println!("RESOLVED faces out: {faces:?}");
    assert_eq!(faces.len(), 4, "the host, and the three regions");

    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);
    assert_eq!(
        mesh.detect_self_intersections().count(),
        0,
        "and nothing pierces anything"
    );
}

/// The claim the whole track is about, asked of the picture rather than of a
/// face id: **the lens stops being drawn twice.**
///
/// Cap A is replaced by the split, so following its id proves nothing. What
/// does is the area the viewport is handed. Two caps that share the lens draw
/// it twice; three regions that tile do not. And the instrument is sound —
/// drawing ONE circle changes the total by x1.0000
/// (`a_drawn_circle_does_not_add_surface.rs`).
#[test]
fn the_lens_is_no_longer_drawn_twice() {
    let drawn = |mesh: &mut Mesh| -> f64 {
        let (pos, _n, idx, _f, _d) = mesh.export_buffers().expect("export");
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
    };
    // 2piRh + 2piR^2 for a diameter-20, height-20 cylinder.
    let truth = 1884.956;

    let mut plain = Mesh::new();
    plain.cylinder_path_b_default = true;
    plain
        .create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0))
        .expect("cylinder");
    let before = drawn(&mut plain);
    assert!(
        (before - truth).abs() < 0.02 * truth,
        "control: an undrawn cylinder reads {before:.3} against {truth:.3}"
    );

    let (mut mesh, _h, _c, _f) = resolved();
    let after = drawn(&mut mesh);
    // Attribute the difference, so a failure says WHICH piece is wrong.
    {
        let (pos, _n, idx, fmap, _d) = mesh.export_buffers().expect("export");
        let mut per: std::collections::BTreeMap<u32, f64> = Default::default();
        for (t, tri) in idx.chunks_exact(3).enumerate() {
            let p = |i: u32| {
                DVec3::new(
                    pos[3 * i as usize] as f64,
                    pos[3 * i as usize + 1] as f64,
                    pos[3 * i as usize + 2] as f64,
                )
            };
            let (a, b, c) = (p(tri[0]), p(tri[1]), p(tri[2]));
            *per.entry(fmap[t]).or_default() += 0.5 * (b - a).cross(c - a).length();
        }
        for (f, a) in &per {
            println!("   face {f:>3} -> {a:>9.3}");
        }
    }
    // Does the render still recognise the pieces it has to clip? The band
    // clipper only knows a "geodesic split loop" — N plain edges whose twins
    // are all co-cylindrical — and after the cut the host's hole is a MIXED
    // loop, part chain and part arc.
    for &f in &_f {
        let face = mesh.faces.get(f).unwrap();
        let outer = mesh.collect_loop_verts(face.outer().start).map(|v| v.len()).unwrap_or(0);
        let inners: Vec<usize> = face
            .inners()
            .iter()
            .map(|l| mesh.collect_loop_verts(l.start).map(|v| v.len()).unwrap_or(0))
            .collect();
        println!("   loops {f:?} outer {outer} inners {inners:?}");
    }
    // The tolerance is a fraction of the LENS, not of the cylinder. The lens is
    // 8.05 mm2 against a surface of 1884.96 — anything measured in percent of
    // the cylinder would pass with the lens still drawn twice, which is how a
    // 3% threshold let a 55 mm2 error through when this was written.
    let lens = {
        let (r, d) = (3.0_f64, 3.6_f64);
        2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt()
    };
    println!("LENS drawn area {before:.3} -> {after:.3} (true {truth:.3}, lens {lens:.3})");
    assert!(
        (after - truth).abs() < 0.5 * lens,
        "two circles that cross must still draw one cylinder: {after:.3} vs          {truth:.3}, off by {:.3} against a lens of {lens:.3}",
        after - truth
    );
}

/// Every piece is still the same wall (ADR-089 A-χ), so a later operation can
/// still tell what surface it is standing on.
#[test]
fn every_piece_keeps_the_cylinder() {
    let (mesh, _host, _cap, faces) = resolved();
    for f in faces {
        assert!(
            mesh.face_surface(f)
                .is_some_and(|s| matches!(s, AnalyticSurface::Cylinder { .. })),
            "face {f:?} kept the Cylinder"
        );
    }
}
