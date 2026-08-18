//! A fuzz session that never finishes — and what it is spinning in.
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
