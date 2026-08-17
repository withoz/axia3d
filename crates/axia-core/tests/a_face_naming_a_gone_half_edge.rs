//! A spur split twice, and the loop that never came home. The fuzz's second
//! find — now the guard that it is fixed.
//!
//! Seed `0x5EED0006` broke at operation 17 of 20 with
//!
//! ```text
//!   face FaceId(20): cannot collect outer loop: HalfEdge HeId(420) not found
//! ```
//!
//! A face still naming a half-edge that is gone — structural corruption, not a
//! geometric one. Eighteen ordinary draws got there: ellipses, circles,
//! kernel-native circles, lines and rects at three heights. No push, no carve.
//!
//! Eighteen operations is not a bug report, so this file shrank it to ten by
//! plain delta debugging — drop one operation, keep the drop if the failure
//! survives. It took two goes to fix, and the first was not enough:
//!
//! 1. The coplanar rebuild removed every edge on the plane it was rebuilding,
//!    including edges a face it had chosen to PRESERVE was still standing on.
//!    Narrowing that turned "half-edge not found" into "Loop traversal exceeded
//!    10000" — the face kept its edges and its ring still did not close.
//! 2. Walking the ring by hand said where. `Mesh::split_edge` copies `prev` and
//!    `next` from the half-edge it replaces, which is right when the neighbour
//!    outlives the call. A **dangling spur** puts both half-edges of an edge in
//!    the same loop, consecutively, so each names the other — and this call
//!    deactivates both. The ring was left running through a dead half-edge
//!    whose own `next` re-entered the middle of the ring.
//!
//! The walk is kept below as a guard, because "the loop closes" is the property
//! and "no violation is reported" is only the symptom.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult};
use glam::DVec3;

/// One drawn shape, described so a shrunk list can be read and re-typed.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Line { x: f64, y: f64, z: f64 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
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
    let cmd = match op {
        Op::Ellipse { x, y, z, rx, ry } => Command::DrawEllipseAsCurve {
            center: DVec3::new(x, y, z),
            ref_dir: DVec3::X,
            normal: DVec3::Z,
            radius_x: rx,
            radius_y: ry,
        },
        Op::Circle { x, y, z, r } => Command::DrawCircleAsShape {
            center: DVec3::new(x, y, z),
            normal: DVec3::Z,
            radius: r,
            segments: 24,
        },
        Op::CircleCurve { x, y, z, r } => Command::DrawCircleAsCurve {
            center: DVec3::new(x, y, z),
            normal: DVec3::Z,
            radius: r,
        },
        Op::Line { x, y, z } => Command::DrawLine {
            start: DVec3::new(x - 150.0, y, z),
            end: DVec3::new(x + 150.0, y, z),
            surface_normal: Some(DVec3::Z),
        },
        Op::Rect { x, y, z, w } => Command::DrawRectAsShape {
            center: DVec3::new(x, y, z),
            normal: DVec3::Z,
            up: DVec3::X,
            width: w,
            height: w * 0.75,
        },
    };
    let _: CommandResult = s.execute(cmd);
}

/// Run a sequence and return the first invariant violation, if any.
fn run(ops: &[Op]) -> Option<(usize, String)> {
    let mut s = prod();
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        let inv = s.mesh.verify_face_invariants();
        if !inv.is_valid() {
            return Some((i, format!("{:?}", inv.violations.first())));
        }
    }
    None
}

/// The session as the fuzz generated it, transcribed from its log.
fn session_6() -> Vec<Op> {
    vec![
        Op::Ellipse { x: -150.0, y: 200.0, z: 100.0, rx: 30.0, ry: 18.0 },
        Op::Circle { x: -100.0, y: -50.0, z: 100.0, r: 110.0 },
        Op::CircleCurve { x: -150.0, y: -50.0, z: 200.0, r: 110.0 },
        Op::Line { x: 150.0, y: -150.0, z: 100.0 },
        Op::CircleCurve { x: 150.0, y: -100.0, z: 100.0, r: 90.0 },
        Op::CircleCurve { x: 200.0, y: -100.0, z: 200.0, r: 110.0 },
        Op::Ellipse { x: -150.0, y: 150.0, z: 0.0, rx: 30.0, ry: 18.0 },
        Op::Line { x: 150.0, y: 100.0, z: 0.0 },
        Op::Circle { x: 50.0, y: -150.0, z: 100.0, r: 50.0 },
        Op::Line { x: 100.0, y: 0.0, z: 0.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 0.0, w: 220.0 },
        Op::CircleCurve { x: 200.0, y: -200.0, z: 100.0, r: 110.0 },
        Op::Circle { x: 50.0, y: -50.0, z: 200.0, r: 110.0 },
        Op::CircleCurve { x: -150.0, y: -200.0, z: 200.0, r: 90.0 },
        Op::Rect { x: 50.0, y: -200.0, z: 100.0, w: 60.0 },
        Op::CircleCurve { x: 50.0, y: 100.0, z: 0.0, r: 30.0 },
    ]
}

/// The transcribed session is sound.
///
/// This used to be the reducer. It ran while the sequence still failed, and
/// what it produced is `shrunk()` below; there is nothing left to shrink.
///
/// ⚠ It reproduces the SHAPE of the fuzz's session, not its exact operation
/// stream — the sizes come from the printed log rather than the generator.
#[test]
fn the_transcribed_session_is_sound() {
    let full = session_6();
    let got = run(&full);
    println!("
  전체 세션 {} 연산 → {got:?}", full.len());
    assert!(
        got.is_none(),
        "session 6 of the fuzz has to stay sound: {got:?}"
    );
}

/// The shrunk sequence, kept as the repro.
///
/// Ten operations, and every one of them is load-bearing — the reducer could
/// not drop any further. Five sit at z = 100 and z = 200, nowhere near the
/// z = 0 plane the break happens on, and removing any of them makes the
/// failure go away. That is worth knowing before anybody assumes the last two
/// operations are the whole story.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Ellipse { x: -150.0, y: 200.0, z: 100.0, rx: 30.0, ry: 18.0 },
        Op::Circle { x: -100.0, y: -50.0, z: 100.0, r: 110.0 },
        Op::CircleCurve { x: -150.0, y: -50.0, z: 200.0, r: 110.0 },
        Op::CircleCurve { x: 150.0, y: -100.0, z: 100.0, r: 90.0 },
        Op::CircleCurve { x: 200.0, y: -100.0, z: 200.0, r: 110.0 },
        Op::Ellipse { x: -150.0, y: 150.0, z: 0.0, rx: 30.0, ry: 18.0 },
        Op::Line { x: 150.0, y: 100.0, z: 0.0 },
        Op::Line { x: 100.0, y: 0.0, z: 0.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 0.0, w: 220.0 },
        // The last one lands INSIDE the rect on the same plane, so it is a
        // containment split (ADR-283) — the rect becomes a ring with a hole.
        Op::CircleCurve { x: 50.0, y: 100.0, z: 0.0, r: 30.0 },
    ]
}

/// The ten operations that used to leave a face naming a gone half-edge, and
/// then a ring that never closed.
///
/// Mutation-checked twice: widening `edges_to_remove` back to "remove every
/// edge on the plane" brings back the missing half-edge, and dropping
/// `split_edge`'s fix-up pass brings back the endless ring.
#[test]
fn the_shrunk_sequence_is_sound() {
    let ops = shrunk();
    let got = run(&ops);
    println!("
  줄인 순서 {} 연산 → {got:?}", ops.len());
    assert!(got.is_none(), "the ten-operation repro has to stay sound: {got:?}");
}

#[test]
fn every_arrangement_is_sound() {
    let ops = shrunk();
    let run_with = |ai: bool, afs: bool, fr: bool, fo: bool| -> Option<(usize, String)> {
        let mut s = Scene::new();
        s.auto_intersect_on_draw = ai;
        s.auto_face_synthesis_on_draw = afs;
        s.face_rederive_on_draw = fr;
        s.freeform_overlap_on_draw = fo;
        for (i, op) in ops.iter().enumerate() {
            apply(&mut s, *op);
            let inv = s.mesh.verify_face_invariants();
            if !inv.is_valid() {
                return Some((i, format!("{:?}", inv.violations.first())));
            }
        }
        None
    };
    let cases = [
        ("all four on (production)", true, true, true, true),
        ("auto_intersect off", false, true, true, true),
        ("auto_face_synthesis off", true, false, true, true),
        ("face_rederive off", true, true, false, true),
        ("freeform_overlap off", true, true, true, false),
        ("all four off (engine default)", false, false, false, false),
    ];
    let mut bad: Vec<String> = Vec::new();
    for (name, a, b, c, d) in cases {
        let r = run_with(a, b, c, d);
        println!(
            "  {name:<32} {}",
            match &r {
                Some((i, why)) => format!("op {i} 위반 — {why}"),
                None => "sound".to_string(),
            }
        );
        if let Some((i, why)) = r {
            bad.push(format!("{name}: op {i} — {why}"));
        }
    }
    // This began as "which arrangement does the break need"; every combination
    // broke except the engine default with all four off. Now every combination
    // has to stay sound, which is the stronger statement and costs nothing.
    assert!(bad.is_empty(), "every flag combination has to stay sound: {bad:?}");
}

/// Where is the face that ends up broken?
///
/// ⚠ Written to test a guess — that a rebuild "scoped" to one plane was
/// reaching past it and removing half-edges belonging to faces on another.
/// The measurement says no: the broken face sits at z = 0, the very plane the
/// last shape is drawn on. So the fault is INSIDE
/// `rebuild_coplanar_faces_analytic_scoped`, not in what it is scoped to, and
/// the five operations at z = 100 and z = 200 matter for some other reason.
/// Kept as the measurement rather than deleted, because "not the scope" is
/// half of knowing where it is.
#[test]
fn the_broken_face_is_on_the_plane_that_was_rebuilt() {
    let ops = shrunk();
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    // Before the last operation: where does each face sit?
    let z_of = |s: &Scene, f: axia_geo::FaceId| -> Option<f64> {
        s.mesh.faces.get(f).and_then(|face| {
            s.mesh
                .collect_loop_verts(face.outer().start)
                .ok()
                .and_then(|vs| {
                    let zs: Vec<f64> = vs
                        .iter()
                        .filter_map(|v| s.mesh.vertex_pos(*v).ok())
                        .map(|p| p.z)
                        .collect();
                    if zs.is_empty() { None } else { Some(zs.iter().sum::<f64>() / zs.len() as f64) }
                })
        })
    };
    let broken = axia_geo::FaceId::new(11);
    let before_z = z_of(&s, broken);
    println!("BEFORE last op: FaceId(11) sits at z = {before_z:?}");
    // What kind of face is it? A self-loop with a freeform curve is preserved
    // by the rebuild (A1); a polygon is rebuilt. Which one it is decides where
    // the fault can be.
    if let Some(face) = s.mesh.faces.get(broken) {
        let start = face.outer().start;
        let n = s.mesh.collect_loop_verts(start).map(|v| v.len()).unwrap_or(0);
        let curve = s
            .mesh
            .hes
            .get(start)
            .map(|he| he.edge())
            .and_then(|e| s.mesh.edges.get(e))
            .map(|e| format!("{:?}", e.curve().map(std::mem::discriminant)));
        println!("  FaceId(11): {n} boundary verts, first edge curve = {curve:?}");
        // Load-bearing, not decoration: the rebuild preserves a single-vertex
        // self-loop carrying a freeform curve (A1) and rebuilds everything
        // else. This face is an ordinary polygon, so it is on the rebuild's
        // side of that line — which is why "the rebuild preserved it wrongly"
        // is not an available explanation for the break.
        assert!(
            n >= 3,
            "FaceId(11) is supposed to be an ordinary polygon — if it has become          a self-loop the repro has moved and this file explains the wrong thing"
        );
    }
    assert!(
        before_z.is_some(),
        "FaceId(11) must exist before the last operation, or the repro has moved"
    );

    apply(&mut s, ops[ops.len() - 1]);
    let inv = s.mesh.verify_face_invariants();
    println!("AFTER: valid = {}", inv.is_valid());

    let z = before_z.unwrap();
    println!("the last op draws on z = 0; the face that breaks sat at z = {z}");
    assert!(
        z.abs() < 1.0,
        "the broken face was measured on the rebuilt plane (z = 0). If it has          moved off it, the scope question is open again — got z = {z}"
    );
    assert!(
        inv.is_valid(),
        "the last operation has to leave it sound — this is where the break          used to land, and the file would be measuring nothing if the face          moved off this plane"
    );
}

/// What the narrowing in `face_rederive` fixed, stated as its own measurement.
///
/// The rebuild removed every edge on the plane it was rebuilding, including
/// edges that a face it had chosen to PRESERVE was still using — the preserved
/// face then named a half-edge that no longer existed:
///
///     face FaceId(11): cannot collect outer loop: HalfEdge HeId(56) not found
///
/// `edges_to_remove` now keeps any edge that a face outside the removal list is
/// still on. This asks the thing directly rather than reading the error text:
/// after the repro sequence, walk every live face's boundary and check that
/// every edge it names is still there.
///
/// Two failure classes are counted separately, because they were fixed by two
/// different changes: a *gone* edge by this narrowing, and a loop that never
/// closes by `split_edge`'s fix-up pass. Both must now be zero, and keeping
/// them apart is what makes a future failure say which change regressed.
///
/// Mutation-checked: widening the retain back to "remove every edge on the
/// plane" puts the gone-edge count back above zero and fails this test.
#[test]
fn no_surviving_face_names_an_edge_that_was_removed() {
    let mut s = prod();
    for op in shrunk() {
        apply(&mut s, op);
    }

    let live: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();

    let mut gone: Vec<String> = Vec::new();
    let mut never_closes: Vec<String> = Vec::new();

    for fid in live {
        match s.mesh.face_outer_edges(fid) {
            Ok(edges) => {
                for eid in edges {
                    let there = s.mesh.edges.get(eid).map_or(false, |e| e.is_active());
                    if !there {
                        gone.push(format!("{fid:?} still names {eid:?}, which is gone"));
                    }
                }
            }
            Err(e) => {
                let why = format!("{e}");
                if why.contains("not found") {
                    gone.push(format!("{fid:?}: {why}"));
                } else {
                    never_closes.push(format!("{fid:?}: {why}"));
                }
            }
        }
    }

    println!("\n  사라진 엣지를 부르는 면 {}건", gone.len());
    for g in &gone {
        println!("    ✗ {g}");
    }
    println!("  루프가 닫히지 않는 면 {}건 (별개 결함, 위에서 고정)", never_closes.len());
    for n in &never_closes {
        println!("    · {n}");
    }

    assert!(
        gone.is_empty(),
        "a face the rebuild chose to keep must still have every edge it names: {gone:?}"
    );
    assert!(
        never_closes.is_empty(),
        "and its ring must close: {never_closes:?}"
    );
}

/// Walk the half-edge chain by hand and say where it stops being a loop.
///
/// `collect_loop_verts` reports "Loop traversal exceeded 10000" and nothing
/// else, which names the symptom and not the place. This steps `next` from the
/// face's start, before the last operation and after it, and prints both
/// chains: which half-edges they visit, which face each one claims, and where
/// the second chain leaves the first.
///
/// Reading it: a chain that returns to its start is a loop. A chain that
/// revisits some OTHER half-edge has been spliced into a second loop, and the
/// half-edge just before the revisit is where the splice happened.
#[test]
fn where_the_loop_stops_being_a_loop() {
    let ops = shrunk();
    let broken = axia_geo::FaceId::new(11);

    let walk = |s: &axia_core::scene::Scene| -> (Vec<String>, Option<usize>) {
        let Some(face) = s.mesh.faces.get(broken) else {
            return (vec!["(면 없음)".into()], None);
        };
        let start = face.outer().start;
        let mut out: Vec<String> = Vec::new();
        let mut seen: Vec<axia_geo::HeId> = Vec::new();
        let mut he = start;
        let mut revisit_at = None;
        for step in 0..2000 {
            if let Some(pos) = seen.iter().position(|x| *x == he) {
                revisit_at = Some(pos);
                out.push(format!("  {step}: {he:?} ← 이미 {pos} 번째에서 지남"));
                break;
            }
            seen.push(he);
            let Some(h) = s.mesh.hes.get(he) else {
                out.push(format!("  {step}: {he:?} 없음"));
                break;
            };
            out.push(format!(
                "  {step}: {he:?} face={:?} edge={:?} dst={:?} active={}",
                h.face(),
                h.edge(),
                h.dst(),
                h.is_active()
            ));
            he = h.next();
        }
        (out, revisit_at)
    };

    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    let (before, before_revisit) = walk(&s);
    println!("\n마지막 연산 전 — FaceId(11) 사슬");
    for l in &before {
        println!("{l}");
    }
    println!("  (되돌아온 지점: {before_revisit:?})");

    apply(&mut s, *ops.last().unwrap());
    let (after, after_revisit) = walk(&s);
    println!("\n마지막 연산 후 — FaceId(11) 사슬");
    for l in &after {
        println!("{l}");
    }
    println!("  (되돌아온 지점: {after_revisit:?})");

    assert_eq!(
        before_revisit,
        Some(0),
        "before the last operation the chain has to be a loop — returning to \
         its own start, not to some half-edge in the middle"
    );
    // FaceId(11) is GONE after the fix, and that is the point: the rebuild now
    // removes and re-derives it instead of preserving a face whose edges it was
    // taking away. So the guard cannot be about that id — it is below.
    println!("  (수정 후 FaceId(11) 은 재유도되어 사라짐 — 아래 가드가 본체)");
}

/// Every live face's ring comes home.
///
/// Walked by hand, stepping `next`, rather than through `collect_loop_verts` —
/// the engine's own walker is what reported the break, and a guard that shares
/// its code path cannot tell "the ring is fine" from "the walker agrees with
/// itself". A ring that revisits some half-edge OTHER than its start has been
/// spliced into a second loop.
///
/// Mutation-checked: removing `split_edge`'s fix-up pass fails this.
#[test]
fn every_live_face_ring_comes_home() {
    let mut s = prod();
    for op in shrunk() {
        apply(&mut s, op);
    }

    let live: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();

    let mut bad: Vec<String> = Vec::new();
    for fid in &live {
        let Some(face) = s.mesh.faces.get(*fid) else { continue };
        let start = face.outer().start;
        let mut seen: Vec<axia_geo::HeId> = Vec::new();
        let mut he = start;
        let mut verdict = String::from("2000 걸음 안에 안 돌아옴");
        for _ in 0..2000 {
            if let Some(pos) = seen.iter().position(|x| *x == he) {
                verdict = if pos == 0 {
                    String::new()
                } else {
                    format!("{he:?} 로 되돌아감 — 고리 중간 {pos} 번째")
                };
                break;
            }
            seen.push(he);
            let Some(h) = s.mesh.hes.get(he) else {
                verdict = format!("{he:?} 없음");
                break;
            };
            he = h.next();
        }
        if !verdict.is_empty() {
            bad.push(format!("{fid:?}: {verdict}"));
        }
    }

    println!("
  살아 있는 면 {}개, 고리가 안 닫히는 면 {}개", live.len(), bad.len());
    for b in &bad {
        println!("    ✗ {b}");
    }
    assert!(bad.is_empty(), "every live face's ring has to close: {bad:?}");
}
