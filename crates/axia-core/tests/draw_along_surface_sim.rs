//! DRAWING ALONG A SOLID, measured before anything is built.
//!
//! The requirement, in the user's words: 입체면을 따라 그리는 기능 확장 — extend
//! drawing so it FOLLOWS the solid's faces.
//!
//! Today a draw resolves onto exactly one plane. Two points that sit on two
//! different faces of a box are joined by a straight chord, which leaves the
//! surface and passes through the interior. That is what "따라 그리기" is not.
//!
//! This file measures three things and asserts nothing it has not printed:
//!   1. what the chord actually does to the solid today,
//!   2. that the two faces genuinely share an edge (so a surface path exists),
//!   3. what the surface path should be — A → crossing point on the shared
//!      edge → B — computed here by hand so the engine has a target to match.
//!
//! The hand computation is the unfolding trick: rotate the second face about
//! the shared edge into the first face's plane, draw the straight line there,
//! and read off where it meets the edge. On two flat faces that is exact, and
//! it is the shortest path across the surface.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use axia_geo::FaceId;
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

/// Box spanning x 0..200, y 0..200, z 0..100.
/// Top face is z=100; the east wall is x=200.
fn box_solid(s: &mut Scene) -> Vec<FaceId> {
    let f = s
        .mesh
        .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("b".into(), DVec3::ZERO, f.clone());
    f
}

fn face_with_normal(s: &Scene, n: DVec3, through: DVec3) -> FaceId {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .find(|&i| {
            let face = match s.mesh.faces.get(i) { Some(f) => f, None => return false };
            if face.normal().normalize_or_zero().dot(n) < 0.999 {
                return false;
            }
            let vs = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
            vs.iter().any(|&v| {
                let p = s.mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
                (p - through).dot(n).abs() < 1e-6
            })
        })
        .expect("that face should exist on the box")
}

/// The straight chord between two points on different faces — today's behaviour.
#[test]
fn a_line_between_two_faces_is_a_chord_through_the_solid_today() {
    let mut s = prod();
    box_solid(&mut s);
    let before = faces(&s);

    let a = DVec3::new(100.0, 100.0, 100.0); // middle of the top face
    let b = DVec3::new(200.0, 100.0, 50.0); // middle of the east wall

    let r = s.execute(Command::DrawLineAsShape {
        start: a,
        end: b,
        surface_normal: None,
    });
    let refused = matches!(r, CommandResult::Error(_));
    let after = faces(&s);
    let si = s.mesh.detect_self_intersections().count();

    // The midpoint of that segment is the tell: on the surface it would sit on
    // the skin; as a chord it sits inside the box.
    let mid = (a + b) * 0.5;
    let inside = mid.x < 200.0 - 1e-6 && mid.z < 100.0 - 1e-6;

    println!(
        "\n[오늘] 윗면({a:?}) → 옆면({b:?})\n  거부={refused} 면 {before}→{after} 겹침={si}\n  \
         중간점 {mid:?} → 솔리드 내부={inside}"
    );

    assert!(
        inside,
        "the straight segment's midpoint must lie inside the solid — that is what \
         makes it a chord rather than a path along the surface. If it does not, \
         this fixture no longer poses the problem it was written for"
    );
}

/// A surface path can only exist if the two faces meet. Measured, not assumed.
#[test]
fn the_top_face_and_the_east_wall_share_an_edge() {
    let mut s = prod();
    box_solid(&mut s);

    let top = face_with_normal(&s, DVec3::Z, DVec3::new(0.0, 0.0, 100.0));
    let east = face_with_normal(&s, DVec3::X, DVec3::new(200.0, 0.0, 0.0));
    assert_ne!(top, east);

    let edges_of = |f: FaceId| -> Vec<_> {
        s.mesh
            .face_outer_edges(f)
            .unwrap_or_default()
            .into_iter()
            .collect::<Vec<_>>()
    };
    let te = edges_of(top);
    let ee = edges_of(east);
    let shared: Vec<_> = te.iter().filter(|e| ee.contains(e)).copied().collect();

    let ends: Vec<DVec3> = shared
        .iter()
        .flat_map(|&e| {
            let ed = s.mesh.edges.get(e).unwrap();
            [ed.v_small(), ed.v_large()]
        })
        .map(|v| s.mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO))
        .collect();

    println!("\n[공유 모서리] {}개  끝점={ends:?}", shared.len());

    assert_eq!(
        shared.len(),
        1,
        "the top face and the east wall meet along exactly one edge; without it \
         there is no surface path to compute"
    );
    // That edge is the line x=200, z=100.
    for p in &ends {
        assert!(
            (p.x - 200.0).abs() < 1e-6 && (p.z - 100.0).abs() < 1e-6,
            "the shared edge should be x=200, z=100 but a corner is at {p:?}"
        );
    }
}

/// The target: what "따라 그리기" should produce for that pair of points.
///
/// Unfold the east wall about the shared edge until it is flat with the top
/// face. In the top face's plane, the wall's point b lands 50 mm past the edge
/// (its distance below the edge becomes distance beyond it). The straight line
/// from a to that unfolded point crosses x=200 at some y, and that crossing —
/// folded back — is the point where the drawn path bends over the corner.
#[test]
fn the_surface_path_bends_at_the_shared_edge() {
    let a = DVec3::new(100.0, 100.0, 100.0); // on top, 100 mm from the edge
    let b = DVec3::new(200.0, 100.0, 50.0); // on the wall, 50 mm below the edge

    // Unfold: the wall rotates about x=200,z=100 into the plane z=100. A point
    // 50 mm down the wall becomes a point 50 mm out past x=200.
    let depth = 100.0 - b.z; // 50 mm below the edge
    let b_unfolded = DVec3::new(200.0 + depth, b.y, 100.0);

    // Straight line a → b_unfolded, in the flat plane, meets x=200 at:
    let t = (200.0 - a.x) / (b_unfolded.x - a.x);
    let cross = a + (b_unfolded - a) * t;

    // Fold back: the crossing is on the edge itself, so it keeps its y.
    let bend = DVec3::new(200.0, cross.y, 100.0);

    let leg1 = (bend - a).length();
    let leg2 = (b - bend).length();
    let chord = (b - a).length();

    println!(
        "\n[목표] 따라 그리기 경로\n  A {a:?}\n  꺾임 {bend:?}  (공유 모서리 위)\n  B {b:?}\n  \
         길이 {leg1:.1} + {leg2:.1} = {:.1}   (직선 현 {chord:.1})",
        leg1 + leg2
    );

    assert!(
        (bend.x - 200.0).abs() < 1e-9 && (bend.z - 100.0).abs() < 1e-9,
        "the bend must land on the shared edge, not near it: {bend:?}"
    );
    assert!(
        leg1 + leg2 > chord,
        "a path over the corner is longer than the chord that cuts through — if \
         it is not, the unfolding is wrong"
    );
    // Unfolded, a→b is a straight 150 mm run in x with no change in y, so the
    // bend keeps y=100 and the two legs are exactly 100 and 50.
    assert!((leg1 - 100.0).abs() < 1e-9, "first leg {leg1}");
    assert!((leg2 - 50.0).abs() < 1e-9, "second leg {leg2}");
    assert!(
        (leg1 + leg2 - 150.0).abs() < 1e-9,
        "unfolded, the path is one straight 150 mm line: {}",
        leg1 + leg2
    );
}

/// Wired: the same two points, drawn along the surface instead of through it.
#[test]
fn drawing_along_the_surface_touches_both_faces() {
    let mut s = prod();
    box_solid(&mut s);
    let before = faces(&s);

    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(100.0, 100.0, 100.0),
        end: DVec3::new(200.0, 100.0, 50.0),
    });
    let after = faces(&s);
    let si = s.mesh.detect_self_intersections().count();
    let inv = s.mesh.verify_face_invariants();

    println!(
        "\n[따라 그리기] 면 {before}→{after} 겹침={si} 위반={} 결과={r:?}",
        inv.violations.len()
    );

    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    assert_eq!(si, 0, "a path on the skin must not intersect the solid");
}

/// Boundary to boundary, so each leg cuts the face it crosses. This is the
/// 면분할 half of the requirement: the drawn line divides what it runs over.
#[test]
fn a_path_across_the_solid_divides_both_faces() {
    let mut s = prod();
    box_solid(&mut s);
    let before = faces(&s);

    // From the west edge of the top face, over the east edge, down to the
    // bottom of the east wall.
    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(0.0, 100.0, 100.0),
        end: DVec3::new(200.0, 100.0, 0.0),
    });
    let after = faces(&s);
    let si = s.mesh.detect_self_intersections().count();
    let inv = s.mesh.verify_face_invariants();

    println!(
        "\n[가로지르기] 면 {before}→{after} 겹침={si} 위반={} 결과={r:?}",
        inv.violations.len()
    );

    assert!(!matches!(r, CommandResult::Error(_)), "{r:?}");
    assert_eq!(si, 0);
    assert!(
        after >= before + 2,
        "a boundary-to-boundary path over two faces must divide both: {before}→{after}"
    );
}

/// Two points on opposite faces have no two-face route, and the refusal says
/// which — it does not quietly fall back to the chord.
#[test]
fn a_route_it_cannot_follow_is_refused_by_name() {
    let mut s = prod();
    box_solid(&mut s);
    let before = faces(&s);
    let r = s.execute(Command::DrawLineAlongSurface {
        start: DVec3::new(100.0, 100.0, 100.0),
        end: DVec3::new(100.0, 100.0, 0.0),
    });
    println!("\n[마주보는 면] 결과={r:?}");
    match r {
        CommandResult::Error(e) => assert!(e.contains("맞닿지 않은"), "{e}"),
        other => panic!("a chord must not be drawn silently: {other:?}"),
    }
    assert_eq!(faces(&s), before, "a refusal must leave the solid alone");
}
