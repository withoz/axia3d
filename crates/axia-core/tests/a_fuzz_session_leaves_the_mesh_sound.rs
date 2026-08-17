//! Stage 5: the combinations nobody imagined.
//!
//! Every grid in this repo is a list somebody wrote down — four hosts by three
//! placements by two shapes, four planes by eight tools, twelve operations on
//! an owned solid. They catch what was thought of. This runs sequences that
//! nobody thought of, and the only thing it asks is the engine's own contract:
//! after every single operation, do the face invariants still hold?
//!
//! Deterministic, so a failure is a bug report rather than a rumour. The
//! generator is a plain LCG seeded per session; a failing session prints its
//! seed and the exact operation list, and re-running that seed replays it
//! exactly. No `rand`, no clock, nothing that changes between runs.
//!
//! ⚠ The log prints every parameter, not just the position. It used to print
//! a circle as `circle(x,y,z,ok)` — no radius — and a report you cannot re-type
//! is a rumour with coordinates. Found by trying to transcribe session 6 into a
//! standalone repro and getting a scene that did not fail.
//!
//! Size: 12 sessions × 20 operations by default, which is what fits a test
//! suite. `AXIA_FUZZ_SESSIONS` and `AXIA_FUZZ_OPS` raise it — the plan's
//! 100 × 50 is `AXIA_FUZZ_SESSIONS=100 AXIA_FUZZ_OPS=50`, and it is worth
//! running by hand before a release rather than on every commit.
//!
//! ⚠ The bar is `verify_face_invariants`, not `damaging_contacts`. Two solids
//! standing in the same place is a legal model — a user may well want it — and
//! failing a fuzz session for it would be failing it for a picture, not a
//! defect. Damage is COUNTED and printed so a run says what it saw, and the
//! assertion is on the manifold contract alone.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

/// The same generator every time, from a seed.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        // Numerical Recipes' constants — any full-period LCG does; what matters
        // is that it is ours and does not move.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n.max(1)
    }
    /// A coordinate on a coarse grid, so shapes actually meet each other.
    fn coord(&mut self) -> f64 {
        (self.below(9) as f64 - 4.0) * 50.0
    }
    fn size(&mut self) -> f64 {
        60.0 + self.below(5) as f64 * 40.0
    }
}

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn active_faces(s: &Scene) -> Vec<FaceId> {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect()
}

/// One operation, chosen by the generator, described so a failure can be read.
fn step(s: &mut Scene, r: &mut Lcg) -> String {
    // Draw at a height an existing face might be at, so draws land ON things
    // as often as beside them.
    let z = [0.0, 100.0, 200.0][r.below(3)];
    let (x, y) = (r.coord(), r.coord());
    let c = DVec3::new(x, y, z);
    let w = r.size();

    match r.below(10) {
        0 => {
            let cmd = Command::DrawRectAsShape {
                center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75,
            };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("rect({x},{y},{z},{w}){}", if e { " REFUSED" } else { "" })
        }
        1 => {
            let cmd = Command::DrawCircleAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24,
            };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("circle({x},{y},{z},r={}){}", w * 0.5, if e { " REFUSED" } else { "" })
        }
        2 => {
            let cmd = Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: w * 0.5 };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("circleCurve({x},{y},{z},r={}){}", w * 0.5, if e { " REFUSED" } else { "" })
        }
        3 => {
            let cmd = Command::DrawEllipseAsCurve {
                center: c, ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: w * 0.5, radius_y: w * 0.3,
            };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("ellipse({x},{y},{z},rx={},ry={}){}", w * 0.5, w * 0.3, if e { " REFUSED" } else { "" })
        }
        4 => {
            let sides = 3 + r.below(5) as u32;
            let cmd = Command::DrawPolygonAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, sides,
            };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("polygon({x},{y},{z},r={},n={}){}", w * 0.5, sides, if e { " REFUSED" } else { "" })
        }
        5 => {
            // A line across whatever is there — one of the paths the draw gate
            // does NOT wrap.
            let cmd = Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            };
            let e = matches!(s.execute(cmd), CommandResult::Error(_));
            format!("line({x},{y},{z},len=300){}", if e { " REFUSED" } else { "" })
        }
        6 => {
            let faces = active_faces(s);
            if faces.is_empty() {
                return "extrude(skip: nothing there)".into();
            }
            let f = faces[r.below(faces.len())];
            let d = 50.0 + r.below(4) as f64 * 50.0;
            let e = matches!(
                s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } }),
                CommandResult::Error(_)
            );
            format!("extrude({f:?},{d}){}", if e { " REFUSED" } else { "" })
        }
        7 => {
            // Push a face IN. Held out of the generator while seeds 6 and 10
            // were the only reproductions of the two breaks then in
            // KNOWN_BREAKS; both are fixed and pinned in their own files
            // (`a_face_naming_a_gone_half_edge.rs`,
            // `a_face_whose_normal_faces_the_other_way.rs`), so the fuzz no
            // longer has to carry them and this came back.
            //
            // It brought two breaks with it, which is the whole reason to want
            // it back — they are inventoried above, reduced to 6 and 4
            // operations, and measured apart rather than lumped together.
            let faces = active_faces(s);
            if faces.is_empty() {
                return "pushIn(skip: nothing there)".into();
            }
            let f = faces[r.below(faces.len())];
            let d = -(50.0 + r.below(3) as f64 * 50.0);
            let e = matches!(
                s.execute(Command::CreateSolid {
                    face_id: f,
                    mode: CreateSolidMode::Extrude { distance: d },
                }),
                CommandResult::Error(_)
            );
            format!("pushIn({f:?},{d}){}", if e { " REFUSED" } else { "" })
        }
        8 => {
            // A box, so a solid exists without needing a draw to succeed first.
            let ok = s
                .mesh
                .create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL)
                .is_ok();
            format!("box({x},{y},{z},{w},{})", if ok { "ok" } else { "FAILED" })
        }
        _ => {
            // Punch a hole in whatever is on top there.
            let a = DVec3::new(x - 30.0, y - 30.0, z);
            let b = DVec3::new(x + 30.0, y + 30.0, z);
            let ok = s.punch_rect_hole(a, b, DVec3::Z).is_ok();
            format!("punch({x},{y},{z},{})", if ok { "ok" } else { "refused" })
        }
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Sessions that break TODAY, with what they break on.
///
/// Not a list of excuses — an inventory, and the test below asserts it in both
/// directions: a session that is not here must stay sound, and a session that
/// IS here must still fail. Fix one and this test tells you to strike it from
/// the list, which is how the harness gets stricter on its own instead of
/// rotting into a set of permanently-tolerated failures.
const KNOWN_BREAKS: &[(u64, &str)] = &[
    // Both entries came back with push-in, and both are reduced and pinned in
    // `pushing_in_leaves_faces_on_top_of_each_other.rs` — 15 operations to 6 and
    // 7 to 4 — with the measurement that they are NOT one defect:
    //
    //   session 9 breaks with every automatic behaviour turned OFF, so the
    //   arrangement is not making these faces. A circle drawn on a plane a box's
    //   bottom already occupies replaces that bottom with a 47-vertex sheet that
    //   does not reuse the box's own boundary edges. The two walls standing on
    //   those edges lose their neighbour and read as sheets, so pushing one is
    //   not move-only and `create_solid` extrudes a second box onto the first.
    //
    //   session 10 needs face-synthesis AND the re-derive; either off and it is
    //   sound. The circle reaches two solid perimeters at once, which the
    //   re-tile deliberately declines to carry (see `face_rederive.rs`), and the
    //   synthesis then lays a sheet over the solid's top face anyway.
    // Session 9 was here — a wall of an opened box, pushed in and extruded
    // instead of moved — and is struck because it is fixed. The double-cover
    // repair in `guard_imprint` was reshaping a SOLID's face: it re-walks a
    // boundary, and the walls standing on the old edges lose their neighbour,
    // so the face that comes back is a free sheet and the box is open. The
    // repair is sheets-only now; solids have the coplanar re-tile. Two
    // operations reproduce the old behaviour in
    // `a_draw_on_a_solids_face_must_not_open_it.rs`.
    // Session 10's op-3-of-four break is FIXED — the re-derive runs on a plane a
    // solid shares and is undone only if it made things worse, and the post-draw
    // repair is undone when it leaves the plane objecting. Its four-operation
    // repro is a guard now (`pushing_in_three_deep_stacks_faces.rs`), and so is
    // the comparison that located it.
    //
    // Session 3 takes its place, and it is NOT what the change cost. Reduced to
    // two operations — a box, and a square drawn at a height inside it — which
    // break the same way on main with the rollbacks nowhere in sight
    // (`session_3_when_the_rollbacks_are_in.rs`). The fuzz's operations depend on
    // the mesh, so a change to how many faces a draw makes shifts every later
    // one; session 3's stream moved onto a defect that was already there.
    (3, "op 1 of two: a square drawn at a height INSIDE a box leaves           EdgeId(22) bearing three faces. Pre-existing; the fuzz reached it           when the stream shifted."),
    // Session 10's four-operation reduction is SOUND now, and is kept as a
    // guard. The full twenty still stops at op 11 on the same edge, so the
    // reduction was not the whole story — the longer sequence reaches a
    // configuration the shorter one does not, and that has not been reduced.
    (10, "op 11, three pushes in: edge EdgeId(69) bears four faces. Its           four-operation reduction is fixed; this longer one is not."),
];

fn run_session(seed_index: u64, ops: usize) -> Result<(usize, usize), (usize, String, Vec<String>)> {
    let seed = 0x5EED_0000_u64 + seed_index;
    let mut r = Lcg(seed);
    let mut s = prod();
    let mut log: Vec<String> = Vec::new();
    for op in 0..ops {
        log.push(step(&mut s, &mut r));
        let inv = s.mesh.verify_face_invariants();
        if !inv.is_valid() {
            let why = inv
                .violations
                .iter()
                .take(2)
                .map(|v| format!("{v:?}"))
                .collect::<Vec<_>>()
                .join("; ");
            return Err((op, why, log));
        }
    }
    Ok((active_faces(&s).len(), s.mesh.damaging_contacts().len()))
}

#[test]
fn a_fuzz_session_leaves_the_mesh_sound() {
    let sessions = env_usize("AXIA_FUZZ_SESSIONS", 12);
    let ops = env_usize("AXIA_FUZZ_OPS", 20);
    println!("
퍼즈 — {sessions} 세션 × {ops} 연산 (결정적, seed 재현)
");

    let known: Vec<u64> = KNOWN_BREAKS.iter().map(|(i, _)| *i).collect();
    let mut new_breaks: Vec<String> = Vec::new();
    let mut fixed: Vec<u64> = Vec::new();

    for session in 0..sessions as u64 {
        match run_session(session, ops) {
            Ok((faces, damage)) => {
                println!("  세션 {session:>3}  면 {faces:>4}  손상 {damage:>3}");
                if known.contains(&session) {
                    fixed.push(session);
                }
            }
            Err((op, why, log)) => {
                println!("  세션 {session:>3}  op {op} 에서 위반 — {why}");
                // An inventoried break prints its operations too. A known
                // failure nobody can re-type is not much better than an
                // unknown one, and this is what a repro is transcribed from.
                for (i, l) in log.iter().enumerate() {
                    println!("        {i}: {l}");
                }
                if !known.contains(&session) {
                    new_breaks.push(format!(
                        "session {session} (seed 0x{:X}) at op {op}: {why}
    {}",
                        0x5EED_0000_u64 + session,
                        log.join("
    ")
                    ));
                }
            }
        }
    }

    assert!(
        new_breaks.is_empty(),
        "a session that was sound is not any more:
{}",
        new_breaks.join("
")
    );
    // The other direction, so the inventory cannot rot: a listed session that
    // has started passing means somebody fixed something, and the list has to
    // say so.
    assert!(
        fixed.is_empty(),
        "session(s) {fixed:?} are listed in KNOWN_BREAKS and no longer break —          strike them from the list and say what started working"
    );
}

/// The harness itself: the same seed gives the same sequence.
///
/// Without this a "deterministic" fuzz is a claim rather than a property, and
/// a failure report naming a seed would be useless.
#[test]
fn the_same_seed_replays_exactly() {
    let run = || -> Vec<String> {
        let mut r = Lcg(0xABCD_1234);
        let mut s = prod();
        (0..12).map(|_| step(&mut s, &mut r)).collect()
    };
    let (a, b) = (run(), run());
    println!("  {} ops, first: {}", a.len(), a.first().cloned().unwrap_or_default());
    assert_eq!(a, b, "the same seed must give the same operations");
    // And a different seed gives a different sequence, or the generator is
    // ignoring its seed — which would make every session identical and the
    // whole harness a single test wearing a loop.
    let other = {
        let mut r = Lcg(0x1234_ABCD);
        let mut s = prod();
        (0..12).map(|_| step(&mut s, &mut r)).collect::<Vec<_>>()
    };
    assert_ne!(a, other, "a different seed must give a different session");
}
