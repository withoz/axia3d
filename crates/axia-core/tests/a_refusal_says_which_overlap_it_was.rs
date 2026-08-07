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
//! **2026-08-06 — the circle row closed.** Turning the coplanar arrangement
//! back on for solids (it used to early-return whenever the affected region
//! touched one) makes `rect then circle` divide the same way `circle then rect`
//! always did: nine faces, every one sealed, no open boundary, no non-manifold
//! edge, no invariant violation. So there is no order to advise any more — both
//! work — and that assertion has become the one below.
//!
//! An ellipse still refuses, in every order, and now for a reason worth
//! recording: it DIVIDES (nine faces, all sealed, no open boundary) but leaves
//! three edges with three coplanar faces stacked on them. Three faces on one
//! edge is normal where a sheet meets a solid's rim — different planes, nothing
//! covering anything — so the guard used to wave any T-junction through. Three
//! faces on the SAME plane is the other thing, and that is now what gets caught.
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

/// A round shape and an angular one divide each other, in EITHER order.
///
/// This used to assert a refusal that named the order to try. There is no order
/// to name now, and the file said this is where that would show up.
#[test]
fn a_circle_and_a_rectangle_divide_each_other_either_way() {
    for reversed in [false, true] {
        let mut s = boxed();
        let r = if reversed {
            circle(&mut s, 30.0);
            rect(&mut s, 0.0)
        } else {
            rect(&mut s, 0.0);
            circle(&mut s, 30.0)
        };
        let order = if reversed { "circle then rect" } else { "rect then circle" };
        assert!(!matches!(r, CommandResult::Error(_)), "{order}: {r:?}");

        let all: Vec<axia_geo::FaceId> =
            s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        assert_eq!(all.len(), 9, "{order}: box 6 + the top cut into three");
        let mi = s.mesh.face_set_manifold_info(&all);
        assert!(mi.is_closed_solid, "{order}: the solid stays closed");
        assert_eq!(mi.boundary_edge_count, 0, "{order}: nothing left open");
        assert_eq!(mi.non_manifold_edge_count, 0, "{order}: no edge bears three faces");
        assert!(
            s.mesh.verify_face_invariants().violations.is_empty(),
            "{order}: {:?}",
            s.mesh.verify_face_invariants().violations
        );
        assert_eq!(
            all.iter().filter(|&&f| s.mesh.is_face_in_volume(f)).count(),
            9,
            "{order}: every face still sealed inside the volume"
        );
    }
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

/// An ellipse overlap DIVIDES now — and still leaves faces stacked.
///
/// This used to assert a refusal naming the curve kind. Refusals are gone
/// (a draw is never refused), so what is left to pin is what the draw
/// actually produces. Measured 2026-08-06, on a box top, all three orders:
///
/// ```text
///   ellipse then rect    9 faces, 0 open edges, 5 coplanar stacks
///   rect then ellipse    9 faces, 0 open edges, 3
///   ellipse then ellipse 9 faces, 0 open edges, 6
/// ```
///
/// So the solid is not opened and the regions ARE cut — the overlap is
/// simply covered twice where the ellipse meets what it crosses. A circle in
/// the same place leaves none of this, which is the whole gap: the analytic
/// path that exists for circles does not exist for ellipses.
///
/// Pinned as it stands, not as it should be. When the ellipse gets that path
/// these numbers go to zero and this test is what will say so.
#[test]
fn an_ellipse_overlap_divides_but_still_stacks_faces() {
    for (name, reversed, both) in [
        ("ellipse then rect", false, false),
        ("rect then ellipse", true, false),
        ("ellipse then ellipse", false, true),
    ] {
        let mut s = boxed();
        let r = if both {
            ellipse(&mut s, -20.0);
            ellipse(&mut s, 20.0)
        } else if reversed {
            rect(&mut s, 0.0);
            ellipse(&mut s, 30.0)
        } else {
            ellipse(&mut s, -20.0);
            rect(&mut s, 30.0)
        };
        assert!(!matches!(r, CommandResult::Error(_)), "{name}: a draw is never refused, got {r:?}");

        let all: Vec<axia_geo::FaceId> =
            s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        let mi = s.mesh.face_set_manifold_info(&all);
        assert_eq!(all.len(), 9, "{name}: the top is cut into regions");
        assert_eq!(mi.boundary_edge_count, 0, "{name}: nothing is left open");

        // The gap, recorded rather than tolerated: where the ellipse meets what
        // it crosses, two faces still hold the same ground.
        //
        // These counts were 5 / 3 / 6 when this was written, over an I5 that
        // called any two coplanar faces on an edge a stack. On 2026-08-07 that
        // was narrowed to faces on the SAME SIDE of the edge — two halves of a
        // plane meeting at their border are neighbours — and the ellipse's
        // overlap still reports every one of these, unchanged. Pinning "stacked"
        // rather than the wording of the reason: the reason is free to improve.
        let stacks = s.mesh.verify_face_invariants().violations;
        assert!(
            !stacks.is_empty(),
            "{name}: if this is now empty the ellipse path landed — drop the              assertion and tighten the two above"
        );
        assert!(
            stacks.iter().all(|v| v.contains("stacked")),
            "{name}: only stacked faces are expected here, got {stacks:?}"
        );
    }
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
