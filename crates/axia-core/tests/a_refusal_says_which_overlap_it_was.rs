//! WHAT THE ENGINE SAYS WHEN IT WILL NOT DIVIDE AN OVERLAP.
//!
//! Every refusal used to read "도형이 기존 면과 같은 자리를 덮습니다 — 면이
//! 겹치지 않게 그려주세요", which by 2026-08-05 is advice that is mostly wrong:
//! overlapping shapes DO divide each other in nearly every case. Measured on a
//! box top after the circle-overlap split landed:
//!
//! ```text
//!   circle then rect     ok        two circles crossing    ok
//!   rect then circle     REFUSED   three circles           ok
//!   anything with an ellipse in it REFUSED, in either order
//! ```
//!
//! So there is exactly one workaround left and it applies to circles: draw the
//! round shape first and the angular one cuts it. Saying that is worth more than
//! repeating "don't overlap"; and for a freeform curve, where no order works,
//! saying THAT is worth more than sending someone round in circles trying.
//!
//! This file is the message, not the geometry. When one of these cases starts
//! working, its assertion here is what will say so.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

fn boxed() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    let f = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    s
}

fn circle(s: &mut Scene, x: f64) -> CommandResult {
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(x, 0.0, 100.0),
        normal: DVec3::Z,
        radius: 40.0,
    })
}

fn rect(s: &mut Scene, cx: f64) -> CommandResult {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(cx, 0.0, 100.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 60.0,
        height: 60.0,
    })
}

fn ellipse(s: &mut Scene, x: f64) -> CommandResult {
    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(x, 0.0, 100.0),
        normal: DVec3::Z,
        ref_dir: DVec3::X,
        radius_x: 40.0,
        radius_y: 25.0,
    })
}

fn refusal(r: CommandResult) -> String {
    match r {
        CommandResult::Error(e) => e,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// A round shape and an angular one: there IS an order that works, so say it.
#[test]
fn a_circle_meeting_a_rectangle_is_told_the_order_that_works() {
    let mut s = boxed();
    rect(&mut s, 0.0);
    let msg = refusal(circle(&mut s, 30.0));
    assert!(msg.contains("원을 먼저"), "the message must name the workaround: {msg}");
    assert!(!msg.contains("겹치지 않게 그려주세요"), "that advice is now wrong: {msg}");
}

/// And the order it names really is the one that works — otherwise this is a
/// message telling the user to do something that also fails.
#[test]
fn drawing_the_circle_first_really_does_work() {
    let mut s = boxed();
    assert!(!matches!(circle(&mut s, 30.0), CommandResult::Error(_)));
    let r = rect(&mut s, 0.0);
    assert!(!matches!(r, CommandResult::Error(_)), "the advised order: {r:?}");
}

/// A freeform curve: no order works, so it must not suggest one.
#[test]
fn an_ellipse_overlap_is_not_told_to_try_another_order() {
    for (first, second) in [("ellipse-rect", true), ("rect-ellipse", false)] {
        let mut s = boxed();
        let msg = if second {
            ellipse(&mut s, -20.0);
            refusal(rect(&mut s, 30.0))
        } else {
            rect(&mut s, 0.0);
            refusal(ellipse(&mut s, 30.0))
        };
        assert!(msg.contains("타원"), "{first}: must name the curve kind: {msg}");
        assert!(!msg.contains("먼저"), "{first}: must not suggest an order that fails: {msg}");
    }
    // Two of them, likewise.
    let mut s = boxed();
    ellipse(&mut s, -20.0);
    let msg = refusal(ellipse(&mut s, 20.0));
    assert!(msg.contains("타원"), "{msg}");
}

/// The cases that no longer refuse at all — the message must not be reachable
/// for them, or it would be describing something that works.
#[test]
fn the_cases_that_work_are_not_refused() {
    let mut s = boxed();
    assert!(!matches!(circle(&mut s, -20.0), CommandResult::Error(_)));
    assert!(!matches!(circle(&mut s, 20.0), CommandResult::Error(_)), "two circles crossing");

    let mut s = boxed();
    assert!(!matches!(ellipse(&mut s, 0.0), CommandResult::Error(_)), "one ellipse alone");
}
