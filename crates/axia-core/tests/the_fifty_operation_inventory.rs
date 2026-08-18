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
//! ── What is left, sorted (2026-08-19) ───────────────────────────────────
//!
//! With the push arms reconciling (see `a_move_only_push_leaves_its_neighbours_
//! unstacked`) the wide run reports fifteen, of which eleven are stacked. Two
//! questions asked of each — what made it, and would an unscoped re-derive on
//! that plane clear it — sort them:
//!
//! ```text
//!   세션  연산                         만든 것            재유도가
//!     3   pushIn   → SolidCreated Box   밀어넣기          못 고친다
//!    12   rect                          그리기            못 고친다
//!    17   ellipse                       그리기            고친다  ←
//!    18   pushIn   → SolidCreated Sweep 밀어넣기          못 고친다
//!    19   extrude  → MoveOnly           밀어넣기          못 고친다
//!    20   rect                          그리기            못 고친다
//!    25   rect                          그리기            못 고친다
//!    26   box      (mesh.create_box)    하네스 직접 호출  고친다  ←
//!    27   rect                          그리기            못 고친다
//!    28   pushIn   → CreateFace         밀어넣기          못 고친다
//!    29   circleCurve                   그리기            고친다  ←
//! ```
//!
//! ⚠ "A DRAW never leaves it, because a draw runs the coplanar re-derive
//! afterwards" — the sentence this file opened with — is FALSE. Six of the
//! eleven are draws. The draw path does re-derive, but SCOPED to the drawn
//! faces (`seed = Some(face_ids)`), and for sessions 17 and 29 the same
//! re-derive with `seed = None` clears the plane outright. The scope is the
//! miss, not the absence.
//!
//! Session 26 is the harness reaching past `Scene` into `mesh.create_box`, so
//! nothing could have reconciled it; that one is an artifact, not a defect.
//!
//! Which leaves two piles of work, and they are different work:
//!
//!   3 sessions  the re-derive would clear it and nothing ran it   (17, 26, 29)
//!   8 sessions  the re-derive runs and changes NOTHING — face count
//!               identical before and after, so it either declined (the scoped
//!               rebuild carries its own rollback) or found nothing to do
//!
//! `whose_refusal_leaves_the_stacking` prints both columns and the face counts;
//! `which_operation_first_stacks` names the operation.
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
//! ── What the five operations do ──────────────────────────────────────────
//!
//! The pair, described:
//!
//! ```text
//!   FaceId(3)   23 corners  area 37071  normal (0,0,+1)  z = 300   the raised cap
//!   FaceId(37)   4 corners  area  2996  normal (0,0,-1)  z = 300   made by the push
//! ```
//!
//! and it is a GENUINE non-manifold, not two solids merely sitting in the same
//! place (which this repo has already decided is legal — see
//! `a_second_solid_on_the_same_plane_stays_whole.rs`). `EdgeId(45)` at z = 300 is
//! shared by THREE faces, and the cap and the new face share two vertices.
//!
//! ── Which push, and why the clamp does not cover it ──────────────────────
//!
//! The face being pushed is a vertical WALL, not a horizontal face:
//!
//! ```text
//!   FaceId(24)   centre (-50, 57, 200)   normal (0.92, -0.40, 0.00)
//!   is_move_only          false
//!   move_only_max_inward  None
//! ```
//!
//! So ADR-196's inward clamp — which stops a face short of its own solid's far
//! side — never applies: this goes down the `create_solid` path instead, and
//! that path builds a cap that lands exactly on a neighbouring cap it shares an
//! edge with. Nothing between them notices, because coincident is not crossing.
//!
//! ── The fix is not here ──────────────────────────────────────────────────
//!
//! The shape of it is: what `create_solid` builds has to be reconciled against
//! coplanar neighbours it shares edges with — merged or divided — the way the
//! coplanar re-derive already does for a draw. `create_solid` does not go
//! through that. Making it do so is a change to the push/pull path broadly and
//! wants its own measurement.
//!
//! What is here is a five-operation scene that holds the defect where it used to
//! take fifty, the producer named, and the reason the existing clamp misses it.

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
    let kind = r.below(10);
    let desc = match kind {
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
    };
    desc
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

/// Which two faces end up on the same ground, and where.
///
/// The reduction names five operations. This asks the scene what the pair
/// actually is — the two faces, their planes, their areas, and whether the push
/// made them or found them.
#[test]
#[ignore = "a description, not a gate — run by hand"]
fn what_is_stacked_on_what() {
    let keep = [2usize, 4, 18, 22];
    let drop: Vec<usize> = (0..S18_OPS.len()).filter(|i| !keep.contains(i)).collect();

    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }
    let before = stacked_pairs(&s);
    let n_before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("\n  밀어넣기 전   면 {n_before}   겹침 {before}");

    play18(&mut s, S18_BREAK);
    let n_after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("  밀어넣기 후   면 {n_after}   겹침 {}\n", stacked_pairs(&s));

    for v in s.mesh.verify_face_invariants().violations.iter() {
        let t = format!("{v:?}");
        if !t.contains("cover the same ground") {
            continue;
        }
        println!("  {t}\n");
        let ids: Vec<u32> = t
            .split("FaceId(")
            .skip(1)
            .filter_map(|q| q.split(')').next())
            .filter_map(|n| n.parse().ok())
            .collect();
        for id in ids {
            let f = FaceId::new(id);
            let Some(face) = s.mesh.faces.get(f) else { continue };
            let Ok(vs) = s.mesh.collect_loop_verts(face.outer().start) else { continue };
            let pts: Vec<DVec3> = vs.iter().filter_map(|v| s.mesh.verts.get(*v).map(|x| x.pos())).collect();
            let n = face.normal().normalize_or_zero();
            let c = face_centre(&s, f);
            let zs: Vec<f64> = pts.iter().map(|p| p.z).collect();
            println!(
                "    {f:?}  정점 {:>3}  넓이 {:>10.1}  법선 ({:.2},{:.2},{:.2})  중심 ({:.0},{:.0},{:.0})  z [{:.0}, {:.0}]",
                pts.len(),
                s.mesh.face_area(f),
                n.x, n.y, n.z,
                c.x, c.y, c.z,
                zs.iter().cloned().fold(f64::INFINITY, f64::min),
                zs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            );
        }
        println!();
    }
}

/// Is it two solids sitting in the same place, or a genuine non-manifold edge?
///
/// The repo has already decided the first is legal — "two solids may sit inside
/// each other quite legitimately" (`scene.rs`), pinned in
/// `a_second_solid_on_the_same_plane_stays_whole.rs`. The verifier reports THIS
/// as "edge shared by N active faces (non-manifold)", which is a different
/// claim, so the difference decides whether there is anything to fix.
#[test]
#[ignore = "a description, not a gate — run by hand"]
fn whether_the_pair_actually_shares_an_edge() {
    let keep = [2usize, 4, 18, 22];
    let drop: Vec<usize> = (0..S18_OPS.len()).filter(|i| !keep.contains(i)).collect();
    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }
    play18(&mut s, S18_BREAK);

    for v in s.mesh.verify_face_invariants().violations.iter() {
        let t = format!("{v:?}");
        if !t.contains("cover the same ground") {
            continue;
        }
        let eid: Option<u32> = t
            .split("EdgeId(")
            .nth(1)
            .and_then(|q| q.split(')').next())
            .and_then(|n| n.parse().ok());
        let ids: Vec<u32> = t
            .split("FaceId(")
            .skip(1)
            .filter_map(|q| q.split(')').next())
            .filter_map(|n| n.parse().ok())
            .collect();
        println!("\n  {t}\n");

        if let Some(eid) = eid {
            let e = axia_geo::EdgeId::new(eid);
            let (faces, _) = s.mesh.get_faces_sharing_edge(e);
            println!("    EdgeId({eid}) 위의 면  {faces:?}");
            if let Some(edge) = s.mesh.edges.get(e) {
                let a = s.mesh.verts.get(edge.v_small()).map(|v| v.pos());
                let b = s.mesh.verts.get(edge.v_large()).map(|v| v.pos());
                if let (Some(a), Some(b)) = (a, b) {
                    println!(
                        "    엣지 끝점  ({:.0},{:.0},{:.0}) - ({:.0},{:.0},{:.0})",
                        a.x, a.y, a.z, b.x, b.y, b.z
                    );
                }
            }
        }

        // Do the two faces share any VERTEX at all? Two solids merely sitting in
        // the same place share none.
        if ids.len() >= 2 {
            let verts = |f: u32| -> Vec<axia_geo::VertId> {
                s.mesh
                    .faces
                    .get(FaceId::new(f))
                    .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
                    .unwrap_or_default()
            };
            let (va, vb) = (verts(ids[0]), verts(ids[1]));
            let shared = va.iter().filter(|v| vb.contains(v)).count();
            println!("    두 면이 공유하는 정점  {shared} / ({} , {})", va.len(), vb.len());
            println!(
                "\n    -> {}\n",
                if shared > 0 {
                    "정점을 공유한다 — 위상적으로 붙어 있다, 진짜 non-manifold"
                } else {
                    "공유하는 정점 없음 — 같은 자리에 있을 뿐 (이 저장소가 합법이라 정한 것)"
                }
            );
        }
    }
}

/// What the inward clamp says about the face that gets pushed.
///
/// ADR-196 clamps an inward push to the solid's own thickness less
/// `MIN_SOLID_THICKNESS`, so a face cannot travel far enough to land exactly on
/// the far side. This one travelled from z = 200 to z = 300 and landed exactly
/// on a cap it shares two vertices with, so either the clamp did not apply or it
/// did not see the obstacle.
#[test]
#[ignore = "a description, not a gate — run by hand"]
fn what_the_inward_clamp_sees() {
    let keep = [2usize, 4, 18, 22];
    let drop: Vec<usize> = (0..S18_OPS.len()).filter(|i| !keep.contains(i)).collect();
    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if drop.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }

    // The face the push targets: nearest centre to the break's point.
    let target = DVec3::new(-52.0, 70.0, 200.0);
    let mut best: Option<(f64, FaceId)> = None;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let d = (face_centre(&s, fid) - target).length();
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, fid));
        }
    }
    let (dist, f) = best.expect("a face to push");
    let n = s.mesh.faces[f].normal().normalize_or_zero();
    let c = face_centre(&s, f);

    println!("\n  밀어넣을 면 {f:?}  (목표에서 {dist:.1})");
    println!("    중심 ({:.0},{:.0},{:.0})  법선 ({:.2},{:.2},{:.2})", c.x, c.y, c.z, n.x, n.y, n.z);
    println!("    MoveOnly?              {}", axia_geo::operations::push_pull::is_move_only(&s.mesh, f));
    println!(
        "    move_only_max_inward   {:?}",
        axia_geo::operations::push_pull::move_only_max_inward(&s.mesh, f)
    );
    println!("    요청한 거리            -100\n");
}

// ── Would the re-derive have cleared it? ─────────────────────────────────────
//
// The producer is `create_solid` building a cap on a plane that already holds
// one, and `create_solid` did not re-derive at all. (⚠ The first version of this
// paragraph went on to say a DRAW would not leave the pair. It does — see the
// table at the top of the file — because the draw path's re-derive is scoped to
// what was drawn.)
//
// Before changing the push/pull path — which carries 245+ regression tests —
// ask whether the re-derive would even have helped. Engine changes: none. If it
// does not clear the pair, the answer is that the fix is elsewhere, and that
// negative is worth more than a wiring change made on a guess.

/// The five-operation scene, built.
fn stacked_scene() -> Scene {
    let keep = [2usize, 4, 18, 22];
    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if !keep.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }
    play18(&mut s, S18_BREAK);
    s
}

/// Does running the coplanar re-derive on the plane the push landed on clear it?
#[test]
#[ignore = "a simulation — run by hand"]
fn the_re_derive_on_that_plane_would_have_cleared_it() {
    let mut s = stacked_scene();
    let before = stacked_pairs(&s);
    let faces_before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let inv_before = s.mesh.verify_face_invariants().violations.len();
    println!("\n  민 상태     면 {faces_before}  겹침 {before}  위반 {inv_before}");

    // The plane both caps sit on.
    let t0 = std::time::Instant::now();
    let r = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        &mut s.mesh,
        DVec3::new(0.0, 0.0, 300.0),
        DVec3::Z,
        1e-3,
        true,
        None,
    );
    let secs = t0.elapsed().as_secs_f64();
    let after = stacked_pairs(&s);
    let faces_after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let inv_after = s.mesh.verify_face_invariants().violations.len();
    println!(
        "  재유도 후   면 {faces_after}  겹침 {after}  위반 {inv_after}   ({secs:.3} 초, {})",
        if r.is_ok() { "ok" } else { "Err" }
    );
    println!(
        "\n  -> {}\n",
        if after < before {
            "재유도가 치운다 — 배선할 가치가 있다"
        } else {
            "재유도로는 안 된다 — 고칠 곳이 다른 데 있다"
        }
    );
}

/// Which arm of `exec_create_solid` the break takes.
///
/// The reconcile was wired onto the SolidCreated arm and did not clear the
/// pair, so the first question is whether the push ever reaches that arm. Q3
/// (ADR-190) falls back to the legacy `push_pull` when `create_solid` returns
/// NotYetSupported, and that arm returns `PushPullDone`, not `SolidCreated`.
#[test]
#[ignore = "a description — run by hand"]
fn which_arm_the_break_takes() {
    let keep = [2usize, 4, 18, 22];
    let mut s = prod();
    for (i, op) in S18_OPS.iter().enumerate() {
        if !keep.contains(&i) {
            continue;
        }
        play18(&mut s, *op);
    }
    // The break, but executed directly so its result can be read.
    let target = DVec3::new(-52.0, 70.0, 200.0);
    let mut best: Option<(f64, FaceId)> = None;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let d = (face_centre(&s, fid) - target).length();
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, fid));
        }
    }
    let (_, f) = best.expect("a face");
    let r = s.execute(Command::CreateSolid {
        face_id: f,
        mode: CreateSolidMode::Extrude { distance: -100.0 },
    });
    println!("\n  결과: {r:?}");
    println!("  겹침 {}\n", stacked_pairs(&s));
}

/// ⚠ Wiring the re-derive into the push was TRIED, and reverted. Measured.
///
/// The simulation above says the re-derive clears the reduced case, so it was
/// wired: after a push, for every plane where a face the push created shares an
/// edge with a coplanar one, run `rebuild_coplanar_faces_analytic_scoped`.
/// Gated on the damage (nothing happens unless a new face is on a stacked pair)
/// and snapshot-guarded.
///
/// It worked on the reduced case — 32 faces and one violation became 34 and
/// none — and it broke the GATE:
///
/// ```text
///   12 x 20, session 10, op 15
///     face FaceId(22): normal is not finite (NaN, NaN, NaN)
///     face FaceId(66): normal is not finite (NaN, NaN, NaN)
/// ```
///
/// A session that had been sound. Two attempts at the guard:
///
///   1. roll back unless the violation COUNT came down — traded one stacked
///      pair for two NaN faces, which the count called an improvement
///   2. roll back unless the stacking came down AND the total did not rise AND
///      no new non-finite normal appeared — still broke, because the NaN faces
///      are not produced by the re-derive itself. They appear two operations
///      LATER, out of a mesh the re-derive left in a state the next push
///      mishandles.
///
/// A guard on the immediate result cannot see that. Reverted.
///
/// ── Which is worth knowing ───────────────────────────────────────────────
///
/// Three things came out of the attempt and they are what the next go should
/// start from:
///
///   - The producer takes the Q3 FALLBACK, not the `SolidCreated` arm. The
///     reduced case reports `PushPullDone { MODE=CreateFace }`, so a reconcile
///     wired to `create_solid`'s success arm never runs. That cost one full
///     round of "wired it, nothing changed".
///   - The re-derive DOES clear the pair, in 10 ms, on the plane the push
///     landed on. The idea is sound; the placement is not.
///   - Whatever the re-derive leaves behind destabilises the NEXT push. That is
///     the thing to understand before trying again — not the stacking.
/// ── What fixed it, written after ────────────────────────────────────────
///
/// Two arms of `exec_create_solid` reconcile the plane they landed on now: the
/// Q3 fallback and the ADR-196 `is_move_only` dispatch. Neither did before.
///
/// ⚠ Four explanations were measured and are wrong. Written down so no one
/// spends the runs again:
///
///   - position relative to `promote_arc_side_faces_to_cylinder`: moving the
///     call back above the promote changes nothing
///   - the ownership adopt: dropping `adopt_retiled_faces` changes nothing
///     (it is kept because a re-derive without it loses face ownership, which
///     is a different defect, not this one)
///   - the rollback guard: needed, but for session 3's op 14, not session 10
///   - "MoveOnly creates no cap to stack": it does. Sessions 4 and 19 of the
///     50-op run are MoveOnly and they stack. That sentence was inferred and
///     sat in a commit message for an hour before a measurement removed it.
///
/// What broke the first attempt was the SECOND call site: reconciling from the
/// `SolidCreated` arm as well makes session 10 fail at op 15 with NaN normals.
///
/// And what made the first MoveOnly wiring do nothing was the GATE. Asking
/// "is a face this op created on a stacked pair" is right for the fallback arm
/// and wrong twice for MoveOnly: it creates nothing, and the pair it leaves is
/// often two of the pushed face's neighbours. The gate is now the damage —
/// which planes carry a stacked pair before, which after — and session 4 goes
/// from seven pairs to none.
///
/// So this test now reads the other way: the reduced case must NOT stack.
#[test]
fn the_reduced_case_no_longer_stacks() {
    let s = stacked_scene();
    let n = stacked_pairs(&s);
    let inv = s.mesh.verify_face_invariants();
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("
  다섯 연산 — 면 {faces}, 겹침 {n}, 위반 {}
", inv.violations.len());
    assert_eq!(
        n, 0,
        "a wall pushed inward beside a coplanar cap must not leave a stacked          pair — `exec_create_solid`'s Q3 fallback reconciles the plane after          the arc promote"
    );
    assert!(
        inv.is_valid(),
        "and clearing it must not cost soundness: {:?}",
        inv.violations
    );
}

// ── What the re-derive leaves behind ─────────────────────────────────────────
//
// The reverted attempt made fuzz session 10 produce NaN normals at operation 15
// — a session that had been sound, and one where the NaN appears TWO OPERATIONS
// AFTER the re-derive that caused it.
//
// It was the second call site. Reconciling from the Q3 fallback arm alone leaves
// session 10 clean; adding the same call to the `SolidCreated` arm brings the NaN
// back. This gate says so by name.

fn nonfinite_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active() && !f.normal().is_finite()).count()
}

/// Session 10 stays free of NaN normals.
///
/// This is the regression the reverted attempt died on, kept as a gate. The
/// harness's own gate would catch it too — `verify_face_invariants` reports a
/// non-finite normal — but it reports the FIRST violation of any kind, and this
/// one is worth naming: it is what the placement fixes.
///
/// Mutation-checked: also call `reconcile_new_solid_with_coplanar_neighbours`
/// from `exec_create_solid`'s `SolidCreated` arm and this fails at op 15 with
/// `FaceId(22)` and `FaceId(66)` carrying NaN normals.
#[test]
fn session_ten_leaves_no_face_without_a_normal() {
    let mut r = Lcg(0x5EED_0000 + 10);
    let mut s = prod();
    for i in 0..20 {
        let desc = step(&mut s, &mut r);
        let bad = nonfinite_faces(&s);
        assert_eq!(
            bad, 0,
            "op {i} ({desc}) left {bad} face(s) whose normal is not a number"
        );
    }
    let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("
  세션 10, 20 연산 — 면 {live}, NaN 면 0
");
}

// ── What is left, and whose refusal it is ────────────────────────────────────
//
// The reconcile keeps its answer only when the answer is better. So a session
// that still stacks after the fix is one of two things, and they want different
// work:
//
//   - the re-derive does not clear that plane   -> the fix is elsewhere
//   - it clears it and the guard drops it       -> the guard is too blunt
//
// This asks which, for whatever the 50-op wide run reports.

/// Replay a session to the operation before it breaks, then ask both questions.
fn what_a_rederive_would_do(session: usize, ops: usize) -> Vec<(String, usize, usize, usize, usize)> {
    let mut r = Lcg(0x5EED_0000 + session as u64);
    let mut s = prod();
    for _ in 0..ops {
        let _ = step(&mut s, &mut r);
    }
    // Every plane carrying a stacked pair, deduplicated.
    let mut planes: Vec<(DVec3, DVec3)> = Vec::new();
    for eid in s.mesh.collect_non_manifold_edges() {
        let Some((a, _)) = s.mesh.edge_stacked_face_pair(eid) else { continue };
        let Some(n) = s.mesh.faces.get(a).map(|f| f.normal().normalize_or_zero()) else { continue };
        if n.length() < 0.5 {
            continue;
        }
        let Some(edge) = s.mesh.edges.get(eid) else { continue };
        let Ok(p) = s.mesh.vertex_pos(edge.v_small()) else { continue };
        if planes.iter().any(|(q, m)| m.dot(n).abs() > 0.999 && (p - *q).dot(n).abs() < 1e-3) {
            continue;
        }
        planes.push((p, n));
    }

    let stacked = |m: &axia_geo::Mesh| -> usize {
        m.collect_non_manifold_edges()
            .into_iter()
            .filter(|&e| m.edge_stacked_face_pair(e).is_some())
            .count()
    };

    let mut rows = Vec::new();
    for (origin, normal) in planes {
        let before = stacked(&s.mesh);
        let viol_before = s.mesh.verify_face_invariants().violations.len();
        let faces_before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let mut trial = s.mesh.clone();
        let ok = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
            &mut trial, origin, normal, 1e-3, true, None,
        )
        .is_ok();
        let after = if ok { stacked(&trial) } else { before };
        let viol_after = if ok { trial.verify_face_invariants().violations.len() } else { viol_before };
        // ⚠ Did it touch anything? The scoped re-derive carries its OWN rollback
        // (curved face lost / more violations / more self-intersections), so
        // "no change" is two different answers: it declined, or it ran and the
        // plane was already what it would have built.
        let faces_after = trial.faces.iter().filter(|(_, f)| f.is_active()).count();
        rows.push((
            format!(
                "n={:?} o={:?} 면 {faces_before}->{faces_after}",
                normal.round(),
                origin.round()
            ),
            before,
            after,
            viol_before,
            viol_after,
        ));
    }
    rows
}

/// For a session the wide run still reports, whose refusal is it.
#[test]
#[ignore = "a description — run by hand, after a wide run names a session"]
fn whose_refusal_leaves_the_stacking() {
    for (session, ops) in [(3usize, 46usize), (12, 30), (17, 41), (18, 25), (19, 27), (20, 38), (25, 32), (26, 33), (27, 38), (28, 29), (29, 47)] {
        let rows = what_a_rederive_would_do(session, ops);
        println!("\n  세션 {session}, {ops} 연산 — 겹친 평면 {}\n", rows.len());
        for (what, b, a, vb, va) in rows {
            let verdict = if a < b && va <= vb {
                "재유도가 고친다 (가드가 통과시킴)"
            } else if a < b {
                "겹침은 줄지만 위반이 는다 (가드가 거절)"
            } else {
                "재유도가 못 고친다"
            };
            println!("    {what:<44} 겹침 {b}->{a}  위반 {vb}->{va}   {verdict}");
        }
    }
    println!();
}

/// Which operation introduces the stacking, and what kind of operation it is.
///
/// `whose_refusal_leaves_the_stacking` splits what is left in two. Session 4 is
/// the half a re-derive WOULD clear (7 stacked pairs to 0) — so the reconcile
/// never ran there, and the question is which operation made it.
#[test]
#[ignore = "a description — run by hand"]
fn which_operation_first_stacks() {
    for (session, ops) in [(3usize, 46usize), (12, 30), (17, 41), (18, 25), (19, 27), (20, 38), (25, 32), (26, 33), (27, 38), (28, 29), (29, 47)] {
        let mut r = Lcg(0x5EED_0000 + session as u64);
        let mut s = prod();
        let mut prev = 0usize;
        println!("\n  세션 {session} — 겹침이 처음 생기는 연산\n");
        for i in 0..ops {
            let desc = step(&mut s, &mut r);
            let n = stacked_pairs(&s);
            if n > prev {
                println!("    op {i:>2}  {desc:<34}  겹침 {prev} -> {n}   <-");
                prev = n;
            } else if n != prev {
                println!("    op {i:>2}  {desc:<34}  겹침 {prev} -> {n}");
                prev = n;
            }
        }
        println!("    끝: 겹침 {prev}");
    }
    println!();
}

/// Which arm of `exec_create_solid` the breaking push takes.
///
/// The reconcile is wired to the Q3 fallback arm, which answers `PushPullDone`.
/// A push that answers `SolidCreated` never reaches it.
#[test]
#[ignore = "a description — run by hand"]
fn which_arm_the_wide_run_breaks_take() {
    for (session, ops) in [(3usize, 46usize), (12, 30), (17, 41), (18, 25), (19, 27), (20, 38), (25, 32), (26, 33), (27, 38), (28, 29), (29, 47)] {
        let mut r = Lcg(0x5EED_0000 + session as u64);
        let mut s = prod();
        let mut answer = String::from("(never ran)");
        for i in 0..ops {
            let z = [0.0, 100.0, 200.0][r.below(3)];
            let (x, y) = (r.coord(), r.coord());
            let c = DVec3::new(x, y, z);
            let w = r.size();
            let live: Vec<FaceId> =
                s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
            let kind = r.below(10);
            if i + 1 == ops && (kind == 6 || kind == 7) && !live.is_empty() {
                let f = live[r.below(live.len())];
                let d = if kind == 6 {
                    50.0 + r.below(4) as f64 * 50.0
                } else {
                    -(50.0 + r.below(3) as f64 * 50.0)
                };
                let move_only = axia_geo::operations::push_pull::is_move_only(&s.mesh, f);
                let res = s.execute(Command::CreateSolid {
                    face_id: f,
                    mode: CreateSolidMode::Extrude { distance: d },
                });
                answer = format!("{res:?}  is_move_only={move_only}");
                break;
            }
            // Not the operation under study — replay it the ordinary way. The
            // draws above already took their numbers, so re-derive the same
            // call from what was drawn.
            replay_kind(&mut s, &mut r, kind, c, w, x, y, z, &live);
        }
        println!("\n  세션 {session} op {} — {answer}", ops - 1);
    }
    println!();
}

/// The body of `step` for one kind, with the random draws already made.
#[allow(clippy::too_many_arguments)]
fn replay_kind(
    s: &mut Scene, r: &mut Lcg, kind: usize, c: DVec3, w: f64,
    x: f64, y: f64, z: f64, live: &[FaceId],
) {
    match kind {
        0 => { s.execute(Command::DrawRectAsShape { center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75 }); }
        1 => { s.execute(Command::DrawCircleAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24 }); }
        2 => { s.execute(Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 }); }
        3 => { s.execute(Command::DrawEllipseAsCurve { center: c, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: w * 0.5, radius_y: w * 0.3 }); }
        4 => { let n = 3 + r.below(5) as u32; s.execute(Command::DrawPolygonAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, sides: n }); }
        5 => { s.execute(Command::DrawLine { start: DVec3::new(x - 150.0, y, z), end: DVec3::new(x + 150.0, y, z), surface_normal: Some(DVec3::Z) }); }
        6 => {
            if live.is_empty() { return; }
            let f = live[r.below(live.len())];
            let d = 50.0 + r.below(4) as f64 * 50.0;
            s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
        }
        7 => {
            if live.is_empty() { return; }
            let f = live[r.below(live.len())];
            let d = -(50.0 + r.below(3) as f64 * 50.0);
            s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
        }
        8 => { let _ = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL); }
        _ => { let _ = s.punch_rect_hole(DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z); }
    }
}

/// Session 4 pushes a face into its neighbours and leaves nothing stacked.
///
/// The MoveOnly half of the family, pinned. Operation 40 is `pushIn` on a face
/// with `is_move_only = true`: it creates no geometry, it slides vertices, and
/// before the fix it left SEVEN stacked pairs — between faces it had not
/// touched, which is why a gate on created-or-moved faces never fired.
///
/// Mutation-checked twice: unwire the reconcile from the `is_move_only`
/// dispatch in `exec_create_solid` and this reports 7; put it back but gate on
/// the pushed face instead of on the damage, and it still reports 7.
#[test]
fn a_move_only_push_leaves_its_neighbours_unstacked() {
    let mut r = Lcg(0x5EED_0000 + 4);
    let mut s = prod();
    for _ in 0..41 {
        let _ = step(&mut s, &mut r);
    }
    let n = stacked_pairs(&s);
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("\n  세션 4, 41 연산 — 면 {faces}, 겹침 {n}\n");
    assert_eq!(n, 0, "a MoveOnly push must not leave its neighbours stacked");
}
