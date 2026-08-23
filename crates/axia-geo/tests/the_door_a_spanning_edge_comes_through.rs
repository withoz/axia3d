//! Where a boundary edge that steps over its own vertex comes from.
//!
//! `a_split_between_collinear_points_leaves_no_area.rs` established the shape:
//! a face whose boundary has an edge running straight over another vertex of
//! the same loop. Every attempt to suppress the flat face it produces traded
//! one symptom for a worse one, because they were all downstream of it. This
//! file names the door it comes through.
//!
//! ## Measured 2026-08-23, wide-fuzz session 12
//!
//! A probe on vertex insertion asked "who lands a vertex inside an existing
//! collinear edge?". 26 hits — but NEITHER of the two vertices that actually
//! caused the flat faces was among them. The damage runs the other way: the
//! vertex is there first, and an edge is later drawn straight over it.
//!
//! Re-probing edge creation instead, 23 hits in one session, and every single
//! one goes through the same door:
//!
//! ```text
//!   add_edge  <-  make_loop  <-  add_face_with_holes
//! ```
//!
//! with three callers reaching it:
//!
//! ```text
//!   11   Scene::subtract_double_covered_faces
//!   10   face_rederive::rebuild_inner
//!    2   Mesh::add_face
//! ```
//!
//! The two faces from the earlier file land one each:
//!
//! ```text
//!   VertId(152) (27.872,-130,200)   spanned by (27.639,-130,200)-(67.500,-130,200)
//!                                   <- face_rederive::rebuild_inner
//!   VertId(263) (67.500,-145.725,0) spanned by (67.500,-130,0)-(67.500,-170.337,0)
//!                                   <- Scene::subtract_double_covered_faces
//! ```
//!
//! ⚠ The earlier file says, about four failed traces: "a split does its own
//! DCEL surgery and never passes through `add_face_with_holes`, so a trace on
//! that one door saw nothing." That was true of the SPLIT. The upstream
//! producer is that very door — four traces were watching the right place for
//! the wrong event.
//!
//! ## What the door does, and does not do
//!
//! `add_face_with_holes` takes a vertex list and joins consecutive entries with
//! `add_edge`. It never asks whether some OTHER existing vertex lies in the
//! interior of the edge it is about to create. `find_collinear_endpoint_splits`
//! exists for the mirror-image case and is wired into `exec_draw_line` only, so
//! a face built directly from a computed vertex list never consults it.
//!
//! Nothing here changes the engine. What is pinned is the door, and the fact
//! that walking through it leaves a face `verify_face_invariants` calls sound.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;

/// True when `edge` (given as two positions) runs straight over `p`.
fn spans(a: DVec3, b: DVec3, p: DVec3) -> bool {
    let ab = b - a;
    let len = ab.length();
    if len < 1e-9 {
        return false;
    }
    let dir = ab / len;
    let w = p - a;
    let along = w.dot(dir);
    if along <= 1e-6 || along >= len - 1e-6 {
        return false;
    }
    (w - dir * along).length() <= 1e-6
}

/// The door: build a face from a vertex list where one vertex sits in the
/// interior of the run between two others, and see whether the kernel notices.
///
/// Geometry is FaceId(86)'s, measured from session 12: the bottom run
/// 27.639 -> 67.500 at y = -130, with 27.872 sitting inside it.
#[test]
fn add_face_with_holes_will_span_an_existing_vertex() {
    let mut m = Mesh::new();

    let bottom_l = m.add_vertex(DVec3::new(27.639, -130.0, 200.0));
    let bottom_r = m.add_vertex(DVec3::new(67.500, -130.0, 200.0));
    let up_r = m.add_vertex(DVec3::new(67.500, -125.633, 200.0));
    let arc = m.add_vertex(DVec3::new(32.700, -125.491, 200.0));
    // The vertex that will be stepped over — it exists BEFORE the face is built.
    let inside = m.add_vertex(DVec3::new(27.872, -130.0, 200.0));

    // PREMISE: it really is interior to the run, so a kernel that checked would
    // have something to find.
    assert!(
        spans(
            DVec3::new(27.639, -130.0, 200.0),
            DVec3::new(67.500, -130.0, 200.0),
            DVec3::new(27.872, -130.0, 200.0),
        ),
        "the fixture must actually pose the question"
    );

    let f = m
        .add_face(
            &[bottom_l, bottom_r, up_r, arc, inside],
            MaterialId::new(0),
        )
        .expect("the kernel accepts this list — that is the point");

    // Walk the face's own boundary and look for an edge over its own vertex.
    let face = m.faces.get(f).expect("face");
    let verts = m
        .collect_loop_verts(face.outer().start)
        .expect("a loop");
    let n = verts.len();
    let mut found = None;
    for i in 0..n {
        let a = m.vertex_pos(verts[i]).expect("pos");
        let b = m.vertex_pos(verts[(i + 1) % n]).expect("pos");
        for (j, &v) in verts.iter().enumerate() {
            if j == i || j == (i + 1) % n {
                continue;
            }
            let p = m.vertex_pos(v).expect("pos");
            if spans(a, b, p) {
                found = Some((a, b, p));
            }
        }
    }

    let (ea, eb, over) = found.expect(
        "add_face_with_holes joined two vertices straight over a third and \
         nobody split the run — this is the door session 12 came through 23 times",
    );
    let span = [ea.x, eb.x];
    assert!(
        span.contains(&27.639) && span.contains(&67.500),
        "the spanning edge is the bottom run, got {ea:?} -> {eb:?}"
    );
    assert!(
        (over - DVec3::new(27.872, -130.0, 200.0)).length() < 1e-9,
        "and it steps over 27.872, got {over:?}"
    );

    // ⚠ And the kernel signs off on it. That is why the damage stays hidden
    // until some later cut has to divide by it.
    let inv = m.verify_face_invariants();
    assert!(
        inv.is_valid(),
        "if this starts failing, the kernel learned to see the defect and this \
         file's premise needs revisiting — {:?}",
        inv.violations
    );
}

/// The mirror-image case IS handled — but only for a COLLINEAR line, which is
/// narrower than it first looks and worth pinning exactly.
///
/// ⚠ Written first with a perpendicular line and it failed. The function
/// rejects anything not parallel to the existing edge (`cross > 1e-6`), so it
/// is not "an endpoint landing on an edge" — it is "a line running ALONG an
/// edge and starting partway into it". FaceId(86)'s cut did run along the
/// bottom edge, so that case really would have been caught; a general claim
/// that this function covers endpoints-on-edges would not be true.
#[test]
fn the_mirror_image_case_is_the_one_that_is_handled() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(27.639, -130.0, 200.0));
    let b = m.add_vertex(DVec3::new(67.500, -130.0, 200.0));
    let c = m.add_vertex(DVec3::new(67.500, -125.633, 200.0));
    let d = m.add_vertex(DVec3::new(27.639, -125.633, 200.0));
    m.add_face(&[a, b, c, d], MaterialId::new(0)).expect("a rect");

    // A line running ALONG the bottom edge and starting partway into it —
    // the case Step 0 is built for.
    let splits = m.find_collinear_endpoint_splits(
        DVec3::new(27.872, -130.0, 200.0),
        DVec3::new(80.000, -130.0, 200.0),
    );
    assert!(
        !splits.is_empty(),
        "find_collinear_endpoint_splits must catch an endpoint landing inside a \
         collinear edge — if it stops, the asymmetry this file documents is gone"
    );

    // And it does NOT catch the other direction, because it is only ever asked
    // about the line being drawn.
    let none = m.find_collinear_endpoint_splits(
        DVec3::new(27.639, -130.0, 200.0),
        DVec3::new(67.500, -130.0, 200.0),
    );
    assert!(
        none.is_empty(),
        "the ends coincide with the edge's own ends, so there is nothing to \
         split — this is the shape add_face_with_holes builds blind"
    );
}
