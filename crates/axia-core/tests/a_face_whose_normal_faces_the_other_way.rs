//! Two solids meeting on one plane, and a circle drawn on the plane between
//! them. Defect 3 of the three the fuzz found — now the guard that it is fixed.
//!
//! Seed `0x5EED000A`, thirteen operations, and the last one left:
//!
//!     face FaceId(39): cached normal opposite to winding (dot=-1.000)
//!     edge EdgeId(69): shared by 3 active faces (non-manifold) —
//!                      FaceId(34) / FaceId(33) cover the same ground (stacked)
//!
//! Reduced by greedy delta debugging to **five** operations. What the shrunk
//! sequence measured, before anything was changed:
//!
//! * the mesh was sound through four operations and the fifth took it from
//!   **0 violations to 24** — one flipped normal and twenty-three stacked pairs;
//! * every stacked pair was solid-face on solid-face, not sheet on solid;
//! * every face in every stacked pair was **made by that last operation** —
//!   eight faces were live before it, and none of them were in a pair.
//!
//! So one circle was building a pile of duplicate solid faces. The plane z=100
//! holds two solids meeting face to face: one below whose top is there, one
//! above whose bottom is there. `rebuild_coplanar_faces_analytic_scoped` fed
//! BOTH perimeters to the arrange — its own comment said "top/bottom" — and
//! re-tiled both, so the tiles for the solid above came out wound against the
//! draw and landed on the tiles for the solid below.
//!
//! The rebuild now decides which solid the draw is on from the WALLS reaching
//! away from the plane, rather than from the on-plane face, because between
//! draws that face is DCEL-absent and only the wall bears its edges — asking
//! the face would drop the very case ADR-281 β-1 exists for.
//!
//! Same method as `a_face_naming_a_gone_half_edge.rs`: transcribe the fuzz log,
//! then greedily drop operations for as long as the failure survives.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

/// The text that identified THIS defect, as the invariant checker prints it.
const WINDING: &str = "cached normal opposite to winding";

/// One operation, described so a shrunk list can be read and re-typed.
#[derive(Clone, Copy, Debug)]
enum Op {
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Extrude { face: u32, d: f64 },
    Line { x: f64, y: f64, z: f64 },
    Ellipse { x: f64, y: f64, z: f64, rx: f64, ry: f64 },
    Punch { x: f64, y: f64, z: f64 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
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
        Op::Extrude { face, d } => {
            s.execute(Command::CreateSolid {
                face_id: FaceId::new(face),
                mode: CreateSolidMode::Extrude { distance: d },
            });
        }
        Op::Line { x, y, z } => {
            s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            });
        }
        Op::Ellipse { x, y, z, rx, ry } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z),
                ref_dir: DVec3::X,
                normal: DVec3::Z,
                radius_x: rx,
                radius_y: ry,
            });
        }
        Op::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z),
                DVec3::new(x + 30.0, y + 30.0, z),
                DVec3::Z,
            );
        }
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
            });
        }
    }
}

/// Every violation the list leaves, whatever kind.
fn violations_after(ops: &[Op]) -> Vec<String> {
    let mut s = prod();
    for op in ops {
        apply(&mut s, *op);
    }
    s.mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|v| format!("{v:?}"))
        .collect()
}

/// The session as the fuzz generated it, transcribed from its log.
///
/// The three `pushIn` steps are omitted because that branch is currently a
/// no-op string — see the note in `a_fuzz_session_leaves_the_mesh_sound.rs`.
fn session_10() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::Extrude { face: 10, d: 50.0 },
        Op::Line { x: 0.0, y: -150.0, z: 200.0 },
        Op::Ellipse { x: -200.0, y: -50.0, z: 200.0, rx: 30.0, ry: 18.0 },
        Op::Punch { x: -50.0, y: -50.0, z: 100.0 },
        Op::Box { x: -200.0, y: -150.0, z: 0.0, w: 180.0 },
        Op::Punch { x: 50.0, y: 100.0, z: 0.0 },
        Op::CircleCurve { x: 0.0, y: -50.0, z: 100.0, r: 70.0 },
    ]
}

/// The shrunk sequence: two rectangles on z=100, a box whose bottom is there,
/// an extrude that puts a second solid's top there too, and then a circle drawn
/// on the plane between them.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 100.0 },
        Op::Rect { x: 0.0, y: 100.0, z: 100.0, w: 180.0 },
        Op::Box { x: -50.0, y: 50.0, z: 100.0, w: 180.0 },
        Op::Extrude { face: 10, d: 50.0 },
        Op::CircleCurve { x: 0.0, y: -50.0, z: 100.0, r: 70.0 },
    ]
}

/// The five operations that used to leave twenty-four violations.
///
/// Mutation-checked: dropping the wall-side test from `onp_ve` — feeding both
/// solids' perimeters to the arrange again — puts the count back to 24 and
/// fails this test.
#[test]
fn the_shrunk_sequence_is_sound() {
    let v = violations_after(&shrunk());
    let winding = v.iter().filter(|t| t.contains(WINDING)).count();
    let stacked = v.iter().filter(|t| t.contains("stacked")).count();
    println!("\n  줄인 순서 위반 {}건 (감기 반대 {winding}, 겹침 {stacked})", v.len());
    for t in &v {
        println!("    ✗ {t}");
    }
    assert!(
        v.is_empty(),
        "the five-operation repro has to stay sound — 24 violations before the \
         rebuild stopped re-tiling both solids on a shared plane: {v:?}"
    );
}

/// And the whole session it was reduced from, which is the fuzz's own case.
#[test]
fn the_whole_session_is_sound() {
    let v = violations_after(&session_10());
    println!("\n  전체 세션 위반 {}건", v.len());
    for t in &v {
        println!("    ✗ {t}");
    }
    assert!(v.is_empty(), "session 10 of the fuzz has to stay sound: {v:?}");
}

/// No operation in the sequence may leave the mesh unsound, not just the last.
///
/// The defect was invisible until the fifth operation — four sound steps and
/// then twenty-four violations at once — so a test that only looked at the end
/// would have said the same thing about a mesh that broke at step two and got
/// tidied up. This walks the sequence.
#[test]
fn no_operation_along_the_way_leaves_a_violation() {
    let ops = shrunk();
    let mut s = prod();
    println!("\n  연산별 위반 수\n");
    let mut bad: Vec<String> = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        apply(&mut s, *op);
        let v: Vec<String> = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        println!("    op {i} {:<48} 위반 {:>3}", format!("{op:?}"), v.len());
        if !v.is_empty() {
            bad.push(format!("op {i} ({op:?}): {}", v.join("; ")));
        }
    }
    assert!(bad.is_empty(), "every step has to leave the mesh sound: {bad:?}");
}

/// The configuration the fix turns on, kept so it cannot quietly go away.
///
/// Two solids meet on z=100 — one below with its top there pointing at the
/// draw, one above with its bottom there pointing away. If the arrangement ever
/// changes so that only one of them is present, the sequence above would keep
/// passing while testing nothing, because there would be no second perimeter to
/// wrongly re-tile.
#[test]
fn the_plane_really_does_have_a_solid_on_each_side() {
    let ops = shrunk();
    let mut s = prod();
    for op in &ops[..ops.len() - 1] {
        apply(&mut s, *op);
    }

    let draw_normal = DVec3::Z; // the circle in the last operation
    let (mut up, mut down) = (0, 0);
    println!("\n  마지막 연산 전, z=100 평면 위의 솔리드 면\n");
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() || !s.mesh.is_face_in_volume(fid) {
            continue;
        }
        let on_plane = s
            .mesh
            .collect_loop_verts(f.outer().start)
            .map(|vv| {
                !vv.is_empty()
                    && vv.iter().all(|v| {
                        s.mesh.verts.get(*v).map_or(false, |p| (p.pos().z - 100.0).abs() < 1e-6)
                    })
            })
            .unwrap_or(false);
        if !on_plane {
            continue;
        }
        let d = f.normal().normalize_or_zero().dot(draw_normal);
        println!("    {fid:?}  · 그리기 방향 = {d:+.3}");
        if d > 0.0 {
            up += 1;
        } else {
            down += 1;
        }
    }
    println!("\n  그리는 쪽을 보는 면 {up}개, 반대쪽을 보는 면 {down}개");
    assert!(
        up > 0 && down > 0,
        "the repro needs a solid facing the draw AND one facing away — with \
         only one of them there is nothing here to get wrong, and the tests \
         above would be passing on an easier scene than the one they name"
    );
}
