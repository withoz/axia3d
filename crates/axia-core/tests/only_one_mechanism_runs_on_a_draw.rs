//! ⚠ CORRECTION — #247 SAID TWO MECHANISMS SHARE THIS PLANE. ONLY ONE RUNS.
//!
//! That commit's message ends: *"the splitter runs on what the user drew, the
//! arrangement runs on what is left, and neither is given the other's result."*
//! The first half is wrong for the configuration that ships. What it measured
//! was the ORDER OF FUNCTION ENTRY —
//!
//! ```text
//!   [ORDER] intersect seeds=[FaceId(27)]
//!   [ORDER] re-derive
//! ```
//!
//! — and `intersect_faces_inner` delegates and returns before it reaches its own
//! coplanar scan:
//!
//! ```rust
//!   if self.face_rederive_on_draw {          // scene.rs:5396, production ON
//!       let coplanar = self.rederive_coplanar_on_draw(face_ids)?;
//!       let crossing = self.split_faces_crossing_other_planes(&before)?;
//!       return Ok(coplanar + crossing);      // 5415
//!   }
//!   ...
//!   auto_intersect_coplanar(...)             // 5598 — never reached
//! ```
//!
//! Measured, with a probe on every branch of that scan: on the draw that leaves
//! the reported damage, **`auto_intersect_coplanar` is not called once**. Not the
//! seed loop, not the candidate loop, not the annulus branches — the whole scan
//! is behind that return. `ADR-186 δ-4b` says so in the code
//! ("flag ON 시 case-by-case auto_intersect/annulus 대신 boundary kernel
//! re-derive"), which is exactly the design; the mistake was mine, reading an
//! entry trace as a call trace.
//!
//! ## Why the correction matters more than the original claim
//!
//! "Two mechanisms that do not know about each other" invites a fifth lever that
//! introduces them. There is nothing to introduce. On a production draw the
//! re-derive is the ONLY mechanism, and the standing-solid footprint is a gap in
//! IT — which is a smaller and more answerable question than the redesign #247
//! pointed at.
//!
//! It also re-reads the four levers: #247's fourth (a second splitter pass after
//! the re-derive) was not "running it again", it was **running it at all**, which
//! is why it cleared a draw's damage (1 -> 0) that nothing else had touched.
//!
//! ⚠ The splitter is NOT dead code — three other call sites read
//! `auto_intersect_on_draw || face_rederive_on_draw` and reach it by other paths.
//! The claim here is scoped to `intersect_faces_inner`'s own coplanar scan.
//!
//! ## The constraint, 2026-09-03 — proved by the fifth lever
//!
//! With #250's correction (only the re-derive runs on a production draw), the
//! narrowest possible lever became obvious: call the splitter after the
//! re-derive on exactly the gap — pairs where one side is in a volume and the
//! other is not. Measured:
//!
//! ```text
//!                       main    sheet-over-solid only
//!   + circle B: damaging   1              0        <- the gap closes
//!   standing damage        3              2
//!   + rect: damaging       0              0        <- and no later draw hurt
//!   + circle C: invariants 0              6        <- but six stacked edges
//! ```
//!
//! And the reason, measured directly on the solid:
//!
//! ```text
//!   solid faces, main        25 -> 25
//!   solid faces, lever 5     25 -> 21     <- four of them gone
//! ```
//!
//! `auto_intersect_coplanar` REMOVES BOTH OPERANDS and adds three faces. Used on
//! a pair that includes a solid's on-plane face, it eats the face the solid's
//! walls stand on — which is exactly what Phase 1's protection exists to prevent,
//! and exactly what widening the filter did (#245). Two levers, one cause.
//!
//! **So the constraint is: whatever divides the sheet must not remove or rebuild
//! the solid's face.** Measured against the engine's dividers —
//!
//! ```text
//!   auto_intersect_coplanar   removes both operands        violates it
//!   the arrangement           rebuilds what it is fed      violates it
//!   split_face_by_line        takes ONE face id            respects it
//!   split_face_by_chain       takes ONE face id            respects it
//! ```
//!
//! ### The sixth shape, and where it stops today
//!
//! Split the SHEET alone, along where the footprint crosses it. Simulated:
//!
//! ```text
//!   split_face_by_line(sheet, crossing0, crossing1)
//!     solid faces 25 -> 25   viol 0      the solid really is untouched
//!     damaging     1  -> 2               and it divides the wrong way
//! ```
//!
//! The boundary between "inside the footprint" and "outside" is the footprint's
//! ARC, not the chord between the crossings, so a straight cut leaves both
//! pieces straddling. Splitting along the sampled arc instead is the right
//! shape and stops one step earlier:
//!
//! ```text
//!   split_face_by_chain(sheet, arc_samples)
//!     Err("chain start vert 49 not on any loop of face 29")
//! ```
//!
//! The chain's endpoints have to BE vertices of the face's loop, and the
//! crossings are mid-edge. Making them vertices means splitting the sheet's
//! boundary edges there — and the sheet is a kernel-native closed curve, so
//! that polygonises it. ⚠ That is an ADR-189 cost, but a much smaller one than
//! the levers before it: only the drawn sheet loses its analytic rim, and the
//! solid keeps everything.
//!
//! That is the first shape that respects the constraint. It is an
//! implementation, not a lever, and it is not built.

//!
//! ## The sixth shape, built and simulated end to end (2026-09-03)
//!
//! Split the SHEET alone, along where the footprint crosses it, then drop the
//! piece that sits on the solid's ground. Six steps, all read-only on the solid:
//!
//! ```text
//!   1  polygonize the sheet          FIRST — a crossing measured on the
//!                                    analytic rim is not on the polygon
//!   2  coplanar_intersection_segments(sheet, solid)      2 crossings
//!   3  split_edge on the sheet's boundary at each        -> 2 loop verts
//!   4  sample the footprint arc between them, add_edge   -> a chain
//!   5  split_face_by_chain(sheet, chain)                 -> two pieces
//!   6  remove the piece whose interior lies on the footprint
//! ```
//!
//! It works, and it is the first thing here that does:
//!
//! ```text
//!   solid faces          25 -> 25     the solid is never named
//!   invariant violations  0 ->  0
//!   the user's drawing    kept — the outside piece, 22,199 of 31,416 mm²
//!   split_face_by_chain   Ok
//! ```
//!
//! ⚠ And one thing does not close: the surviving piece still reads as a
//! `CoplanarOverlap` with the solid's face, because it reaches **0.884225 mm**
//! inside the footprint circle. That number is IDENTICAL with a 9-point chain
//! and a 207-point one, which is what says it is not the chain:
//!
//! ```text
//!   crossing point (59.26, ±79.43)   distance from the axis  99.10
//!   footprint radius                                        100.00
//!   difference                                                0.90  = the reach
//! ```
//!
//! `coplanar_intersection_segments` computes crossings POLYGON-to-POLYGON — the
//! solid's face is sampled at 23 vertices, so its chords sit a sagitta inside
//! the true circle — while `classify_contact` reads that face ANALYTICALLY
//! (`face_outer_area` = 31,415.9 = πr², from its curve). Every point of the cut
//! inherits that 0.9 mm, so the piece the cut leaves outside is 0.9 mm inside.
//!
//! Giving the chain edges their true `AnalyticCurve::Arc` does not help either:
//! `face_tessellation` still walks the chords (areas move by 2 mm², the reach
//! does not move at all).
//!
//! ### So the last gap is one mismatch, and it is not in this shape
//!
//! The cut can only be as exact as the crossings it is given. Closing it means
//! `coplanar_intersection_segments` intersecting against a face's CURVE where it
//! has one, rather than against its sampled boundary — which is a change to the
//! measurement, not to the divider. That is the next piece, and it is a
//! different file.

//!
//! ## Fixing the measurement — the seventh attempt, and why it stops here
//!
//! The mismatch is in one function. `collect_face_boundary` returns a face's raw
//! loop vertices when the loop has three or more, so a boundary whose EDGES
//! carry arcs is read as its chords; only the 1-vertex self-loop case samples a
//! curve. Meanwhile `face_outer_area` on the same face reads the curve. Two
//! readers, one boundary — 메타-원칙 #4 says make it one.
//!
//! Built: walk the loop's half-edges, and where an edge carries a curve, sample
//! it at a chord tolerance of 1e-3 mm instead of taking the straight hop.
//! Measured:
//!
//! ```text
//!   axia-geo   2552 passed / 0 failed      internally consistent
//!   axia-core   822 passed / 6 failed
//! ```
//!
//! and the failures are not recorded values moving:
//!
//! ```text
//!   adr101_b4_two_circles_partial_overlap_auto_splits
//!       two circles partial overlap -> 3 sub-faces, got 2
//!   a_fuzz_session_leaves_the_mesh_sound
//!       session 10 op 11: edge shared by 4 active faces (stacked)
//! ```
//!
//! A denser boundary changes the crossing indices, and `face_a_edge` /
//! `face_b_edge` are contracts: `subtract_double_covered_faces` builds its own
//! `base2d` from `collect_loop_verts` and indexes into it with numbers this
//! function produced. Densify one side only and they no longer refer to the same
//! polygon. Reverted.
//!
//! ### What that means for the next attempt
//!
//! Not another lever. Making one boundary reader means moving every consumer of
//! the crossing indices onto it in the same change — `scene.rs`'s repair, the
//! internal `auto_intersect_coplanar` walk, and the index contract in the
//! doc comment. That is a coordinated change across two crates with LOCKED #41's
//! three-sub-face behaviour as its acceptance test, not something a probe can
//! settle.
//!
//! ⚠ Seven attempts, seven reverts, and the pattern held every time: each was
//! sound in isolation and each damaged a scene that was fine. The engine is
//! unchanged after all of them. What the file now carries instead is where each
//! one stops, which is the part that does not have to be paid for twice.

use axia_core::{Command, Scene};
use glam::DVec3;

fn production() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The gate, read from the source, because the fact IS about control flow: the
/// coplanar scan sits after a `return` that production always takes.
#[test]
fn the_coplanar_scan_sits_behind_the_re_derives_return() {
    let src = include_str!("../src/scene.rs");
    let gate = src
        .find("if self.face_rederive_on_draw {")
        .expect("the re-derive gate moved — re-measure before trusting this file");
    let ret = src[gate..]
        .find("return Ok(coplanar + crossing);")
        .map(|i| gate + i)
        .expect("the gate's early return moved");
    let call = src
        .find("auto_intersect_coplanar(")
        .expect("the splitter call moved");
    println!("  gate @{gate}  return @{ret}  splitter @{call}");
    assert!(
        ret < call,
        "the splitter is no longer behind the re-derive's return — the two \
         mechanisms may now both run, which is what #247 assumed. Re-measure."
    );
}

/// And the behaviour that follows: the drawn sheet and the solid's footprint
/// overlap BEFORE anything could have split them, and nothing does.
///
/// ⚠ This asserts the damage that ships. When the re-derive learns to divide
/// against a standing solid's footprint it goes RED — that is the signal to
/// rewrite it, not to delete it.
#[test]
fn a_draw_over_a_standing_footprint_is_left_overlapping() {
    let mut s = production();
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        radius: 100.0,
    });
    let first = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: first,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    assert!(s.mesh.damaging_contacts().is_empty(), "the solid alone is sound");

    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(120.0, 0.0, 0.0),
        normal: DVec3::Z,
        radius: 100.0,
    });
    let dmg = s.mesh.damaging_contacts();
    for (a, b) in &dmg {
        let vc = |f: axia_geo::FaceId| {
            s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).map(|v| v.len()).unwrap_or(0)
        };
        println!(
            "  {a:?}(v{} solid {}) x {b:?}(v{} solid {})",
            vc(*a), s.mesh.is_face_in_volume(*a), vc(*b), s.mesh.is_face_in_volume(*b)
        );
    }
    assert_eq!(dmg.len(), 1, "the standing overlap changed: {dmg:?}");
    // One side is the solid's own on-plane face — the one the re-derive protects
    // and therefore never divides against.
    let (a, b) = dmg[0];
    assert!(
        s.mesh.is_face_in_volume(a) != s.mesh.is_face_in_volume(b),
        "expected a sheet over a SOLID's footprint, got two of a kind"
    );
    assert_eq!(s.mesh.verify_face_invariants().violations.len(), 0, "invariants still hold");
}
