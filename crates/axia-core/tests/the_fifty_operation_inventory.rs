//! The 50-operation inventory, sorted into families.
//!
//! With the wedge fixed, the wide run completes all thirty sessions and reports
//! sixteen violations, every one of them past operation 24 — far beyond the
//! twelve-by-twenty gate, which stays green. They are not sixteen defects:
//!
//! ```text
//!   12   two faces cover the same ground (stacked, non-manifold)
//!    2   NaN normal
//!    1   inner[0] cannot collect — a hole's half-edge chain is broken
//!    1   cached normal opposite to the computed one
//! ```
//!
//! Three quarters of the inventory is ONE family, and it is the family this
//! whole area keeps coming back to: **coincident is not crossing**
//! (`detect_self_intersections` cannot see two faces lying on each other), so
//! whatever makes a duplicate face is invisible to the scan that is supposed to
//! catch it.
//!
//! This file works that family. Session 18 breaks earliest — operation 24 — and
//! greedy shrinking takes its twenty-three operations down to four:
//!
//! ```text
//!    2  circleCurve(-150, 100, z=100, r=110)   a Path B circle
//!    4  raise it by 200                        its wall now spans z=100..300
//!   18  polygon(-200, 150, z=100, r=110, n=5)  on the same plane
//!   22  rect(-150, 50, z=100, w=180)           on the same plane
//!   ->  push in a face at z≈200 by 100         two faces on the same ground
//! ```
//!
//! ⚠ Necessary is not sufficient. The drop-one sweep says operations 2 and 4 are
//! each needed — remove either and the stacking goes — but keeping ONLY those
//! two gives zero stacked pairs. A set where every member matters is not a set
//! that reproduces. The greedy shrink accumulates its removals and finds the
//! four above; the one-at-a-time sweep is kept beside it because the difference
//! between the two answers is the point.
//!
//! The fix is not here yet. What is here is a five-operation scene that holds
//! the defect, where before it took fifty.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
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

/// The fuzz harness generator, verbatim.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n.max(1)
    }
    fn coord(&mut self) -> f64 {
        (self.below(9) as f64 - 4.0) * 50.0
    }
    fn size(&mut self) -> f64 {
        60.0 + self.below(5) as f64 * 40.0
    }
}

/// The average of a face's boundary vertices — a handle that survives dropping
/// an operation, which a `FaceId` does not.
fn face_centre(s: &Scene, f: FaceId) -> DVec3 {
    let Some(face) = s.mesh.faces.get(f) else { return DVec3::ZERO };
    let Ok(vs) = s.mesh.collect_loop_verts(face.outer().start) else { return DVec3::ZERO };
    if vs.is_empty() {
        return DVec3::ZERO;
    }
    let n = vs.len() as f64;
    vs.iter().filter_map(|v| s.mesh.verts.get(*v).map(|q| q.pos())).fold(DVec3::ZERO, |a, b| a + b) / n
}

/// One operation, named before it runs.
fn step(s: &mut Scene, r: &mut Lcg) -> String {
    let z = [0.0, 100.0, 200.0][r.below(3)];
    let (x, y) = (r.coord(), r.coord());
    let c = DVec3::new(x, y, z);
    let w = r.size();
    let live: Vec<FaceId> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    match r.below(10) {
        0 => {
            s.execute(Command::DrawRectAsShape {
                center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75,
            });
            format!("rect({x},{y},{z},{w})")
        }
        1 => {
            s.execute(Command::DrawCircleAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24,
            });
            format!("circle({x},{y},{z},r={})", w * 0.5)
        }
        2 => {
            s.execute(Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 });
            format!("circleCurve({x},{y},{z},r={})", w * 0.5)
        }
        3 => {
            s.execute(Command::DrawEllipseAsCurve {
                center: c, ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: w * 0.5, radius_y: w * 0.3,
            });
            format!("ellipse({x},{y},{z},{},{})", w * 0.5, w * 0.3)
        }
        4 => {
            let n = 3 + r.below(5) as u32;
            s.execute(Command::DrawPolygonAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, sides: n,
            });
            format!("polygon({x},{y},{z},r={},n={n})", w * 0.5)
        }
        5 => {
            s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            });
            format!("line({x},{y},{z})")
        }
        // ⚠ Two arms, not one. The harness draws `below(4)` for an extrude and
        // `below(3)` for a push-in, so collapsing them takes a different number
        // out of the generator and every operation after diverges. A first
        // version did exactly that and reported no stacked pair in fifty
        // operations — a replay of a session that does not exist.
        6 => {
            if live.is_empty() {
                return "extrude(skip)".into();
            }
            let f = live[r.below(live.len())];
            let d = 50.0 + r.below(4) as f64 * 50.0;
            let c = face_centre(s, f);
            s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
            format!("extrude@({:.0},{:.0},{:.0}),{d}", c.x, c.y, c.z)
        }
        7 => {
            if live.is_empty() {
                return "pushIn(skip)".into();
            }
            let f = live[r.below(live.len())];
            let d = -(50.0 + r.below(3) as f64 * 50.0);
            let c = face_centre(s, f);
            s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
            format!("pushIn@({:.0},{:.0},{:.0}),{d}", c.x, c.y, c.z)
        }
        8 => {
            let _ = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL);
            format!("box({x},{y},{z},{w})")
        }
        _ => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z,
            );
            format!("punch({x},{y},{z})")
        }
    }
}

/// Every pair of live faces that cover the same ground, as the verifier says it.
fn stacked_pairs(s: &Scene) -> usize {
    s.mesh
        .verify_face_invariants()
        .violations
        .iter()
        .filter(|v| format!("{v:?}").contains("cover the same ground"))
        .count()
}

/// Which operation of session 18 first leaves two faces on the same ground.
///
/// The generator is the harness's, arm for arm — including `extrude` drawing
/// `below(4)` and `pushIn` drawing `below(3)`, which is what a first version got
/// wrong. Collapsing them into one arm took a different number out of the
/// stream, every operation after diverged, and the run reported no stacked pair
/// in fifty operations: a replay of a session that does not exist.
#[test]
fn when_a_stacked_pair_first_appears() {
    let mut r = Lcg(0x5EED_0000 + 18);
    let mut s = prod();
    println!("\n  세션 18 계열 — 겹친 면이 처음 나오는 곳\n");
    let mut first: Option<usize> = None;
    for op in 0..50 {
        let desc = step(&mut s, &mut r);
        let stacked = stacked_pairs(&s);
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        if stacked > 0 && first.is_none() {
            first = Some(op);
            println!("    op {op:>2}  {desc:<34}  면 {live:>3}  겹침 {stacked}   <- 여기");
            break;
        }
        println!("    op {op:>2}  {desc:<34}  면 {live:>3}  겹침 {stacked}");
    }
    match first {
        Some(op) => println!("\n  처음 겹친 곳: op {op}\n"),
        None => println!("\n  50 연산 동안 겹침 없음\n"),
    }
}

// ── Session 18 as data ───────────────────────────────────────────────────────
//
// Transcribed from the replay above, which prints each operation's parameters
// and — for the two solid operations — the CENTRE of the face it chose. A face
// id moves the moment an operation is dropped; a centre does not.

#[derive(Clone, Copy, Debug)]
enum S18 {
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Line { x: f64, y: f64, z: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Punch { x: f64, y: f64, z: f64 },
    SolidNear { x: f64, y: f64, z: f64, d: f64 },
}

const S18_OPS: &[S18] = &[
    S18::Polygon { x: -150.0, y: -200.0, z: 0.0, r: 30.0, n: 5 },
    S18::Polygon { x: -50.0, y: -200.0, z: 0.0, r: 70.0, n: 7 },
    S18::CircleCurve { x: -150.0, y: 100.0, z: 100.0, r: 110.0 },
    S18::CircleCurve { x: 0.0, y: -100.0, z: 200.0, r: 70.0 },
    S18::SolidNear { x: -40.0, y: 100.0, z: 100.0, d: 200.0 },
    S18::Circle { x: -200.0, y: 0.0, z: 200.0, r: 110.0 },
    S18::Rect { x: 50.0, y: -150.0, z: 0.0, w: 60.0 },
    S18::Line { x: 100.0, y: -50.0, z: 100.0 },
    S18::Ellipse { x: 100.0, y: 50.0, z: 200.0, rx: 110.0, ry: 66.0 },
    S18::Ellipse { x: -50.0, y: 50.0, z: 100.0, rx: 30.0, ry: 18.0 },
    S18::Ellipse { x: 150.0, y: -200.0, z: 100.0, rx: 110.0, ry: 66.0 },
    S18::Punch { x: -100.0, y: -50.0, z: 0.0 },
    S18::CircleCurve { x: 0.0, y: -200.0, z: 0.0, r: 70.0 },
    S18::Circle { x: 50.0, y: -150.0, z: 0.0, r: 70.0 },
    S18::CircleCurve { x: 0.0, y: 100.0, z: 200.0, r: 90.0 },
    S18::Rect { x: -200.0, y: -100.0, z: 0.0, w: 100.0 },
    S18::Box { x: -200.0, y: -200.0, z: 200.0, w: 60.0 },
    S18::SolidNear { x: -150.0, y: -200.0, z: 0.0, d: -150.0 },
    S18::Polygon { x: -200.0, y: 150.0, z: 100.0, r: 110.0, n: 5 },
    S18::Box { x: -50.0, y: -100.0, z: 0.0, w: 180.0 },
    S18::Circle { x: -100.0, y: -50.0, z: 100.0, r: 110.0 },
    S18::Rect { x: -100.0, y: 50.0, z: 200.0, w: 220.0 },
    S18::Rect { x: -150.0, y: 50.0, z: 100.0, w: 180.0 },
];

/// The push-in that leaves two faces on the same ground.
const S18_BREAK: S18 = S18::SolidNear { x: -52.0, y: 70.0, z: 200.0, d: -100.0 };

fn play18(s: &mut Scene, op: S18) {
    match op {
        S18::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, sides: n,
            });
        }
        S18::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, segments: 24,
            });
        }
        S18::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r,
            });
        }
        S18::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z), ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: rx, radius_y: ry,
            });
        }
        S18::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, up: DVec3::X,
                width: w, height: w * 0.75,
            });
        }
        S18::Line { x, y, z } => {
            s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            });
        }
        S18::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(DVec3::new(x, y, z + 60.0), w, 120.0, w, FORM_MATERIAL);
        }
        S18::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z,
            );
        }
        S18::SolidNear { x, y, z, d } => {
            let target = DVec3::new(x, y, z);
            let mut best: Option<(f64, FaceId)> = None;
            for (fid, f) in s.mesh.faces.iter() {
                if !f.is_active() {
                    continue;
                }
                let c = face_centre(s, fid);
                let dist = (c - target).length();
                if best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                    best = Some((dist, fid));
                }
            }
            if let Some((_, f)) = best {
                s.execute(Command::CreateSolid {
                    face_id: f, mode: CreateSolidMode::Extrude { distance: d },
                });
            }
        }
    }
}

/// Replay with some operations dropped; how many stacked pairs the break leaves.
fn s18_stacked(dropped: &[usize]) -> usize {
    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if dropped.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }
    play18(&mut s, S18_BREAK);
    stacked_pairs(&s)
}

/// Does session 18 replayed as data still stack faces?
///
/// It has to, or the transcription is wrong and the reduction below is
/// measuring a different scene.
#[test]
fn session_18_as_data_still_stacks() {
    let n = s18_stacked(&[]);
    println!("\n  세션 18, 데이터로 재생 — 겹친 면 {n}\n");
    assert!(n > 0, "the transcription has to reach the same defect");
}

/// Drop one operation at a time; which removals make the stacking go away.
#[test]
#[ignore = "a reduction sweep — run by hand"]
fn which_operations_the_stacking_needs() {
    println!("\n  하나씩 빼기 — 겹침이 사라지면 그 연산이 필요하다\n");
    let mut needed: Vec<usize> = Vec::new();
    for i in 0..S18_OPS.len() {
        let n = s18_stacked(&[i]);
        if n == 0 {
            needed.push(i);
        }
        println!(
            "    {i:>2} 빼기  {:<44} 겹침 {n}{}",
            format!("{:?}", S18_OPS[i]),
            if n == 0 { "   <- 필요" } else { "" }
        );
    }
    println!("\n  필요한 연산: {needed:?}\n");
}

/// Greedy shrink: keep dropping while the stacking survives.
///
/// ⚠ Necessary is not sufficient, and the drop-one sweep only finds the first.
/// It says operations 2 and 4 are each needed — remove either and the stacking
/// goes — but keeping ONLY those two gives zero stacked pairs. A set where every
/// member matters is not a set that reproduces on its own.
///
/// So this accumulates: try removing each operation IN ADDITION to what has
/// already been removed, and keep the removal whenever the defect survives.
/// What comes out is a set that both reproduces and cannot be cut further one
/// operation at a time.
#[test]
fn the_smallest_set_that_still_stacks() {
    let mut dropped: Vec<usize> = Vec::new();
    let mut changed = true;
    while changed {
        changed = false;
        for i in 0..S18_OPS.len() {
            if dropped.contains(&i) {
                continue;
            }
            let mut trial = dropped.clone();
            trial.push(i);
            if s18_stacked(&trial) > 0 {
                dropped = trial;
                changed = true;
            }
        }
    }
    let kept: Vec<usize> = (0..S18_OPS.len()).filter(|i| !dropped.contains(i)).collect();
    println!("
  탐욕 축소 — 남은 연산 {} 개 (원래 {})
", kept.len(), S18_OPS.len());
    for &i in &kept {
        println!("    {i:>2}  {:?}", S18_OPS[i]);
    }
    println!("    ->  {:?}", S18_BREAK);
    let n = s18_stacked(&dropped);
    println!("
    겹친 면 {n}
");
    assert!(n > 0, "the shrink has to keep reproducing");
}
