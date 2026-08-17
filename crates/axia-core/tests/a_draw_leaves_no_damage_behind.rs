//! Stage 4, as this codebase can honestly have it.
//!
//! The plan says "turn on the draw rollback gate". The gate is there —
//! `guard_imprint`, wrapping all eight closed-shape draws — but it does not
//! roll anything back, and that is a recorded decision rather than an
//! oversight:
//!
//! > A DRAW IS NEVER REFUSED (사용자 결정 2026-08-06).
//! > "기하적으로 가능한것을 막으면 안돼."
//!
//! Three branches used to roll a draw back — a new self-intersection, a new
//! coplanar stack, a new non-manifold edge — and each turned a shape the user
//! had just placed into nothing at all, silently, because the WASM layer
//! renders `Error` as `-1`. The same note says where the work belongs instead:
//!
//! > Anything still unsound after a draw is a hole in the arrangement, and it
//! > belongs in the arrangement, not behind a refusal.
//!
//! So this file is the gate's measuring half rather than its refusing half: it
//! asks, of every gated draw in configurations that used to be the hard ones,
//! whether anything DAMAGING survives — using stage 3.5's shared judgment
//! (`Mesh::damaging_contacts`), which counts a coplanar overlap or a genuine
//! crossing and does not count two faces merely touching.
//!
//! What this can catch that the existing grids cannot: `draw_freely_matrix`
//! asks whether the draw survives and whether the face divides, and its
//! cleanliness check is its own local coplanar test. This asks the engine's
//! own question, of the same shapes, on both a sheet and a solid.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
use axia_geo::CreateSolidMode;
use glam::DVec3;

const H: f64 = 200.0;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// A flat sheet to draw on.
fn sheet() -> (Scene, f64) {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });
    (s, 0.0)
}

/// A solid, built the way the app builds one, to draw on the top of.
fn solid() -> (Scene, f64) {
    let (mut s, _) = sheet();
    let base = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .next()
        .expect("the sheet");
    s.execute(Command::CreateSolid {
        face_id: base,
        mode: CreateSolidMode::Extrude { distance: H },
    });
    (s, H)
}

/// The eight draws the gate wraps, each placed at `(dx, dy)` on the host plane.
fn gated_draws(z: f64, dx: f64, dy: f64) -> Vec<(&'static str, Command)> {
    let c = DVec3::new(dx, dy, z);
    let cp = |a: f64, b: f64| -> Vec<DVec3> {
        let p = |x: f64, y: f64| DVec3::new(dx + x, dy + y, z);
        vec![p(a, 0.0), p(0.0, b), p(-a, 0.0), p(0.0, -b), p(a, 0.0)]
    };
    let knots = vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
    vec![
        ("사각형", Command::DrawRectAsShape {
            center: c, normal: DVec3::Z, up: DVec3::X, width: 120.0, height: 90.0,
        }),
        ("원 (다각형)", Command::DrawCircleAsShape {
            center: c, normal: DVec3::Z, radius: 60.0, segments: 24,
        }),
        ("정다각형", Command::DrawPolygonAsShape {
            center: c, normal: DVec3::Z, radius: 60.0, sides: 6,
        }),
        ("원 (곡선)", Command::DrawCircleAsCurve { center: c, normal: DVec3::Z, radius: 60.0 }),
        ("타원", Command::DrawEllipseAsCurve {
            center: c, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: 80.0, radius_y: 40.0,
        }),
        ("닫힌 베지어", Command::DrawClosedBezierAsCurve { control_pts: cp(80.0, 60.0) }),
        ("닫힌 B-스플라인", Command::DrawClosedBSplineAsCurve {
            control_pts: cp(80.0, 60.0), knots: knots.clone(), degree: 3,
        }),
        ("닫힌 NURBS", Command::DrawClosedNURBSAsCurve {
            control_pts: cp(80.0, 60.0), weights: vec![1.0; 5], knots, degree: 3,
        }),
    ]
}

/// Every gated draw, on a sheet and on a solid, drawn alone and then drawn
/// again overlapping itself — which is the configuration the removed rollback
/// used to refuse.
#[test]
fn no_gated_draw_leaves_damage_behind() {
    println!("\n게이트를 지나는 그리기 — 손상이 남는가 (3.5 판정)\n");
    println!("  {:<16} {:<10} {:>8} {:>8}", "도형", "호스트", "단독", "겹쳐서");
    let mut fails: Vec<String> = Vec::new();

    for (host_name, build, z) in [
        ("시트", sheet as fn() -> (Scene, f64), 0.0),
        ("입체 윗면", solid as fn() -> (Scene, f64), H),
    ] {
        for (name, _) in gated_draws(z, 0.0, 0.0) {
            // Alone.
            let (mut s, _) = build();
            let cmd = gated_draws(z, 0.0, 0.0)
                .into_iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| c)
                .expect("command");
            let alone_err = matches!(s.execute(cmd), CommandResult::Error(_));
            let alone = s.mesh.damaging_contacts().len();

            // And again, shifted so the two cover part of the same ground.
            let second = gated_draws(z, 45.0, 0.0)
                .into_iter()
                .find(|(n, _)| *n == name)
                .map(|(_, c)| c)
                .expect("command");
            let over_err = matches!(s.execute(second), CommandResult::Error(_));
            let over = s.mesh.damaging_contacts().len();

            println!("  {name:<16} {host_name:<10} {alone:>8} {over:>8}");
            if alone > 0 {
                fails.push(format!("{host_name} / {name} — 단독 그리기가 손상 {alone}"));
            }
            if over > 0 {
                fails.push(format!("{host_name} / {name} — 겹쳐 그리기가 손상 {over}"));
            }
            // A refusal is not damage, but it is worth seeing.
            if alone_err || over_err {
                println!("      (거부: 단독 {alone_err}, 겹침 {over_err})");
            }
        }
    }

    println!("\n  손상 {}건", fails.len());
    for f in &fails {
        println!("    ✗ {f}");
    }
    assert_eq!(
        fails.len(),
        0,
        "a draw must not leave a coplanar overlap or a crossing behind — and if \
         it does, the hole is in the arrangement, not in a refusal that is not \
         coming back: {fails:?}"
    );
}

/// The control: the judgment is capable of reading non-zero here.
///
/// Without this the grid above proves only that `damaging_contacts` returns
/// something empty, which a function that always returned empty would also do.
/// Two faces built directly, covering the same ground, on the same scene type.
#[test]
fn the_same_judgment_does_report_damage_when_there_is_some() {
    let mut s = prod();
    let quad = |s: &mut Scene, x0: f64, x1: f64| {
        let v: Vec<_> = [
            DVec3::new(x0, -100.0, 0.0),
            DVec3::new(x1, -100.0, 0.0),
            DVec3::new(x1, 100.0, 0.0),
            DVec3::new(x0, 100.0, 0.0),
        ]
        .iter()
        .map(|p| s.mesh.add_vertex(*p))
        .collect();
        s.mesh.add_face(&v, FORM_MATERIAL).expect("face");
    };
    quad(&mut s, -100.0, 100.0);
    quad(&mut s, -40.0, 160.0);
    assert!(
        !s.mesh.damaging_contacts().is_empty(),
        "two sheets covering the same ground must read as damage, or the grid \
         above is measuring nothing"
    );
}
