//! A PUNCHED FACE LEAVES NO HALF-EDGE POINTING AT IT.
//!
//! The three punches — circle, rect, polygon — re-derive their host face with the
//! new hole in it: read the loops, drop the face, add it back through
//! `add_face_with_holes`. They dropped it with `self.faces.remove(host)`, which
//! takes the Face out of storage and leaves every half-edge of its loops still
//! naming it.
//!
//! Such a half-edge is in a third state that nothing expects. It is not free —
//! `face().is_null()` is false — so `find_halfedge` Pass 1 will not reuse it and
//! Pass 2 allocates a second pair alongside. And it is not valid either:
//! `faces.contains(he.face())` is false, so anything that indexes
//! `faces[he.face()]` finds nothing there.
//!
//! Measured 2026-09-28 on a clean box (200 x 200 x 100, rect bore 80 x 80):
//!
//! ```text
//!                    dangling   spare   half-edges
//!   one punch         4 -> 0    8 -> 4    40 -> 32
//!   through-drill     8 -> 0   24 -> 16   80 -> 64
//! ```
//!
//! `remove_face` is the same drop with one more step: it sets each loop
//! half-edge's face to `FaceId::NULL` first, so the pair is free and Pass 1 takes
//! it back. Ids are never recycled (`SlotStorage::insert` only ever counts up),
//! so a dangling one stays dangling rather than coming to mean some other face —
//! it is a panic waiting on an unguarded index, not silent corruption.
//!
//! ⚠ What the drill still carries is 2 spares on each of its 8 bore-rim edges,
//! and they come from `bridge_through_loops`, not from the punch: the tube wall
//! asks for a rim half-edge in a direction the free one does not point, so Pass 2
//! allocates again. That is a separate question and the number below holds it.

use axia_core::{Scene, FORM_MATERIAL};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn boxed() -> Scene {
    let mut s = prod();
    s.mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .expect("box");
    s
}

/// Active half-edges naming a face that is not in storage.
fn dangling(s: &Scene) -> Vec<String> {
    s.mesh
        .hes
        .iter()
        .filter(|(_, h)| h.is_active() && !h.face().is_null() && !s.mesh.faces.contains(h.face()))
        .map(|(h, he)| format!("{h:?} names {:?}", he.face()))
        .collect()
}

fn spare(s: &Scene) -> usize {
    s.mesh
        .hes
        .iter()
        .filter(|(_, h)| h.is_active() && h.face().is_null())
        .count()
}

#[test]
fn a_rect_punch_leaves_nothing_naming_the_face_it_replaced() {
    let mut s = boxed();
    s.mesh
        .punch_rect_hole(
            DVec3::new(-40.0, -40.0, 100.0),
            DVec3::new(40.0, 40.0, 100.0),
            DVec3::Z,
        )
        .expect("punch");
    let d = dangling(&s);
    assert!(d.is_empty(), "half-edges naming a face that is gone:\n  {}", d.join("\n  "));
    assert_eq!(
        spare(&s),
        4,
        "an unbridged hole leaves its four rim edges open — that is the hole, not litter"
    );
}

#[test]
fn a_circular_punch_leaves_nothing_naming_the_face_it_replaced() {
    let mut s = boxed();
    s.mesh
        .punch_circular_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, 16)
        .expect("punch");
    let d = dangling(&s);
    assert!(d.is_empty(), "half-edges naming a face that is gone:\n  {}", d.join("\n  "));
}

#[test]
fn a_through_drill_leaves_nothing_naming_a_face_that_is_gone() {
    let mut s = boxed();
    s.drill_rect_through_hole(
        DVec3::new(-40.0, -40.0, 100.0),
        DVec3::new(40.0, 40.0, 100.0),
        DVec3::Z,
    )
    .expect("drill");
    let d = dangling(&s);
    assert!(d.is_empty(), "half-edges naming a face that is gone:\n  {}", d.join("\n  "));

    // The rest is `bridge_through_loops`, two per bore-rim edge. If this drops,
    // that question was answered — say what the answer was and lower it.
    assert_eq!(spare(&s), 16, "the tube builder still allocates a second pair per rim edge");
}

/// The control: a box nobody punched, and the drop that does it properly.
#[test]
fn a_solid_nobody_punched_has_nothing_dangling() {
    let s = boxed();
    assert!(dangling(&s).is_empty(), "a fresh box");
    assert_eq!(spare(&s), 0, "and nothing spare");

    // `remove_face` is what the punches now use: the face goes and its half-edges
    // are freed, so they can be taken back rather than doubled.
    let mut s = boxed();
    let f = s
        .mesh
        .faces
        .iter()
        .filter(|(_, x)| x.is_active())
        .map(|(i, _)| i)
        .next()
        .expect("a face");
    s.mesh.remove_face(f).expect("remove_face");
    assert!(dangling(&s).is_empty(), "remove_face must leave nothing naming it");
    assert_eq!(spare(&s), 4, "its four half-edges are free now, not dangling");
}
