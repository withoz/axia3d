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
//! ⚠ This header described an OPEN gap until 2026-08-16, and the gap is
//! closed: every WASM merge entry goes through `Scene::merge_faces_by_edge_owned`
//! — measured, zero direct-to-mesh merge call sites remain — and the decision
//! the header said had not been taken was taken (first-selected inherits,
//! 사용자 결재 2026-08-13) and is pinned below. The one test still describing
//! a limit is the solid-top case, and what it pins is a REFUSAL: on that
//! configuration the two faces do not merge at all, so the ownership question
//! has no reach there.

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
/// then removes both and makes a third, and the third must join the shape the
/// halves belonged to.
///
/// ⚠ This was named `..._leaves_the_survivor_unowned` until 2026-08-16, from
/// when it pinned the gap rather than the fix. The body had been updated and
/// the name had not, so a search for the defect found a test that asserts its
/// absence — the same shape of doc-lag LOCKED #88 exists to prevent.
#[test]
fn the_survivor_of_a_merge_keeps_the_shape_that_owned_the_halves() {
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
    // Through the SCENE, which is where ownership lives — every WASM merge
    // entry goes this way now. `preferred` names the winner; here both halves
    // have the same owner so either answers.
    let merged = s
        .merge_faces_by_edge_owned(shared, halves.first().copied(), None)
        .expect("merge");

    assert!(
        s.mesh.faces.get(merged).map_or(false, |f| f.is_active()),
        "the merge produced a live face"
    );
    assert!(
        s.face_to_shape.contains_key(&merged),
        "the survivor keeps the shape that owned the halves — this read \
         'nobody' while every WASM merge site called the mesh straight and \
         skipped the reconcile the carve family got in PR #117"
    );
    assert_eq!(
        dead_ids(&s),
        0,
        "and the shape stops naming the two halves, which are gone"
    );
}

/// The same on a solid — where, measured, the merge REFUSES.
///
/// The body has always allowed both answers and the refusal is the one that
/// happens, so the name said the wrong half until 2026-08-16
/// (`..._leaves_dead_ids_behind`). What it actually pins is that a refused
/// merge is free: the owner's list comes out exactly as it went in. The
/// merging branch is kept because the day the configuration starts merging is
/// the day the ownership question reaches here.
#[test]
fn a_merge_refused_on_a_solid_top_costs_nothing() {
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

/// The rule itself: when the two faces have DIFFERENT owners, the one named
/// first inherits (사용자 결재 2026-08-13).
///
/// Not the larger face and not the lower id — the operand the user picked
/// first. Which is why the winner is a PARAMETER: "first selected" is a fact
/// about clicks, and the mesh op takes an edge, not a sequence. The caller that
/// knows the selection order names it.
#[test]
fn the_first_named_owner_inherits_when_the_two_differ() {
    let build = || -> (Scene, axia_geo::EdgeId, Vec<axia_geo::FaceId>) {
        let mut s = Scene::new();
        s.auto_intersect_on_draw = true;
        s.auto_face_synthesis_on_draw = true;
        s.face_rederive_on_draw = true;
        s.execute(Command::DrawRectAsShape {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            up: DVec3::X,
            width: 200.0,
            height: 200.0,
        });
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
        assert_eq!(halves.len(), 2);
        // Give the second half a shape of its own, so the two differ.
        let other = s.create_shape("Other".to_string(), vec![halves[1]]);
        assert_ne!(
            s.face_to_shape.get(&halves[0]).copied(),
            Some(other),
            "the halves must now have different owners"
        );
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
            .expect("the halves share an edge");
        (s, shared, halves)
    };

    // Name the first half → the merged face joins the FIRST half's shape.
    let (mut s, edge, halves) = build();
    let want_first = s.face_to_shape.get(&halves[0]).copied().expect("half 0 owned");
    let merged = s
        .merge_faces_by_edge_owned(edge, Some(halves[0]), None)
        .expect("merge");
    assert_eq!(
        s.face_to_shape.get(&merged).copied(),
        Some(want_first),
        "naming the first half must hand the result to ITS shape"
    );

    // Name the second → the other shape wins instead. Same geometry, same edge:
    // only the pick differs, which is what makes this the rule and not an
    // accident of ids or areas.
    let (mut s2, edge2, halves2) = build();
    let want_second = s2.face_to_shape.get(&halves2[1]).copied().expect("half 1 owned");
    let merged2 = s2
        .merge_faces_by_edge_owned(edge2, Some(halves2[1]), None)
        .expect("merge");
    assert_eq!(
        s2.face_to_shape.get(&merged2).copied(),
        Some(want_second),
        "and naming the second hands it to the other one"
    );
    assert_ne!(want_first, want_second, "the two picks really are different shapes");
}
