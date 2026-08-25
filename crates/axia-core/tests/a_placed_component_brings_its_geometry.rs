//! Converting a group to a component promises something placement never kept.
//!
//! "컴포넌트로 변환" sits in the menu, in a toolbar dropdown, and in the
//! right-click menu — three ways in. `Command::PlaceComponent` is the way out,
//! and it carried this:
//!
//! ```text
//!     // TODO: 실제 geometry 복제 구현 필요
//!     // 현재는 인스턴스 메타데이터만 생성
//! ```
//!
//! Nothing in `web/src` or `axia-wasm` calls it either, so a component could be
//! MADE and never PLACED. Measured 2026-08-21.
//!
//! ⚠ Written BEFORE the fix, as a simulation, because four plausible diagnoses
//! were refuted by measurement earlier the same day. What it establishes first
//! is the shape of the failure, so the fix has something to be judged against.
//!
//! The duplication primitive already exists and is proven in production:
//! `array_linear_faces(faces, 1, offset)` is exactly what `CopyTool` uses for a
//! single copy. It REFUSES a zero offset, which matters here — placing a
//! component back at its own origin asks for exactly that.

use axia_core::scene::Scene;
use axia_core::Command;
use axia_geo::CreateSolidMode;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// A box, grouped, turned into a component. Returns the def id.
fn a_box_component(s: &mut Scene) -> u32 {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 200.0,
        height: 200.0,
    });
    let base = *s
        .list_shape_ids()
        .first()
        .and_then(|id| s.get_shape(*id))
        .map(|sh| sh.face_ids.first().expect("a face"))
        .expect("a shape");
    s.execute(Command::CreateSolid {
        face_id: base,
        mode: CreateSolidMode::Extrude { distance: 100.0 },
    });

    let faces: Vec<axia_geo::FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    assert!(faces.len() >= 6, "the box needs its six faces, got {}", faces.len());

    let gid = match s.execute(Command::CreateGroup {
        name: "box".into(),
        face_ids: faces,
    }) {
        axia_core::CommandResult::GroupUpdated(g) => g,
        other => panic!("CreateGroup did not return a group: {other:?}"),
    };
    match s.execute(Command::MakeComponent {
        group_id: gid,
        name: "box-def".into(),
    }) {
        axia_core::CommandResult::GroupUpdated(d) => d,
        other => panic!("MakeComponent did not return a def: {other:?}"),
    }
}

/// Placing a component duplicates its geometry at the new position.
#[test]
fn placing_a_component_duplicates_its_faces() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);
    let before = active(&s);
    let def_faces = s
        .groups
        .component_defs
        .get(&def_id)
        .map(|d| d.face_ids.len())
        .unwrap_or(0);
    assert!(def_faces >= 6, "the def must carry the box's faces, got {def_faces}");

    let r = s.execute(Command::PlaceComponent {
        def_id,
        position: DVec3::new(500.0, 0.0, 0.0),
    });
    let after = active(&s);
    println!("\n  면 {before} → {after}  (정의 {def_faces} 면)  결과 {r:?}\n");

    assert!(
        after >= before + def_faces,
        "placing a component must bring its geometry: {before} → {after}, \
         def carries {def_faces} faces"
    );

    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "and leave the mesh sound: {:?}", inv.violations);
}

/// The instance owns what it placed, so the copy can be selected and moved.
#[test]
fn a_placed_instance_owns_the_faces_it_brought() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);
    let inst = match s.execute(Command::PlaceComponent {
        def_id,
        position: DVec3::new(500.0, 0.0, 0.0),
    }) {
        axia_core::CommandResult::GroupUpdated(i) => i,
        other => panic!("PlaceComponent did not return an instance: {other:?}"),
    };
    let owned = s
        .groups
        .component_instances
        .get(&inst)
        .map(|i| i.face_ids.len())
        .unwrap_or(0);
    println!("\n  인스턴스 {inst} 가 가진 면 {owned}\n");
    assert!(
        owned >= 6,
        "an instance with no faces is metadata, not a placement — got {owned}"
    );
    for &f in &s.groups.component_instances[&inst].face_ids {
        assert!(
            s.mesh.faces.get(f).map(|x| x.is_active()).unwrap_or(false),
            "instance holds {f:?}, which is not an active face"
        );
    }
}

/// Placing at the origin asks `array_linear_faces` for a zero offset, which it
/// refuses. Say so IN THE USER'S WORDS instead of passing the kernel's up.
///
/// ⚠ Mutation-checked and found VACUOUS on the first attempt: with the explicit
/// guard removed the test still passed, because `array_linear_faces` refuses a
/// zero offset on its own and the wrapper turns that into a `CommandResult::
/// Error` either way. What the guard actually buys is the MESSAGE — "원본과
/// 겹칩니다" instead of "array_linear: offset must be non-zero (use copies=1
/// with zero offset for no-op)", which is written for a caller, not a person
/// clicking ⊕ in a panel. So that is what this now asserts.
#[test]
fn placing_at_the_definitions_own_origin_is_refused_cleanly() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);
    let origin = s.groups.component_defs[&def_id].origin;
    let before = active(&s);
    let r = s.execute(Command::PlaceComponent { def_id, position: origin });
    let after = active(&s);
    println!("\n  원점 배치 → {r:?}   면 {before} → {after}\n");
    let axia_core::CommandResult::Error(msg) = &r else {
        panic!("a zero-offset placement must be refused, not silently half-done: {r:?}");
    };
    assert!(
        msg.contains("원본과 겹칩니다"),
        "the refusal must be in the user's words, not the kernel's — got {msg:?}"
    );
    assert_eq!(
        before, after,
        "and a refusal must leave the mesh exactly as it was"
    );
}

/// ⚠ And Ctrl+Z must actually take it back.
///
/// Found 2026-08-24 by an engine-wide audit, in code this same session added.
/// `Command::PlaceComponent` calls `transactions.begin()` and
/// `transactions.commit()` but never `set_before_snapshot` /
/// `set_after_snapshot` — the two setters 41 and 46 other call sites in
/// scene.rs do use. The frame commits EMPTY.
///
/// `TransactionManager::undo` pops that frame and `Command::Undo` only acts
/// when `before_snapshot` is non-empty, so the first Ctrl+Z restores nothing
/// and still reports success — the component stays. ⚠ The SECOND Ctrl+Z is
/// worse than confusing: it pops the PREVIOUS operation's frame and undoes
/// that, so one keystroke removes an unrelated earlier edit while the
/// placement survives, unredoable (its after_snapshot is empty too).
#[test]
fn placing_a_component_can_be_undone() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);
    let before = active(&s);

    s.execute(Command::PlaceComponent {
        def_id,
        position: DVec3::new(500.0, 0.0, 0.0),
    });
    let placed = active(&s);
    assert!(placed > before, "premise: the placement added faces ({before} -> {placed})");

    let r = s.execute(Command::Undo);
    let after = active(&s);
    println!("
  면 {before} -> {placed} -> undo -> {after}   결과 {r:?}
");

    assert_eq!(
        after, before,
        "one Ctrl+Z must put the scene back: {before} -> {placed} -> {after}"
    );
    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "and leave it sound: {:?}", inv.violations);
}
/// The destructive half: a second Ctrl+Z must not eat an unrelated edit.
///
/// With the frame empty, the first Ctrl+Z was a no-op that still reported
/// success, so a user would press it again — and THAT one popped the previous
/// operation's frame. One keystroke removed an earlier edit while the
/// placement survived. This asserts the ordering a working undo gives:
/// placement first, then the older operation.
#[test]
fn a_second_undo_takes_the_previous_edit_not_a_bystander() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);

    // An unrelated edit AFTER the component exists, so it owns the older frame.
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(-900.0, -900.0, 0.0),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 100.0,
        height: 100.0,
    });
    let with_rect = active(&s);

    s.execute(Command::PlaceComponent {
        def_id,
        position: DVec3::new(500.0, 0.0, 0.0),
    });
    let with_both = active(&s);
    assert!(with_both > with_rect, "premise: the placement added faces");

    s.execute(Command::Undo);
    let after_one = active(&s);
    assert_eq!(
        after_one, with_rect,
        "the FIRST undo takes the placement, leaving the rect: \
         {with_rect} -> {with_both} -> {after_one}"
    );

    s.execute(Command::Undo);
    let after_two = active(&s);
    println!("
  {with_rect} -> {with_both} -> undo -> {after_one} -> undo -> {after_two}
");
    assert!(
        after_two < after_one,
        "and the SECOND undo takes the rect, not nothing: {after_one} -> {after_two}"
    );
}
/// And Redo must bring it back — which is what `set_after_snapshot` buys.
///
/// ⚠ Written because the two tests above did NOT catch removing the after
/// setter: both only look at undo, and undo reads `before_snapshot`. With the
/// after half missing the placement is undoable but not redoable — a silent
/// half-fix that a mutation check exposed.
#[test]
fn an_undone_placement_can_be_redone() {
    let mut s = prod();
    let def_id = a_box_component(&mut s);
    let before = active(&s);

    s.execute(Command::PlaceComponent {
        def_id,
        position: DVec3::new(500.0, 0.0, 0.0),
    });
    let placed = active(&s);
    assert!(placed > before, "premise: the placement added faces");

    s.execute(Command::Undo);
    assert_eq!(active(&s), before, "premise: undo worked");

    let r = s.execute(Command::Redo);
    let again = active(&s);
    println!("
  면 {before} -> {placed} -> undo -> {before} -> redo -> {again}   결과 {r:?}
");
    assert_eq!(
        again, placed,
        "redo must restore the placement: expected {placed}, got {again}"
    );
}