//! What the fuzz found on its first session, cut down to the operations that
//! matter.
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
//! Two faces on the same ground, and a third face on the edge they share. That
//! is the coplanar-overlap kind of damage from stage 3.5, arrived at through
//! ordinary Push/Pull rather than through a drawing overlap — which is why no
//! draw grid found it and a random sequence did in five operations.

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
fn pushing_a_solids_face_inward_can_leave_two_faces_stacked() {
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
        let mut bad = 0;
        for f in &bfaces {
            let mut t = prod();
            t.restore_scene_snapshot(&snap);
            let res = t.execute(Command::CreateSolid {
                face_id: *f,
                mode: CreateSolidMode::Extrude { distance: -20.0 },
            });
            if !matches!(res, CommandResult::Error(_)) && !t.mesh.verify_face_invariants().is_valid()
            {
                bad += 1;
            }
        }
        println!("  대조군 박스 -20 → 깨진 면 {bad} / {}", bfaces.len());
    }

    // How deep is too deep? The solid is radius 90, height 50, so -60 pushes a
    // wall past the far side and the top through the floor. If a shallow push
    // is clean the boundary is over-push, which ADR-196 already clamps for
    // boxes; if even a shallow one breaks, it is not about depth at all.
    for depth in [-10.0_f64, -20.0, -40.0, -60.0] {
        let mut bad = 0;
        for f in &faces {
            let mut t = prod();
            t.restore_scene_snapshot(&s.scene_snapshot());
            let res = t.execute(Command::CreateSolid {
                face_id: *f,
                mode: CreateSolidMode::Extrude { distance: depth },
            });
            if !matches!(res, CommandResult::Error(_)) && !t.mesh.verify_face_invariants().is_valid()
            {
                bad += 1;
            }
        }
        println!("  깊이 {depth:>6} → 깨진 면 {bad} / {}", faces.len());
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

    // ⚠ PINNED AS MEASURED — this is an OPEN defect, not a passing behaviour.
    //
    // A box is clean at every depth (0 of 6). A 24-segment extruded circle
    // breaks on 23 of its 25 faces, at −10 mm as surely as at −60, so it is
    // not over-push and ADR-196's clamp is not what is missing. Every one of
    // those pushes is ACCEPTED and leaves two faces covering the same ground
    // with a third on the edge they share.
    //
    // The number is pinned so the day it changes, this says so. Fewer means
    // somebody fixed it — lower the number and name what started working.
    // More means a regression.
    assert_eq!(
        broke.len(),
        23,
        "measured today: pushing a side panel of a many-sided extruded solid          leaves it stacked. A box does not. Found {broke:?}"
    );
}
