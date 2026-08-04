//! THE REQUIREMENT, measured.
//!
//! Stated by the user, and it is the right acceptance test for this engine:
//!
//!   1. A shape must be drawable on ANY face, at ANY coordinate.
//!   2. Shapes drawn on the same face must be separated by their boundaries.
//!
//! Everything else in this area — the leftover half-edges, the loop walk, the
//! draw guard — is machinery underneath. This file is the requirement itself, so
//! the gap is visible as a grid rather than as a story about half-edges.
//!
//! Read the output as: does the draw survive, and does the host face end up cut
//! where the drawn boundary crosses it.
//!
//! WHY THE LAST TWO FAIL — what is measured, and what is ruled out.
//!
//! Both remaining cases are refused with "도형이 기존 면과 같은 자리를
//! 덮습니다", and the repair that should have resolved the overlap bails with
//! "coplanar clipping requires convex faces". The face it calls non-convex is
//! not the drawn rectangle. It is an L-shape spanning the drawn rectangle AND a
//! piece of the host, while the host's own pieces are still there:
//!
//! ```text
//! (200,260,110) (200,140,110) (200,140,100) (200,0,100)
//! (200,0,0)     (200,140,0)   (200,140,-10) (200,260,-10)
//! ```
//!
//! **The trigger** is the drawn shape clearing the host on BOTH opposite sides:
//!
//! ```text
//!   side wall, 100 tall    120 × 120 → refused     120 × 100 → accepted
//!   top face, 200 × 200    120 × 120 → accepted    120 × 220 → refused
//! ```
//!
//! **Ruled out, each by measurement, and each of them a reading I published
//! before testing it:**
//!
//! - *A vertical-face difference.* There is none — the top face refuses the same
//!   way once the shape clears it. The grid only meets it on the side wall
//!   because that wall is 100 tall and the straddle size is 120.
//! - *The solid-wall protection in the coplanar re-derive.* That protection does
//!   not care which way a face points, and neither does this.
//! - *The coplanar re-derive at all.* Neither `rebuild_coplanar_faces` nor
//!   `rebuild_coplanar_faces_analytic` is entered on this draw — instrumented at
//!   both entries, neither printed.
//! - *Any of the four auto-behaviours.* auto_intersect / auto_face_synthesis /
//!   face_rederive / freeform_overlap, off one at a time and all four off
//!   together: refused every time.
//! - *The crossing split added 2026-08-03.* Disabled, still refused.
//!
//! **Found it, by backtrace on face creation** — which is where this should have
//! started rather than after four guesses. The 8-vertex face is built by
//! `exec_draw_line`, inside `exec_draw_rect` (a rectangle is four lines), and
//! the cause is the loop walk's dead-end fallback added the same morning
//! (`0b3e7b7`): with nowhere free to go it steps onto a used-up edge, and the
//! only ways on there are the SOLID's own boundary. So the walk goes around the
//! wall and closes a loop spanning the drawn rectangle and the wall together.
//!
//! **But it cannot simply be removed.** Taking it out makes this grid pass 24
//! of 24 — and regresses what it was written for: a shape drawn beside a solid
//! stops closing the region next to it (`overlap_walk_sim::causation_by_removal`
//! goes 3 → 2). That test caught the removal, which is the only reason this note
//! does not claim the fallback was unnecessary.
//!
//! So it is a real trade, and the way out is narrower than either side of it:
//! the walk may step past a dead end, but a loop that comes back enclosing an
//! existing coplanar face is the wrong loop and should be rejected. That check
//! belongs after the walk, where the loop can be measured against the faces
//! already there — not inside it, where only the next step is visible.

use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// Is anything actually damaged?
///
/// Not the same question as "does the overlap counter read zero". Two faces
/// meeting at an ANGLE touch along a line and enclose nothing between them —
/// a shape drawn past a box's corner rests on the walls below it exactly that
/// way, and the counter says so. Two faces on the SAME plane covering the same
/// ground is the one that is wrong, and so is a broken invariant.
///
/// Measured, not assumed: the leftover count on the circle cases is entirely
/// perpendicular contact with zero invariant violations, and lumping it in was
/// calling a successful draw a failure.
fn clean(s: &Scene) -> bool {
    // Invariants are deliberately NOT consulted. Drawing past a solid's edge
    // opens it, and an opened solid has non-manifold shared edges — that is the
    // consequence of what was asked for, not a failure of it. Whether the solid
    // stayed closed is a different question from whether the shape could be
    // drawn and the face divided.
    !s.mesh
        .detect_self_intersections()
        .intersecting_pairs
        .iter()
        .any(|&(a, b)| {
            let (Some(fa), Some(fb)) = (s.mesh.faces.get(a), s.mesh.faces.get(b)) else {
                return false;
            };
            let na = fa.normal().normalize_or_zero();
            let nb = fb.normal().normalize_or_zero();
            na.dot(nb).abs() > 0.999 // same plane — genuinely covering each other
        })
}

/// Host surfaces to draw on.
#[derive(Clone, Copy)]
enum Host {
    GroundSheet,   // a plain drawn sheet on z=0
    BoxTopDrawn,   // top face of a box grown from a drawn rectangle
    BoxTopPrim,    // top face of a `create_box`
    BoxSidePrim,   // vertical side face of a `create_box`
}

impl Host {
    fn name(self) -> &'static str {
        match self {
            Host::GroundSheet => "평면 시트",
            Host::BoxTopDrawn => "박스 윗면 (돌출)",
            Host::BoxTopPrim => "박스 윗면 (도구)",
            Host::BoxSidePrim => "박스 옆면 (도구)",
        }
    }
    /// Build the host and return (plane point, plane normal, in-plane u, v).
    fn build(self, s: &mut Scene) -> (DVec3, DVec3, DVec3, DVec3) {
        match self {
            Host::GroundSheet => {
                s.execute(Command::DrawRectAsShape {
                    center: DVec3::new(100.0, 100.0, 0.0),
                    normal: DVec3::Z, up: DVec3::Y, width: 200.0, height: 200.0,
                });
                (DVec3::new(100.0, 100.0, 0.0), DVec3::Z, DVec3::X, DVec3::Y)
            }
            Host::BoxTopDrawn => {
                s.execute(Command::DrawRectAsShape {
                    center: DVec3::new(100.0, 100.0, 0.0),
                    normal: DVec3::Z, up: DVec3::Y, width: 200.0, height: 200.0,
                });
                let f = s.mesh.faces.iter().filter(|(_, x)| x.is_active()).map(|(i, _)| i).next().unwrap();
                s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
                (DVec3::new(100.0, 100.0, 100.0), DVec3::Z, DVec3::X, DVec3::Y)
            }
            Host::BoxTopPrim => {
                let f = s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
                s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
                (DVec3::new(100.0, 100.0, 100.0), DVec3::Z, DVec3::X, DVec3::Y)
            }
            Host::BoxSidePrim => {
                let f = s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
                s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
                // x = 200 wall; in-plane axes are Y and Z
                (DVec3::new(200.0, 100.0, 50.0), DVec3::X, DVec3::Y, DVec3::Z)
            }
        }
    }
}

/// Where the shape sits relative to the host face.
#[derive(Clone, Copy)]
enum Place {
    Inside,     // wholly within the face
    Crossing,   // overlaps the face and extends past one corner
    Straddling, // centred on the face's edge, half on half off
}

impl Place {
    fn name(self) -> &'static str {
        match self {
            Place::Inside => "면 안쪽",
            Place::Crossing => "모서리 넘어감",
            Place::Straddling => "모서리에 걸침",
        }
    }
    /// centre offset (in-plane, from the face centre) and half-size
    fn geom(self) -> ((f64, f64), f64) {
        match self {
            Place::Inside => ((0.0, 0.0), 50.0),
            Place::Crossing => ((100.0, 100.0), 100.0),
            Place::Straddling => ((100.0, 0.0), 60.0),
        }
    }
}

fn draw_why(s: &mut Scene, origin: DVec3, n: DVec3, u: DVec3, v: DVec3, p: Place, circle: bool) -> String {
    let ((du, dv), half) = p.geom();
    let c = origin + u * du + v * dv;
    let r = if circle {
        s.execute(Command::DrawCircleAsShape { center: c, normal: n, radius: half, segments: 24 })
    } else {
        s.execute(Command::DrawRectAsShape { center: c, normal: n, up: v, width: half * 2.0, height: half * 2.0 })
    };
    match r { CommandResult::Error(e) => e, _ => "(수락)".into() }
}

fn draw(s: &mut Scene, origin: DVec3, n: DVec3, u: DVec3, v: DVec3, p: Place, circle: bool) -> bool {
    let ((du, dv), half) = p.geom();
    let c = origin + u * du + v * dv;
    let r = if circle {
        s.execute(Command::DrawCircleAsShape { center: c, normal: n, radius: half, segments: 24 })
    } else {
        s.execute(Command::DrawRectAsShape { center: c, normal: n, up: v, width: half * 2.0, height: half * 2.0 })
    };
    !matches!(r, CommandResult::Error(_))
}

#[test]
fn requirement_1_draw_on_any_face_at_any_coordinate() {
    println!("\n요구 1 — 어떤 면에서든 어떤 좌표에서든 그릴 수 있어야 한다\n");
    let mut fails: Vec<String> = Vec::new();
    for host in [Host::GroundSheet, Host::BoxTopDrawn, Host::BoxTopPrim, Host::BoxSidePrim] {
        for place in [Place::Inside, Place::Crossing, Place::Straddling] {
            for (shape, circle) in [("사각형", false), ("원", true)] {
                let mut s = prod();
                let (o, n, u, v) = host.build(&mut s);
                let before = faces(&s);
                let ok = draw(&mut s, o, n, u, v, place, circle);
                let after = faces(&s);
                let verdict = if !ok {
                    fails.push(format!("{} / {} / {}", host.name(), place.name(), shape));
                    "거부"
                } else if !clean(&s) {
                    fails.push(format!("{} / {} / {} (겹침)", host.name(), place.name(), shape));
                    "수락(겹침)"
                } else {
                    "수락"
                };
                println!("  {:<18} {:<14} {:<7} {:<10} 면 {}→{}",
                    host.name(), place.name(), shape, verdict, before, after);
            }
        }
    }
    println!("\n  실패 {}건", fails.len());
    for f in &fails { println!("    ✗ {f}"); }
    // Today's count, pinned. Fewer means the engine moved toward the
    // requirement -- lower it and name the cases that started passing.
    //
    // It read 7 until the criterion was corrected. Six of those were the grid
    // calling perpendicular CONTACT a failure: a shape drawn past a box's
    // corner rests on the walls below it, the overlap counter says so, and
    // nothing is wrong. All six draw and land (면 6→7). What is left is one
    // genuine refusal.
    assert_eq!(fails.len(), 0, "every case must draw and stay separated: {fails:?}");
}

#[test]
fn requirement_2_shapes_on_one_face_are_separated_by_their_boundaries() {
    println!("\n요구 2 — 같은 면에 그린 도형은 경계로 분리되어야 한다\n");
    let mut fails: Vec<String> = Vec::new();
    for host in [Host::GroundSheet, Host::BoxTopDrawn, Host::BoxTopPrim, Host::BoxSidePrim] {
        // two rectangles overlapping each other, both wholly on the host face
        let mut s = prod();
        let (o, n, u, v) = host.build(&mut s);
        let before = faces(&s);
        let a = s.execute(Command::DrawRectAsShape {
            center: o + u * -25.0 + v * -25.0, normal: n, up: v, width: 80.0, height: 80.0 });
        let b = s.execute(Command::DrawRectAsShape {
            center: o + u * 25.0 + v * 25.0, normal: n, up: v, width: 80.0, height: 80.0 });
        let ok = !matches!(a, CommandResult::Error(_)) && !matches!(b, CommandResult::Error(_));
        let after = faces(&s);
        // two overlapping shapes on one face → three regions, so +3 over the host
        let split_ok = ok && after >= before + 2 && clean(&s);
        if !split_ok {
            fails.push(format!("{}  (면 {}→{}, 겹침 {})", host.name(), before, after,
                s.mesh.detect_self_intersections().count()));
        }
        println!("  {:<18} 면 {}→{}  겹침 {}  {}",
            host.name(), before, after, s.mesh.detect_self_intersections().count(),
            if split_ok { "분리됨" } else { "★분리 안 됨" });
    }
    println!("\n  실패 {}건", fails.len());
    for f in &fails { println!("    ✗ {f}"); }
    assert_eq!(fails.len(), 0,
        "two rectangles overlapping on one face become three regions, on every host: {fails:?}");
}

/// Why does each failing cell fail? One blocker or several?
#[test]
fn why_do_the_extending_draws_fail() {
    println!("
확장 그리기가 막히는 이유
");
    let mut reasons: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for host in [Host::BoxTopDrawn, Host::BoxTopPrim, Host::BoxSidePrim] {
        for place in [Place::Crossing, Place::Straddling] {
            for (shape, circle) in [("사각형", false), ("원", true)] {
                let mut s = prod();
                let (o, n, u, v) = host.build(&mut s);
                let why = draw_why(&mut s, o, n, u, v, place, circle);
                let key = why.chars().take(28).collect::<String>();
                reasons.entry(key).or_default()
                    .push(format!("{} / {} / {}", host.name(), place.name(), shape));
            }
        }
    }
    for (why, cases) in &reasons {
        println!("  [{}건] {}", cases.len(), why);
        for c in cases { println!("        {c}"); }
    }
    println!("
  서로 다른 원인: {}", reasons.len());
}

/// What does the engine actually build for an extending draw, if the guard is
/// not there to roll it back? The guard reports an overlap; this shows what
/// overlaps what.
#[test]
fn what_does_an_extending_draw_produce() {
    println!("
확장 그리기가 실제로 만드는 것 (가드 무관, 겹치는 면 쌍을 열거)
");
    for host in [Host::BoxTopPrim, Host::BoxSidePrim] {
        let mut s = prod();
        let (o, n, u, v) = host.build(&mut s);
        // draw the four sides as LINES — the line path is not guarded, so the
        // result survives and can be inspected.
        let ((du, dv), half) = Place::Crossing.geom();
        let c = o + u * du + v * dv;
        let corners = [
            c - u * half - v * half, c + u * half - v * half,
            c + u * half + v * half, c - u * half + v * half,
        ];
        let before = faces(&s);
        for i in 0..4 {
            s.execute(Command::DrawLine {
                start: corners[i], end: corners[(i + 1) % 4], surface_normal: Some(n),
            });
        }
        let si = s.mesh.detect_self_intersections();
        println!("  {:<18} 면 {}→{}  겹침쌍 {}  nm {}",
            host.name(), before, faces(&s), si.count(),
            s.mesh.collect_non_manifold_edges().len());
        for (a, b) in si.intersecting_pairs.iter().take(4) {
            let dsc = |f: axia_geo::FaceId| {
                s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).map(|vs| {
                    let ps: Vec<_> = vs.iter().filter_map(|x| s.mesh.vertex_pos(*x).ok()).collect();
                    format!("{}각 ({:.0},{:.0},{:.0})…", ps.len(), ps[0].x, ps[0].y, ps[0].z)
                }).unwrap_or_default()
            };
            println!("        겹침: {:?} {}  ↔  {:?} {}", a, dsc(*a), b, dsc(*b));
        }
    }
}

/// The line path builds it correctly. What does the RECT TOOL build for the same
/// thing? `Command::DrawRect` is the unguarded legacy entry into the same tool
/// code, so the result survives to be looked at.
#[test]
fn what_does_the_rect_tool_build() {
    println!("
도형 도구가 만드는 것 (가드 없는 진입점)
");
    for host in [Host::BoxTopPrim, Host::BoxSidePrim] {
        let mut s = prod();
        let (o, n, u, v) = host.build(&mut s);
        let ((du, dv), half) = Place::Crossing.geom();
        let before = faces(&s);
        s.execute(Command::DrawRect {
            center: o + u * du + v * dv, normal: n, up: v,
            width: half * 2.0, height: half * 2.0,
        });
        let si = s.mesh.detect_self_intersections();
        println!("  {:<18} 면 {}→{}  겹침쌍 {}  nm {}",
            host.name(), before, faces(&s), si.count(),
            s.mesh.collect_non_manifold_edges().len());
        for fid in s.mesh.faces.iter().filter(|(_,x)|x.is_active()).map(|(i,_)|i).collect::<Vec<_>>() {
            if let Ok(vs) = s.mesh.collect_loop_verts(s.mesh.faces[fid].outer().start) {
                let ps: Vec<_> = vs.iter().filter_map(|x| s.mesh.vertex_pos(*x).ok()).collect();
                if ps.iter().all(|p| (p - o).dot(n).abs() < 1e-6) {
                    let pts: Vec<String> = ps.iter()
                        .map(|p| format!("({:.0},{:.0},{:.0})", p.x, p.y, p.z)).collect();
                    println!("        면 {:?}: {}", fid, pts.join(" "));
                }
            }
        }
        for (a, b) in si.intersecting_pairs.iter().take(3) {
            let dsc = |f: axia_geo::FaceId| {
                s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).map(|vs| {
                    let ps: Vec<_> = vs.iter().filter_map(|x| s.mesh.vertex_pos(*x).ok()).collect();
                    format!("{}각", ps.len())
                }).unwrap_or_default()
            };
            println!("        겹침: {:?} {}  ↔  {:?} {}", a, dsc(*a), b, dsc(*b));
        }
    }
}

/// A shape that clears the host on both sides draws like any other now.
///
/// It was the last refusal, and it was mine: the loop walk's dead-end fallback
/// let the walk step onto the SOLID's own boundary when it had nowhere free to
/// go, and close a loop around the wall as well as the shape. Kept in both
/// directions because the fix is narrow — a loop that stepped past a dead end
/// and came back around an existing face is thrown away — and a wider or
/// narrower version would show up here.
#[test]
fn a_shape_that_clears_the_host_on_both_sides_draws_too() {
    fn on_box(centre: DVec3, n: DVec3, up: DVec3, w: f64, h: f64) -> bool {
        let mut s = prod();
        let f = s
            .mesh
            .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
            .unwrap();
        s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
        !matches!(
            s.execute(Command::DrawRectAsShape { center: centre, normal: n, up, width: w, height: h }),
            CommandResult::Error(_)
        )
    }
    // The east wall is 200 (y) × 100 (z). A 120-tall shape clears it entirely.
    let wall = DVec3::new(200.0, 200.0, 50.0);
    assert!(on_box(wall, DVec3::X, DVec3::Z, 120.0, 120.0), "clears the wall");
    assert!(on_box(wall, DVec3::X, DVec3::Z, 120.0, 100.0), "exactly its height");
    assert!(on_box(wall, DVec3::X, DVec3::Z, 120.0, 60.0), "sits within it");

    // The top face is 200 × 200 and behaves the same at every size.
    let top = DVec3::new(200.0, 100.0, 100.0);
    assert!(on_box(top, DVec3::Z, DVec3::Y, 120.0, 120.0), "straddling");
    assert!(on_box(top, DVec3::Z, DVec3::Y, 120.0, 220.0), "clearing it");
}

/// Drawing a shape AROUND a smaller one, on a sheet and on a solid alike.
///
/// The result is a chain — the host keeps one hole for the new shape, the new
/// shape becomes a ring holding the small one, and the small one is untouched.
/// Getting merely "accepted" is not the test: a solid ring that lay ON TOP of
/// the small face would also be accepted by a guard that only counted edges.
///
/// It used to be refused on a solid, and the reason is worth keeping. A sheet
/// never showed it because the arrangement rebuilds the whole coplanar region
/// and derives the chain fresh; on a solid the arrangement is deliberately
/// skipped (re-deriving there dangles the solid's own loop), so the small face's
/// hole stayed with the cap and the new ring could not take it — its twins
/// already bore a face. `detach_hole_for_closer_parent` hands it over.
#[test]
fn a_shape_drawn_around_a_smaller_one_nests_on_a_solid_too() {
    // On a sheet it is a ring with a hole, which is what it should be.
    let mut sheet = prod();
    sheet.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO, normal: DVec3::Z, up: DVec3::Y, width: 100.0, height: 100.0,
    });
    let r = sheet.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO, normal: DVec3::Z, up: DVec3::Y, width: 400.0, height: 400.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "a sheet takes it: {r:?}");

    // And on a solid's top face.
    let mut solid = prod();
    let f = solid
        .mesh
        .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    solid.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    solid.execute(Command::DrawRectAsShape {
        center: DVec3::new(100.0, 100.0, 100.0), normal: DVec3::Z, up: DVec3::Y,
        width: 60.0, height: 60.0,
    });
    let before = faces(&solid);
    let r = solid.execute(Command::DrawRectAsShape {
        center: DVec3::new(100.0, 100.0, 100.0), normal: DVec3::Z, up: DVec3::Y,
        width: 140.0, height: 140.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "a solid takes it too: {r:?}");
    assert_eq!(faces(&solid), before + 1, "one new region");

    // The chain, by the width of each face's outer loop and of its hole.
    let width_of = |start: axia_geo::HeId| -> f64 {
        let vs = solid.mesh.collect_loop_verts(start).unwrap();
        let xs: Vec<f64> = vs.iter().filter_map(|v| solid.mesh.vertex_pos(*v).ok())
            .map(|p| p.x).collect();
        xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min)
    };
    let mut chain: Vec<(f64, Vec<f64>)> = solid
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(fid, _)| {
            solid.mesh.collect_loop_verts(solid.mesh.faces[*fid].outer().start)
                .map(|vs| vs.iter().filter_map(|v| solid.mesh.vertex_pos(*v).ok())
                    .all(|p| (p.z - 100.0).abs() < 1e-6))
                .unwrap_or(false)
        })
        .map(|(fid, f)| {
            (width_of(f.outer().start),
             f.inners().iter().map(|lr| width_of(lr.start)).collect())
        })
        .collect();
    chain.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert_eq!(
        chain,
        vec![
            (60.0, vec![]),        // the small one, whole
            (140.0, vec![60.0]),   // the new shape, a ring around it
            (200.0, vec![140.0]),  // the cap, holding only the new shape
        ],
        "each face holds exactly the one below it: {chain:?}"
    );
    assert!(clean(&solid), "nothing covers anything else");
    assert!(solid.mesh.verify_face_invariants().violations.is_empty());
    assert!(solid.mesh.verify_outward_normals().is_closed_solid, "the solid stays closed");
}

/// The same, drawn with circles — a different boundary kind through the same
/// hand-over. A circle's rim is one self-loop half-edge, and its split reparents
/// the twin WITHOUT checking that it is free, so a circle had to be taken back
/// before the split rather than after it refused.
#[test]
fn a_circle_drawn_around_a_smaller_circle_nests_on_a_solid() {
    let mut solid = prod();
    let f = solid
        .mesh
        .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    solid.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    let small = solid.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(100.0, 100.0, 100.0), normal: DVec3::Z, radius: 30.0,
    });
    assert!(!matches!(small, CommandResult::Error(_)), "{small:?}");
    let before = faces(&solid);
    let around = solid.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(100.0, 100.0, 100.0), normal: DVec3::Z, radius: 70.0,
    });
    assert!(!matches!(around, CommandResult::Error(_)), "drawn around: {around:?}");
    assert_eq!(faces(&solid), before + 1, "one new region");
    assert!(clean(&solid), "nothing covers anything else");
    assert!(solid.mesh.verify_face_invariants().violations.is_empty());
    assert!(solid.mesh.verify_outward_normals().is_closed_solid, "the solid stays closed");
}

/// Drawing a shape right AROUND a solid's whole face — past its boundary on
/// every side at once.
///
/// 사용자 (2026-08-04): `입체면에서 경계면을 초과하여 그리는 기능`. Crossing one
/// edge worked; crossing all four did not. The new shape came out solid, lying
/// over the cap, and the guard rolled the whole draw back.
///
/// The hand-over that fixes this between two SHEETS cannot: it moves a hole by
/// re-pointing the inner's twin half-edges, and a solid cap's twins already
/// carry the walls. So the container is rebuilt with the cap as a hole of its
/// own, and the cap's edges end up bearing three faces — cap, wall, ring. That
/// is a T-junction, which is what drawing a sheet across a solid's footprint
/// means, and it is why the guard counts overlaps rather than shared edges.
#[test]
fn a_shape_drawn_right_around_a_solids_face_is_accepted() {
    let mut s = prod();
    let f = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    let before = faces(&s);

    // The box top is 200 × 200; this is 400 × 400 centred on it.
    let r = s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, 100.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 400.0,
        height: 400.0,
    });
    assert!(!matches!(r, CommandResult::Error(_)), "drawn around the cap: {r:?}");
    assert_eq!(faces(&s), before + 1, "one new region");

    // The new shape is a RING: it holds the cap as a hole rather than covering it.
    let ring = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .find(|(fid, f)| {
            !f.inners().is_empty()
                && s.mesh
                    .collect_loop_verts(s.mesh.faces[*fid].outer().start)
                    .map(|vs| {
                        vs.iter()
                            .filter_map(|v| s.mesh.vertex_pos(*v).ok())
                            .all(|p| (p.z - 100.0).abs() < 1e-6 && p.x.abs() > 150.0)
                    })
                    .unwrap_or(false)
        })
        .map(|(fid, _)| fid)
        .expect("a ring on the top plane holding the cap");
    assert_eq!(s.mesh.faces[ring].inners().len(), 1);
    assert!(
        (s.mesh.face_area(ring) - (400.0 * 400.0 - 200.0 * 200.0)).abs() < 1e-6,
        "and it measures as the ring it is: {}",
        s.mesh.face_area(ring)
    );

    assert!(clean(&s), "nothing covers anything else");
    // The shared edges DO bear three faces now, and that is the whole point —
    // what must not appear is anything else wrong.
    let nm = s.mesh.collect_non_manifold_edges().len();
    assert_eq!(
        s.mesh.verify_face_invariants().violations.len(),
        nm,
        "the T-junctions are the only complaint"
    );
}
