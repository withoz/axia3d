//! The session the two rollbacks looked like they cost — and did not.
//!
//! The fuzz answered that session 3 was sound and is not any more, right after
//! the re-derive and the post-draw repair learned to undo themselves. That looked
//! like a regression. Transcribed and shrunk the way the other four were, it is
//! **two operations** — a box, and a square drawn at a height inside it — and
//! those two broke the SAME WAY on main, with the rollbacks nowhere in sight.
//!
//! The fuzz's operations depend on the mesh: a push or an extrude picks its face
//! out of the live set, so any change to how many faces a draw makes shifts every
//! later operation. Session 3 did not start breaking; its stream moved onto a
//! defect that was already there.
//!
//! ── Located, then fixed ──────────────────────────────────────────────────
//!
//! Probed through the draw: the plane was clean at every stage until
//! `split_faces_crossing_other_planes`, which took it from seven faces and no
//! violations to ten and two. And it was the SECOND wall that did it:
//!
//! ```text
//!   square crossing one wall    8 faces   0 violations
//!   square crossing two walls  10 faces   2 violations
//! ```
//!
//! The scene layer was not calling twice — `crossing` refuses to queue a face
//! whose partner is already there, so `intersect_faces_with_model` is entered
//! ONCE. It came back with the middle piece twice, and the chain ran down to a
//! function with no meshes in it at all: `split_polygon_2d` took every crossing
//! segment at once and paired the intersection points by projecting them all
//! onto the direction of the FIRST segment. With two lines that paired one
//! line's end with the other line's end, and the walk following those pairings
//! went round the outside and returned the whole polygon.
//!
//! ```text
//!   one line    2 pieces  [6000, 4000]           sums to 10,000
//!   two lines   2 pieces  [10000, 10000]         sums to 20,000   ← was
//!   two lines   3 pieces  [3000, 4000, 3000]     sums to 10,000   ← is
//! ```
//!
//! It cuts by one line at a time now, each piece from the previous line offered
//! to the next, which reuses the one-line path exactly as it was — measured
//! correct all along. Pinned in `split_polygon_2d_two_cuts_give_three_pieces`.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const STACKED: &str = "cover the same ground";

#[derive(Clone, Copy, Debug)]
enum Op {
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Punch { x: f64, y: f64, z: f64 },
    Extrude { face: u32, d: f64 },
    PushIn { face: u32, d: f64 },
}

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn apply(s: &mut Scene, op: Op) {
    match op {
        Op::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, segments: 24,
            });
        }
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r,
            });
        }
        Op::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z), ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: rx, radius_y: ry,
            });
        }
        Op::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, sides: n,
            });
        }
        Op::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, up: DVec3::X,
                width: w, height: w * 0.75,
            });
        }
        Op::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(
                DVec3::new(x, y, z) + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL,
            );
        }
        Op::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z,
            );
        }
        Op::Extrude { face, d } | Op::PushIn { face, d } => {
            s.execute(Command::CreateSolid {
                face_id: FaceId::new(face),
                mode: CreateSolidMode::Extrude { distance: d },
            });
        }
    }
}

/// Every violation the list leaves, whatever kind, with the step it appeared at.
fn violations(ops: &[Op]) -> Vec<(usize, String)> {
    let mut s = prod();
    let mut out = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        for v in s.mesh.verify_face_invariants().violations.iter() {
            out.push((i, format!("{v:?}")));
        }
    }
    out
}

/// Session 3 as the fuzz generates it. Its op 0 is `pushIn(skip: nothing there)`
/// and its op 3 a refused punch, both omitted as no-ops.
fn session_3() -> Vec<Op> {
    vec![
        Op::Circle { x: 0.0, y: 150.0, z: 0.0, r: 30.0 },
        Op::Ellipse { x: -150.0, y: 150.0, z: 100.0, rx: 90.0, ry: 54.0 },
        Op::Punch { x: -50.0, y: 200.0, z: 0.0 },
        Op::PushIn { face: 2, d: -100.0 },
        Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::CircleCurve { x: -200.0, y: 0.0, z: 100.0, r: 50.0 },
        Op::Rect { x: 150.0, y: 0.0, z: 0.0, w: 100.0 },
        Op::Extrude { face: 1, d: 200.0 },
        Op::PushIn { face: 27, d: -100.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
    ]
}

/// The shrunk sequence: a box, and a square drawn at a height INSIDE it.
///
/// z=200 is not a face of the box — it spans 100 to 220 — so the square is a
/// sheet passing through the solid's middle, crossing two of its walls.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
    ]
}

/// The two operations that used to leave a stacked pair.
///
/// Mutation-checked: making `group_collinear_cuts` return one group for
/// everything — the old behaviour of handing every segment over at once — brings
/// the pair back and fails this.
#[test]
fn the_shrunk_sequence_is_sound() {
    let v = violations(&shrunk());
    println!("\n  줄인 순서 2 연산 → 위반 {}건", v.len());
    for (at, t) in &v {
        println!("    ✗ op {at}: {t}");
    }
    assert!(
        v.is_empty(),
        "a square drawn inside a box has to stay sound — it used to come back \
         with its middle piece twice: {v:?}"
    );
}

/// And the whole session it was reduced from, which is the fuzz's own case.
#[test]
fn the_whole_session_is_sound() {
    let v = violations(&session_3());
    println!("\n  전체 세션 위반 {}건", v.len());
    for (at, t) in &v {
        println!("    ✗ op {at}: {t}");
    }
    assert!(v.is_empty(), "session 3 of the fuzz has to stay sound: {v:?}");
}

/// One wall or two — the reading that located it, kept as the guard.
///
/// The square in the repro sticks out of the box on TWO sides: its `(100,170)`
/// corner past the +Y wall and its `(30,100)` corner past the −X wall. Moving it
/// to `(150,100)` leaves it crossing one. One wall was always clean and two
/// duplicated the middle piece; both are clean now, and the one-wall row stays
/// so a fix that trades one case for the other fails here.
#[test]
fn crossing_one_wall_and_crossing_two_are_both_clean() {
    let mut readings: Vec<(&str, usize, usize)> = Vec::new();
    for (name, cx, cy) in [("벽 하나", 150.0, 100.0), ("벽 둘", 100.0, 100.0)] {
        let mut s = prod();
        apply(&mut s, Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 });
        apply(&mut s, Op::Polygon { x: cx, y: cy, z: 200.0, r: 70.0, n: 4 });
        let v: Vec<String> = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("\n  {name}: 면 {faces}, 위반 {}", v.len());
        for t in v.iter().take(2) {
            println!("      ✗ {t}");
        }
        readings.push((name, faces, v.len()));
    }

    for (name, faces, bad) in &readings {
        assert_eq!(*bad, 0, "{name}: {faces} faces and {bad} violations");
    }
    // Two walls divide the square into three, so it has to leave MORE faces than
    // one wall does. Without this the test would still pass on a version that
    // stopped cutting altogether.
    assert!(
        readings[1].1 > readings[0].1,
        "two walls have to leave more faces than one — {} against {}, which \
         would mean the second wall stopped cutting rather than stopped \
         duplicating",
        readings[1].1,
        readings[0].1
    );
}

/// What the square becomes, so a later change that leaves it whole is visible.
///
/// The box is six faces; the square crosses two walls, so it arrives as three
/// pieces and every one of them is a sheet, not a solid face.
#[test]
fn the_square_arrives_as_three_pieces() {
    let ops = shrunk();
    let mut s = prod();
    apply(&mut s, ops[0]);
    let before: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .collect();
    println!("\n  상자만: {}면", before.len());

    apply(&mut s, ops[1]);
    println!("\n  사각형(z=200, 상자 속) 그린 뒤\n");
    let mut new_at_z200 = 0;
    for (fid, face) in s.mesh.faces.iter() {
        if !face.is_active() || before.contains(&fid) {
            continue;
        }
        let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
        let on_plane = !vv.is_empty()
            && vv.iter().all(|v| {
                s.mesh.verts.get(*v).map_or(false, |p| (p.pos().z - 200.0).abs() < 1e-6)
            });
        if !on_plane {
            continue;
        }
        new_at_z200 += 1;
        let n = face.normal().normalize_or_zero();
        println!(
            "    {fid:?}  n=({:+.0},{:+.0},{:+.0})  verts={}",
            n.x, n.y, n.z, vv.len()
        );
    }
    println!("\n  z=200 위 새 면 {new_at_z200}개");
    assert_eq!(
        new_at_z200, 3,
        "the square crosses two walls, so it lands as three pieces — two would \
         mean one line stopped cutting, four that a piece is duplicated again"
    );
}
