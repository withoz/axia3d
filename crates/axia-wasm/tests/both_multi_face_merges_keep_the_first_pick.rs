//! BOTH MULTI-FACE MERGES HAND THE RESULT TO THE FACE THE USER PICKED FIRST.
//!
//! `Scene::merge_faces_by_edge_owned` takes a `preferred` face: whichever of
//! the two operands the caller names keeps the ownership. That is a 사용자 결재
//! ("먼저 선택한 쪽이 상속"), and `a_merged_face_has_an_owner.rs` in axia-core
//! proves the Scene honours it.
//!
//! What that test cannot see is whether the WASM entry points ASK for it.
//! Measured 2026-09-02: `try_merge_adjacent_faces_tol` passed `preferred` and
//! its no-tolerance sibling `try_merge_adjacent_faces` passed `None` — the same
//! operation, one honouring the decision and one silently dropping it. The
//! bridge prefers the `_tol` export whenever the engine offers it, so the loss
//! only showed on an older WASM build; it was still the rule being lost.
//!
//! ## Why this reads the source
//!
//! `AxiaEngine`'s methods cannot be called from `cargo test` — the crate uses
//! js-sys and panics at the wasm-bindgen marshalling layer (see
//! `step6_additive_only.rs`'s header). Source is what is available, so the
//! assertion is written to be about something source can actually decide.
//!
//! ## What makes this guard non-vacuous
//!
//! The local `edge_id` exists ONLY inside the two multi-face merge loops, and
//! those are exactly the two places where a caller-ordered working set
//! (`current`) is in scope — so the first pick is knowable there and nowhere
//! else. Every other call site names its edge `eid` and legitimately passes
//! `None`, because the user selected an EDGE and there is no first-picked face
//! to honour.
//!
//! So: a `merge_faces_by_edge_owned(edge_id, None` is a dropped decision, and
//! there should be none. ⚠ Mutation-checked — restore either `None` and this
//! fails.

const LIB: &str = include_str!("../src/lib.rs");

#[test]
fn no_multi_face_merge_drops_the_first_pick() {
    let dropped = LIB.matches("merge_faces_by_edge_owned(edge_id, None").count();
    assert_eq!(
        dropped, 0,
        "a multi-face merge passes None where the caller's first pick is in \
         scope — the merged face will belong to whichever operand the topology \
         happened to name first"
    );
}

#[test]
fn both_multi_face_merges_are_still_here_and_still_ask() {
    // The counterpart to the assertion above: it would also read 0 if the
    // call sites vanished, or were renamed, or the file stopped being read.
    let asking = LIB
        .matches("merge_faces_by_edge_owned(edge_id, preferred")
        .count();
    assert_eq!(
        asking, 2,
        "expected exactly the two multi-face merge loops \
         (try_merge_adjacent_faces and try_merge_adjacent_faces_tol) to name a \
         preferred face; found {asking}"
    );

    // And the pick has to be computed from the caller's order, not invented.
    let computed = LIB
        .matches("let preferred = current.iter().copied().find(|&f| f == f1 || f == f2);")
        .count();
    assert_eq!(
        computed, 2,
        "both loops must derive `preferred` from `current`, whose order is the \
         user's click order (`Array.from(Set)` is insertion-ordered)"
    );
}
