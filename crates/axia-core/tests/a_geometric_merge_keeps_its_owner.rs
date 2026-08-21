//! Merging two faces geometrically left the result owned by nobody.
//!
//! Eight WASM entry points merge faces. Five go through
//! `Scene::merge_faces_by_edge_owned`, which reads the operands' owner BEFORE
//! the merge (the operands are gone afterwards) and adopts the result into it.
//! Three do not — they call `Mesh::merge_coplanar_faces_geometric` and
//! `Mesh::merge_coplanar_containing` directly, and those are mesh-level: they
//! remove both operands and add a face nobody owns.
//!
//! The cost is visible at export. A Shape keeps two dead ids and the live face
//! belongs to no element, so IFC writes the Shape's body from nothing and the
//! merged region comes out as an orphan.
//!
//! ⚠ Which owner wins is the user's decision, taken 2026-08-11 for the
//! edge-merge sibling and applied here unchanged: **the first-selected face
//! inherits**. `merge_faces_by_edge_owned` spells it `preferred` and falls back
//! to whichever operand owns anything.

use axia_core::scene::Scene;
use axia_core::Command;
use axia_geo::CreateSolidMode;
use glam::DVec3;

/// A box, then its top divided by a drawn rect — two coplanar faces with one
/// owner between them.
fn a_box_with_a_divided_top() -> (Scene, Vec<axia_geo::FaceId>) {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;

    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });
    let base = *s
        .list_shape_ids()
        .first()
        .and_then(|id| s.get_shape(*id))
        .map(|sh| sh.face_ids.first().expect("a face"))
        .expect("a shape");
    s.execute(Command::CreateSolid {
        face_id: base,
        mode: CreateSolidMode::Extrude { distance: 200.0 },
    });

    // Two coplanar pieces on the top by drawing a smaller rect inside it.
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, 200.0),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 150.0,
        height: 150.0,
    });

    let top: Vec<axia_geo::FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(id, f)| {
            f.is_active()
                && f.normal().normalize_or_zero().z > 0.9
                && s.mesh
                    .collect_loop_verts(f.outer().start)
                    .ok()
                    .and_then(|v| v.first().and_then(|x| s.mesh.vertex_pos(*x).ok()))
                    .map(|p| (p.z - 200.0).abs() < 1e-6)
                    .unwrap_or(false)
                && {
                    let _ = id;
                    true
                }
        })
        .map(|(id, _)| id)
        .collect();
    (s, top)
}

fn owner_of(s: &Scene, f: axia_geo::FaceId) -> Option<String> {
    s.shapes
        .iter()
        .find(|(_, sh)| sh.face_ids.contains(&f))
        .map(|(id, _)| format!("Shape {}", id.raw()))
        .or_else(|| {
            s.xias
                .iter()
                .find(|(_, x)| x.face_ids.contains(&f))
                .map(|(id, _)| format!("XIA {id}"))
        })
}

/// A geometric merge hands the result to the first operand's owner.
///
/// Mutation-checked: call `mesh.merge_coplanar_faces_geometric` directly
/// instead of the Scene wrapper and the merged face comes back owner-less.
#[test]
fn a_geometric_merge_leaves_the_result_owned() {
    let (mut s, top) = a_box_with_a_divided_top();
    assert!(
        top.len() >= 2,
        "the fixture needs two coplanar pieces on the top, got {}",
        top.len()
    );
    let (a, b) = (top[0], top[1]);
    let before: Vec<Option<String>> = vec![owner_of(&s, a), owner_of(&s, b)];
    assert!(
        before.iter().any(|o| o.is_some()),
        "at least one operand has to have an owner or the test proves nothing"
    );

    // ⚠ Say what the fixture is before asserting anything about it. The first
    // version of this test picked two faces and expected a merge; the merge
    // returned Err and the panic said nothing about why.
    for &f in &top {
        let verts = s
            .mesh
            .faces
            .get(f)
            .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
            .map(|v| v.len())
            .unwrap_or(0);
        let holes = s.mesh.faces.get(f).map(|x| x.inners().len()).unwrap_or(0);
        println!(
            "    {f:?} 정점 {verts} 구멍 {holes} 넓이 {:.0} 주인 {:?}",
            s.mesh.face_area(f),
            owner_of(&s, f)
        );
    }
    let merged = match s.merge_coplanar_faces_geometric_owned(a, b, 1.0) {
        Ok(m) => m,
        Err(e) => {
            // Containment, not adjacency — an inner rect sits INSIDE the top,
            // so the sibling entry is the one that applies.
            println!("    인접 병합 거부: {e}");
            s.merge_coplanar_containing_owned(a, b, 1.0)
                .or_else(|_| s.merge_coplanar_containing_owned(b, a, 1.0))
                .expect("one of the two is contained in the other")
        }
    };
    let after = owner_of(&s, merged);
    println!(
        "\n  기하 병합 — 주인 {:?} + {:?}  →  {:?}\n",
        before[0], before[1], after
    );
    assert!(
        after.is_some(),
        "a merged face must belong to somebody — before: {before:?}"
    );
    assert!(
        !s.mesh
            .faces
            .iter()
            .any(|(id, f)| f.is_active() && id != merged && owner_of(&s, id).is_none() && {
                let n = f.normal().normalize_or_zero();
                n.z > 0.9
            }),
        "and it must not leave an unowned sibling behind"
    );
}

// ⚠ What the fixture showed while this was being written, kept because it is a
// finding and not a passing test.
//
// Drawing a 150 rect inside a 400 box's top gives:
//
// ```text
//   FaceId(7)  4정점 구멍0  넓이  22,500  주인 Shape 1   ← the drawn piece
//   FaceId(8)  4정점 구멍1  넓이 137,500  주인 None      ← the rest of the top
// ```
//
// The drawn piece goes to the solid, which is (a2) working. The REMAINDER —
// the ring the top became — belongs to nobody. That is the split side of the
// same rule this file tests on the merge side, and it is not fixed here.
//
// A test that asserted "the first-selected operand inherits" used to sit at
// this spot and PASSED without merging anything: both operands answered the
// same (one owner, one None), so it took its early return. A guard that cannot
// fail is worse than no guard, so it is gone rather than left green.
