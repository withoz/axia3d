//! What came back with push-in, minimised.
//!
//! The fuzz's generator stopped producing push-in while seeds 6 and 10 were the
//! only reproductions of the two breaks in its inventory. Both are fixed and
//! pinned in their own files, so the branch is back — and it brings two
//! sessions with it, both saying the same kind of thing:
//!
//! ```text
//!   session 10, op 7:  edge EdgeId(18): shared by 4 active faces
//!                      (non-manifold) — FaceId(5) / FaceId(23) cover the
//!                      same ground (stacked)
//!   session  9, op 14: edge EdgeId(24): shared by 3 active faces
//!                      (non-manifold) — FaceId(11) / FaceId(35) cover the
//!                      same ground (stacked)
//!
//! ```
//!
//! Same method as the other two: transcribe the log, then greedily drop
//! operations for as long as the failure survives. The reducer targets the
//! stacked-face text specifically, so it cannot wander off to some other
//! violation and report on the wrong thing.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

/// The text that identifies this defect, as the invariant checker prints it.
const STACKED: &str = "cover the same ground";

#[derive(Clone, Copy, Debug)]
enum Op {
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    Line { x: f64, y: f64, z: f64 },
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
        Op::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                up: DVec3::X,
                width: w,
                height: w * 0.75,
            });
        }
        Op::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(
                DVec3::new(x, y, z) + DVec3::new(0.0, 0.0, 60.0),
                w,
                120.0,
                w,
                FORM_MATERIAL,
            );
        }
        Op::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
                segments: 24,
            });
        }
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
            });
        }
        Op::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z),
                ref_dir: DVec3::X,
                normal: DVec3::Z,
                radius_x: rx,
                radius_y: ry,
            });
        }
        Op::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
                sides: n,
            });
        }
        Op::Line { x, y, z } => {
            s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            });
        }
        Op::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z),
                DVec3::new(x + 30.0, y + 30.0, z),
                DVec3::Z,
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

/// First operation that leaves two faces covering the same ground.
fn run(ops: &[Op]) -> Option<(usize, String)> {
    let mut s = prod();
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        if let Some(v) = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|v| format!("{v:?}"))
            .find(|t| t.contains(STACKED))
        {
            return Some((i, v));
        }
    }
    None
}

/// Session 10, transcribed. Its op 0 was `pushIn(skip: nothing there)`.
fn session_10() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::PushIn { face: 11, d: -150.0 },
        Op::Rect { x: 150.0, y: -200.0, z: 0.0, w: 180.0 },
        Op::PushIn { face: 4, d: -100.0 },
        Op::CircleCurve { x: 0.0, y: 100.0, z: 100.0, r: 110.0 },
    ]
}

/// Session 9, transcribed.
fn session_9() -> Vec<Op> {
    vec![
        Op::Box { x: 100.0, y: -50.0, z: 200.0, w: 100.0 },
        Op::Ellipse { x: 200.0, y: -200.0, z: 100.0, rx: 30.0, ry: 18.0 },
        Op::Punch { x: 100.0, y: 200.0, z: 200.0 },
        Op::PushIn { face: 1, d: -100.0 },
        Op::Ellipse { x: 0.0, y: 200.0, z: 200.0, rx: 30.0, ry: 18.0 },
        Op::Box { x: -50.0, y: -150.0, z: 200.0, w: 140.0 },
        Op::Line { x: 200.0, y: -100.0, z: 0.0 },
        Op::Circle { x: 0.0, y: -50.0, z: 0.0, r: 110.0 },
        Op::CircleCurve { x: -200.0, y: 150.0, z: 0.0, r: 90.0 },
        Op::Extrude { face: 1, d: 100.0 },
        Op::Polygon { x: -100.0, y: -50.0, z: 200.0, r: 30.0, n: 5 },
        Op::CircleCurve { x: 100.0, y: -150.0, z: 100.0, r: 90.0 },
        Op::Box { x: 50.0, y: -150.0, z: 0.0, w: 220.0 },
        Op::CircleCurve { x: -100.0, y: -100.0, z: 200.0, r: 30.0 },
        Op::PushIn { face: 13, d: -50.0 },
    ]
}

fn shrink(name: &str, full: Vec<Op>) -> Vec<Op> {
    assert!(
        run(&full).is_some(),
        "{name}: the transcription does not reproduce the stacked faces — check \
         it against the fuzz log before shrinking anything"
    );
    let n0 = full.len();
    let mut ops = full;
    let mut i = 0;
    while i < ops.len() {
        let mut without = ops.clone();
        without.remove(i);
        if run(&without).is_some() {
            ops = without;
        } else {
            i += 1;
        }
    }
    println!("\n  {name}: {n0} 연산 → {} 연산\n", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("    {i}: {op:?}");
    }
    let (at, why) = run(&ops).expect("still fails");
    println!("\n  op {at} 에서 {why}\n");
    ops
}

#[test]
fn both_sessions_shrink_to_something_readable() {
    shrink("세션 10", session_10());
    shrink("세션 9", session_9());
}

/// Session 10's four operations, kept as the repro.
fn shrunk_10() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::PushIn { face: 4, d: -100.0 },
        Op::CircleCurve { x: 0.0, y: 100.0, z: 100.0, r: 110.0 },
    ]
}

/// Session 9's six.
fn shrunk_9() -> Vec<Op> {
    vec![
        Op::Box { x: 100.0, y: -50.0, z: 200.0, w: 100.0 },
        Op::Ellipse { x: 200.0, y: -200.0, z: 100.0, rx: 30.0, ry: 18.0 },
        Op::Ellipse { x: 0.0, y: 200.0, z: 200.0, rx: 30.0, ry: 18.0 },
        Op::Box { x: -50.0, y: -150.0, z: 200.0, w: 140.0 },
        Op::CircleCurve { x: -100.0, y: -100.0, z: 200.0, r: 30.0 },
        Op::PushIn { face: 13, d: -50.0 },
    ]
}

/// Where each sequence stops being sound, counting EVERY violation.
///
/// `run` only looks for the stacked text, which is the right predicate for
/// shrinking and the wrong one for understanding — it walks past anything else
/// without a word. One of these two breaks on the push itself and the other on
/// a draw that comes after one, so the difference matters.
#[test]
fn where_each_one_turns() {
    for (name, ops) in [("세션 10", shrunk_10()), ("세션 9", shrunk_9())] {
        let mut s = prod();
        println!("\n  {name} — 연산별 위반 수 (겹침 / 그 외)\n");
        for (i, op) in ops.iter().enumerate() {
            apply(&mut s, *op);
            let v: Vec<String> = s
                .mesh
                .verify_face_invariants()
                .violations
                .iter()
                .map(|x| format!("{x:?}"))
                .collect();
            let st = v.iter().filter(|t| t.contains(STACKED)).count();
            println!(
                "    op {i} {:<52} 위반 {:>3}  ({st} / {})",
                format!("{op:?}"),
                v.len(),
                v.len() - st
            );
            for t in v.iter().take(2) {
                println!("        · {t}");
            }
        }
    }
}

/// What is stacked on what, for both sequences.
///
/// Three questions per pair, because each one rules something out: were the
/// faces there before the last operation, does either belong to a solid, and
/// what height do they sit at. "Two new sheets on a solid's top" and "a solid's
/// own face duplicated" want different fixes.
#[test]
fn what_the_push_leaves_stacked() {
    for (name, ops) in [("세션 10", shrunk_10()), ("세션 9", shrunk_9())] {
        let mut s = prod();
        for op in &ops[..ops.len() - 1] {
            apply(&mut s, *op);
        }
        let before: Vec<FaceId> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .collect();
        apply(&mut s, *ops.last().unwrap());

        let z_of = |s: &Scene, f: FaceId| -> Option<f64> {
            let face = s.mesh.faces.get(f)?;
            let vv = s.mesh.collect_loop_verts(face.outer().start).ok()?;
            let zs: Vec<f64> = vv
                .iter()
                .filter_map(|v| s.mesh.verts.get(*v).map(|p| p.pos().z))
                .collect();
            if zs.is_empty() {
                return None;
            }
            let first = zs[0];
            zs.iter().all(|z| (z - first).abs() < 1e-6).then_some(first)
        };

        let mut seen: Vec<(FaceId, FaceId)> = Vec::new();
        for v in s.mesh.verify_face_invariants().violations.iter() {
            let t = format!("{v:?}");
            if !t.contains(STACKED) {
                continue;
            }
            let ids: Vec<usize> = t
                .split("FaceId(")
                .skip(1)
                .filter_map(|p| p.split(')').next())
                .filter_map(|n| n.parse().ok())
                .collect();
            if ids.len() < 2 {
                continue;
            }
            let (a, b) = (FaceId::new(ids[0] as u32), FaceId::new(ids[1] as u32));
            if !seen.contains(&(a, b)) {
                seen.push((a, b));
            }
        }

        println!("\n  {name} — 마지막 연산 전 활성 면 {}개, 겹침 짝 {}건\n", before.len(), seen.len());
        for (a, b) in &seen {
            let tag = |f: &FaceId| {
                format!(
                    "{f:?}({}, {}, z={:?})",
                    if before.contains(f) { "old" } else { "new" },
                    if s.mesh.is_face_in_volume(*f) { "solid" } else { "sheet" },
                    z_of(&s, *f)
                )
            };
            println!("    {} / {}", tag(a), tag(b));
        }
        assert!(!seen.is_empty(), "{name}: the repro has to leave stacked pairs");
    }
}

/// Which automatic behaviour leaves the stack.
///
/// Four flags run in production and any of them can add a face. Turning them off
/// one at a time says which one is responsible — and, when a break survives all
/// four being off, that it is not the arrangement's doing at all.
#[test]
fn which_arrangement_each_break_needs() {
    for (name, ops) in [("세션 10", shrunk_10()), ("세션 9", shrunk_9())] {
        println!("\n  {name}\n");
        let run_with = |ai: bool, afs: bool, fr: bool, fo: bool| -> Option<(usize, String)> {
            let mut s = Scene::new();
            s.auto_intersect_on_draw = ai;
            s.auto_face_synthesis_on_draw = afs;
            s.face_rederive_on_draw = fr;
            s.freeform_overlap_on_draw = fo;
            for (i, op) in ops.iter().enumerate() {
                apply(&mut s, *op);
                if let Some(v) = s
                    .mesh
                    .verify_face_invariants()
                    .violations
                    .iter()
                    .map(|v| format!("{v:?}"))
                    .find(|t| t.contains(STACKED))
                {
                    return Some((i, v));
                }
            }
            None
        };
        let mut seen: Vec<(&str, bool)> = Vec::new();
        for (label, a, b, c, d) in [
            ("all four on (production)", true, true, true, true),
            ("auto_intersect off", false, true, true, true),
            ("auto_face_synthesis off", true, false, true, true),
            ("face_rederive off", true, true, false, true),
            ("freeform_overlap off", true, true, true, false),
            ("all four off (engine default)", false, false, false, false),
        ] {
            let r = run_with(a, b, c, d);
            println!(
                "    {label:<32} {}",
                match &r {
                    Some((i, _)) => format!("op {i} 겹침"),
                    None => "sound".to_string(),
                }
            );
            seen.push((label, r.is_some()));
        }
        let broke = |l: &str| seen.iter().find(|(x, _)| *x == l).map(|(_, b)| *b).unwrap();
        if name == "세션 10" {
            // Needs the arrangement: either of these two off and it is sound.
            assert!(broke("all four on (production)"), "세션 10 must still break");
            assert!(!broke("auto_face_synthesis off"), "세션 10 needs face synthesis");
            assert!(!broke("face_rederive off"), "세션 10 needs the re-derive");
            assert!(!broke("all four off (engine default)"), "세션 10 is the arrangement's");
        } else {
            // Needs nothing: this one is `create_solid` on its own.
            assert!(
                broke("all four off (engine default)"),
                "세션 9 breaks with every automatic behaviour off — if it stops                  doing that, it has become a different defect and the account in                  this file is wrong"
            );
        }
    }
}

/// What is the face session 9 pushes, and what does the engine decide about it?
///
/// Session 9 breaks with every automatic behaviour turned off, so the
/// arrangement is not making these faces — `create_solid` is. Which face is
/// being pushed, and whether the engine classifies it as move-only (ADR-196),
/// decides whether this is a dispatch question or an extrude question.
#[test]
fn session_9_pushes_a_wall_the_engine_thinks_is_a_sheet() {
    let ops = shrunk_9();
    let mut s = Scene::new(); // engine default: every automatic behaviour off
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    let pushed = FaceId::new(13);
    let live: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();

    println!("\n  밀기 전 활성 면 {}개", live.len());
    for f in &live {
        let Some(face) = s.mesh.faces.get(*f) else { continue };
        let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
        let n = face.normal().normalize_or_zero();
        println!(
            "    {f:?}  {}  n=({:+.0},{:+.0},{:+.0})  verts={}",
            if s.mesh.is_face_in_volume(*f) { "solid" } else { "sheet" },
            n.x,
            n.y,
            n.z,
            vv.len()
        );
    }
    let described = s.mesh.faces.get(pushed).map(|f| {
        let vv = s.mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
        let zs: Vec<f64> = vv
            .iter()
            .filter_map(|v| s.mesh.verts.get(*v).map(|p| p.pos().z))
            .collect();
        (
            f.is_active(),
            s.mesh.is_face_in_volume(pushed),
            f.normal().normalize_or_zero(),
            vv.len(),
            zs.first().copied(),
        )
    });
    println!("  {pushed:?} = {described:?}");
    let move_only_before = axia_geo::operations::push_pull::is_move_only(&s.mesh, pushed);
    println!("  is_move_only = {move_only_before:?}");

    apply(&mut s, *ops.last().unwrap());
    let after: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    println!("  밀기 후 활성 면 {}개", after.len());
    let v: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    println!("  위반 {}건", v.len());
    for t in v.iter().take(4) {
        println!("    ✗ {t}");
    }
    assert!(described.is_some(), "the pushed face has to exist before the push");
    let (active, in_volume, _, _, _) = described.unwrap();
    assert!(active, "the pushed face has to be live");

    // The measured root cause, pinned.
    //
    // The table above shows box 2 (FaceIds 8..13) with its BOTTOM face gone and
    // a 47-vertex sheet in its place — the circle drawn on z=200, coincident
    // with that bottom, replaced it without reusing the box's own boundary
    // edges. The two walls that stood on those edges lost their neighbour, so
    // they read as sheets; pushing one is therefore not move-only, and
    // `create_solid` extrudes it into a second box on top of the first.
    assert!(
        !in_volume,
        "FaceId(13) is a wall of a box that the engine no longer sees as closed.          If this starts reporting `solid`, the draw has stopped opening the box          and the push should be move-only — which is the fix, and this assertion          is how it announces itself"
    );
    assert!(
        !move_only_before,
        "and so the push is not move-only — same signal, one layer up. Read          BEFORE the push: afterwards the wall has become part of the second box          this defect builds, and reports move-only again"
    );
}

/// ⚠ PINNED AS MEASURED — two OPEN defects.
///
/// Both are inventoried in the fuzz's `KNOWN_BREAKS` as well. These assert the
/// reduced sequences still fail, so the day either stops, the test says so
/// instead of passing quietly.
#[test]
fn both_reduced_sequences_still_break() {
    let a = run(&shrunk_10());
    let b = run(&shrunk_9());
    println!("
  세션 10 → {a:?}
  세션 9  → {b:?}");
    assert!(
        a.is_some(),
        "세션 10 no longer leaves faces on top of each other — if somebody fixed          it, say what, strike it from KNOWN_BREAKS, and turn this into the test          that proves it"
    );
    assert!(b.is_some(), "세션 9 likewise");
}
