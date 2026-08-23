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
//! ⚠⚠⚠ AND SO DOES EVERY OTHER WAY OF NOT EMITTING IT. Three were built and
//! measured, and all three moved session 12's failure from op 29 to op 15:
//!
//!   1. refuse the split (`Err`) — `exec_draw_line` does `continue`, so the
//!      whole segment is dropped;
//!   2. add the edge `v1↔v2` but emit no second face;
//!   3. make no cut at all and return an empty `new_faces`.
//!
//! Bypassing the one place it fires puts the failure back at op 29 every time.
//! So what the pipeline depends on is not the edge, and not the flat face —
//! it is **the division having happened**. Here is the case, in full:
//!
//! ```text
//!   cut    VertId(152)(27.872, -130, 200) → VertId(147)(67.500, -130, 200)
//!   arc A  [152, 148, 147]                   all at y = -130 — no width
//!   arc B  [147, 79, 28, 29, 30, 31, 152]    the real piece, area 280.7
//! ```
//!
//! `VertId(148)` sits on the flat arc, 0.233 mm from one end of the cut. Any
//! answer has to say where that vertex goes, and none of the three above did —
//! which is why they all failed the same way.
//!
//! The engine is therefore UNCHANGED. What is pinned below is the behaviour as
//! it stands, with the collinear case named so the next attempt starts from
//! measurement rather than from a fourth guess.
//!
//! ── Re-measured 2026-08-23, and three details above are wrong ──────────
//!
//! The conclusion held; the mechanism did not. Running session 12 with the
//! area check actually applied, against the same session unpatched:
//!
//! ```text
//!                                    미수정      넓이 가드
//!   최종 violation                      2            13
//!   그중 NaN 법선                        0             2
//!   넓이 0 면                            1             0
//! ```
//!
//! So the corrections:
//!
//!   1. The guard fires TWICE, not once — `FaceId(86)` (arc areas 0.000000 /
//!      286.967811) and `FaceId(137)` (0.000000 / 181.930198).
//!   2. Both `FaceId(86)` and `FaceId(141)` were said to be "session 12 op 29".
//!      They are two different moments; the classifier only ever named op 29
//!      because its criterion is `face_area == 0.0` EXACTLY, and the earlier
//!      damage is stacked-but-not-flat, so it goes unnamed.
//!   3. "fails EARLIER, at op 15" described the classifier's output moving,
//!      not the damage moving. Unpatched, session 12's FIRST stacked pair is
//!      `FaceId(89)/FaceId(46)`, long before op 29's `143/142/141` family —
//!      it is already broken there. The area check does remove every flat
//!      face, and in exchange the FINAL violation count goes 2 -> 13, two of
//!      them faces with a NaN normal.
//!
//!      ⚠ Count the FINAL block, not the dumps. `debug_verify_invariants`
//!      prints on every mutation, so a cumulative tally (26 vs 92 here) partly
//!      measures how often it ran (8 vs 10), not how much is wrong.
//!
//! That last row is the load-bearing part, now with a number on it. Refusing
//! the cut does not leave the face intact — it leaves it UNDIVIDED, and
//! whatever runs next re-tiles the undivided face over and over.
//!
//! ⚠ A fourth correction, and a fifth guess refuted. The note above said
//! `exec_draw_line` does `continue` on an `Err`. It does not — `scene.rs`'s
//! `Err(_)` arm falls through to (b) free-edge loop detection, commented
//! `// split 실패 시 loop detection으로 fallback`. That looked like the
//! re-tiler, so it was tested: guard PLUS `continue` on that specific error,
//! skipping the fallback entirely. Result identical — 13 violations, the same
//! 2 NaN faces. Loop detection is not where the extra damage comes from.
//!
//! So the next question is narrower than before: with the cut refused, the
//! host face survives undivided and something downstream lays 11 more
//! overlapping faces onto it. Not loop detection. The post-draw arrangement
//! (`face_rederive` / the coplanar reconcile) is the next place to read,
//! because it is what runs on the surviving face after the segment loop ends.
//!
//! ── Answered 2026-08-23. It is not downstream at all ───────────────────
//!
//! Printing the whole boundary loop of both offending faces, rather than just
//! the flat arc, says the damage is already IN the face before the cut:
//!
//! ```text
//!   FaceId(86)  loop  148(27.639,-130) 147(67.500,-130) 79 28 29 30 31
//!                     152(27.872,-130)
//!                     -> 152 lies INSIDE edge 148->147.  s = 0.0058
//!
//!   FaceId(141) loop  262(67.5,-170.337) 86(67.5,-174.367) 149(67.5,-130)
//!                     85 4 3 2 1 263(67.5,-145.725)
//!                     -> edge 86->149 steps over BOTH 262 and 263
//! ```
//!
//! One shape, not two:
//!
//!   **a boundary edge that steps over another vertex of its own loop.**
//!
//! Everything else follows. The two cut points both sit on that overlapping
//! stretch, so one side of the cut is the stretch doubling back on itself and
//! has no width. The cut is not creating the degeneracy; it is the first thing
//! that has to divide by it.
//!
//! ⚠ Two more corrections to the header above, both from the same printout:
//!
//!   - "`VertId(148)` sits on the flat arc, 0.233 mm from one end of the cut"
//!     is true and misleading. 148 is at x = 27.639 and the cut runs
//!     27.872 -> 67.500, so 148 is OUTSIDE the cut, on the far side of its own
//!     start. `find_vertices_on_line` was right not to break the line there —
//!     148 is not on the segment. The question "where does VertId(148) go" was
//!     the wrong question.
//!   - The two faces were described as the same case. They are the same DEFECT
//!     but were reached differently: 86's extra vertex is where an arc landed
//!     0.233 mm inside an existing line edge; 141's are two earlier vertices on
//!     a vertical that a later edge simply spanned.
//!
//! `find_collinear_endpoint_splits` (Step 0, Phase B in `exec_draw_line`) would
//! have accepted 86's case — `s = 0.0058` against its `s_eps = 1e-3`. It did
//! not run on it, because it only ever looks at the endpoints of the line BEING
//! DRAWN against existing edges.
//!
//! ⚠ It is also narrower than "built for exactly this" suggests: it rejects any
//! line not PARALLEL to the existing edge (`cross > 1e-6`), so it answers "a
//! line running along an edge and starting partway into it", not "a vertex
//! landed on an edge". 86's cut did run along the bottom edge, so that case
//! qualifies; the general claim would not. Pinned in
//! `the_door_a_spanning_edge_comes_through.rs`.
//!
//! ## Upstream, found 2026-08-23
//!
//! The damage runs the other way from what this file assumed. The vertex is
//! there FIRST, and an edge is later drawn straight over it — 23 times in
//! session 12, every one through one door:
//!
//! ```text
//!   add_edge  <-  make_loop  <-  add_face_with_holes
//! ```
//!
//! ⚠ Which is the door this file dismisses above: "a split does its own DCEL
//! surgery and never passes through `add_face_with_holes`, so a trace on that
//! one door saw nothing." True of the SPLIT. The upstream producer is that very
//! door — the four traces were watching the right place for the wrong event.
//!
//! See `the_door_a_spanning_edge_comes_through.rs` for the callers and counts.
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

/// The shape the whole file is about, stated as a check anyone can run.
///
/// A boundary edge must not step over another vertex of its own loop. This is
/// what `verify_face_invariants` does NOT look for, and it is the condition
/// that makes a later cut produce a piece with no area.
///
/// Built here from the rectangle above plus an extra vertex placed INSIDE the
/// bottom edge without splitting it — the same arrangement the fuzz reached by
/// landing an arc 0.233 mm inside an existing line.
fn steps_over_own_vertex(
    m: &Mesh,
    f: axia_geo::FaceId,
) -> Option<(DVec3, DVec3, DVec3)> {
    let face = m.faces.get(f)?;
    if !face.is_active() { return None; }
    let verts = m.collect_loop_verts(face.outer().start).ok()?;
    let n = verts.len();
    for i in 0..n {
        let a = m.vertex_pos(verts[i]).ok()?;
        let b = m.vertex_pos(verts[(i + 1) % n]).ok()?;
        let ab = b - a;
        let len = ab.length();
        if len < 1e-9 { continue; }
        let dir = ab / len;
        for (j, &v) in verts.iter().enumerate() {
            if j == i || j == (i + 1) % n { continue; }
            let p = m.vertex_pos(v).ok()?;
            let w = p - a;
            let along = w.dot(dir);
            if along <= 1e-6 || along >= len - 1e-6 { continue; }
            if (w - dir * along).length() > 1e-6 { continue; }
            return Some((a, b, p));
        }
    }
    None
}

#[test]
fn an_edge_that_steps_over_its_own_vertex_is_findable() {
    // A clean rectangle does not have it.
    let (m, _verts) = rect_with_a_split_left_side();
    let clean: Vec<axia_geo::FaceId> = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    for &f in &clean {
        assert_eq!(
            steps_over_own_vertex(&m, f),
            None,
            "a plain rectangle must not have an edge stepping over its own vertex"
        );
    }

    // And it must be able to say YES, or the loop above proves nothing. Build
    // the exact shape measured off FaceId(86): a loop whose LAST vertex sits
    // in the interior of the edge formed by its first two.
    //
    // ⚠ The first version of this test stopped at the `None` loop plus two
    // assertions about arithmetic, and passed with the detector stubbed to
    // return None. Mutation-checked now: return None unconditionally from
    // `steps_over_own_vertex` and this fails.
    let mut bad = Mesh::new();
    let vs: Vec<axia_geo::VertId> = [
        DVec3::new(27.639, -130.0, 200.0), // 148 — edge start
        DVec3::new(67.500, -130.0, 200.0), // 147 — edge end
        DVec3::new(67.500, -125.633, 200.0),
        DVec3::new(32.700, -125.491, 200.0),
        DVec3::new(27.872, -130.0, 200.0), // 152 — INSIDE 148->147
    ]
    .iter()
    .map(|q| bad.add_vertex(*q))
    .collect();
    let bf = bad
        .add_face(&[vs[0], vs[1], vs[2], vs[3], vs[4]], MaterialId::new(0))
        .expect("the fuzz built this shape, so the kernel accepts it");

    let found = steps_over_own_vertex(&bad, bf);
    let (ea, eb, over) = found.expect(
        "an edge stepping over its own loop vertex must be detectable — this is \
         the shape session 12 hit twice",
    );
    // Winding is the kernel's to choose, so assert the GEOMETRY, not indices:
    // the offending edge is the long bottom run, and 152 is what it steps over.
    let span = [ea.x, eb.x];
    assert!(
        span.contains(&27.639) && span.contains(&67.500),
        "the offending edge must be the bottom run 27.639 <-> 67.500, got {ea:?} {eb:?}"
    );
    assert!(
        (over - DVec3::new(27.872, -130.0, 200.0)).length() < 1e-9,
        "and the vertex it steps over must be 152 at x=27.872, got {over:?}"
    );

    // ⚠ And the kernel currently calls this face sound. That is the gap: the
    // damage is invisible until something has to divide by it.
    let inv = bad.verify_face_invariants();
    assert!(
        inv.is_valid(),
        "if this ever starts failing, verify_face_invariants learned to see the \
         defect and this file's premise needs revisiting — violations: {:?}",
        inv.violations
    );
}
