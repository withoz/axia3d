//! Who owns the face a merge leaves behind.
//!
//! D4 of FACE-PIPELINE-PLAN-2026-08-13 names ownership as the open item, and
//! the merge family is where it is most plainly absent: `merge_faces_by_edge`
//! removes BOTH operands and creates one face, so the owner is left holding two
//! dead ids and the survivor belongs to nobody. Every WASM merge call site goes
//! straight to the mesh, so ADR-203 §36's reconcile — which PR #117 wired into
//! the eight carve/drill ops — never runs here.
//!
//! Measured rather than asserted from the code, because the same table's D7 and
//! D9′ turned out not to be defects at all.
//!
//! ⚠ This file states TODAY. Two of its three tests pin an open behaviour, and
//! the fix needs a decision this repo has not taken: when the two merged faces
//! have DIFFERENT owners, which one inherits. Pinning "nobody" is not endorsing
//! it — it is making the day it changes visible.

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

const TOP: f64 = 100.0;

fn production_solid() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    s
}

/// Active faces owned by nobody — neither a Xia nor a Shape names them.
fn unowned(s: &Scene) -> usize {
    s.mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && s.get_xia_for_face(*fid).is_none()
                && !s.face_to_shape.contains_key(fid)
        })
        .count()
}

/// Ids an owner still names that are no longer live faces.
fn dead_ids(s: &Scene) -> usize {
    let live = |f: &axia_geo::FaceId| {
        s.mesh.faces.get(*f).map_or(false, |x| x.is_active())
    };
    let xia: usize = s.xias.values().flat_map(|x| x.face_ids.iter()).filter(|f| !live(f)).count();
    let shape: usize = s.shapes.values().flat_map(|x| x.face_ids.iter()).filter(|f| !live(f)).count();
    xia + shape
}

/// The control: a box straight out of `create_box` is unowned to begin with,
/// so "unowned" only means something as a DIFFERENCE across an operation.
#[test]
fn a_bare_box_is_already_unowned_so_measure_the_change() {
    let s = production_solid();
    assert_eq!(unowned(&s), 6, "a primitive box's faces have no owner");
    assert_eq!(dead_ids(&s), 0, "and nobody names a dead one");
}

/// Drawing a line across an owned face's top and merging it back.
///
/// The draw gives both halves to whoever owned the face (PR #121). The merge
/// then removes both and makes a third — and TODAY nothing hands the ownership
/// on, so the owner is left naming two ids that no longer exist.
#[test]
fn merging_two_owned_halves_leaves_the_survivor_unowned() {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    // An owned sheet: a rect drawn as a Shape owns its face.
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 200.0,
        height: 200.0,
    });
    let owned_before = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && s.face_to_shape.contains_key(fid))
        .count();
    assert_eq!(owned_before, 1, "the drawn rect owns its face");

    // Cut it in two with a line across.
    s.execute(Command::DrawLine {
        start: DVec3::new(-120.0, 0.0, 0.0),
        end: DVec3::new(120.0, 0.0, 0.0),
        surface_normal: Some(DVec3::Z),
    });
    let halves: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && s.face_to_shape.contains_key(fid))
        .map(|(fid, _)| fid)
        .collect();
    assert_eq!(halves.len(), 2, "both halves stay with the shape (PR #121)");

    // Merge them back across the edge they share.
    let shared = s
        .mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .find(|(eid, _)| {
            let (fs, _) = s.mesh.get_faces_sharing_edge(*eid);
            let live: Vec<_> = fs
                .iter()
                .copied()
                .filter(|f| s.mesh.faces.get(*f).map_or(false, |x| x.is_active()))
                .collect();
            live.len() == 2 && live.iter().all(|f| halves.contains(f))
        })
        .map(|(eid, _)| eid)
        .expect("the two halves share an edge");
    let merged = s.mesh.merge_faces_by_edge(shared).expect("merge");

    assert!(
        s.mesh.faces.get(merged).map_or(false, |f| f.is_active()),
        "the merge produced a live face"
    );
    assert!(
        !s.face_to_shape.contains_key(&merged) && s.get_xia_for_face(merged).is_none(),
        "TODAY the survivor belongs to nobody — when a reconcile lands it will \
         name an owner and this assertion is what says so"
    );
    assert_eq!(
        dead_ids(&s),
        2,
        "TODAY the shape still names both halves, which are gone"
    );
}

/// The same on a solid, which is where it costs something: an owner naming dead
/// ids is what made a drilled solid unsliceable (PR #117) and put an orphan
/// element in an IFC file (PR #118). Those two fixed the carve family; merge
/// was explicitly out of their scope.
#[test]
fn merging_on_a_solid_top_leaves_dead_ids_behind() {
    let mut s = production_solid();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 80.0,
        height: 80.0,
    });
    let before_dead = dead_ids(&s);
    let owned: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && s.face_to_shape.contains_key(fid))
        .map(|(fid, _)| fid)
        .collect();
    assert!(!owned.is_empty(), "the drawn rect owns something");

    // Merge the drawn piece into a neighbour across a shared edge.
    let candidate = s
        .mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .find(|(eid, _)| {
            let (fs, _) = s.mesh.get_faces_sharing_edge(*eid);
            let live: Vec<_> = fs
                .iter()
                .copied()
                .filter(|f| s.mesh.faces.get(*f).map_or(false, |x| x.is_active()))
                .collect();
            live.len() == 2 && live.iter().any(|f| owned.contains(f))
        })
        .map(|(eid, _)| eid);
    // No escape hatch: a test that quietly returns proves nothing, and this
    // repo has been caught by exactly that (ADR-299's `expect`-less specs).
    // ⚠ Written with two `return`s first, and it PASSED that way — on this
    // configuration the merge refuses, so the assertion below was never
    // reached. That is worth stating rather than hiding: on a solid top the
    // drawn piece and its neighbour do not merge, so the ownership gap has no
    // reach here, and the sheet case above is what carries the evidence.
    let eid = candidate.expect("the drawn piece shares an edge with a neighbour");
    let refusal = s.mesh.merge_faces_by_edge(eid).err().map(|e| e.to_string());
    let Some(why) = refusal else {
        // It merged after all — then the gap DOES reach here and must be shown.
        assert!(
            dead_ids(&s) > before_dead,
            "a merge on a solid leaves the owner naming ids that are gone"
        );
        return;
    };
    assert!(
        !why.is_empty(),
        "the merge refuses on a solid top, and the reason is why this case \
         cannot exercise the ownership gap: {why}"
    );
    assert_eq!(
        dead_ids(&s),
        before_dead,
        "a refused merge must leave the owner's list exactly as it was"
    );
}
