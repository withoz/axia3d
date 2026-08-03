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
//! WHY THE LAST TWO FAIL (measured 2026-08-03, both on a vertical side face).
//!
//! 요구1's remaining case and 요구2's are one cause, and it is not where the
//! error message points. Drawing a rectangle straddling the box's east wall is
//! refused with "도형이 기존 면과 같은 자리를 덮습니다", and the repair that
//! should have resolved that overlap bails with "coplanar clipping requires
//! convex faces".
//!
//! But the face it calls non-convex is not the drawn rectangle — it is this:
//!
//! ```text
//! (200,260,110) (200,140,110) (200,140,100) (200,0,100)
//! (200,0,0)     (200,140,0)   (200,140,-10) (200,260,-10)
//! ```
//!
//! The drawn rectangle (y 140..260, z −10..110) MERGED with the wall region
//! (y 0..140, z 0..100) into one L-shaped face — while the wall pieces it now
//! covers are still there as separate faces. The coplanar re-derive emitted a
//! face overlapping faces it did not replace. The overlap is real; the repair
//! cannot take it because clipping is convex-only (ADR-101 B-1, and LOCKED #41
//! already names general non-convex clipping as a separate ADR).
//!
//! So there are two ways out and they are different sizes: have the re-derive
//! replace what it covers, or bring in general polygon clipping
//! (Weiler–Atherton / Vatti). Neither is a small change, and picking one is not
//! a decision to make inside a test file.
//!
//! CORRECTION, measured right after: there is no vertical-face difference. The
//! trigger is the shape overhanging the host on BOTH opposite sides, and the top
//! face refuses it just the same.
//!
//! ```text
//!   side wall, 100 tall    120 × 120 → refused     120 × 100 → accepted
//!   top face, 200 × 200    120 × 120 → accepted    120 × 220 → refused
//! ```
//!
//! The grid only meets it on the side face because that wall is 100 tall while
//! the straddle size is 120 — the shape clears it above and below. Where the
//! shape merely straddles, the re-derive is fine.
//!
//! So "whatever the re-derive does differently on a vertical face" was the wrong
//! reading, and with it the idea that this is the solid-wall protection showing
//! through: that protection does not care which way a face points, and neither
//! does this. It is the spanning case the arrangement mishandles.
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
    assert_eq!(fails.len(), 1, "24 cases, 1 still failing: {fails:?}");
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
    assert_eq!(fails.len(), 1,
        "two rectangles overlapping on one face must become three regions. Only the vertical side face still fails: {fails:?}");
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

/// What actually refuses a draw: the shape clearing the host on BOTH sides.
///
/// Pinned because the first reading of this was wrong. It looked like a
/// vertical-face problem — the grid's one failure is on a side wall — but the
/// wall is simply shorter than the shape. Give the top face a shape that clears
/// IT on both sides and it refuses in exactly the same way.
#[test]
fn a_shape_that_clears_the_host_on_both_sides_is_refused() {
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
    // The east wall is 200 (y) × 100 (z). A 120-tall shape clears it.
    let wall = DVec3::new(200.0, 200.0, 50.0);
    assert!(!on_box(wall, DVec3::X, DVec3::Z, 120.0, 120.0), "120 clears the 100 wall");
    assert!(on_box(wall, DVec3::X, DVec3::Z, 120.0, 100.0), "100 reaches its edges, no more");
    assert!(on_box(wall, DVec3::X, DVec3::Z, 120.0, 60.0), "60 sits within it");

    // The top face is 200 × 200, and refuses just the same once cleared.
    let top = DVec3::new(200.0, 100.0, 100.0);
    assert!(on_box(top, DVec3::Z, DVec3::Y, 120.0, 120.0), "120 does not clear 200");
    assert!(!on_box(top, DVec3::Z, DVec3::Y, 120.0, 220.0), "220 does — same refusal");
}
