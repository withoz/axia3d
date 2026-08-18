//! A fuzz session that never finished — and what it was spinning in.
//!
//! ── FIXED. The same crossing was being recorded twice ────────────────────
//!
//! A cut passing exactly through a polygon CORNER is found on both edges that
//! meet there, and `pair_intersection_points` paired the duplicates
//! sequentially — so one chord became two "pairs". The walk downstream emits one
//! sub-polygon per intersection POINT, so every phantom pair cost two more
//! overlapping pieces, and where two pieces overlap the ring comes back to a
//! vertex it already passed.
//!
//! Collapsing coincident points before pairing (`boolean.rs`, a nanometre
//! tolerance — far below the 0.15 μm the mesh dedups at, so it can only merge
//! points that are the same point):
//!
//! ```text
//!                          was      is
//!   faces out              450      58
//!   walking a vertex       386       2
//!   invariant breaks        70       8
//!   the five-face call      23 s     0.1 s
//!   the draw that wedged   > 15 min  0.105 s
//! ```
//!
//! Everything below is the hunt, kept because the two wrong turns in it are
//! worth more than the answer.
//!
//! The 100 x 50 wide run stopped printing at session 22 and stayed there,
//! burning a full core at a flat 137 MB:
//!
//! ```text
//!   486.8s CPU   137 MB
//!   490.8s CPU   137 MB      +1 CPU-second per wall second
//!   494.8s CPU   137 MB      memory flat -> spinning, not growing
//! ```
//!
//! Not merely slow. Given fifteen minutes in a RELEASE build, one rectangle
//! still had not returned:
//!
//! ```text
//!   op 18  kind 0 at (150,100,200) w=180 ...        real  15m0.132s
//! ```
//!
//! Same category as the crash it followed: the harness contract never gets to
//! speak. A wedged session reports nothing at all — no seed, no operation list,
//! no violation — so it is worse to read than a failure. The harness now
//! time-boxes each session and reports a wedge like a break; this file is where
//! the report points.
//!
//! ── Which operation ──────────────────────────────────────────────────────
//!
//! Replaying it with each operation NAMED BEFORE the call put it on screen:
//! operation 18 is `rect(x=150, y=100, z=200, w=180)`. An ordinary rectangle,
//! on a plane that already holds three box bottoms, several circles, an ellipse
//! and the top of a solid raised from z = 100.
//!
//! ── Where it is, measured stage by stage ─────────────────────────────────
//!
//! The flag bisection below points at the coplanar re-derive. That was right
//! about the SWITCH and wrong about the place — the gate decides whether the
//! draw reaches the stage that wedges, and that stage is two levels further on.
//! Every stage of the draw now has a stopwatch:
//!
//! ```text
//!   all pairwise intersections on the plane      0.063 s
//!   arrange (47 curves -> 16 faces)              0.062 s
//!   the coplanar re-derive (gate on OR off)      0.209 s
//!   the rollback guard's four whole-mesh scans   0.019 s
//!   detect_self_intersections (48 faces)         0.148 s
//!   ------------------------------------------------------
//!   split_faces_crossing_other_planes            > 120 s   <- here
//! ```
//!
//! And it is not being handed much: 48 live faces, **18** self-intersecting
//! pairs, and the crossing set is a subset of those. It calls one thing —
//! `Mesh::intersect_faces_with_model` (axia-geo `operations/boolean.rs`) — with
//! a handful of faces, and does not come back. A small input that never returns
//! is a loop, not a load.
//!
//! ⚠ Two of these numbers cost a wrong measurement first, and both are worth
//! knowing:
//!
//!   - The re-derive was "timed" by calling `play(S22_WEDGE)` and then the
//!     re-derive. That call IS the wedge, so the thread never reached the
//!     stopwatch and the timeout measured the pipeline again. The rectangle has
//!     to go in with the auto-behaviours OFF.
//!   - The freeform curve count was taken at the arrangement's chord tolerance
//!     of 1e-2, which is what `intersect_curves` gets from a 1e-3 caller. The
//!     arrangement passes 1e-4. Points grow as 1/sqrt(tol): 257 became 3325.
//!
//! ── What it is doing there ───────────────────────────────────────────────
//!
//! `split_faces_crossing_other_planes` calls one thing:
//! `Mesh::intersect_faces_with_model`, with the faces that crossed another
//! plane. Each of those eight, handed over ALONE, finishes in milliseconds.
//! Grow the set and it tips:
//!
//! ```text
//!   first 4    0.001 s    61 faces out
//!   first 5   26.024 s   436 faces out     <- FaceId(28), a box side
//! ```
//!
//! 436 faces from a model of 48, and of the 450 the full call leaves, **386**
//! have a boundary that walks the same vertex twice — one with 30 repeats in 57
//! corners — with 70 invariant violations. Coordinates do NOT move: [-310, 320]
//! before and after, no non-finite vertex. So it is not a point flying off; the
//! splitter is producing overlapping pieces, and where they overlap the ring
//! comes back to a vertex it already passed.
//!
//! ⚠ The obvious fix is the wrong layer, measured. Splitting each self-touching
//! ring at its touches — what `polygon_difference_by_clip` needed in
//! `operations/coplanar.rs` — does exactly what it says and makes the rest
//! worse:
//!
//! ```text
//!   repeated-vertex faces   386 -> 0
//!   faces                   450 -> 1920
//!   the five-face call      23 s -> 129 s
//! ```
//!
//! The self-touches are a symptom of over-production, not the cause of it.
//! Reverted; the disease is wherever `sub_polys` decides how many pieces a face
//! becomes.
//!
//! ── Which subsystem ──────────────────────────────────────────────────────
//!
//! No debugger needed — run the same 19 operations with one automatic
//! behaviour off at a time and see which turn returns:
//!
//! ```text
//!   nothing off   wedges
//!   rederive off  RETURNS
//!   intersect off wedges
//!   synth off     wedges
//!   freeform off  RETURNS
//! ```
//!
//! Both turns that return are the same path: `freeform_overlap_on_draw` gates
//! `detect_freeform_overlaps`, which `rebuild_inner` calls from inside the
//! coplanar re-derive. It walks every coplanar freeform curve against every
//! other coplanar curve, calling `intersect_curves` on each pair — the same
//! function that hosted the clockwise-arc crash one commit earlier.
//!
//! ⚠ NOT caused by that fix. Session 22 was measured with the pre-fix clamp
//! restored: it wedges there too, with zero panics. The fix stopped a crash; it
//! did not start this.
//!
//! ── What it is NOT ───────────────────────────────────────────────────────
//!
//! Three measurements, three hypotheses dead.
//!
//! **Not the pairwise intersections.** The plane holds 1 freeform (a NURBS,
//! 3325 points at the arrangement's own 1e-4 chord tolerance) and 46 other
//! curves. One freeform against a line is 0.9 ms; all 46 come to 0.04 s.
//!
//! **Not the shape of the scene.** Built by hand — one ellipse, N circles, N
//! solids whose bottoms sit on the plane — every cell of a 4x3 grid finishes
//! under a second, growing about 0.09 s per curve added.
//!
//! **Not the size of the arrangement.** This is the one that matters. Dropping
//! any single operation makes the rectangle return, so it looks like a cliff —
//! but count what `arrange` has to carry and the numbers go the WRONG way:
//!
//! ```text
//!   dropped          curves   crossings   all pairs    the final draw
//!   nothing              54        4513     0.063 s    > 15 minutes
//!   13 (pushIn)          54        4514     0.063 s    > 15 minutes
//!   11 (circle z=0)      60        7682     0.073 s    7.5 s
//!   12 (ellipse)         50        2757     0.020 s    0.08 s
//!   3 (a solid)          41        1381     0.051 s    0.15 s
//! ```
//!
//! A variant with MORE curves and MORE crossings finishes in seven seconds
//! while the original never does. So it is not volume — it is a particular
//! configuration, and the next place to look is what `arrange` does AFTER the
//! crossings: splitting curves at them and walking the planar graph.
//!
//! ── What is NOT enough to reproduce it ───────────────────────────────────
//!
//! `the_z200_plane_alone_does_not_wedge` below is a recorded negative: every
//! draw session 22 put on z = 200, plus the solid it raised from z = 100 so its
//! top landed there, then the rectangle — and it returns. Whatever the wedge
//! needs, it is not just the plane.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The z = 200 half of fuzz session 22, in the order it ran them.
///
/// Only the position-driven operations: the session's `extrude` and `pushIn`
/// picked faces by index into the live list, which cannot be replayed once
/// anything is dropped. If the wedge survives without them, they were never
/// part of it.
#[derive(Clone, Copy, Debug)]
enum Op {
    Box { x: f64, y: f64, w: f64 },
    Circle { x: f64, y: f64, r: f64 },
    CircleCurve { x: f64, y: f64, r: f64 },
    Ellipse { x: f64, y: f64, rx: f64, ry: f64 },
    Rect { x: f64, y: f64, w: f64 },
    /// Draw a circle on the z = 100 plane — the level the session raised.
    CircleAt100 { x: f64, y: f64, r: f64 },
    /// Extrude the live face whose centroid is nearest `(x, y, z)`. Face ids
    /// move when operations are dropped; a position does not.
    ExtrudeNear { x: f64, y: f64, z: f64, d: f64 },
}

const Z: f64 = 200.0;

fn run(s: &mut Scene, op: Op) -> String {
    match op {
        Op::Box { x, y, w } => {
            let c = DVec3::new(x, y, Z + 60.0);
            let ok = s.mesh.create_box(c, w, 120.0, w, FORM_MATERIAL).is_ok();
            format!("box({x},{y},{w}) {}", if ok { "ok" } else { "FAILED" })
        }
        Op::Circle { x, y, r } => {
            let e = matches!(
                s.execute(Command::DrawCircleAsShape {
                    center: DVec3::new(x, y, Z),
                    normal: DVec3::Z,
                    radius: r,
                    segments: 24,
                }),
                CommandResult::Error(_)
            );
            format!("circle({x},{y},r={r}){}", if e { " REFUSED" } else { "" })
        }
        Op::CircleCurve { x, y, r } => {
            let e = matches!(
                s.execute(Command::DrawCircleAsCurve {
                    center: DVec3::new(x, y, Z),
                    normal: DVec3::Z,
                    radius: r,
                }),
                CommandResult::Error(_)
            );
            format!("circleCurve({x},{y},r={r}){}", if e { " REFUSED" } else { "" })
        }
        Op::Ellipse { x, y, rx, ry } => {
            let e = matches!(
                s.execute(Command::DrawEllipseAsCurve {
                    center: DVec3::new(x, y, Z),
                    ref_dir: DVec3::X,
                    normal: DVec3::Z,
                    radius_x: rx,
                    radius_y: ry,
                }),
                CommandResult::Error(_)
            );
            format!("ellipse({x},{y},{rx},{ry}){}", if e { " REFUSED" } else { "" })
        }
        Op::CircleAt100 { x, y, r } => {
            let e = matches!(
                s.execute(Command::DrawCircleAsShape {
                    center: DVec3::new(x, y, 100.0),
                    normal: DVec3::Z,
                    radius: r,
                    segments: 24,
                }),
                CommandResult::Error(_)
            );
            format!("circle@100({x},{y},r={r}){}", if e { " REFUSED" } else { "" })
        }
        Op::ExtrudeNear { x, y, z, d } => {
            let target = DVec3::new(x, y, z);
            let mut best: Option<(f64, axia_geo::FaceId)> = None;
            for (fid, f) in s.mesh.faces.iter() {
                if !f.is_active() {
                    continue;
                }
                let Ok(vs) = s.mesh.collect_loop_verts(f.outer().start) else { continue };
                if vs.is_empty() {
                    continue;
                }
                let n = vs.len() as f64;
                let c = vs
                    .iter()
                    .filter_map(|v| s.mesh.verts.get(*v).map(|q| q.pos()))
                    .fold(DVec3::ZERO, |a, b| a + b)
                    / n;
                let dist = (c - target).length();
                if best.map(|(bd, _)| dist < bd).unwrap_or(true) {
                    best = Some((dist, fid));
                }
            }
            let Some((dist, f)) = best else {
                return "extrudeNear(nothing there)".into();
            };
            let e = matches!(
                s.execute(Command::CreateSolid {
                    face_id: f,
                    mode: axia_geo::CreateSolidMode::Extrude { distance: d },
                }),
                CommandResult::Error(_)
            );
            format!("extrudeNear({x},{y},{z},d={d}) dist={dist:.0}{}", if e { " REFUSED" } else { "" })
        }
        Op::Rect { x, y, w } => {
            let e = matches!(
                s.execute(Command::DrawRectAsShape {
                    center: DVec3::new(x, y, Z),
                    normal: DVec3::Z,
                    up: DVec3::X,
                    width: w,
                    height: w * 0.75,
                }),
                CommandResult::Error(_)
            );
            format!("rect({x},{y},{w}){}", if e { " REFUSED" } else { "" })
        }
    }
}

/// Everything session 22 put on the z = 200 plane before the rectangle wedged.
const SETUP: &[Op] = &[
    Op::Circle { x: 0.0, y: 100.0, r: 50.0 },
    // The session raised a circle from z = 100 so its top landed on z = 200.
    Op::CircleAt100 { x: -200.0, y: 150.0, r: 70.0 },
    Op::ExtrudeNear { x: -200.0, y: 163.0, z: 100.0, d: 100.0 },
    Op::Box { x: -50.0, y: 50.0, w: 180.0 },
    Op::CircleCurve { x: -100.0, y: -100.0, r: 90.0 },
    Op::Circle { x: 0.0, y: 0.0, r: 30.0 },
    Op::Box { x: -200.0, y: 150.0, w: 100.0 },
    Op::Ellipse { x: 0.0, y: 150.0, rx: 110.0, ry: 66.0 },
    Op::Rect { x: -200.0, y: 0.0, w: 60.0 },
    Op::Box { x: -200.0, y: -50.0, w: 220.0 },
];

/// The operation that never returned.
const WEDGE: Op = Op::Rect { x: 150.0, y: 100.0, w: 180.0 };

/// A recorded negative: the z = 200 plane alone does not wedge.
///
/// Everything session 22 drew on that plane, plus the solid it raised from
/// z = 100 so its top landed there, then the rectangle that wedged — and it
/// returns in under two seconds. The wedge needs something else, and this says
/// so rather than leaving it to be guessed at again.
///
/// ⚠ Not `#[ignore]`d, because it PASSES. If a change ever makes this one hang,
/// that is the reduction arriving and the file needs rewriting around it.
#[test]
fn the_z200_plane_alone_does_not_wedge() {
    let mut s = prod();
    println!("\n  session 22, z = 200 only\n");
    for (i, op) in SETUP.iter().enumerate() {
        let d = run(&mut s, *op);
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!("    {i:>2}  {d:<34}  faces {live:>4}");
    }
    print!("    ->  {WEDGE:?} ... ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let d = run(&mut s, WEDGE);
    println!("{d}");
    println!("\n  returned — the z = 200 half alone does NOT wedge\n");
}

// ── Which face did the session's extrude and pushIn actually pick? ───────────
//
// The op list alone cannot say: both choose by index into the live face list,
// so the answer moves the moment anything is dropped. This replays the real
// generator and prints the chosen face's centroid, which does not move.

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

/// Replays session 22 up to (not including) the wedge, naming the faces that
/// `extrude` and `pushIn` chose by where they are rather than by their id.
#[test]
fn which_faces_the_solid_operations_picked() {
    use axia_geo::{CreateSolidMode, FaceId};
    let mut r = Lcg(0x5EED_0000 + 22);
    let mut s = prod();
    println!("
  session 22 — the solid operations, by position
");
    for op in 0..18 {
        let z = [0.0, 100.0, 200.0][r.below(3)];
        let (x, y) = (r.coord(), r.coord());
        let c = DVec3::new(x, y, z);
        let w = r.size();
        let kind = r.below(10);
        let live: Vec<FaceId> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .collect();
        match kind {
            0 => { s.execute(Command::DrawRectAsShape { center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75 }); }
            1 => { s.execute(Command::DrawCircleAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24 }); }
            2 => { s.execute(Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 }); }
            3 => { s.execute(Command::DrawEllipseAsCurve { center: c, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: w * 0.5, radius_y: w * 0.3 }); }
            4 => { let n = 3 + r.below(5) as u32; s.execute(Command::DrawPolygonAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, sides: n }); }
            5 => { s.execute(Command::DrawLine { start: DVec3::new(x - 150.0, y, z), end: DVec3::new(x + 150.0, y, z), surface_normal: Some(DVec3::Z) }); }
            6 | 7 => {
                if live.is_empty() {
                    println!("    op {op:>2}  {} (skip: nothing there)", if kind == 6 { "extrude" } else { "pushIn" });
                    continue;
                }
                let f = live[r.below(live.len())];
                let d = if kind == 6 {
                    50.0 + r.below(4) as f64 * 50.0
                } else {
                    -(50.0 + r.below(3) as f64 * 50.0)
                };
                let vs = s
                    .mesh
                    .faces
                    .get(f)
                    .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
                    .unwrap_or_default();
                let n = vs.len().max(1) as f64;
                let ctr = vs
                    .iter()
                    .filter_map(|v| s.mesh.verts.get(*v).map(|x| x.pos()))
                    .fold(DVec3::ZERO, |a, b| a + b)
                    / n;
                let nrm = s.mesh.compute_normal(&vs).unwrap_or(DVec3::ZERO);
                let e = matches!(
                    s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } }),
                    CommandResult::Error(_)
                );
                println!(
                    "    op {op:>2}  {:<8} d={d:<6} at ({:.0},{:.0},{:.0}) n=({:.2},{:.2},{:.2}){}",
                    if kind == 6 { "extrude" } else { "pushIn" },
                    ctr.x, ctr.y, ctr.z, nrm.x, nrm.y, nrm.z,
                    if e { "  REFUSED" } else { "" }
                );
                continue;
            }
            8 => { let _ = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL); }
            _ => { let _ = s.punch_rect_hole(DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z); }
        }
        println!("    op {op:>2}  kind {kind} at ({x},{y},{z}) w={w}");
    }
    println!();
}

/// Which of the four automatic behaviours is the one that spins?
///
/// No debugger needed: run the exact 19-operation replay with one flag off at a
/// time and see which turn returns. `AXIA_WEDGE_OFF` names the flag —
/// `intersect`, `synth`, `rederive`, `freeform`, or `none`.
///
/// ⚠ HANGS unless the flag that is off is the guilty one, so `#[ignore]`d and
/// run by hand under a timeout:
///
/// ```text
///   AXIA_WEDGE_OFF=rederive timeout 60 cargo test -p axia-core ///     --test a_fuzz_session_that_never_finishes wedge_with -- --ignored --nocapture
/// ```
#[test]
#[ignore = "hangs unless the disabled flag is the guilty one"]
fn wedge_with_one_automatic_behaviour_off() {
    use axia_geo::{CreateSolidMode, FaceId};
    let off = std::env::var("AXIA_WEDGE_OFF").unwrap_or_else(|_| "none".into());
    let mut s = prod();
    match off.as_str() {
        "intersect" => s.auto_intersect_on_draw = false,
        "synth" => s.auto_face_synthesis_on_draw = false,
        "rederive" => s.face_rederive_on_draw = false,
        "freeform" => s.freeform_overlap_on_draw = false,
        _ => {}
    }
    println!("
  session 22, 19 operations, {off} OFF
");
    let mut r = Lcg(0x5EED_0000 + 22);
    for op in 0..19 {
        let z = [0.0, 100.0, 200.0][r.below(3)];
        let (x, y) = (r.coord(), r.coord());
        let c = DVec3::new(x, y, z);
        let w = r.size();
        let kind = r.below(10);
        print!("    op {op:>2}  kind {kind} at ({x},{y},{z}) w={w} ... ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let live: Vec<FaceId> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        match kind {
            0 => { s.execute(Command::DrawRectAsShape { center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75 }); }
            1 => { s.execute(Command::DrawCircleAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24 }); }
            2 => { s.execute(Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 }); }
            3 => { s.execute(Command::DrawEllipseAsCurve { center: c, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: w * 0.5, radius_y: w * 0.3 }); }
            4 => { let n = 3 + r.below(5) as u32; s.execute(Command::DrawPolygonAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, sides: n }); }
            5 => { s.execute(Command::DrawLine { start: DVec3::new(x - 150.0, y, z), end: DVec3::new(x + 150.0, y, z), surface_normal: Some(DVec3::Z) }); }
            6 | 7 => {
                if live.is_empty() { println!("skip"); continue; }
                let f = live[r.below(live.len())];
                let d = if kind == 6 { 50.0 + r.below(4) as f64 * 50.0 } else { -(50.0 + r.below(3) as f64 * 50.0) };
                s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
            }
            8 => { let _ = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL); }
            _ => { let _ = s.punch_rect_hole(DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z); }
        }
        println!("done");
    }
    println!("
  all 19 returned with {off} off
");
}

// ── What is it actually doing? ───────────────────────────────────────────────
//
// Knowing the subsystem is not knowing the cause. `detect_freeform_overlaps`
// walks every coplanar freeform curve against every other coplanar curve:
//
//     for each freeform i:  for each other curve:  intersect_curves(...)
//
// and `intersect_curves` tessellates BOTH curves, then walks every pair of
// polyline segments. So the bill is
//
//     |freeform| x |others| x (points(freeform) x points(other))
//
// which is either a large finite number or a loop, and those need different
// fixes. This counts each factor.

/// Every coplanar curve on the plane the wedge draws into, counted the way
/// `detect_freeform_overlaps` counts them, and how many points each tessellates
/// to.
#[test]
fn what_the_wedge_is_actually_working_on() {
    use axia_geo::curves::{AnalyticCurve, CurveOps};
    use axia_geo::{CreateSolidMode, FaceId};

    // Build the scene the wedge draws into: session 22, operations 0..17.
    let mut r = Lcg(0x5EED_0000 + 22);
    let mut s = prod();
    for _ in 0..18 {
        let z = [0.0, 100.0, 200.0][r.below(3)];
        let (x, y) = (r.coord(), r.coord());
        let c = DVec3::new(x, y, z);
        let w = r.size();
        let kind = r.below(10);
        let live: Vec<FaceId> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
        match kind {
            0 => { s.execute(Command::DrawRectAsShape { center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75 }); }
            1 => { s.execute(Command::DrawCircleAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24 }); }
            2 => { s.execute(Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 }); }
            3 => { s.execute(Command::DrawEllipseAsCurve { center: c, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: w * 0.5, radius_y: w * 0.3 }); }
            4 => { let n = 3 + r.below(5) as u32; s.execute(Command::DrawPolygonAsShape { center: c, normal: DVec3::Z, radius: w * 0.5, sides: n }); }
            5 => { s.execute(Command::DrawLine { start: DVec3::new(x - 150.0, y, z), end: DVec3::new(x + 150.0, y, z), surface_normal: Some(DVec3::Z) }); }
            6 | 7 => {
                if live.is_empty() { continue; }
                let f = live[r.below(live.len())];
                let d = if kind == 6 { 50.0 + r.below(4) as f64 * 50.0 } else { -(50.0 + r.below(3) as f64 * 50.0) };
                s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } });
            }
            8 => { let _ = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL); }
            _ => { let _ = s.punch_rect_hole(DVec3::new(x - 30.0, y - 30.0, z), DVec3::new(x + 30.0, y + 30.0, z), DVec3::Z); }
        }
    }

    // The plane the wedging rectangle draws into.
    let plane_z = 200.0;
    let tol = 1e-3;
    let on_plane = |p: DVec3| (p.z - plane_z).abs() < tol;

    // ⚠ The first version of this counted freeform SELF-LOOP FACES, the way
    // Phase 0.5 does, and found ZERO — which cannot be what wedges, because
    // that pass returns immediately on an empty list. Reading further: the flag
    // gates only Phase 0.5, but Phase 0.5's OUTPUT (a `freeform_curve_source`
    // per owner id) is what B6 later projects into `arrange` as an
    // `InputCurve::Freeform`. Turning the flag off means nothing is marked, so
    // no freeform ever reaches `arrange`. That is what is being counted now:
    // the arrangement's input, not the detector's.
    let mut freeform_owners: Vec<u32> = Vec::new();
    let mut lines = 0usize;
    let mut circles = 0usize;
    let mut arcs = 0usize;
    for (eid, e) in s.mesh.edges.iter() {
        if !e.is_active() {
            continue;
        }
        let (Some(a), Some(b)) = (
            s.mesh.verts.get(e.v_small()).map(|v| v.pos()),
            s.mesh.verts.get(e.v_large()).map(|v| v.pos()),
        ) else {
            continue;
        };
        if !(on_plane(a) && on_plane(b)) {
            continue;
        }
        if let Some(owner) = e.curve_owner_id() {
            if s.mesh.freeform_curve_source(owner).is_some() {
                if !freeform_owners.contains(&owner) {
                    freeform_owners.push(owner);
                }
                continue;
            }
        }
        match s.mesh.edge_curve(eid) {
            Some(AnalyticCurve::Circle { .. }) => circles += 1,
            Some(AnalyticCurve::Arc { .. }) => arcs += 1,
            _ => lines += 1,
        }
    }
    let total = freeform_owners.len() + circles + arcs + lines;

    println!("
  op 18 이 그려 들어가는 평면 (z = {plane_z}) — arrange 에 들어가는 것
");
    println!("    freeform (owner 별)  {}", freeform_owners.len());
    println!("    circle               {circles}");
    println!("    arc                  {arcs}");
    println!("    line                 {lines}");
    println!("    합계                 {total}");
    println!("    쌍 (arrange 는 전부 서로 교차 검사)  {}
", total * total.saturating_sub(1) / 2);

    // ⚠ Not any tolerance — the one the arrangement actually uses.
    //
    //   face_rederive.rs:2035   arrange(&input_curves, 1e-4)
    //   analytic_arrange.rs     tol       = (eps * 0.1).max(1e-9)   = 1e-5
    //   curves/intersect.rs     chord_tol = (tol * 10).max(1e-4)    = 1e-4
    //
    // A first pass here used 1e-2 because that is what `intersect_curves` gets
    // from a 1e-3 caller, and reported 257 points. The arrangement's tolerance
    // is a hundred times finer, and points grow as 1/sqrt(tol).
    const ARRANGE_EPS: f64 = 1e-4;
    let tol = (ARRANGE_EPS * 0.1f64).max(1e-9);
    let chord_tol = (tol * 10.0f64).max(1e-4);
    println!("    arrange eps {ARRANGE_EPS}  ->  tol {tol}  ->  chord_tol {chord_tol}
");

    let mut freeform_pts = 0usize;
    for owner in &freeform_owners {
        let Some(src) = s.mesh.freeform_curve_source(*owner) else { continue };
        let t0 = std::time::Instant::now();
        let n = src.tessellate(chord_tol, &s.mesh).map(|v| v.len()).unwrap_or(0);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        println!(
            "    owner {owner:>4}  {:<8} {n:>8} 점   tessellate {ms:>8.1} ms",
            match src {
                AnalyticCurve::Bezier { .. } => "Bezier",
                AnalyticCurve::BSpline { .. } => "BSpline",
                AnalyticCurve::NURBS { .. } => "NURBS",
                AnalyticCurve::Circle { .. } => "Circle",
                AnalyticCurve::Arc { .. } => "Arc",
                AnalyticCurve::Line { .. } => "Line",
            },
            ms = ms
        );
        freeform_pts = freeform_pts.max(n);
    }

    // And what one pair costs — the freeform against a straight edge, which is
    // the cheapest partner it has. There are 46 of them.
    if let Some(owner) = freeform_owners.first() {
        if let Some(src) = s.mesh.freeform_curve_source(*owner).cloned() {
            let line = AnalyticCurve::Bezier {
                control_pts: vec![DVec3::new(-400.0, 0.0, plane_z), DVec3::new(400.0, 0.0, plane_z)],
            };
            let t0 = std::time::Instant::now();
            let hits = axia_geo::curves::intersect::intersect_curves(&src, &line, &s.mesh, tol);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            println!(
                "
    freeform x 직선 한 쌍   {:>9.1} ms   교차 {}",
                ms,
                hits.map(|v| v.len()).unwrap_or(0)
            );
            println!("    x {} 쌍            {:>9.1} s
", total - 1, ms * (total - 1) as f64 / 1000.0);
        }
    }

    assert!(
        total > 0,
        "the wedging plane has to hold curves, or this is measuring an empty room"
    );
}

/// A synthetic version, so the cost can be watched growing.
///
/// The pairwise intersections are not the bill — one freeform against a line is
/// 0.9 ms and there are 46 of them. So this builds the same SHAPE of scene by
/// hand and times the last draw.
///
/// ⚠ Curves alone do not reproduce it: one ellipse plus eight circles costs
/// 0.68 s and grows roughly linearly (about 0.09 s per curve added). What
/// session 22's plane also had, and this first version did not, is THREE SOLIDS
/// whose bottoms sit on it — which is what puts `volume_edges` and
/// `force_include` into the re-derive's input selection. So the grid below
/// varies both.
///
/// ⚠ Time-boxed per step, on its own thread, for the same reason the harness
/// is: a step that wedges must report rather than hang the suite.
#[test]
fn how_the_last_draw_grows_with_what_is_already_on_the_plane() {
    fn one_run(n_boxes: usize, n_circles: usize, budget: std::time::Duration) -> Option<f64> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut s = prod();
            // One freeform, the kind the flag bisection implicated.
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(0.0, 0.0, 0.0),
                ref_dir: DVec3::X,
                normal: DVec3::Z,
                radius_x: 110.0,
                radius_y: 66.0,
            });
            // Solids sitting ON the plane: `create_box` is centred, so a box of
            // height 120 centred at z = 60 has its bottom at z = 0.
            for i in 0..n_boxes {
                let a = i as f64 * 1.3;
                let _ = s.mesh.create_box(
                    DVec3::new(a.cos() * 90.0, a.sin() * 90.0, 60.0),
                    180.0,
                    120.0,
                    180.0,
                    FORM_MATERIAL,
                );
            }
            for i in 0..n_circles {
                let a = i as f64 * 0.7;
                s.execute(Command::DrawCircleAsShape {
                    center: DVec3::new(a.cos() * 80.0, a.sin() * 80.0, 0.0),
                    normal: DVec3::Z,
                    radius: 50.0,
                    segments: 24,
                });
            }
            // The draw that wedged, in shape: a rectangle across the lot.
            let t0 = std::time::Instant::now();
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(60.0, 40.0, 0.0),
                normal: DVec3::Z,
                up: DVec3::X,
                width: 180.0,
                height: 135.0,
            });
            let _ = tx.send(t0.elapsed().as_secs_f64());
        });
        rx.recv_timeout(budget).ok()
    }

    // 15 s, not 90: every cell measured under a second, so this is generous by
    // fifteen times and still cannot hang the suite for long if one regresses.
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(15),
    );
    println!("
  마지막 draw 비용 — 입체 수 x 원 수 (초, {}초 예산)
", budget.as_secs());
    println!("            원 0      원 2      원 4");
    for nb in [0usize, 1, 2, 3] {
        let mut row = format!("    입체 {nb}  ");
        for nc in [0usize, 2, 4] {
            match one_run(nb, nc, budget) {
                Some(secs) => row.push_str(&format!("{secs:>8.3}  ")),
                None => {
                    row.push_str("   멈춤  ");
                    println!("{row}");
                    panic!(
                        "an ordinary draw over {nb} solids and {nc} circles has to                          finish — this grid measured every cell under a second"
                    );
                }
            }
        }
        println!("{row}");
    }
    println!("
  이 격자 안에서는 전부 끝난다
");
}

// ── Session 22 as data, so operations can be dropped ─────────────────────────
//
// Building the shape by hand did not reproduce it: one ellipse plus eight
// circles is 0.68 s, and a grid of up to three solids by four circles is all
// under a second. So the reduction has to start from the REAL scene and
// subtract, and that needs the operations as data rather than as an RNG stream.
//
// Transcribed from the instrumented replay, which printed each operation's kind
// and parameters before running it. The two solid operations are given by
// POSITION — face ids move the moment anything is dropped, a centroid does not.

#[derive(Clone, Copy, Debug)]
enum S22 {
    Circle { x: f64, y: f64, z: f64, r: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    ExtrudeNear { x: f64, y: f64, z: f64, d: f64 },
}

/// Operations 1..18 of fuzz session 22 (0 was a skip), then the one that wedges.
const S22_OPS: &[S22] = &[
    S22::Circle { x: 150.0, y: 150.0, z: 0.0, r: 50.0 },
    S22::Circle { x: 0.0, y: 100.0, z: 200.0, r: 50.0 },
    S22::CircleCurve { x: -200.0, y: 50.0, z: 100.0, r: 110.0 },
    S22::Box { x: -50.0, y: 50.0, z: 200.0, w: 180.0 },
    S22::CircleCurve { x: -100.0, y: -100.0, z: 200.0, r: 90.0 },
    S22::Circle { x: -200.0, y: 150.0, z: 100.0, r: 70.0 },
    S22::Circle { x: 0.0, y: 0.0, z: 200.0, r: 30.0 },
    S22::Ellipse { x: 200.0, y: -100.0, z: 100.0, rx: 110.0, ry: 66.0 },
    S22::Box { x: -200.0, y: 150.0, z: 200.0, w: 100.0 },
    S22::Ellipse { x: -50.0, y: 150.0, z: 0.0, rx: 30.0, ry: 18.0 },
    S22::ExtrudeNear { x: -200.0, y: 163.0, z: 100.0, d: 100.0 },
    S22::Circle { x: 0.0, y: 0.0, z: 0.0, r: 50.0 },
    S22::Ellipse { x: 0.0, y: 150.0, z: 200.0, rx: 110.0, ry: 66.0 },
    S22::ExtrudeNear { x: -200.0, y: 73.0, z: 100.0, d: -50.0 },
    S22::Rect { x: -200.0, y: 0.0, z: 200.0, w: 60.0 },
    S22::Box { x: -200.0, y: -50.0, z: 200.0, w: 220.0 },
];

/// The operation that never returned.
const S22_WEDGE: S22 = S22::Rect { x: 150.0, y: 100.0, z: 200.0, w: 180.0 };

fn play(s: &mut Scene, op: S22) {
    match op {
        S22::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r, segments: 24,
            });
        }
        S22::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z), normal: DVec3::Z, radius: r,
            });
        }
        S22::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z), ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: rx, radius_y: ry,
            });
        }
        S22::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z), normal: DVec3::Z, up: DVec3::X,
                width: w, height: w * 0.75,
            });
        }
        S22::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(
                DVec3::new(x, y, z + 60.0), w, 120.0, w, FORM_MATERIAL,
            );
        }
        S22::ExtrudeNear { x, y, z, d } => {
            let target = DVec3::new(x, y, z);
            let mut best: Option<(f64, axia_geo::FaceId)> = None;
            for (fid, f) in s.mesh.faces.iter() {
                if !f.is_active() { continue; }
                let Ok(vs) = s.mesh.collect_loop_verts(f.outer().start) else { continue };
                if vs.is_empty() { continue; }
                let n = vs.len() as f64;
                let c = vs.iter().filter_map(|v| s.mesh.verts.get(*v).map(|q| q.pos()))
                    .fold(DVec3::ZERO, |a, b| a + b) / n;
                let dist = (c - target).length();
                if best.map(|(bd, _)| dist < bd).unwrap_or(true) { best = Some((dist, fid)); }
            }
            if let Some((_, f)) = best {
                s.execute(Command::CreateSolid {
                    face_id: f,
                    mode: axia_geo::CreateSolidMode::Extrude { distance: d },
                });
            }
        }
    }
}

/// Replay with a set of operation indices dropped; time the final rectangle.
/// `None` = it did not return inside the budget.
fn s22_time(dropped: &[usize], budget: std::time::Duration) -> Option<f64> {
    let dropped: Vec<usize> = dropped.to_vec();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = prod();
        for (i, op) in S22_OPS.iter().enumerate() {
            if dropped.contains(&i) { continue; }
            play(&mut s, *op);
        }
        let t0 = std::time::Instant::now();
        play(&mut s, S22_WEDGE);
        let _ = tx.send(t0.elapsed().as_secs_f64());
    });
    rx.recv_timeout(budget).ok()
}

/// Does session 22 replayed AS DATA still wedge?
///
/// It has to, or the transcription is wrong and everything below measures a
/// different scene. Positions replace face ids for the two solid operations, so
/// this is not the RNG stream — it is the same geometry reached another way.
#[test]
#[ignore = "the full list wedges by design — run by hand under a timeout"]
fn session_22_as_data_still_wedges() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
    );
    println!("\n  세션 22, 데이터로 재생 — 마지막 사각형\n");
    match s22_time(&[], budget) {
        Some(secs) => println!("    {secs:.3} 초 만에 끝남 — 전사가 장면을 놓쳤다\n"),
        None => println!("    {}초 안에 안 끝남 — 전사가 맞다\n", budget.as_secs()),
    }
}

/// Drop one operation at a time and see which removals let the rectangle return.
///
/// An operation whose removal makes it FINISH is one the wedge needs. Anything
/// that still wedges without it was never part of the cause.
///
/// ⚠ Not `#[ignore]`d for tidiness — it takes minutes and is a reduction tool,
/// not a gate. Run it by hand:
///
/// ```text
///   AXIA_WEDGE_STEP_SECS=45 cargo test --release -p axia-core \
///     --test a_fuzz_session_that_never_finishes which_operations -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a reduction sweep, minutes long — run by hand"]
fn which_operations_the_wedge_needs() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(45),
    );
    println!("\n  하나씩 빼기 — 끝나면 그 연산이 원인에 필요하다 ({}초 예산)\n", budget.as_secs());
    let mut needed: Vec<usize> = Vec::new();
    for i in 0..S22_OPS.len() {
        let verdict = match s22_time(&[i], budget) {
            Some(secs) => {
                needed.push(i);
                format!("끝남 {secs:>7.3} 초   <- 필요")
            }
            None => "여전히 멈춤".to_string(),
        };
        println!("    {i:>2} 빼기  {:<40} {verdict}", format!("{:?}", S22_OPS[i]));
    }
    println!("\n  필요한 연산: {needed:?}\n");
}

/// How big is the arrangement the last draw has to build?
///
/// The drop-one sweep says almost every operation is necessary — take any one
/// away and the rectangle returns in well under a second. That is a CLIFF, not
/// a culprit: some quantity grows explosively with what is already on the plane
/// and this scene sits just past the knee.
///
/// `arrange` splits every input curve at every crossing and walks the resulting
/// planar graph, so its size is driven by the number of crossings. This counts
/// them, for the full scene and for the scene with one operation removed.
#[test]
#[ignore = "counts every pair on the plane — run by hand"]
fn how_many_crossings_the_arrangement_has_to_carry() {
    use axia_geo::curves::AnalyticCurve;

    fn crossings_on_plane(dropped: &[usize], plane_z: f64) -> (usize, usize, f64) {
        let mut s = prod();
        for (i, op) in S22_OPS.iter().enumerate() {
            if dropped.contains(&i) {
                continue;
            }
            play(&mut s, *op);
        }
        let tol = 1e-5;
        let on_plane = |p: DVec3| (p.z - plane_z).abs() < 1e-3;
        // Every coplanar curve, as `arrange` would receive it.
        let mut curves: Vec<AnalyticCurve> = Vec::new();
        for (eid, e) in s.mesh.edges.iter() {
            if !e.is_active() {
                continue;
            }
            let (Some(a), Some(b)) = (
                s.mesh.verts.get(e.v_small()).map(|v| v.pos()),
                s.mesh.verts.get(e.v_large()).map(|v| v.pos()),
            ) else {
                continue;
            };
            if !(on_plane(a) && on_plane(b)) {
                continue;
            }
            match s.mesh.edge_curve(eid) {
                Some(c) => curves.push(c.clone()),
                None => curves.push(AnalyticCurve::Bezier { control_pts: vec![a, b] }),
            }
        }
        let t0 = std::time::Instant::now();
        let mut hits = 0usize;
        for i in 0..curves.len() {
            for j in (i + 1)..curves.len() {
                hits += axia_geo::curves::intersect::intersect_curves(
                    &curves[i], &curves[j], &s.mesh, tol,
                )
                .map(|v| v.len())
                .unwrap_or(0);
            }
        }
        (curves.len(), hits, t0.elapsed().as_secs_f64())
    }

    println!("\n  z = 200 평면 — arrange 가 짊어지는 크기\n");
    println!("    빠진 것      곡선    교차점    모든 쌍 검사");
    for (label, dropped) in [
        ("없음", vec![]),
        ("13 (pushIn)", vec![13usize]),
        ("11 (원 z=0)", vec![11usize]),
        ("12 (타원 z=200)", vec![12usize]),
        ("3 (입체)", vec![3usize]),
    ] {
        let (n, hits, secs) = crossings_on_plane(&dropped, 200.0);
        println!("    {label:<16} {n:>4}   {hits:>7}   {secs:>8.3} 초");
    }
    println!();
}

// ── Inside `arrange` ─────────────────────────────────────────────────────────
//
// Every pair on the plane intersects in 0.063 s total, so the cost is
// downstream of the crossings. `arrange` is public, and so are `InputCurve` and
// `Freeform2D`, so the plane's curves can be handed to it directly. If it wedges
// there, the boundary kernel owns this; if it returns, the cost is in what the
// re-derive does with the result.

/// Build the plane's curves as `arrange` receives them.
///
/// ⚠ Close to, not identical with, `collect_input_curves` — that is private and
/// also merges collinear line segments and de-duplicates circles. This keeps
/// every edge, so if anything it hands `arrange` MORE than production does.
fn plane_input_curves(s: &Scene, plane_z: f64) -> Vec<axia_geo::boundary_kernel::InputCurve> {
    use axia_geo::boundary_kernel::{Freeform2D, InputCurve, Vec2};
    use axia_geo::curves::AnalyticCurve;

    let on_plane = |p: DVec3| (p.z - plane_z).abs() < 1e-3;
    let flat = |p: DVec3| Vec2::new(p.x, p.y);
    let mut out: Vec<InputCurve> = Vec::new();
    let mut freeform_seen: Vec<u32> = Vec::new();

    for (eid, e) in s.mesh.edges.iter() {
        if !e.is_active() {
            continue;
        }
        let (Some(a), Some(b)) = (
            s.mesh.verts.get(e.v_small()).map(|v| v.pos()),
            s.mesh.verts.get(e.v_large()).map(|v| v.pos()),
        ) else {
            continue;
        };
        if !(on_plane(a) && on_plane(b)) {
            continue;
        }
        // A freeform fragment restores its whole original, once per owner.
        if let Some(owner) = e.curve_owner_id() {
            if let Some(src) = s.mesh.freeform_curve_source(owner) {
                if freeform_seen.contains(&owner) {
                    continue;
                }
                freeform_seen.push(owner);
                let ff = match src {
                    AnalyticCurve::Bezier { control_pts } => {
                        Some(Freeform2D::bezier(control_pts.iter().map(|p| flat(*p)).collect()))
                    }
                    AnalyticCurve::BSpline { control_pts, knots, degree } => Some(Freeform2D::bspline(
                        control_pts.iter().map(|p| flat(*p)).collect(),
                        knots.clone(),
                        *degree,
                    )),
                    AnalyticCurve::NURBS { control_pts, weights, knots, degree } => {
                        Some(Freeform2D::nurbs(
                            control_pts.iter().map(|p| flat(*p)).collect(),
                            weights.clone(),
                            knots.clone(),
                            *degree,
                        ))
                    }
                    _ => None,
                };
                if let Some(ff) = ff {
                    out.push(InputCurve::Freeform(ff.with_owner(Some(owner))));
                }
                continue;
            }
        }
        match s.mesh.edge_curve(eid) {
            Some(AnalyticCurve::Circle { center, radius, .. }) => {
                out.push(InputCurve::Circle { center: flat(*center), radius: *radius })
            }
            Some(AnalyticCurve::Arc { center, radius, start_angle, end_angle, .. }) => {
                out.push(InputCurve::Arc {
                    center: flat(*center),
                    radius: *radius,
                    a0: *start_angle,
                    a1: *end_angle,
                })
            }
            _ => out.push(InputCurve::Line { a: flat(a), b: flat(b) }),
        }
    }
    out
}

/// Does `arrange` itself wedge on this plane?
///
/// Time-boxed on its own thread — the point of the question is that it might
/// not come back.
#[test]
#[ignore = "may not return — run by hand under a timeout"]
fn does_arrange_itself_wedge() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
    );
    println!("\n  arrange 를 직접 호출 ({}초 예산)\n", budget.as_secs());

    for (label, dropped) in [("없음", vec![]), ("11 (원 z=0)", vec![11usize])] {
        let dropped2 = dropped.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut s = prod();
            for (i, op) in S22_OPS.iter().enumerate() {
                if dropped2.contains(&i) {
                    continue;
                }
                play(&mut s, *op);
            }
            // The rectangle's own four edges, as the draw would add them.
            let curves = plane_input_curves(&s, 200.0);
            let n = curves.len();
            let t0 = std::time::Instant::now();
            let faces = axia_geo::boundary_kernel::arrange(&curves, 1e-4);
            let _ = tx.send((n, faces.len(), t0.elapsed().as_secs_f64()));
        });
        match rx.recv_timeout(budget) {
            Ok((n, f, secs)) => {
                println!("    {label:<14} 곡선 {n:>3}  ->  면 {f:>4}   {secs:>8.3} 초");
            }
            Err(_) => {
                println!("    {label:<14} {}초 안에 안 끝남  <- arrange 안에서 멈춘다\n", budget.as_secs());
                return;
            }
        }
    }
    println!("\n  arrange 는 둘 다 끝난다 — 비용은 그 뒤에 있다\n");
}

/// What the re-derive's own rollback guard costs on this scene.
///
/// `rebuild_coplanar_faces_analytic_scoped` clones the whole mesh and runs
/// `detect_self_intersections` and `verify_face_invariants` BEFORE the rebuild,
/// then again after, so it can put the mesh back if the work made things worse.
/// That is four scans of the whole model per plane, and `arrange` itself was
/// only 0.062 s — so the guard is the next thing worth a stopwatch.
#[test]
#[ignore = "a stopwatch, not a gate — run by hand"]
fn what_the_rollback_guard_costs() {
    let mut s = prod();
    for op in S22_OPS {
        play(&mut s, *op);
    }
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let edges = s.mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
    println!("\n  op 17 까지의 장면 — 면 {faces}, 엣지 {edges}\n");

    let t0 = std::time::Instant::now();
    let clone = s.mesh.clone();
    println!("    mesh.clone()                  {:>9.3} 초", t0.elapsed().as_secs_f64());
    drop(clone);

    let t0 = std::time::Instant::now();
    let si = s.mesh.detect_self_intersections().count();
    println!(
        "    detect_self_intersections     {:>9.3} 초   ({si} 건)",
        t0.elapsed().as_secs_f64()
    );

    let t0 = std::time::Instant::now();
    let inv = s.mesh.verify_face_invariants().violations.len();
    println!(
        "    verify_face_invariants        {:>9.3} 초   ({inv} 건)",
        t0.elapsed().as_secs_f64()
    );

    println!("\n    가드는 이걸 앞뒤로 두 번씩 돈다 — 평면마다.\n");
}

/// The re-derive alone, with the freeform gate on and off.
///
/// Every measured piece is fast — intersections 0.063 s, `arrange` 0.062 s, the
/// rollback guard 0.019 s on a scene of 41 faces and 110 edges. So the draw is
/// not spending its time in any of them, and the flag bisection says the
/// freeform gate decides. The gate only changes what the re-derive DOES, so
/// call the re-derive directly, both ways, and see what it leaves behind.
///
/// After it, the draw runs `split_faces_crossing_other_planes` over everything
/// new. If the freeform path leaves far more faces, that is where the bill goes.
#[test]
#[ignore = "may not return with the gate on — run by hand under a timeout"]
fn what_the_re_derive_leaves_behind_with_the_gate_on_and_off() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
    );
    println!("\n  재유도만 직접 호출 ({}초 예산)\n", budget.as_secs());
    println!("    freeform   면 전 -> 면 후    엣지 후    시간");

    for gate in [false, true] {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut s = prod();
            for op in S22_OPS {
                play(&mut s, *op);
            }
            // ⚠ The rectangle has to go in WITHOUT the draw pipeline. A first
            // version called `play(S22_WEDGE)` here and the thread never
            // reached the line below — that call IS the wedge, so the timeout
            // measured the pipeline again and said nothing about the re-derive.
            s.face_rederive_on_draw = false;
            s.auto_intersect_on_draw = false;
            s.auto_face_synthesis_on_draw = false;
            play(&mut s, S22_WEDGE);
            let before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
            let t0 = std::time::Instant::now();
            let r = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
                &mut s.mesh,
                DVec3::new(0.0, 0.0, 200.0),
                DVec3::Z,
                1e-3,
                gate,
                None,
            );
            let secs = t0.elapsed().as_secs_f64();
            let after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
            let edges = s.mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
            let _ = tx.send((before, after, edges, secs, r.is_ok()));
        });
        match rx.recv_timeout(budget) {
            Ok((b, a, e, secs, ok)) => println!(
                "    {:<10} {b:>5} -> {a:<6}  {e:>6}    {secs:>8.3} 초  {}",
                gate,
                if ok { "" } else { "(Err)" }
            ),
            Err(_) => {
                println!("    {:<10} {}초 안에 안 끝남  <- 재유도 자체가 멈춘다\n", gate, budget.as_secs());
                return;
            }
        }
    }
    println!("\n  재유도는 양쪽 다 끝난다 — 비용은 그 다음 패스에 있다\n");
}

/// The last stage, timed.
///
/// Everything else has a number now:
///
/// ```text
///   all pairwise intersections on the plane   0.063 s
///   arrange                                   0.062 s
///   the re-derive (gate on OR off)            0.209 s
///   the rollback guard's four scans           0.019 s
/// ```
///
/// and the draw takes over fifteen minutes. `split_faces_crossing_other_planes`
/// is the only stage of that draw left unmeasured — it runs immediately after
/// the re-derive, over every face that did not exist before.
#[test]
#[ignore = "may not return — run by hand under a timeout"]
fn what_the_crossing_split_costs() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120),
    );
    println!("\n  마지막 단계 — split_faces_crossing_other_planes ({}초 예산)\n", budget.as_secs());

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = prod();
        for op in S22_OPS {
            play(&mut s, *op);
        }
        // The rectangle, without the draw pipeline that wedges.
        s.face_rederive_on_draw = false;
        s.auto_intersect_on_draw = false;
        s.auto_face_synthesis_on_draw = false;
        play(&mut s, S22_WEDGE);
        let before: std::collections::HashSet<axia_geo::FaceId> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(i, _)| i)
            .collect();
        // The re-derive the draw would have run, timed already at 0.209 s.
        let _ = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
            &mut s.mesh, DVec3::new(0.0, 0.0, 200.0), DVec3::Z, 1e-3, true, None,
        );
        let n_before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let t0 = std::time::Instant::now();
        let split = s.split_faces_crossing_other_planes(&before);
        let secs = t0.elapsed().as_secs_f64();
        let n_after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let _ = tx.send((n_before, n_after, split.unwrap_or(0), secs));
    });
    match rx.recv_timeout(budget) {
        Ok((b, a, split, secs)) => {
            println!("    면 {b} -> {a}   분할 {split}   {secs:>8.3} 초");
            println!("\n  이 단계도 끝난다 — 남은 곳은 draw 의 다른 갈래다\n");
        }
        Err(_) => println!(
            "    {}초 안에 안 끝남  <- 여기가 멈춤\n",
            budget.as_secs()
        ),
    }
}

/// How many faces the crossing split hands to `intersect_faces_with_model`.
///
/// The stage that wedges calls one thing: `Mesh::intersect_faces_with_model`,
/// given the faces `detect_self_intersections` reported as crossing another
/// plane. A small input means the loop is inside that call; a large one means
/// the pass is asking for too much.
#[test]
#[ignore = "counts the crossing set — run by hand"]
fn how_many_faces_the_crossing_split_is_given() {
    let mut s = prod();
    for op in S22_OPS {
        play(&mut s, *op);
    }
    s.face_rederive_on_draw = false;
    s.auto_intersect_on_draw = false;
    s.auto_face_synthesis_on_draw = false;
    play(&mut s, S22_WEDGE);
    let _ = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        &mut s.mesh, DVec3::new(0.0, 0.0, 200.0), DVec3::Z, 1e-3, true, None,
    );

    let t0 = std::time::Instant::now();
    let si = s.mesh.detect_self_intersections();
    let pairs = si.intersecting_pairs.len();
    let secs = t0.elapsed().as_secs_f64();
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("\n  교차 분할이 받는 것\n");
    println!("    살아 있는 면       {faces}");
    println!("    자기교차 쌍        {pairs}   ({secs:.3} 초에 검출)");
    println!("\n    -> 이 쌍에서 고른 면이 intersect_faces_with_model 로 간다\n");
}

// ── Inside `intersect_faces_with_model` ──────────────────────────────────────
//
// It is public, so the input can be bisected. Its body is a straight line —
// `prepare_solid` twice, `find_intersections`, `split_faces_by_intersections`
// twice — so a single face that wedges on its own names the pair, and from
// there the loop is one function away.

/// Rebuild the scene the crossing split works on, and hand back the faces it
/// would pass to `intersect_faces_with_model`.
fn crossing_candidates() -> (Scene, Vec<axia_geo::FaceId>) {
    let mut s = prod();
    for op in S22_OPS {
        play(&mut s, *op);
    }
    s.face_rederive_on_draw = false;
    s.auto_intersect_on_draw = false;
    s.auto_face_synthesis_on_draw = false;
    play(&mut s, S22_WEDGE);
    let _ = axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        &mut s.mesh, DVec3::new(0.0, 0.0, 200.0), DVec3::Z, 1e-3, true, None,
    );
    // The same choice the crossing split makes: one side of each crossing pair.
    let pairs = s.mesh.detect_self_intersections().intersecting_pairs;
    let mut crossing: Vec<axia_geo::FaceId> = Vec::new();
    for (fa, fb) in pairs {
        let (Some(a), Some(b)) = (s.mesh.faces.get(fa), s.mesh.faces.get(fb)) else { continue };
        if !a.is_active() || !b.is_active() {
            continue;
        }
        let na = a.normal().normalize_or_zero();
        let nb = b.normal().normalize_or_zero();
        // Coplanar pairs belong to the re-derive, not here.
        if na.dot(nb).abs() > 0.999 {
            continue;
        }
        if !crossing.contains(&fa) && !crossing.contains(&fb) {
            crossing.push(fa);
        }
    }
    (s, crossing)
}

/// Which face, on its own, makes `intersect_faces_with_model` never return.
///
/// The whole crossing set wedges. Each face is handed over alone, so the answer
/// is one face id and the triangle counts that go with it.
#[test]
#[ignore = "several of these may not return — run by hand under a timeout"]
fn which_single_face_wedges_the_intersect() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30),
    );
    let (s, crossing) = crossing_candidates();
    println!("\n  교차 분할이 넘기는 면 {} 개 ({}초 예산)\n", crossing.len(), budget.as_secs());

    // How big is each one, as triangles?
    let tri_count = |s: &Scene, f: axia_geo::FaceId| -> usize {
        s.mesh
            .faces
            .get(f)
            .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
            .map(|v| v.len().saturating_sub(2))
            .unwrap_or(0)
    };
    let others_tris: usize = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| tri_count(&s, fid))
        .sum();
    println!("    모델 전체 삼각형 대략 {others_tris}\n");

    for &f in &crossing {
        let tris = tri_count(&s, f);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s2, c2) = crossing_candidates();
            let target = c2.iter().copied().find(|x| *x == f);
            let Some(target) = target else {
                let _ = tx.send(None);
                return;
            };
            let t0 = std::time::Instant::now();
            let r = s2.mesh.intersect_faces_with_model(&[target], FORM_MATERIAL);
            let _ = tx.send(Some((t0.elapsed().as_secs_f64(), r.map(|v| v.len()).unwrap_or(0))));
        });
        match rx.recv_timeout(budget) {
            Ok(Some((secs, n))) => println!("    {f:?}  삼각형 {tris:>3}   {secs:>8.3} 초   결과 면 {n}"),
            Ok(None) => println!("    {f:?}  (재구성에서 사라짐)"),
            Err(_) => println!("    {f:?}  삼각형 {tris:>3}   {}초 안에 안 끝남  <- 이 면", budget.as_secs()),
        }
    }
    println!();
}

/// Where the set tips over.
///
/// Each of the eight crossing faces finishes on its own in milliseconds. The
/// eight together do not return. The function's body has no loop over
/// `selected` — it triangulates, finds intersections, splits — so growing the
/// set one face at a time says which addition costs what.
#[test]
#[ignore = "the larger sets may not return — run by hand under a timeout"]
fn where_the_crossing_set_tips_over() {
    let budget = std::time::Duration::from_secs(
        std::env::var("AXIA_WEDGE_STEP_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30),
    );
    let (_, crossing) = crossing_candidates();
    println!("\n  집합을 키우며 ({}초 예산)\n", budget.as_secs());
    for k in 1..=crossing.len() {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut s, c) = crossing_candidates();
            let take: Vec<axia_geo::FaceId> = c.into_iter().take(k).collect();
            let t0 = std::time::Instant::now();
            let r = s.mesh.intersect_faces_with_model(&take, FORM_MATERIAL);
            let _ = tx.send((t0.elapsed().as_secs_f64(), r.map(|v| v.len()).unwrap_or(0)));
        });
        match rx.recv_timeout(budget) {
            Ok((secs, n)) => println!("    앞 {k} 개   {secs:>9.3} 초   결과 면 {n}"),
            Err(_) => {
                println!("    앞 {k} 개   {}초 안에 안 끝남  <- 여기서 넘어간다\n", budget.as_secs());
                return;
            }
        }
    }
    println!("\n  여덟 개 전부 끝난다 — 넘기는 순서가 다른 것이다\n");
}

/// What the fifth face is, and what the 436 faces are made of.
///
/// Four faces cost a millisecond and leave 61. Adding the fifth costs 26
/// seconds and leaves 436 — nine times the model it started from. Either that
/// is a lot of real division, or the split is producing slivers.
#[test]
#[ignore = "runs the 26-second call — by hand"]
fn what_the_fifth_face_does() {
    let (s, crossing) = crossing_candidates();
    let fifth = crossing[4];
    println!("\n  다섯 번째 면 = {fifth:?}\n");
    if let Some(f) = s.mesh.faces.get(fifth) {
        let vs = s.mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
        let pts: Vec<DVec3> = vs.iter().filter_map(|v| s.mesh.verts.get(*v).map(|x| x.pos())).collect();
        let n = f.normal().normalize_or_zero();
        println!("    법선 ({:.3}, {:.3}, {:.3})   정점 {}", n.x, n.y, n.z, pts.len());
        for p in &pts {
            println!("      ({:>8.2}, {:>8.2}, {:>8.2})", p.x, p.y, p.z);
        }
        println!("    넓이 {:.1}", s.mesh.face_area(fifth));
    }

    // Run the five and look at what comes out.
    let (mut s2, c2) = crossing_candidates();
    let take: Vec<axia_geo::FaceId> = c2.into_iter().take(5).collect();
    let before = s2.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let t0 = std::time::Instant::now();
    let _ = s2.mesh.intersect_faces_with_model(&take, FORM_MATERIAL);
    let secs = t0.elapsed().as_secs_f64();

    let mut areas: Vec<f64> = s2
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| s2.mesh.face_area(fid))
        .collect();
    areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = areas.len();
    let slivers = areas.iter().filter(|a| **a < 1.0).count();
    let tiny = areas.iter().filter(|a| **a < 100.0).count();
    println!("\n    면 {before} -> {n}   ({secs:.1} 초)");
    println!("    넓이 < 1      {slivers}");
    println!("    넓이 < 100    {tiny}");
    println!(
        "    가장 작은 다섯: {:?}",
        areas.iter().take(5).map(|a| (a * 1000.0).round() / 1000.0).collect::<Vec<_>>()
    );
    println!(
        "    가장 큰 셋:     {:?}\n",
        areas.iter().rev().take(3).map(|a| a.round()).collect::<Vec<_>>()
    );
}

/// Where the vertices go.
///
/// The five-face call leaves faces of area 1.38e9 in a model whose largest was
/// 21,600. sqrt(1.38e9) is about 37,000 mm — thirty-seven metres, in a scene
/// 400 mm across. Something is computing a point far away, and the likeliest
/// source is an intersection line between two nearly PARALLEL planes, where the
/// denominator goes to zero and the point flies off.
#[test]
#[ignore = "runs the 23-second call — by hand"]
fn where_the_vertices_go() {
    let (s0, _) = crossing_candidates();
    let extent = |s: &Scene| -> (f64, f64, usize) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut nonfinite = 0usize;
        for (_, v) in s.mesh.verts.iter() {
            if !v.is_active() {
                continue;
            }
            let p = v.pos();
            if !p.is_finite() {
                nonfinite += 1;
                continue;
            }
            for c in [p.x, p.y, p.z] {
                lo = lo.min(c);
                hi = hi.max(c);
            }
        }
        (lo, hi, nonfinite)
    };
    let (lo0, hi0, nf0) = extent(&s0);
    println!("\n  자르기 전   좌표 [{lo0:.1}, {hi0:.1}]   비유한 {nf0}");

    let (mut s, c) = crossing_candidates();
    let take: Vec<axia_geo::FaceId> = c.into_iter().take(5).collect();
    let _ = s.mesh.intersect_faces_with_model(&take, FORM_MATERIAL);
    let (lo, hi, nf) = extent(&s);
    println!("  자른 뒤     좌표 [{lo:.1}, {hi:.1}]   비유한 {nf}\n");

    assert!(
        hi - lo < 10_000.0 && nf == 0,
        "a scene 400 mm across must not grow to [{lo:.1}, {hi:.1}] with {nf} \
         non-finite vertices — the intersection is computing points that are \
         not on either face"
    );
}

/// What the 450 faces actually are.
///
/// Coordinates do not move — [-310, 320] before and after, no non-finite
/// vertex. So an area of 1.38e9 in a scene 630 mm across cannot come from a far
/// away point; it comes from a LOOP that does not enclose what it claims. This
/// counts the shapes of the loops that come out.
#[test]
#[ignore = "runs the 23-second call — by hand"]
fn what_shape_the_new_faces_are() {
    let (mut s, c) = crossing_candidates();
    let take: Vec<axia_geo::FaceId> = c.into_iter().take(5).collect();
    let before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let _ = s.mesh.intersect_faces_with_model(&take, FORM_MATERIAL);

    let mut repeats = 0usize;
    let mut longest = 0usize;
    let mut unreadable = 0usize;
    let mut worst: Option<(axia_geo::FaceId, usize, usize, f64)> = None;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let Ok(vs) = s.mesh.collect_loop_verts(f.outer().start) else {
            unreadable += 1;
            continue;
        };
        let mut seen = std::collections::HashSet::new();
        let dups = vs.iter().filter(|v| !seen.insert(**v)).count();
        longest = longest.max(vs.len());
        if dups > 0 {
            repeats += 1;
            let area = s.mesh.face_area(fid);
            if worst.map(|(_, _, d, _)| dups > d).unwrap_or(true) {
                worst = Some((fid, vs.len(), dups, area));
            }
        }
    }
    let after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let inv = s.mesh.verify_face_invariants().violations.len();

    println!("\n  다섯 면을 자른 뒤\n");
    println!("    면            {before} -> {after}");
    println!("    루프가 정점을 두 번 지나는 면   {repeats}");
    println!("    읽을 수 없는 루프              {unreadable}");
    println!("    가장 긴 루프                   {longest} 정점");
    println!("    불변식 위반                    {inv}");
    if let Some((fid, n, d, area)) = worst {
        println!("\n    최악: {fid:?}  {n} 정점, 중복 {d}, 넓이 {area:.0}");
    }
    println!();

    // The same crossing was being recorded twice.
    //
    // A cut through a polygon CORNER is found on both edges that meet there, and
    // `pair_intersection_points` paired the duplicates sequentially — so one
    // chord became two "pairs", and the walk emits one sub-polygon per
    // intersection POINT. Every phantom pair cost two more overlapping pieces,
    // and where pieces overlap the ring comes back to a vertex it already
    // passed.
    //
    // Collapsing coincident points before pairing:
    //
    // ```text
    //                      was    is
    //   faces out          450    58
    //   walking a vertex   386     2
    //   invariant breaks    70     8
    //   the five-face call  23 s   0.1 s
    // ```
    //
    // ⚠ An earlier attempt split each self-touching ring at its touches instead
    // — the fix `polygon_difference_by_clip` needed in `operations/coplanar.rs`.
    // It took the repeats to zero and the faces to 1920, because a corrupt ring
    // cut in two is two rings. Treating the symptom multiplied it; the duplicate
    // crossings were the disease.
    //
    // ⚠ A second attempt refused any cut with more than one entry/exit pair, on
    // the grounds that `split_polygon_2d_one_line` says "one line" in its name.
    // With the dedup in place that buys five fewer invariant violations and
    // costs every genuine multi-crossing split, so it is not here.
    assert!(
        repeats < 20 && after < 2 * before,
        "a five-face intersect leaves {after} faces from {before}, {repeats} of          them walking a vertex twice — it used to be 450 and 386"
    );
}
