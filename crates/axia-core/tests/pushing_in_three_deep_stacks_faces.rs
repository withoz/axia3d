//! What session 10 stops at now, minimised.
//!
//! Its old break — a drawn circle lying whole on a ring-topped solid — went
//! when the coplanar re-tile started counting owners rather than components.
//! The session then gets four operations further and stops at a different one,
//! three push-ins deep on a scene holding four solids and two circles:
//!
//! ```text
//!   op 11: edge EdgeId(69): shared by 4 active faces (non-manifold) —
//!          FaceId(26) / FaceId(49) cover the same ground (stacked)
//! ```
//!
//! Same method as the four before it: transcribe the log, then greedily drop
//! operations for as long as the failure survives, targeting the stacked-face
//! text so the reducer cannot walk off to a different violation.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const STACKED: &str = "cover the same ground";

#[derive(Clone, Copy, Debug)]
enum Op {
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
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
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
            });
        }
        Op::PushIn { face, d } => {
            s.execute(Command::CreateSolid {
                face_id: FaceId::new(face),
                mode: CreateSolidMode::Extrude { distance: d },
            });
        }
    }
}

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

/// Session 10 as the fuzz generates it. Its op 0 is `pushIn(skip: nothing there)`.
fn session_10() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::PushIn { face: 11, d: -150.0 },
        Op::Rect { x: 150.0, y: -200.0, z: 0.0, w: 180.0 },
        Op::PushIn { face: 4, d: -100.0 },
        Op::CircleCurve { x: 0.0, y: 100.0, z: 100.0, r: 110.0 },
        Op::CircleCurve { x: -50.0, y: -100.0, z: 0.0, r: 30.0 },
        Op::Box { x: -100.0, y: -150.0, z: 0.0, w: 100.0 },
        Op::PushIn { face: 27, d: -50.0 },
        Op::PushIn { face: 16, d: -50.0 },
    ]
}

#[test]
fn the_sequence_shrinks_to_something_readable() {
    let full = session_10();
    assert!(
        run(&full).is_some(),
        "the transcription does not reproduce the stacked faces — check it \
         against the fuzz log before shrinking anything"
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

    println!("\n  {n0} 연산 → {} 연산\n", ops.len());
    for (i, op) in ops.iter().enumerate() {
        println!("    {i}: {op:?}");
    }
    let (at, why) = run(&ops).expect("still fails");
    println!("\n  op {at} 에서 {why}\n");
    assert!(ops.len() <= n0, "the reducer must not make it longer");
}

/// The shrunk sequence, kept as the repro.
///
/// Four operations, and **not one push-in among them** — the reducer dropped all
/// three. What is left is a ring and its inner face on z=100, a box whose BOTTOM
/// is on that plane, and a circle over the lot.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::CircleCurve { x: 0.0, y: 100.0, z: 100.0, r: 110.0 },
    ]
}

/// ⚠ PINNED AS MEASURED — an OPEN defect, inventoried in the fuzz's
/// `KNOWN_BREAKS`.
#[test]
fn the_shrunk_sequence_still_stacks() {
    let got = run(&shrunk());
    println!("\n  줄인 순서 4 연산 → {got:?}");
    assert!(
        got.is_some(),
        "the four-operation repro no longer stacks — if somebody fixed it, say \
         what, strike it from KNOWN_BREAKS, and turn this into the test that \
         proves it"
    );
}

/// What sits on the drawing plane before the circle, reported honestly.
///
/// ⚠ The reading below is partly an instrument problem, and saying so is the
/// point of keeping it. `is_face_in_volume` reports TWO faces on z=100 as part
/// of a solid — one facing away from the draw at −Z, which is the box's bottom
/// and real, and one facing +Z, which is the inner rectangle: a coplanar SHEET
/// lying under the box's bottom. A coplanar sheet over a solid's face confusing
/// that function is written down already (`what_divides_on_a_solid_top.rs`
/// names it as the fourth instrument in this area to do it), and it is why an
/// assertion of "one solid face, facing away" fails here while being true of
/// the geometry.
///
/// So this asserts only what the geometry says — the box's bottom is on the
/// plane and faces away from the draw — and prints the rest.
#[test]
fn what_sits_on_the_drawing_plane() {
    let ops = shrunk();
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    let mut facing_away = 0;
    println!("
  마지막 연산 전, z=100 평면 위의 면
");
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let Ok(vv) = s.mesh.collect_loop_verts(f.outer().start) else { continue };
        let on_plane = !vv.is_empty()
            && vv.iter().all(|v| {
                s.mesh.verts.get(*v).map_or(false, |p| (p.pos().z - 100.0).abs() < 1e-6)
            });
        if !on_plane {
            continue;
        }
        let d = f.normal().normalize_or_zero().dot(DVec3::Z);
        println!(
            "    {fid:?}  verts={}  n·draw={d:+.3}  instrument says {}",
            vv.len(),
            if s.mesh.is_face_in_volume(fid) { "solid" } else { "sheet" }
        );
        if d < 0.0 {
            facing_away += 1;
        }
    }
    println!("
  그리기 반대쪽을 보는 면 {facing_away}개");
    assert_eq!(
        facing_away, 1,
        "the box's bottom is on this plane and faces away from the draw — that          is the geometry the re-tile's side rule reads"
    );
}
