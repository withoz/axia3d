//! FUZZ SESSION 10, TRANSCRIBED — AND IT DOES NOT REPRODUCE.
//!
//! That is the finding, so it is the title.
//!
//! `face_rederive`'s on-plane volume-edge filter keeps an edge only when an
//! adjacent face reaches BELOW the plane (`!saw_a_wall || reaches_below`). That
//! is the right question when the plane is a solid's top, and it silently
//! answers no when a solid STANDS on the plane — so ADR-281 β-1's re-tile, the
//! imprint that lets walls keep the edges they stand on, never engages there.
//! A draw beside a standing solid leaves stacked faces instead (reported
//! 2026-09-01; the chain is in `what_overlapping_draws_leave_on_the_ground.rs`).
//!
//! Widening the filter fixes that and breaks three fuzz sessions. This is the
//! earliest of them — session 10, seed 0x5EED000A, operation 7 — typed out from
//! the harness log so it could be worked on in four seconds instead of twenty.
//!
//! ⚠ It reaches a different scene. With the widening applied the fuzz reports
//! two violations here and this file reports none: 15 faces, nothing stacked.
//! The harness header warned about exactly this — "a report you cannot re-type
//! is a rumour with coordinates. Found by trying to transcribe session 6 into a
//! standalone repro and getting a scene that did not fail" — and the same trap
//! caught the next person to try it.
//!
//! What is missing has not been found. The log records every parameter, the
//! ops are transcribed verbatim, and the one thing that cannot be transcribed
//! is which face the harness pushed: it picks by RANDOM INDEX and logs the
//! centroid, so `face_at` here picks the nearest face to that point, which is a
//! different question when two faces share a centroid.
//!
//! Kept because it is cheap, because it is the fixture to fix rather than write
//! again, and because it says out loud that the transcription is not evidence.
//! Anyone using it to reason about the break should stop and read this first.
//!
//! It asserts the CURRENT scene — sound, with the narrow filter in the tree —
//! so that if the transcription's meaning drifts, that is visible too.
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

fn centroid(s: &Scene, f: FaceId) -> Option<DVec3> {
    let tris = s.mesh.face_tessellation(f)?;
    if tris.is_empty() {
        return None;
    }
    let mut sum = DVec3::ZERO;
    let mut n = 0.0;
    for t in &tris {
        for p in t {
            sum += *p;
            n += 1.0;
        }
    }
    Some(sum / n)
}

/// The face the harness picked, found by where it was rather than by its id —
/// ids shift with every operation, positions are what the log records.
fn face_at(s: &Scene, want: DVec3) -> FaceId {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter_map(|(id, _)| centroid(s, id).map(|c| (id, (c - want).length())))
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
        .expect("a face near the logged position")
}

fn rect(s: &mut Scene, x: f64, y: f64, z: f64, w: f64) -> CommandResult {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(x, y, z),
        normal: DVec3::Z,
        up: DVec3::X,
        width: w,
        height: w * 0.75,
    })
}

fn push_in(s: &mut Scene, at: DVec3, d: f64) -> CommandResult {
    let f = face_at(s, at);
    s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: d } })
}

/// Session 10 up to and including operation 7, from the harness log:
///
/// ```text
///   1: rect(0,100,100,100)
///   2: rect(0,100,100,180)
///   3: box(-50,50,100,180,ok)
///   4: pushIn(FaceId(11)@(-140.000,50.000,160.000),-150)
///   5: rect(150,-200,0,180)
///   6: pushIn(FaceId(4)@(0.000,100.000,100.000),-100)
///   7: circleCurve(0,100,100,r=110)
/// ```
fn session_10_through_op_7() -> Scene {
    let mut s = prod();
    rect(&mut s, 0.0, 100.0, 100.0, 100.0);
    rect(&mut s, 0.0, 100.0, 100.0, 180.0);
    // box(c, w) is create_box(c + (0,0,60), w, 120, w)
    let _ = s.mesh.create_box(
        DVec3::new(-50.0, 50.0, 100.0) + DVec3::new(0.0, 0.0, 60.0),
        180.0,
        120.0,
        180.0,
        FORM_MATERIAL,
    );
    push_in(&mut s, DVec3::new(-140.0, 50.0, 160.0), -150.0);
    rect(&mut s, 150.0, -200.0, 0.0, 180.0);
    push_in(&mut s, DVec3::new(0.0, 100.0, 100.0), -100.0);
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, 100.0),
        normal: DVec3::Z,
        radius: 110.0,
    });
    s
}

#[test]
fn the_transcription_reaches_the_same_scene_the_fuzz_does() {
    let s = session_10_through_op_7();
    let inv = s.mesh.verify_face_invariants();
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("  faces {faces}   invariant violations {}", inv.violations.len());
    for v in inv.violations.iter().take(4) {
        println!("    {v}");
    }
    // ⚠ Fidelity first: a transcription that reaches a DIFFERENT scene is worse
    // than none, because work done on it looks like progress. The fuzz reports
    // two violations at this operation — an edge with four active faces, twice —
    // and this file reports none.
    //
    // ⚠ Sound — and the fuzz says session 10 op 7 is NOT, once the filter is
    // widened. This asserts the scene this file actually builds, which is not
    // the scene the fuzz builds. Do not read a pass here as the break being
    // absent; read it as this transcription still reaching the same wrong place.
    assert!(
        inv.violations.is_empty(),
        "the transcription drifted: {:?}",
        inv.violations
    );
}
