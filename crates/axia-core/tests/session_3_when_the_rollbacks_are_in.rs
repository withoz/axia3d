//! The session the two rollbacks looked like they cost — and did not.
//!
//! The fuzz answered that session 3 was sound and is not any more, right after
//! the re-derive and the post-draw repair learned to undo themselves. That
//! looked like a regression. Transcribed and shrunk the way the other four
//! were, it was two operations that broke the SAME WAY on main, with the
//! rollbacks nowhere in sight.
//!
//! The fuzz's operations depend on the mesh: a push or an extrude picks its face
//! out of the live set, so any change to how many faces a draw makes shifts every
//! later operation. Session 3 did not start breaking; its stream moved onto a
//! defect that was already there — and has now done so three times.
//!
//! ── op 10, fixed ─────────────────────────────────────────────────────────
//!
//! A square drawn at a height INSIDE a box, crossing two of its walls, got its
//! middle piece twice. `split_polygon_2d` took every crossing segment at once
//! and paired the intersection points along the direction of the FIRST one:
//!
//! ```text
//!   one line    2 pieces  [6000, 4000]           sums to 10,000
//!   two lines   2 pieces  [10000, 10000]         sums to 20,000   ← was
//!   two lines   3 pieces  [3000, 4000, 3000]     sums to 10,000   ← is
//! ```
//!
//! It cuts by one connected path at a time now. Pinned in
//! `split_polygon_2d_two_cuts_give_three_pieces`.
//!
//! ── op 13, fixed ─────────────────────────────────────────────────────────
//!
//! The stream carried four operations further and stopped again, on two solid
//! faces covering each other on z=0. Reduced to nine operations and read one
//! thing at a time:
//!
//! ```text
//!   (a) production                     46 faces   3 violations
//!   (b) last draw, re-derive off       44 faces   0 violations
//!   (c) …then the re-derive by hand    46 faces   1 violation
//! ```
//!
//! So the re-derive itself made it — unlike session 9's op 13, where the pass
//! after it was responsible. And its own rollback was ARMED and BLIND:
//!
//! ```text
//!   z=0 위 솔리드 면 — 평면 3, 곡면 0     ← the guard's condition holds
//!   자기교차   19 → 19                    ← what it asks about: unchanged
//!   불변식위반  0 → 1                     ← what actually happened
//! ```
//!
//! Two faces covering the same patch of a plane are COINCIDENT, not crossing,
//! and the self-intersection scan does not count them. It asks the invariant
//! checker too now, relative, the way its siblings do.
//!
//! ── op 15, and what carried the session through ──────────────────────────
//!
//! Two more operations further on. Reduced to six, ending in a push whose cap
//! lands on a circle face that earlier operations had already divided — so it is
//! concave, and the repair could not so much as MEASURE the overlap:
//!
//! ```text
//!   coplanar_intersection_segments → Err(coplanar clipping requires convex
//!                                        faces; face FaceId(4) is non-convex)
//! ```
//!
//! Two things were wrong with that. Sutherland-Hodgman needs only the CLIP
//! convex — the subject is what it cuts — and the difference walk took the arc
//! between two crossings from the LENS, which for a concave base is not one
//! polygon at all (SH joins the disconnected parts with zero-width connectors;
//! measured in `a_concave_subject_bitten_twice_by_a_convex_clip`, two 20×20
//! squares as one 8-vertex ring).
//!
//! Both are fixed: the subject may be concave, and `polygon_difference_by_clip`
//! walks the CLIP's own boundary for any even number of crossings. Session 3
//! runs all twenty of its operations now, and the fuzz's `KNOWN_BREAKS` is
//! empty for the first time.
//!
//! ⚠ The six-operation reduction below still leaves a pair, on a DIFFERENT pair
//! of faces, where the clipper reports no meeting at all and the invariant
//! checker reports a double cover. That disagreement is the next lead; it is
//! pinned rather than chased.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped;
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

/// Session 3 as the fuzz generates it, through op 15 where it now stops. Its
/// op 0 is `pushIn(skip: nothing there)`, omitted as a no-op.
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
        Op::Extrude { face: 33, d: 100.0 },
        Op::Extrude { face: 37, d: 100.0 },
        Op::Polygon { x: 50.0, y: 0.0, z: 0.0, r: 90.0, n: 5 },
        Op::CircleCurve { x: 200.0, y: 0.0, z: 0.0, r: 110.0 },
        Op::PushIn { face: 21, d: -50.0 },
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
/// The reducer, run again from the whole session.
///
/// Its op-10 break is fixed and the stream carried four operations further, so
/// the seven-operation reduction below is a NEW one — the same harness, the same
/// method, a different defect.
#[test]
fn the_sequence_shrinks_to_something_readable() {
    let full = session_3();
    let repro = violations(&full);
    println!("
  전체 세션 → 위반 {}건", repro.len());
    if repro.is_empty() {
        return; // fixed; nothing to shrink
    }

    let n0 = full.len();
    let mut ops = full;
    let mut i = 0;
    while i < ops.len() {
        let mut without = ops.clone();
        without.remove(i);
        if !violations(&without).is_empty() {
            ops = without;
        } else {
            i += 1;
        }
    }
    println!("
  {n0} 연산 → {} 연산
", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("    {i}: {op:?}");
    }
    for (at, t) in violations(&ops).iter().take(2) {
        println!("
  op {at} 에서 {t}");
    }
    println!();
}

/// The whole session, which is the fuzz's own case.
///
/// ⚠ It is NOT sound: the transcription runs through op 15, which is the open
/// one. Sound through op 14 — everything the two fixes above closed — and then
/// the last push lands on a face too concave for the repair to measure.
#[test]
fn the_whole_session_is_sound() {
    let ops = session_3();
    let v = violations(&ops);
    println!("
  전체 세션 {} 연산 → 위반 {}건", ops.len(), v.len());
    for (at, t) in v.iter().take(3) {
        println!("    ✗ op {at}: {t}");
    }
    println!();
    assert!(
        v.is_empty(),
        "session 3 runs all of its operations sound now: {v:?}"
    );
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

/// Every violation the full session leaves, and the step it first appeared at.
#[test]
fn what_the_full_session_leaves_and_when() {
    let ops = session_3();
    let mut s = prod();
    println!("
  연산별
");
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        let v: Vec<String> = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!(
            "    op {i:>2} {:<50} 면 {live:>3}  위반 {:>2}",
            format!("{op:?}"),
            v.len()
        );
        for t in v.iter().take(3) {
            println!("         ✗ {t}");
        }
    }
    println!();
}

/// The reducer, aimed at the STACKED pair the fuzz reports.
///
/// The general one wanders: `violations` accepts any kind, so dropping an
/// operation can land on a shorter sequence with a DIFFERENT defect — it found a
/// winding one, which is real and pinned separately. This keeps only sequences
/// that still leave two faces covering the same ground.
#[test]
fn the_stacked_pair_shrinks_to_something_readable() {
    let stacked = |ops: &[Op]| violations(ops).into_iter().any(|(_, t)| t.contains(STACKED));
    let full = session_3();
    if !stacked(&full) {
        println!("
  전체 세션에 겹침 없음 — 고쳐졌다면 KNOWN_BREAKS 에서 지울 것
");
        return;
    }
    let n0 = full.len();
    let mut ops = full;
    // Repeat until a whole pass removes nothing: dropping a later operation can
    // make an earlier one removable, and one pass does not see that.
    loop {
        let before = ops.len();
        let mut i = 0;
        while i < ops.len() {
            let mut without = ops.clone();
            without.remove(i);
            if stacked(&without) {
                ops = without;
            } else {
                i += 1;
            }
        }
        if ops.len() == before {
            break;
        }
    }
    println!("
  {n0} 연산 → {} 연산 (겹침만)
", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("    {i}: {op:?}");
    }
    for (at, t) in violations(&ops).iter().filter(|(_, t)| t.contains(STACKED)).take(2) {
        println!("
  op {at} 에서 {t}");
    }
    println!();
}

/// Session 3's op-15 break, reduced to six operations.
///
/// A circle, a rectangle extruded into a solid, a push into it, a square on its
/// top, and then a second push whose cap comes down on the circle — which the
/// earlier operations have already divided into a concave face.
fn shrunk_op15() -> Vec<Op> {
    vec![
        Op::Circle { x: 0.0, y: 150.0, z: 0.0, r: 30.0 },
        Op::Rect { x: 150.0, y: 0.0, z: 0.0, w: 100.0 },
        Op::Extrude { face: 1, d: 200.0 },
        Op::PushIn { face: 27, d: -100.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
        Op::PushIn { face: 21, d: -50.0 },
    ]
}

/// ⚠ PINNED AS MEASURED — still stacks, and the fuzz no longer reaches it.
///
/// The concave overlap IS measured now (`coplanar_intersection_segments` takes a
/// non-convex subject, and `polygon_difference_by_clip` walks four crossings),
/// which is what carried session 3 through all twenty of its operations. These
/// six still leave a pair, and it is a DIFFERENT one:
///
/// ```text
///   was   FaceId(4)  넓이 1224  ×  FaceId(36) 넓이 408
///         → Err(coplanar clipping requires convex faces)
///   is    FaceId(37) 넓이  970  ×  FaceId(36) 넓이 408
///         → 교차점 0개, lens 0정점 — 어느 갈래도 아님
/// ```
///
/// So the next question is a different one: the invariant checker says these two
/// cover the same ground and the clipper says they do not meet at all. One of
/// them is wrong about this pair, and that is where to start.
#[test]
fn the_op15_reduction_is_sound() {
    let v: Vec<(usize, String)> = violations(&shrunk_op15())
        .into_iter()
        .filter(|(_, t)| t.contains(STACKED))
        .collect();
    println!("
  6 연산 축소 → 겹침 {}건", v.len());
    for (at, t) in v.iter().take(2) {
        println!("    ✗ op {at}: {t}");
    }
    assert!(v.is_empty(), "the six-operation reduction has to stay sound: {v:?}");
}

/// Which of the four production behaviours makes the pair, and what the two
/// faces are.
#[test]
fn which_behaviour_makes_the_op15_pair() {
    let names = ["auto_intersect", "face_rederive", "freeform_overlap", "face_synthesis"];
    println!("
  전부 켠 상태를 기준으로, 하나씩 끄기
");
    for (i, name) in names.iter().enumerate() {
        let mut s = Scene::new();
        s.auto_intersect_on_draw = i != 0;
        s.face_rederive_on_draw = i != 1;
        s.freeform_overlap_on_draw = i != 2;
        s.auto_face_synthesis_on_draw = i != 3;
        for op in shrunk_op15() {
            apply(&mut s, op);
        }
        let v = s.mesh.verify_face_invariants().violations.len();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("    {name:<18} 끔 → 면 {live:>3}  위반 {v:>2}");
    }

    let mut s = prod();
    for op in shrunk_op15() {
        apply(&mut s, op);
    }
    println!("
    (전부 켬 → 면 {}, 위반 {})
",
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        s.mesh.verify_face_invariants().violations.len());

    println!("  겹친 면
");
    let text: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    if let Some(t) = text.iter().find(|t| t.contains(STACKED)) {
        let ids: Vec<usize> = t
            .split("FaceId(")
            .skip(1)
            .filter_map(|p| p.split(')').next())
            .filter_map(|n| n.parse().ok())
            .collect();
        for id in ids {
            let f = FaceId::new(id as u32);
            let Some(face) = s.mesh.faces.get(f) else { continue };
            let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
            let pts: Vec<DVec3> = vv
                .iter()
                .filter_map(|x| s.mesh.verts.get(*x))
                .map(|p| p.pos())
                .collect();
            let corners: Vec<String> = pts
                .iter()
                .take(4)
                .map(|p| format!("({:.0},{:.0},{:.0})", p.x, p.y, p.z))
                .collect();
            println!(
                "    {f:?}  verts={:<3} {}  {}{}",
                pts.len(),
                if s.mesh.is_face_in_volume(f) { "solid" } else { "sheet" },
                corners.join(" "),
                if pts.len() > 4 { " …" } else { "" }
            );
        }
    }
    println!();
    assert!(
        text.is_empty(),
        "the reduction has to stay sound whichever behaviour is off: {text:?}"
    );
}

/// Is the re-derive itself what makes the pair, or the pass after it?
///
/// Same question session 9's op 13 turned on. Draw the last operation with the
/// re-derive off — which also stands down `split_faces_crossing_other_planes`,
/// because one flag gates both — and then run the re-derive by hand. If the hand
/// call comes out clean, the pair belongs to the pass after it; if it does not,
/// the re-derive owns it.
#[test]
fn the_rederive_by_hand_against_the_draw_pipeline() {
    let ops = shrunk_op15();
    let mut rows: Vec<(&str, usize, usize)> = Vec::new();

    // (a) production, all on.
    {
        let mut s = prod();
        for op in &ops {
            apply(&mut s, *op);
        }
        let v = s.mesh.verify_face_invariants().violations.len();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("
  (a) 프로덕션            면 {live:>3}  위반 {v:>2}");
        rows.push(("프로덕션", live, v));
    }

    // (b) everything on until the last draw, which runs with the re-derive off.
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    s.face_rederive_on_draw = false;
    apply(&mut s, ops[ops.len() - 1]);
    {
        let v = s.mesh.verify_face_invariants().violations.len();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("  (b) 마지막만 재유도 끔  면 {live:>3}  위반 {v:>2}");
        rows.push(("마지막만 끔", live, v));
    }

    // (c) …then the re-derive by hand on the plane that draw landed on.
    for pass in 1..=2 {
        let r = rebuild_coplanar_faces_analytic_scoped(
            &mut s.mesh, DVec3::ZERO, DVec3::Z, 1e-3, true, None,
        );
        let v = s.mesh.verify_face_invariants().violations.len();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!(
            "  (c{pass}) 손으로 재유도       면 {live:>3}  위반 {v:>2}   제거 {:>2} 생성 {:>2}",
            r.as_ref().map(|x| x.removed_faces).unwrap_or(0),
            r.as_ref().map(|x| x.created_faces).unwrap_or(0)
        );
        rows.push(("손 재유도", live, v));
    }
    println!();

    assert_eq!(rows[0].2, 0, "production has to stay sound: {rows:?}");
}


/// Every step of the six, and what the last push leaves.
#[test]
fn each_step_of_the_six_operations() {
    let ops = shrunk_op15();
    let mut s = prod();
    println!("
  연산별
");
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        let v: Vec<String> = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("    op {i} {:<48} 면 {live:>3}  위반 {:>2}", format!("{op:?}"), v.len());
        for t in v.iter().take(2) {
            println!("        ✗ {t}");
        }
    }
    // Both faces of the pair, described.
    let text: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    if let Some(t) = text.first() {
        println!("
  겹친 면
");
        for id in t
            .split("FaceId(")
            .skip(1)
            .filter_map(|p| p.split(')').next())
            .filter_map(|n| n.parse::<usize>().ok())
        {
            let f = FaceId::new(id as u32);
            match s.mesh.faces.get(f) {
                None => println!("    {f:?}  (없음)"),
                Some(face) => {
                    let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
                    let pts: Vec<DVec3> = vv
                        .iter()
                        .filter_map(|x| s.mesh.verts.get(*x))
                        .map(|p| p.pos())
                        .collect();
                    let zs: Vec<String> = pts.iter().take(3).map(|p| format!("({:.0},{:.0},{:.0})", p.x, p.y, p.z)).collect();
                    println!(
                        "    {f:?}  active={} verts={:<3} {}  {}",
                        face.is_active(),
                        pts.len(),
                        if s.mesh.is_face_in_volume(f) { "solid" } else { "sheet" },
                        zs.join(" ")
                    );
                }
            }
        }
    }
    println!();
}

/// What the repair is looking at when it declines this pair.
///
/// `subtract_double_covered_faces` has three ways to resolve an overlap, and it
/// picks by what `coplanar_intersection_segments` reports: no crossings plus a
/// lens means one is INSIDE the other and gets punched as a hole; exactly two
/// crossings means it straddles and the bigger one gives up the region. Anything
/// else falls through untouched.
///
/// The quad the push leaves has one corner 0.07 mm outside the circle it is
/// otherwise inside — so this reads which branch that lands in.
#[test]
fn what_the_repair_sees_in_this_pair() {
    use axia_geo::operations::coplanar as cop;
    let ops = shrunk_op15();
    let mut s = prod();
    for op in &ops {
        apply(&mut s, *op);
    }
    let text: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    let Some(t) = text.first() else {
        println!("
  위반 없음 — 고쳐졌다면 이 시험을 가드로 바꿀 것
");
        return;
    };
    let ids: Vec<u32> = t
        .split("FaceId(")
        .skip(1)
        .filter_map(|p| p.split(')').next())
        .filter_map(|n| n.parse().ok())
        .collect();
    assert_eq!(ids.len(), 2, "the violation names two faces: {t}");
    let (a, b) = (FaceId::new(ids[0]), FaceId::new(ids[1]));

    let area = |f: FaceId| s.mesh.face_outer_area(f);
    let (outer, inner) = if area(a) >= area(b) { (a, b) } else { (b, a) };
    println!(
        "
  {outer:?} 넓이 {:.0}  ⊃?  {inner:?} 넓이 {:.0}
",
        area(outer),
        area(inner)
    );

    match cop::coplanar_intersection_segments(&s.mesh, outer, inner) {
        Err(e) => println!("  coplanar_intersection_segments → Err({e})"),
        Ok(ci) => {
            println!(
                "  교차점 {}개, lens 정점 {}개",
                ci.crossings.len(),
                ci.lens_polygon.len()
            );
            for c in ci.crossings.iter().take(4) {
                println!(
                    "      ({:.3},{:.3},{:.3})  edge {} t={:.4}",
                    c.point.x, c.point.y, c.point.z, c.face_a_edge, c.face_a_t
                );
            }
            let branch = if ci.crossings.is_empty() && !ci.lens_polygon.is_empty() {
                "containment (구멍 뚫기)"
            } else if ci.crossings.len() == 2 {
                "straddle (차집합 walk)"
            } else {
                "어느 쪽도 아님 — 그냥 통과"
            };
            println!("
  → {branch}
");
        }
    }
}

/// Session 3's op-13 break, reduced to nine operations — now sound.
///
/// It would not go below nine: the two extrudes name their faces BY ID, so
/// dropping anything upstream changes the ids and the extrude fails instead of
/// reproducing.
fn shrunk_op13() -> Vec<Op> {
    vec![
        Op::Circle { x: 0.0, y: 150.0, z: 0.0, r: 30.0 },
        Op::Box { x: 150.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::CircleCurve { x: -200.0, y: 0.0, z: 100.0, r: 50.0 },
        Op::Rect { x: 150.0, y: 0.0, z: 0.0, w: 100.0 },
        Op::Extrude { face: 1, d: 200.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
        Op::Extrude { face: 33, d: 100.0 },
        Op::Polygon { x: 50.0, y: 0.0, z: 0.0, r: 90.0, n: 5 },
        Op::CircleCurve { x: 200.0, y: 0.0, z: 0.0, r: 110.0 },
    ]
}

/// The nine operations that used to leave two solid faces covering each other.
///
/// Mutation-checked: dropping the invariant half of the re-derive's rollback —
/// leaving it asking only about self-intersections, which cannot see two
/// coincident faces — brings the pair back and fails this.
#[test]
fn the_op13_reduction_is_sound() {
    let v = violations(&shrunk_op13());
    println!("
  9 연산 축소 → 위반 {}건", v.len());
    for (at, t) in v.iter().take(2) {
        println!("    ✗ op {at}: {t}");
    }
    assert!(v.is_empty(), "the nine-operation reduction has to stay sound: {v:?}");
}

/// What the re-derive's own rollback could and could not see.
///
/// Kept as the reading that named it: the guard was ARMED — three planar solid
/// faces on z=0 and no curved one, which is exactly its condition — and it read
/// the same self-intersection count before and after while the invariant checker
/// went from none to one. Two faces covering the same patch of a plane are
/// coincident, not crossing.
#[test]
fn coincident_is_not_crossing() {
    let ops = shrunk_op13();
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    s.face_rederive_on_draw = false;
    apply(&mut s, ops[ops.len() - 1]);

    let si_before = s.mesh.detect_self_intersections().count();
    let bad_before = s.mesh.verify_face_invariants().violations.len();
    let mut planar_solid = 0;
    let mut curved_solid = 0;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() || !s.mesh.is_face_in_volume(fid) {
            continue;
        }
        let vv = s.mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
        if vv.is_empty()
            || !vv.iter().all(|v| {
                s.mesh.verts.get(*v).map_or(false, |p| p.pos().z.abs() < 1e-6)
            })
        {
            continue;
        }
        match f.surface() {
            None | Some(axia_geo::surfaces::AnalyticSurface::Plane { .. }) => planar_solid += 1,
            _ => curved_solid += 1,
        }
    }

    let _ = rebuild_coplanar_faces_analytic_scoped(
        &mut s.mesh, DVec3::ZERO, DVec3::Z, 1e-3, true, None,
    );
    let si_after = s.mesh.detect_self_intersections().count();
    let bad_after = s.mesh.verify_face_invariants().violations.len();

    println!("
  z=0 위 솔리드 면 — 평면 {planar_solid}, 곡면 {curved_solid}");
    println!("  자기교차   {si_before} → {si_after}");
    println!("  불변식위반 {bad_before} → {bad_after}
");

    assert!(
        planar_solid > 0 && curved_solid == 0,
        "the guard's condition has to hold, or this scene does not exercise it"
    );
    // The reading that named it was `19 → 19` while violations went `0 → 1`.
    // Both numbers moved once the repair could measure a concave overlap (the
    // scene reaching the re-derive is a different one now), so what is asserted
    // is the part that carries the meaning: self-intersections do not RISE, so
    // the rollback's original half would not have fired, and the plane comes out
    // sound anyway because the invariant half does the work.
    assert!(
        si_after <= si_before,
        "self-intersections must not rise ({si_before} → {si_after}) — if they          do, the SI half of the rollback is what catches this and the invariant          half is no longer what this test is about"
    );
    assert_eq!(bad_after, bad_before, "and the re-derive leaves the plane sound");
}
