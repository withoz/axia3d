//! Session 10 of the fuzz, measured until it stopped being a guess.
//!
//! Four operations — a small rectangle, a bigger one drawn around it, a push
//! that takes the inner one down into a solid, and a circle over the lot — leave
//! four edges each bearing four active faces. The account of it went through two
//! wrong ideas before it went through a right one, and both are kept as guards
//! below because each was plausible enough to cost somebody an afternoon:
//!
//! 1. "A draw reaching two solids stacks." Two boxes sharing a top plane with
//!    one circle over both is SOUND — 12 faces to 13, nothing stacked.
//! 2. "The bigger rectangle never got divided." It did. `face_outer_area`
//!    reports the outer loop and says nothing about holes, and the big one has
//!    one; the two rectangles are a proper ring plus its inner face. The third
//!    instrument in this area to misreport a face this way.
//!
//! What the edges actually carry:
//!
//! ```text
//!   EdgeId(18) → FaceId(4)  ring, solid       outer area 24,300
//!                FaceId(12) wall of the push, solid
//!                FaceId(5)  its top,          solid       7,500
//!                FaceId(15) the circle,       SHEET      38,013
//! ```
//!
//! `FaceId(15)` is the drawn circle itself — one anchor, one self-loop, π·110².
//! It was never divided against the solid's top, so it lies over it. The
//! coplanar re-tile is what would have divided it and it declined: the plane
//! reaches more than one perimeter, which `face_rederive.rs` documents as a case
//! it cannot carry ("a face belongs to one owner"), with its own measurements.
//!
//! Removing that decline fixes THIS and breaks two others —
//! `drawing_across_a_hole_rim_keeps_the_solid_closed` and
//! `two_boxes_sharing_the_ground_stay_twelve_whole_faces` — so the rule is
//! load-bearing and the fix is not "decline less". It is for the re-tile to
//! carry one solid's two rims, which is a larger piece of work than this file.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const TOP: f64 = 100.0;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn stacked(s: &Scene) -> Vec<String> {
    s.mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|v| format!("{v:?}"))
        .filter(|t| t.contains("cover the same ground"))
        .collect()
}

/// Two boxes whose tops are both on z=100, and one circle over both.
///
/// Written as the first guess at what session 10 is, and it is SOUND — which is
/// why it is a guard rather than a repro. "A draw reaching two solids" is not
/// the trigger.
#[test]
fn a_circle_over_two_solid_tops_is_sound() {
    let mut s = prod();
    // Tops at z=100: centre z = 100 - 60, height 120.
    s.mesh
        .create_box(DVec3::new(-80.0, 0.0, TOP - 60.0), 100.0, 120.0, 100.0, FORM_MATERIAL)
        .expect("box a");
    s.mesh
        .create_box(DVec3::new(80.0, 0.0, TOP - 60.0), 100.0, 120.0, 100.0, FORM_MATERIAL)
        .expect("box b");
    let before = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();

    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 0.0, TOP),
        normal: DVec3::Z,
        radius: 150.0,
    });

    let after = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let st = stacked(&s);
    println!("\n  상자 둘 + 덮는 원: 면 {before} → {after}, 겹침 {}", st.len());
    for t in &st {
        println!("    ✗ {t}");
    }
    assert!(st.is_empty(), "a draw reaching two solids must not stack: {st:?}");
}

/// ⚠ PINNED AS MEASURED — an OPEN defect, inventoried in the fuzz's
/// `KNOWN_BREAKS`.
///
/// The circle lies over the solid's top because the re-tile declined to divide
/// it. Pinned so the day that stops, this says so instead of passing quietly.
#[test]
fn the_shrunk_session_10_still_stacks() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 100.0,
        height: 75.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 180.0,
        height: 135.0,
    });
    s.execute(Command::CreateSolid {
        face_id: FaceId::new(4),
        mode: CreateSolidMode::Extrude { distance: -100.0 },
    });
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        radius: 110.0,
    });

    let st = stacked(&s);
    println!("\n  줄인 세션 10: 겹침 {}", st.len());
    for t in &st {
        println!("    ✗ {t}");
    }
    assert!(
        !st.is_empty(),
        "세션 10 no longer stacks — if somebody taught the re-tile to carry one          solid's two rims, say so, strike it from KNOWN_BREAKS, and turn this          into the test that proves it"
    );
}

/// What the two stacked faces actually are.
///
/// The repair in `guard_imprint` handles a straddle (two crossings) and a
/// containment (a lens with none). A pair that is neither — one face lying
/// exactly where another already is — has no polygon operation to fix it, which
/// would explain why it survives every repair the draw runs.
#[test]
fn what_the_two_stacked_faces_are() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });
    s.execute(Command::CreateSolid {
        face_id: FaceId::new(4),
        mode: CreateSolidMode::Extrude { distance: -100.0 },
    });
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, radius: 110.0,
    });

    let mut pairs: Vec<(FaceId, FaceId)> = Vec::new();
    for t in stacked(&s) {
        let ids: Vec<usize> = t
            .split("FaceId(")
            .skip(1)
            .filter_map(|p| p.split(')').next())
            .filter_map(|n| n.parse().ok())
            .collect();
        if ids.len() >= 2 {
            let p = (FaceId::new(ids[0] as u32), FaceId::new(ids[1] as u32));
            if !pairs.contains(&p) {
                pairs.push(p);
            }
        }
    }

    println!("\n  서로 다른 겹침 짝 {}건\n", pairs.len());
    for (a, b) in &pairs {
        let d = |f: FaceId| {
            let vv = s
                .mesh
                .faces
                .get(f)
                .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
                .map(|v| v.len())
                .unwrap_or(0);
            format!(
                "{f:?}({}, verts={vv}, area={:.1})",
                if s.mesh.is_face_in_volume(f) { "solid" } else { "sheet" },
                s.mesh.face_outer_area(f)
            )
        };
        println!("    {} / {}", d(*a), d(*b));
    }
    // Does the repair even see them as intersecting?
    let si = s.mesh.detect_self_intersections();
    println!("  detect_self_intersections: {} pairs", si.intersecting_pairs.len());
    for (a, b) in si.intersecting_pairs.iter().take(6) {
        println!("    {a:?} / {b:?}");
    }
    // The violation names an EDGE with four active faces on it; the pair is the
    // checker's account of why. Which four is the more useful question.
    for t in stacked(&s) {
        let Some(e) = t.split("EdgeId(").nth(1).and_then(|x| x.split(')').next()) else { continue };
        let Ok(eid) = e.parse::<u32>() else { continue };
        let eid = axia_geo::EdgeId::new(eid);
        let (adj, _) = s.mesh.get_faces_sharing_edge(eid);
        let live: Vec<String> = adj
            .iter()
            .filter(|f| s.mesh.faces.get(**f).map_or(false, |x| x.is_active()))
            .map(|f| {
                format!(
                    "{f:?}({}, area={:.0})",
                    if s.mesh.is_face_in_volume(*f) { "solid" } else { "sheet" },
                    s.mesh.face_outer_area(*f)
                )
            })
            .collect();
        println!("  {eid:?} → {}", live.join(" · "));
    }
    assert!(!pairs.is_empty(), "the repro has to leave a stacked pair to describe");
}

/// Two operations: a small rectangle, then a bigger one drawn AROUND it.
///
/// ⚠ WRITTEN TO TEST A GUESS, AND THE GUESS WAS WRONG. The four-operation
/// repro's violating edges each carry four faces, and the face table showed the
/// big rectangle among them at its full 24,300 — which read like "the
/// containment never split it". It did: `face_outer_area` reports the OUTER
/// loop and says nothing about holes, and the big rectangle has one. The two
/// rectangles are a proper ring plus its inner face.
///
/// Kept as the correction. The same shape of mistake is written down twice
/// already — three instruments in this area misreport a face — and a guard
/// that pins what is actually there is cheaper than making it a third time.
#[test]
fn a_rect_drawn_around_an_existing_one_divides() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });

    let live: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(i, _)| i).collect();
    println!("
  작은 rect → 감싸는 큰 rect: 면 {}개
", live.len());
    for f in &live {
        println!(
            "    {f:?}  outer_area={:.0}  구멍 {}",
            s.mesh.face_outer_area(*f),
            s.mesh.faces[*f].inners().len()
        );
    }

    assert_eq!(live.len(), 2, "a ring and the face inside it");
    let holed = live.iter().filter(|f| !s.mesh.faces[**f].inners().is_empty()).count();
    assert_eq!(
        holed, 1,
        "and the bigger one carries the hole — its outer area is still 24,300,          which is not the same claim as covering that ground"
    );
    assert!(
        s.mesh.verify_face_invariants().is_valid(),
        "two operations in, nothing is wrong yet"
    );
}
