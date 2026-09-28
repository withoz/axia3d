//! I7 — A HALF-EDGE NAMES A FACE THAT IS STILL THERE.
//!
//! A face dropped without freeing its loops leaves every half-edge of them
//! naming it, and once the Face is out of storage that name means nothing. Such
//! a half-edge sits in a third state that no reader expects:
//!
//! ```text
//!   not free    `face().is_null()` is false, so `find_halfedge` Pass 1 will
//!               not reuse it and Pass 2 allocates a second pair alongside
//!   not valid   `faces.contains(he.face())` is false, so anything that
//!               indexes `faces[he.face()]` finds nothing there
//! ```
//!
//! Two ways in, and both are now closed. `faces.remove(id)` in place of
//! `remove_face(id)` — the three punches did that (measured 2026-09-28: one
//! punch left 4, a through-drill 8). And `remove_face` / `soft_remove_face`
//! themselves, which free only the half-edges they can WALK: a merge takes the
//! shared edge out before removing its two faces, and a chain split rewrites the
//! loops before removing the face, so the walk finds a short loop or none and
//! the rest keep the name. Both now sweep for whatever still names the face.
//!
//! The sweep costs a pass over every half-edge per removal. Measured on the
//! heaviest suite (`the_fifty_operation_inventory`, 41 random ops × 6 sessions):
//! 25.05s → 25.61s, about 2%.
//!
//! Ids are never recycled — `SlotStorage::insert` only counts up — so a dangling
//! name can only ever mean "gone", never "some other face now". It is a panic
//! waiting on an unguarded index, not silent corruption.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

fn boxed() -> Mesh {
    let mut m = Mesh::new();
    m.create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, mat()).expect("box");
    m
}

fn dangling_violations(m: &Mesh) -> Vec<String> {
    m.verify_face_invariants()
        .violations
        .into_iter()
        .filter(|v| v.contains("which is not in storage"))
        .collect()
}

#[test]
fn the_verifier_reports_a_face_dropped_without_freeing_its_loops() {
    let mut m = boxed();
    assert!(m.verify_face_invariants().is_valid(), "a fresh box is sound");

    let fid = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a face");
    m.faces.remove(fid); // the raw drop — what the punches used to do

    let found = dangling_violations(&m);
    assert_eq!(
        found.len(),
        4,
        "a quad's four half-edges still name it; the verifier said {found:?}"
    );
    assert!(
        found.iter().all(|v| v.contains(&format!("{fid:?}"))),
        "and each one names that face: {found:?}"
    );
}

#[test]
fn remove_face_leaves_the_verifier_nothing_to_report() {
    let mut m = boxed();
    let fid = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a face");
    m.remove_face(fid).expect("remove_face");

    assert!(
        dangling_violations(&m).is_empty(),
        "remove_face frees the loops it drops"
    );
    let free = m
        .hes
        .iter()
        .filter(|(_, h)| h.is_active() && h.face().is_null())
        .count();
    assert_eq!(free, 4, "its four half-edges are free now, not dangling");
}

/// The sweep, on the case that needs it: a face whose loops no longer walk to
/// everything that names it. Breaking one `next` pointer is enough to make
/// `collect_loop_hes` come back short, which is the shape a merge and a chain
/// split both produce by rewriting before removing.
#[test]
fn a_face_whose_loop_no_longer_walks_is_still_freed() {
    let mut m = boxed();
    let fid = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a face");

    // Cut the loop: the walk from `start` can no longer reach the rest.
    let start = m.faces[fid].outer().start;
    let second = m.hes[start].next();
    m.hes[second].set_next(second); // a one-step cycle, so the walk ends early

    m.remove_face(fid).expect("remove_face");
    let found = dangling_violations(&m);
    assert!(
        found.is_empty(),
        "the sweep must catch what the walk could not reach: {found:?}"
    );
}
