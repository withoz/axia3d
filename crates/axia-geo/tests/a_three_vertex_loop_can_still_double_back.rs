//! The smallest shape the defect takes, and the one route 1 does not explain.
//!
//! Session 12 has five faces whose outer loop steps over one of its own
//! vertices (measured 2026-08-23 by probing `verify_face_invariants` for the
//! first moment each acquires it):
//!
//! ```text
//!   FaceId(146)  loop 12   z=0     <- subtract_double_covered_faces
//!   FaceId(147)  loop 12   z=0     <- subtract_double_covered_faces
//!   FaceId(148)  loop 15   z=0     <- subtract_double_covered_faces
//!   FaceId(142)  loop  4   z=0     <- exec_draw_line
//!   FaceId( 87)  loop  3   z=200   <- exec_draw_line
//! ```
//!
//! Route 1 (`subtract_double_covered_faces`, fed by the 2D clipper — see
//! `the_clipper_emits_a_ring_that_doubles_back.rs`) makes the big ones. The
//! small ones are the flat faces that actually get reported.
//!
//! ⚠ `FaceId(142)`'s geometry is verbatim one of route 1's, so it is a route-1
//! face split later rather than a second producer. `FaceId(87)` is at z=200 and
//! every route-1 case is at z=0 — it has no route-1 ancestor.
//!
//! This file pins the SHAPE of that last one, which is as small as the defect
//! gets: three vertices, one long edge, and the third vertex sitting on it.
//!
//! ⚠⚠ It does not pin the producer. The backtrace named `exec_draw_line`
//! through `flip_face_safe`, but that is where the CHECK ran —
//! `debug_verify_invariants` fires after every mutation, so the frame above it
//! is just the first check that followed. Which statement inside
//! `exec_draw_line` builds it is still open.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;

/// Does an edge of the ring run straight over another of its own vertices?
fn steps_over_own_vertex(m: &Mesh, f: axia_geo::FaceId) -> Option<(DVec3, DVec3, DVec3)> {
    let face = m.faces.get(f)?;
    if !face.is_active() {
        return None;
    }
    let verts = m.collect_loop_verts(face.outer().start).ok()?;
    let n = verts.len();
    for i in 0..n {
        let a = m.vertex_pos(verts[i]).ok()?;
        let b = m.vertex_pos(verts[(i + 1) % n]).ok()?;
        let d = b - a;
        let len = d.length();
        if len < 1e-9 {
            continue;
        }
        for (j, &v) in verts.iter().enumerate() {
            if j == i || j == (i + 1) % n {
                continue;
            }
            let p = m.vertex_pos(v).ok()?;
            let w = p - a;
            let t = w.dot(d) / len;
            if t <= 1e-6 || t >= len - 1e-6 {
                continue;
            }
            if (w - d / len * t).length() > 1e-6 {
                continue;
            }
            return Some((a, b, p));
        }
    }
    None
}

/// Three vertices is enough. Built from FaceId(87)'s measured geometry: the
/// bottom run 27.639 -> 67.500 at y = -130, z = 200, and 27.872 on it.
#[test]
fn three_vertices_are_enough_to_double_back() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(27.639, -130.0, 200.0));
    let b = m.add_vertex(DVec3::new(67.500, -130.0, 200.0));
    let mid = m.add_vertex(DVec3::new(27.872, -130.0, 200.0));

    // PREMISE: all three really are collinear, so a ring through them has no
    // width and the kernel has something to object to.
    let d = DVec3::new(67.500, -130.0, 200.0) - DVec3::new(27.639, -130.0, 200.0);
    let w = DVec3::new(27.872, -130.0, 200.0) - DVec3::new(27.639, -130.0, 200.0);
    let t = w.dot(d) / d.length();
    assert!(t > 1e-6 && t < d.length() - 1e-6, "the middle one is interior");
    assert!((w - d / d.length() * t).length() < 1e-9, "and on the line");

    let f = m.add_face(&[a, b, mid], MaterialId::new(0));

    // ⚠ Whether the kernel accepts a fully-degenerate triangle is its own
    // decision, and either answer is informative. Say which happened.
    match f {
        Ok(fid) => {
            let found = steps_over_own_vertex(&m, fid);
            let (ea, eb, over) = found.expect(
                "a 3-vertex ring of collinear points must register as doubling back — \
                 this is FaceId(87)'s shape from session 12",
            );
            let span = [ea.x, eb.x];
            assert!(
                span.contains(&27.639) && span.contains(&67.500),
                "the long run is the offending edge, got {ea:?} -> {eb:?}"
            );
            assert!(
                (over - DVec3::new(27.872, -130.0, 200.0)).length() < 1e-9,
                "and it steps over 27.872, got {over:?}"
            );
            println!("\n  accepted, and the defect is visible: {ea:?} -> {eb:?} over {over:?}\n");
        }
        Err(e) => {
            // Also a fine answer — but then session 12's FaceId(87) did NOT come
            // through `add_face`, which narrows the hunt rather than ending it.
            println!("\n  add_face refused it: {e}\n");
            println!("  So FaceId(87) reached the mesh by some other door — direct");
            println!("  DCEL surgery, the way `Mesh::split_face` works.\n");
        }
    }
}

/// The control: move the third vertex off the line and nothing is flagged.
#[test]
fn a_real_triangle_is_not_flagged() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(27.639, -130.0, 200.0));
    let b = m.add_vertex(DVec3::new(67.500, -130.0, 200.0));
    let c = m.add_vertex(DVec3::new(27.872, -100.0, 200.0));
    let f = m
        .add_face(&[a, b, c], MaterialId::new(0))
        .expect("an ordinary triangle");
    assert_eq!(
        steps_over_own_vertex(&m, f),
        None,
        "a triangle with area must not register"
    );
}
