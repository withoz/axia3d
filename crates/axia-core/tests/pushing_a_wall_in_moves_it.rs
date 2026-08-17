//! What the fuzz found on its first session — and what fixed it.
//!
//! Seed `0x5EED0000` broke at operation 4 of 20:
//!
//! ```text
//!   circle(50,100,0)      a circle on the ground
//!   line(150,-100,100)    a line, elsewhere
//!   extrude(FaceId(1),50) the circle becomes a solid
//!   pushIn(FaceId(11),-60) one of its faces pushed inward
//! ```
//!
//! leaving
//!
//! ```text
//!   edge EdgeId(11): shared by 3 active faces (non-manifold)
//!                    — FaceId(2) / FaceId(31) cover the same ground (stacked)
//! ```
//!
//! Two faces on the same ground, and a third on the edge they share, arrived at
//! through ordinary Push/Pull rather than through a drawing overlap — which is
//! why no draw grid found it and a random sequence did in five operations.
//!
//! **The cause.** `is_move_only` asked whether every edge connecting to the
//! face runs parallel to its normal. A box side passes that by a coincidence of
//! the square: the ring edges at its corners happen to run along its own
//! normal. On a 24-sided prism they run tangentially, so every panel failed and
//! went to `create_solid` — which preserves the profile and patches the caps
//! beside themselves, leaving three faces on one edge.
//!
//! **The fix, in two parts, the second found by the fuzz too.** A face of a
//! closed volume with no coplanar neighbour is a WALL, and pushing a wall moves
//! it. The coplanar clause is what keeps an embedded face out: a rect drawn on
//! a solid's top is also in a closed volume, but its neighbour is the ring
//! around it on the same plane, and pushing that is a boss (ADR-264).
//!
//! Routing walls to MoveOnly then broke two fuzz sessions that had been sound,
//! with a new signature — "cached normal opposite to winding". `push_pull_move_only`
//! refreshed only the PUSHED face's normal. On a box that is enough, because
//! its neighbours stay planar and keep pointing the same way; on a prism they
//! SKEW to follow the wall and their cached normals turn while the cache does
//! not. Every face holding a moved vertex is refreshed now.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult};
use axia_geo::CreateSolidMode;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The sequence, minimised: the line turned out not to matter.
#[test]
fn pushing_a_solids_wall_inward_moves_it_and_leaves_the_mesh_sound() {
    let mut s = prod();
    s.execute(Command::DrawCircleAsShape {
        center: DVec3::new(50.0, 100.0, 0.0),
        normal: DVec3::Z,
        radius: 90.0,
        segments: 24,
    });
    let base = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .next()
        .expect("the circle");
    let r = s.execute(Command::CreateSolid {
        face_id: base,
        mode: CreateSolidMode::Extrude { distance: 50.0 },
    });
    assert!(!matches!(r, CommandResult::Error(_)), "extrude: {r:?}");
    let after_extrude = s.mesh.verify_face_invariants();
    assert!(
        after_extrude.is_valid(),
        "the solid itself is sound: {:?}",
        after_extrude.violations
    );

    // Push one of its faces inward — ordinary Push/Pull, the same command the
    // user reaches for.
    let faces: Vec<_> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    // `Scene` is not `Clone`, so each face is tried on a scene rebuilt from
    // the snapshot rather than on a copy.
    let fresh = s.scene_snapshot();
    // Control: the same push on a BOX built the same way. If a box is clean
    // the finding is about the many-sided solid; if a box breaks too, it is
    // about push-in itself.
    {
        let mut b = prod();
        b.execute(Command::DrawRectAsShape {
            center: DVec3::new(50.0, 100.0, 0.0),
            normal: DVec3::Z,
            up: DVec3::X,
            width: 180.0,
            height: 180.0,
        });
        let base = b
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .next()
            .expect("the rect");
        b.execute(Command::CreateSolid {
            face_id: base,
            mode: CreateSolidMode::Extrude { distance: 50.0 },
        });
        let snap = b.scene_snapshot();
        let bfaces: Vec<_> = b
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .collect();
        let (mut bad, mut refused) = (0, 0);
        for f in &bfaces {
            let mut t = prod();
            t.restore_scene_snapshot(&snap);
            let res = t.execute(Command::CreateSolid {
                face_id: *f,
                mode: CreateSolidMode::Extrude { distance: -20.0 },
            });
            if matches!(res, CommandResult::Error(_)) {
                refused += 1;
            } else if !t.mesh.verify_face_invariants().is_valid() {
                bad += 1;
            }
        }
        use axia_geo::operations::push_pull::is_move_only;
        let mv = bfaces.iter().filter(|f| is_move_only(&b.mesh, **f)).count();
        println!(
            "  대조군 박스 -20 → 깨진 면 {bad} / {} (거부 {refused}, MoveOnly {mv})",
            bfaces.len()
        );
    }

    // Which dispatch does each face take? `is_move_only` is the key ADR-196
    // routes on: true → the MoveOnly push that keeps a solid a solid, false →
    // `create_solid`, which builds a NEW solid from the profile and preserves
    // it (the ADR-087 K-ε sandwich).
    {
        use axia_geo::operations::push_pull::is_move_only;
        let mv = faces.iter().filter(|f| is_move_only(&s.mesh, **f)).count();
        println!("  24각 솔리드: MoveOnly {mv} / {} 면", faces.len());
    }

    // Are the faces the violation names even alive before the push? The same
    // pair is reported no matter WHICH face is pushed, which is not what a
    // defect caused by the pushed face would look like.
    {
        let live: Vec<u32> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid.raw())
            .collect();
        println!("  압출 직후 살아있는 면: {live:?}");
    }

    // The experiment the fix turns on: send a side panel down the MoveOnly
    // path instead, and see whether the mesh survives. If it does, the defect
    // is the dispatch key; if it does not, MoveOnly is not the answer for a
    // panel either and the sandwich has to be fixed where it is made.
    {
        let snap = s.scene_snapshot();
        let (mut moved_ok, mut moved_bad, mut moved_err) = (0, 0, 0);
        for f in &faces {
            let mut t = prod();
            t.restore_scene_snapshot(&snap);
            match t.mesh.push_pull(*f, -20.0, axia_geo::MaterialId::new(0)) {
                Ok(_) => {
                    if t.mesh.verify_face_invariants().is_valid() {
                        moved_ok += 1;
                    } else {
                        if moved_bad == 0 {
                            let inv = t.mesh.verify_face_invariants();
                            for v in inv.violations.iter().take(2) {
                                println!("      MoveOnly 위반: {v:?}");
                            }
                            // What ARE the accused faces? Centroid and normal
                            // say whether a new face landed on an old one's
                            // plane or the old one moved onto the new.
                            for id in [2u32, 3, 29, 31] {
                                let fid = axia_geo::FaceId::new(id);
                                let alive = t.mesh.faces.get(fid).is_some_and(|f| f.is_active());
                                let c = t
                                    .mesh
                                    .faces
                                    .get(fid)
                                    .and_then(|f| t.mesh.collect_loop_verts(f.outer().start).ok())
                                    .map(|vs| {
                                        let ps: Vec<_> = vs
                                            .iter()
                                            .filter_map(|v| t.mesh.vertex_pos(*v).ok())
                                            .collect();
                                        let n = ps.len().max(1) as f64;
                                        ps.iter().fold(DVec3::ZERO, |a, p| a + *p) / n
                                    });
                                println!(
                                    "        face {id}: alive={alive} centroid={:?}",
                                    c.map(|p| (p.x.round(), p.y.round(), p.z.round()))
                                );
                            }
                            println!("        (pushed face was {f:?})");
                        }
                        moved_bad += 1;
                    }
                }
                Err(_) => moved_err += 1,
            }
        }
        println!(
            "  같은 면을 MoveOnly 로 보내면 → 건전 {moved_ok}, 깨짐 {moved_bad}, 거부 {moved_err}"
        );
    }

    // How deep is too deep? The solid is radius 90, height 50, so -60 pushes a
    // wall past the far side and the top through the floor. If a shallow push
    // is clean the boundary is over-push, which ADR-196 already clamps for
    // boxes; if even a shallow one breaks, it is not about depth at all.
    for depth in [-10.0_f64, -20.0, -40.0, -60.0] {
        let (mut bad, mut refused) = (0, 0);
        for f in &faces {
            let mut t = prod();
            t.restore_scene_snapshot(&s.scene_snapshot());
            let res = t.execute(Command::CreateSolid {
                face_id: *f,
                mode: CreateSolidMode::Extrude { distance: depth },
            });
            if matches!(res, CommandResult::Error(_)) {
                refused += 1;
            } else if !t.mesh.verify_face_invariants().is_valid() {
                bad += 1;
            }
        }
        println!(
            "  깊이 {depth:>6} → 깨진 면 {bad} / {} (거부 {refused})",
            faces.len()
        );
    }

    let mut broke: Vec<String> = Vec::new();
    for f in faces {
        let mut t = prod();
        t.restore_scene_snapshot(&fresh);
        // Control: the restore must not be the thing that breaks it.
        let restored = t.mesh.verify_face_invariants();
        assert!(
            restored.is_valid(),
            "the snapshot restore itself broke the mesh before any push: {:?}",
            restored.violations.iter().take(2).collect::<Vec<_>>()
        );
        let res = t.execute(Command::CreateSolid {
            face_id: f,
            mode: CreateSolidMode::Extrude { distance: -60.0 },
        });
        let refused = matches!(res, CommandResult::Error(_));
        let inv = t.mesh.verify_face_invariants();
        if !refused && !inv.is_valid() {
            broke.push(format!(
                "{f:?}: {}",
                inv.violations
                    .iter()
                    .take(1)
                    .map(|v| format!("{v:?}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }

    println!("PUSH-IN on each of the solid's faces — broke {}", broke.len());
    for b in broke.iter().take(3) {
        println!("    {b}");
    }

    // Was 23 of 25, at −10 mm as surely as at −60; a box was clean at every
    // depth, which is the control that said this was about the shape and not
    // about push-in. Now 0 at every depth, and the walls route to MoveOnly
    // (25 of 25 above) instead of building a new solid over the old one.
    assert!(
        broke.is_empty(),
        "an accepted push must leave the mesh sound: {broke:?}"
    );
}
