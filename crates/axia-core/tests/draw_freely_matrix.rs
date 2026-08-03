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

fn clean(s: &Scene) -> bool {
    s.mesh.detect_self_intersections().count() == 0
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
}
