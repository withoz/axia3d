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

/// Re-shrunk after the re-derive learned to run on a solid's plane.
///
/// The first reduction of this session — four operations, no push-in among them
/// — is SOUND now and kept below as a guard. The full eleven still stop at the
/// same place, so the reducer runs again from the whole session to find what is
/// left.
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

/// The four operations that used to leave the circle lying on the rectangles.
///
/// Mutation-checked: putting the re-derive's early return back stacks them
/// again, and so does dropping the repair's violation check.
#[test]
fn the_shrunk_sequence_is_sound() {
    let got = run(&shrunk());
    println!("
  줄인 순서 4 연산 → {got:?}");
    assert!(got.is_none(), "the four-operation repro has to stay sound: {got:?}");
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

/// Three formulations of "carry it when there is only one body", all refused.
///
/// The obvious fix is to let the side rule stand down when there is nothing to
/// disambiguate against. Three ways of saying that were written and measured,
/// and each one put `a_face_whose_normal_faces_the_other_way.rs` — defect 3, two
/// solids on one plane — back:
///
/// 1. **One shell.** Bodies joined through shared edges are one body. Two solids
///    that MEET on a plane share the edges they meet on, so this calls them one
///    and stands the rule down exactly where it is needed.
/// 2. **Walls reaching both ways.** Never fires: in defect 3's scene no on-plane
///    volume edge has a wall reaching the far side at all.
/// 3. **Counting near-side bodies by shared endpoints.** Reads 1 there too — the
///    two solids' perimeters are welded at the plane into one component.
///
/// By every structural measure tried, defect 3's scene and this one look the
/// same on the near side: one body above the plane. So the difference that makes
/// feeding right in one and wrong in the other is NOT the number of bodies, and
/// the next attempt should start by finding what it is rather than by trying a
/// fourth way to count them.
///
/// This test holds the finding rather than a behaviour: it asserts the two
/// scenes really are alike in the way the measurements said, so that if one of
/// them changes shape the note above stops being true out loud.
#[test]
fn the_two_scenes_look_alike_on_the_near_side() {
    let mut a = prod();
    for op in &shrunk()[..shrunk().len() - 1] {
        apply(&mut a, *op);
    }

    // Defect 3's scene, up to just before its circle.
    let mut b = prod();
    b.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, 100.0),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    b.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, 100.0),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });
    let _ = b.mesh.create_box(
        DVec3::new(-50.0, 50.0, 160.0), 180.0, 120.0, 180.0, FORM_MATERIAL,
    );
    b.execute(Command::CreateSolid {
        face_id: FaceId::new(10),
        mode: CreateSolidMode::Extrude { distance: 50.0 },
    });

    let mut readings: Vec<(usize, usize)> = Vec::new();
    for (name, s) in [("이 결함", &a), ("결함 3", &b)] {
        let mut above = 0;
        let mut below = 0;
        for (fid, f) in s.mesh.faces.iter() {
            if !f.is_active() {
                continue;
            }
            let Ok(vv) = s.mesh.collect_loop_verts(f.outer().start) else { continue };
            let on_plane = !vv.is_empty()
                && vv.iter().all(|v| {
                    s.mesh.verts.get(*v).map_or(false, |p| (p.pos().z - 100.0).abs() < 1e-6)
                });
            if !on_plane || !s.mesh.is_face_in_volume(fid) {
                continue;
            }
            if f.normal().normalize_or_zero().dot(DVec3::Z) < 0.0 { below += 1 } else { above += 1 }
        }
        println!("  {name}: z=100 위 솔리드 면 — 그리기 반대쪽 {below}, 같은 쪽 {above}");
        readings.push((below, above));
    }
    assert_eq!(
        readings[0], readings[1],
        "the two scenes read the same on the plane the draw lands on — which is          why counting bodies cannot tell them apart. If they stop reading the          same, the note above is stale and the fourth attempt has something new          to work with"
    );
}

/// What is left of session 10 after the re-derive learned to run.
///
/// Four operations again, but not the same four. The first reduction was all on
/// z=100; this one ends with a circle on **z=0**, a plane nothing else is on —
/// one rectangle and a box at z=100, a circle over them, and then a small circle
/// somewhere else entirely.
fn shrunk_op11() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::CircleCurve { x: 0.0, y: 100.0, z: 100.0, r: 110.0 },
        Op::CircleCurve { x: -50.0, y: -100.0, z: 0.0, r: 30.0 },
    ]
}

/// The four operations that used to leave two faces on z=100 covering each
/// other after a circle was drawn on z=0.
///
/// Mutation-checked: unscoping the containment pass — handing it every active
/// face in the scene again — puts them back.
#[test]
fn the_op11_reduction_is_sound() {
    let got = run(&shrunk_op11());
    println!("
  op11 축소 4 연산 → {got:?}");
    assert!(got.is_none(), "the four-operation reduction has to stay sound: {got:?}");
}

/// Which two faces cover each other, and where they are.
///
/// ── Probed to the step, 2026-08-17 ──────────────────────────────────────
///
/// The last draw is on z=0 and the rebuild it triggers is on z=0 too:
///
/// ```text
///   PLANE origin=(67.5, 10, 100) normal=+Z  seeds=1   → viol=0
///   PLANE origin=(110, 100, 100) normal=+Z  seeds=1   → viol=0
///   PLANE origin=(-20, -100, 0)  normal=+Z  seeds=1   → viol=2
/// ```
///
/// Inside that third rebuild, split finer:
///
/// ```text
///   after the re-derive        viol=0
///   after assign_circle_holes  viol=0
///   after assign_polygon_holes viol=2   ← here
/// ```
///
/// `assign_polygon_holes` is handed EVERY active face in the scene — the call
/// site collects `self.mesh.faces` and filters only on `is_active` — so a
/// rebuild of z=0 reparents among faces on z=100. That is how a draw on one
/// plane changes another.
///
/// Scoping that list to the plane being rebuilt is the obvious next move and is
/// NOT the whole fix, measured: the reduction below still stacks (something else
/// takes over), and the fuzz answers with session 9 breaking at op 13. So the
/// containment pass wants scoping AND whatever it was compensating for.
///
/// The last operation draws on z=0 and nothing else is there, so a stack it
/// leaves has to involve something from another plane — which is worth knowing
/// before anyone looks for the cause on z=0.
#[test]
fn what_the_last_circle_stacks_with() {
    let ops = shrunk_op11();
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }
    let before: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .collect();
    apply(&mut s, *ops.last().unwrap());

    let z_of = |s: &Scene, f: FaceId| -> Option<f64> {
        let face = s.mesh.faces.get(f)?;
        let vv = s.mesh.collect_loop_verts(face.outer().start).ok()?;
        let zs: Vec<f64> = vv
            .iter()
            .filter_map(|v| s.mesh.verts.get(*v).map(|p| p.pos().z))
            .collect();
        let first = *zs.first()?;
        zs.iter().all(|z| (z - first).abs() < 1e-6).then_some(first)
    };

    println!("\n  마지막 원은 z=0 에, 나머지는 z=100 에\n");
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
        for id in ids {
            let f = FaceId::new(id as u32);
            println!(
                "    {f:?}  z={:?}  {}  {}",
                z_of(&s, f),
                if s.mesh.is_face_in_volume(f) { "solid" } else { "sheet" },
                if before.contains(&f) { "old" } else { "new" }
            );
        }
    }
    assert!(
        s.mesh.verify_face_invariants().violations.is_empty(),
        "nothing is left stacked — the containment pass is scoped to the plane          being rebuilt, so a draw on z=0 no longer reparents among faces on z=100"
    );
}
