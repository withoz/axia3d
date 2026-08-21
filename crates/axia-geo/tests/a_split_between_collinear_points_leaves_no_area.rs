//! Counting steps along a boundary is not the same as having area.
//!
//! `Mesh::split_face` guarded itself with `dist_fwd >= 2 && dist_bwd >= 2` —
//! how many VERTICES apart the two cut points are. Three apart is plenty,
//! unless those vertices are COLLINEAR, and then that side of the split is a
//! ring of zero width.
//!
//! It happens on a face whose boundary already runs along one straight line in
//! more than one piece: a long edge on one side, its sub-edges on the other,
//! and a cut joining two points on that same line. Measured 2026-08-21 in
//! wide-fuzz session 12 op 29 — a plain rect draw, whose four sides each go
//! through `split_face_by_line`:
//!
//! ```text
//!     FaceId(86)   3 steps apart   arc areas 0.000000 / 286.97
//!     FaceId(141)  4 steps apart   arc areas 0.000000 / 181.93
//! ```
//!
//! The second left `FaceId(142)`: four vertices at four DISTINCT positions,
//! all at x = 67.5 (the rect's own left edge), area exactly zero, stacked on
//! its 19,256 mm² neighbour and reported non-manifold by
//! `verify_face_invariants`.
//!
//! ⚠⚠ THE OBVIOUS FIX MAKES THINGS WORSE, MEASURED. Refusing the split (an
//! area check beside the step count) removes `FaceId(142)` — and session 12
//! then fails EARLIER, at op 15 instead of op 29, with a stacked pair the
//! refusal left behind. Traced: the guard fires exactly ONCE in that session
//! (`FaceId(86)`, arc areas 0.000000 / 286.97), and bypassing that one refusal
//! moves the failure back to op 29. So the zero-width face is currently
//! LOAD-BEARING: something downstream needs that division to have happened,
//! and refusing it strands an overlap instead.
//!
//! The engine is therefore UNCHANGED. What is pinned below is the behaviour as
//! it stands, with the collinear case named so the next attempt starts from
//! measurement rather than from the same wrong guess. The real fix has to
//! divide the face WITHOUT emitting the flat side — which means changing what
//! `split_face` produces, not whether it runs.
//!
//! ⚠ It cost FOUR wrong diagnoses first, all of which asked who CREATED the
//! face. Nobody did, in the sense they meant: a split does its own DCEL surgery
//! (`faces.insert` + half-edge splicing) and never passes through
//! `add_face_with_holes`, so a trace on that one door saw nothing. What found
//! it was tracing the FaceId itself at `SlotStorage::insert`.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;

/// A rectangle whose left side is carried by TWO edges, not one — the shape
/// that lets a later cut join two collinear points.
///
/// ```text
///     (0,100) ──────────── (100,100)
///        │                     │
///     (0,50)   ← an extra vertex on the left side
///        │                     │
///     (0,0)  ───────────── (100,0)
/// ```
fn rect_with_a_split_left_side() -> (Mesh, Vec<axia_geo::VertId>) {
    let mut m = Mesh::new();
    let vs: Vec<axia_geo::VertId> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(100.0, 0.0, 0.0),
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(0.0, 100.0, 0.0),
        DVec3::new(0.0, 50.0, 0.0), // on the left side, between (0,100) and (0,0)
    ]
    .iter()
    .map(|p| m.add_vertex(*p))
    .collect();
    let f = m
        .add_face(&[vs[0], vs[1], vs[2], vs[3], vs[4]], MaterialId::new(0))
        .expect("a five-vertex rectangle");
    let _ = f;
    (m, vs)
}

/// Two collinear points, far enough apart by the step count, currently
/// ACCEPTED — and the result is a face with no area.
///
/// This is the defect, recorded rather than fixed (see the header). It asserts
/// today's behaviour so the day it changes, this fails and says what changed.
#[test]
fn a_cut_between_collinear_points_makes_a_flat_face() {
    let (mut m, vs) = rect_with_a_split_left_side();
    let f = m
        .faces
        .iter()
        .find(|(_, x)| x.is_active())
        .map(|(id, _)| id)
        .expect("the face");
    let before = m.faces.iter().filter(|(_, x)| x.is_active()).count();

    // (0,100) and (0,0) — three steps apart the long way round, and the SHORT
    // way runs (0,100) → (0,50) → (0,0), which is a straight line.
    let (a, b) = m
        .split_face(f, vs[3], vs[0])
        .expect("the step-count guard lets this through");
    let after = m.faces.iter().filter(|(_, x)| x.is_active()).count();
    let (aa, ab) = (m.face_area(a), m.face_area(b));
    println!("
  일직선 컷 -> {a:?} 넓이 {aa:.6},  {b:?} 넓이 {ab:.6}   면 {before} -> {after}
");
    assert_eq!(after, before + 1, "it does split, into two");
    // THE DEFECT: one of them has no area at all.
    assert!(
        aa.min(ab) < 1e-9,
        "expected one flat side; got {aa:.6} and {ab:.6} — if this now fails, the collinear case was fixed and the header needs rewriting"
    );
    assert!(
        aa.max(ab) > 1.0,
        "and the other side should be the real one: {aa:.6} / {ab:.6}"
    );
}


/// The control: a cut that DOES enclose area on both sides comes out with area
/// on both sides. Without this, "every split makes a flat face" would also pass
/// the test above.
#[test]
fn a_cut_across_the_face_still_splits_it() {
    let (mut m, vs) = rect_with_a_split_left_side();
    let f = m
        .faces
        .iter()
        .find(|(_, x)| x.is_active())
        .map(|(id, _)| id)
        .expect("the face");
    let before = m.faces.iter().filter(|(_, x)| x.is_active()).count();

    // (100,0) to (0,100) — a diagonal, with real area on both sides.
    let (a, b) = m.split_face(f, vs[1], vs[3]).expect("a diagonal cut divides it");
    let after = m.faces.iter().filter(|(_, x)| x.is_active()).count();
    println!("
  대각선 컷 -> {a:?} + {b:?},  면 {before} -> {after}
");
    assert_eq!(after, before + 1, "one face became two");
    for id in [a, b] {
        assert!(
            m.face_area(id) > 1.0,
            "{id:?} came out with no area ({})",
            m.face_area(id)
        );
    }
    assert!(
        m.verify_face_invariants().is_valid(),
        "{:?}",
        m.verify_face_invariants().violations
    );
}

/// The old guard, restated so it is clear the new one did not replace it:
/// adjacent vertices are still refused, and for a different reason.
#[test]
fn adjacent_vertices_are_still_refused() {
    let (mut m, vs) = rect_with_a_split_left_side();
    let f = m
        .faces
        .iter()
        .find(|(_, x)| x.is_active())
        .map(|(id, _)| id)
        .expect("the face");
    let r = m.split_face(f, vs[0], vs[1]);
    assert!(r.is_err(), "neighbours on the boundary cannot be cut apart");
}
