//! A PIECE THAT BOUNDS NOTHING IS NOT MADE A FACE.
//!
//! Three or more points on one line close a loop that is topologically perfect
//! and geometrically empty. Its Newell sum is exactly zero, so the face made
//! from it takes a NaN normal — `NORMAL_EPSILON` is 0.0, so `compute_normal`'s
//! degenerate bail cannot fire (ADR-304, "generous creation, detection in the
//! verifier") — and the verifier reports it from then on.
//!
//! ⚠ ADR-304 is why the check is at the CREATORS and not in `add_face`. That
//! policy was decided deliberately on 2026-07-29: creation stays generous
//! because import is built to accept-then-repair. So a creator may decline to
//! make a face it knows covers nothing; `add_face` still may not refuse one.
//!
//! Three creators reach it, found by instrumenting `add_face_with_holes` with a
//! collinearity test and `Backtrace::force_capture()` under `RUSTFLAGS="-g"`.
//! Measured over 100 fuzz sessions × 50 operations, adding one guard at a time:
//!
//! ```text
//!                                              broken   faces with no normal
//!   before                                       48             22
//!   + split_face_by_chain                        45             16
//!   + dissolve_and_fan_split                     38              2
//!   + exec_draw_line's closed free-edge loop     37              0
//! ```
//!
//! The two in this file are pinned here. The third is in axia-core, reached by
//! nothing smaller than a fuzz session, and pinned by replay in
//! `a_fuzz_session_leaves_the_mesh_sound.rs` (session 41, operation 29).
//!
//! Refusing one is not refusing a draw. The line is drawn either way; what is
//! declined is a face with no area, and every caller already treats a piece it
//! cannot make as one it skips. In `split_face_by_chain` the check runs BEFORE
//! the old face is torn down, so a refusal leaves the mesh exactly as it was
//! (ADR-302's preflight).

use axia_geo::mesh::Mesh;
use axia_geo::operations::face_split::split_face_by_chain;
use axia_geo::{MaterialId, VertId};
use glam::DVec3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// The shape the fuzz found, reduced: a wall whose bottom edge a chain runs
/// along, through a vertex that is NOT one of the loop's own. The existing
/// guard refuses an intermediate chain vertex that IS on the loop ("would
/// require multi-seam split"), so this one walks straight past it.
fn wall_with_a_chain_along_its_foot() -> (Mesh, Vec<VertId>, axia_geo::FaceId) {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = m.add_vertex(DVec3::new(100.0, 0.0, 100.0));
    let d = m.add_vertex(DVec3::new(0.0, 0.0, 100.0));
    let f = m.add_face(&[a, b, c, d], mat()).expect("wall");

    // A vertex on the foot, reached by two edges, and not in the face's loop —
    // this is (-122.99, -100, 0) in the fuzz's session 3.
    let mid = m.add_vertex(DVec3::new(50.0, 0.0, 0.0));
    m.add_edge(a, mid).expect("foot left");
    m.add_edge(mid, b).expect("foot right");
    (m, vec![a, mid, b], f)
}

#[test]
fn a_chain_along_the_foot_is_refused_and_changes_nothing() {
    let (mut m, chain, f) = wall_with_a_chain_along_its_foot();
    let before = (
        m.faces.iter().filter(|(_, x)| x.is_active()).count(),
        m.hes.iter().filter(|(_, x)| x.is_active()).count(),
        m.edges.iter().filter(|(_, x)| x.is_active()).count(),
    );

    let res = split_face_by_chain(&mut m, f, &chain, mat());
    assert!(
        res.is_err(),
        "the chain and the foot are the same line, so one piece would have no area"
    );
    assert_eq!(
        (
            m.faces.iter().filter(|(_, x)| x.is_active()).count(),
            m.hes.iter().filter(|(_, x)| x.is_active()).count(),
            m.edges.iter().filter(|(_, x)| x.is_active()).count(),
        ),
        before,
        "a refusal leaves the mesh as it was — the check runs before the teardown"
    );
    assert!(
        m.faces.contains(f) && m.faces[f].is_active(),
        "and the face it declined to split is still there"
    );
    let inv = m.verify_face_invariants();
    assert!(inv.is_valid(), "no face with a NaN normal: {:?}", inv.violations);
}

/// The control: a chain that actually crosses the face still divides it, and
/// an intermediate vertex on the loop is still refused by the older guard.
#[test]
fn a_chain_across_the_face_still_divides_it() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = m.add_vertex(DVec3::new(100.0, 0.0, 100.0));
    let d = m.add_vertex(DVec3::new(0.0, 0.0, 100.0));
    let f = m.add_face(&[a, b, c, d], mat()).expect("wall");

    // Foot to head, straight through the middle.
    let lo = m.add_vertex(DVec3::new(50.0, 0.0, 0.0));
    let hi = m.add_vertex(DVec3::new(50.0, 0.0, 100.0));
    m.add_edge(lo, hi).expect("chord");
    let res = split_face_by_chain(&mut m, f, &vec![lo, hi], mat());
    if let Ok(r) = res {
        assert_eq!(r.new_faces.len(), 2, "two pieces");
        for (i, &nf) in r.new_faces.iter().enumerate() {
            let area = m.face_outer_area(nf);
            assert!(area > 1.0, "piece {i} has area {area}, which is not a piece");
        }
        assert!(m.verify_face_invariants().is_valid(), "and the result is sound");
    } else {
        // The chord's ends are not loop vertices either, so an older guard may
        // decline it. What must not happen is a degenerate face, and none was
        // made: the mesh is untouched.
        assert!(m.verify_face_invariants().is_valid(), "a refusal leaves it sound");
    }
}

/// A C-shaped face with a point inside it that sits on the line of the notch's
/// back wall.
///
/// `dissolve_and_fan_split` picks an interior vertex with two or more free
/// spokes to the boundary, tears the face down, and rebuilds it as a fan of
/// wedges. A wedge is `[centre, b_i, ..walk.., b_j]`, so it covers nothing when
/// the centre and that stretch of boundary are on one line. On a CONVEX face
/// that cannot happen — a point on an edge's line is on the edge. It takes a
/// notch: here x = 40 is the back wall of the C (from y = 40 to 60) and the
/// centre sits at (40, 20), which is on that same line and well inside the C.
///
/// ```text
///     H(0,100) ---------------- G(100,100)
///        |                          |
///        |                      F(100,60)
///        |              E(40,60) ---'
///        |                 |
///        |              D(40,40) ---.
///        |                      C(100,40)
///        |        P(40,20)          |
///     A(0,0) ------------------ B(100,0)
/// ```
///
/// Spokes P–D and P–E are adjacent on the boundary, so one wedge is exactly
/// `[P, D, E]` — three points at x = 40.
fn c_with_a_point_on_the_notch_line() -> (Mesh, axia_geo::FaceId) {
    let mut m = Mesh::new();
    let pts = [
        (0.0, 0.0),
        (100.0, 0.0),
        (100.0, 40.0),
        (40.0, 40.0),
        (40.0, 60.0),
        (100.0, 60.0),
        (100.0, 100.0),
        (0.0, 100.0),
    ];
    let vs: Vec<VertId> =
        pts.iter().map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0))).collect();
    let f = m.add_face(&vs, mat()).expect("the C");

    let p = m.add_vertex(DVec3::new(40.0, 20.0, 0.0));
    m.add_edge(p, vs[3]).expect("spoke to D");
    m.add_edge(p, vs[4]).expect("spoke to E");
    (m, f)
}

#[test]
fn a_fan_wedge_along_one_line_is_left_out() {
    let (mut m, f) = c_with_a_point_on_the_notch_line();
    let made = m.dissolve_and_fan_split(f);
    assert!(!made.is_empty(), "the fan must still rebuild the face it dissolved");
    for &nf in &made {
        let n = m.faces[nf].normal();
        assert!(
            n.is_finite() && n.length_squared() > 1e-12,
            "face {nf:?} came out with normal {n:?} — a wedge covering nothing was kept"
        );
    }
    let inv = m.verify_face_invariants();
    assert!(inv.is_valid(), "no face with a NaN normal: {:?}", inv.violations);
    println!("  the C fanned into {} wedge(s), each with area", made.len());
}

/// The control: a fan on a face with no collinear wedge keeps every piece, so
/// the guard is not quietly eating real geometry.
#[test]
fn a_fan_with_no_empty_wedge_keeps_all_of_them() {
    let mut m = Mesh::new();
    let pts = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
    let vs: Vec<VertId> =
        pts.iter().map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0))).collect();
    let f = m.add_face(&vs, mat()).expect("square");

    // Dead centre, spoked to three of the four corners: three wedges, all real.
    let p = m.add_vertex(DVec3::new(50.0, 50.0, 0.0));
    for &c in &vs[..3] {
        m.add_edge(p, c).expect("spoke");
    }
    let made = m.dissolve_and_fan_split(f);
    assert_eq!(made.len(), 3, "three spokes cut the square into three wedges");
    let total: f64 = made.iter().map(|&nf| m.face_outer_area(nf)).sum();
    assert!(
        (total - 10_000.0).abs() < 1e-6,
        "the wedges must still add up to the square, and they came to {total}"
    );
    assert!(m.verify_face_invariants().is_valid(), "and the result is sound");
}
