//! The fuzz's second find, minimised.
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
//! Eighteen operations is not a bug report, so this file shrinks it. The
//! reducer drops one operation at a time and keeps the drop whenever the
//! failure survives — plain delta debugging, deterministic, and the shrunk
//! sequence is printed so the next person starts from it rather than from the
//! session log.

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

/// Drop what can be dropped, and print what is left.
///
/// ⚠ This reproduces the SHAPE of the fuzz's session, not its exact operation
/// stream — the fuzz's sizes come from its generator and are re-derived here
/// from the printed log. If the sequence below stops failing, that is worth
/// knowing on its own and the test says so rather than passing quietly.
#[test]
fn the_sequence_shrinks_to_something_readable() {
    let full = session_6();
    let Some((at, why)) = run(&full) else {
        panic!(
            "the transcribed session no longer fails — the operation stream was \
             reconstructed from the fuzz's printed log, so a size or a coordinate \
             is off, or the defect moved. Re-derive it from \
             `a_fuzz_session_leaves_the_mesh_sound.rs` seed 0x5EED0006 before \
             concluding anything from this file."
        );
    };
    println!("FULL: {} ops, breaks at {at} — {why}", full.len());

    // Greedy delta debugging: try without each op, keep the drop if it still
    // breaks. One pass is enough to get from eighteen to a handful; repeat
    // until nothing more can go.
    let mut ops = full.clone();
    loop {
        let mut dropped_any = false;
        let mut i = 0;
        while i < ops.len() {
            let mut candidate = ops.clone();
            candidate.remove(i);
            if run(&candidate).is_some() {
                ops = candidate;
                dropped_any = true;
            } else {
                i += 1;
            }
        }
        if !dropped_any {
            break;
        }
    }

    let (at, why) = run(&ops).expect("the shrunk sequence still fails");
    println!("\nSHRUNK to {} ops, breaks at {at}:", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("  {i}: {op:?}");
    }
    println!("  → {why}");

    assert!(
        ops.len() < full.len(),
        "the reducer must actually reduce something"
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

/// ⚠ PINNED AS MEASURED — an OPEN defect.
///
/// The last operation leaves a face naming a half-edge that no longer exists.
/// Pinned so the day it stops happening, this says so.
#[test]
fn the_shrunk_sequence_still_breaks() {
    let ops = shrunk();
    let broke = run(&ops);
    println!("SHRUNK REPRO: {:?}", broke);
    let (at, why) = broke.expect(
        "the shrunk sequence no longer breaks — if that is because somebody          fixed it, say what, and turn this into the test that proves it",
    );
    assert_eq!(at, ops.len() - 1, "it is the last operation that breaks it");
    assert!(
        why.contains("cannot collect outer loop"),
        "the signature is a face naming a half-edge that is gone, got: {why}"
    );
}

/// Which path does the last operation take?
///
/// Narrowing before touching any code: the flags the engine runs with are the
/// four the app sets, and turning each one off in turn says which of them the
/// break needs. A defect that survives all four off is in the plain draw; one
/// that needs a particular flag is in that flag's arrangement.
#[test]
fn which_arrangement_the_break_needs() {
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
    for (name, a, b, c, d) in cases {
        let r = run_with(a, b, c, d);
        println!(
            "  {name:<32} {}",
            match &r {
                Some((i, why)) => format!("op {i} 위반 — {why}"),
                None => "sound".to_string(),
            }
        );
    }
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
        !inv.is_valid(),
        "the last operation still has to break it, or this file is measuring          a defect that is gone"
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
/// Two failure classes are counted separately on purpose. A *gone* edge is the
/// one this narrowing is responsible for and must be zero. A loop that never
/// closes is a different break — the same face, still open, pinned by
/// `the_shrunk_sequence_still_breaks` above — and this test deliberately does
/// not assert it away, because nothing here fixed it.
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
}
