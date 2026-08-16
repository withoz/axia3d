//! Does the open-seam split reach the sphere and the cone the app actually
//! makes?
//!
//! `split_curved_face_by_open_seam` has been covered since ADR-284, but by
//! tests that build their host with `create_sphere_kernel_native` — the equator
//! form: two hemispheres sharing a rim. That is not what the app builds. Path B
//! `create_sphere` dispatches to `create_sphere_axis_native`, which is poles
//! plus one meridian seam and **one** face. (The dispatch still carries the
//! comment "Returns 2 hemisphere FaceIds" above a line that returns `vec![f]`.)
//!
//! So the question is not whether the engine can split a hemisphere. It is
//! whether the seam path reaches the shape a user gets by drawing a sphere, and
//! the same for a cone. Measured here rather than assumed, because three
//! separate places — the Rust coverage, the tool wiring added by ADR-284 for
//! Bezier and Freehand, and C-3's wiring for the line — all rest on the answer,
//! and none of them asked.

use axia_core::scene::Scene;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

fn curved_face(scene: &Scene) -> Option<FaceId> {
    scene
        .mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && matches!(
                    f.surface(),
                    Some(AnalyticSurface::Sphere { .. } | AnalyticSurface::Cone { .. })
                )
        })
        .map(|(fid, _)| fid)
}

/// The sphere a user gets. One face, a meridian seam, no shared-rim twin.
#[test]
fn the_app_sphere_is_one_face_and_the_seam_path_declines_it() {
    let mut scene = Scene::new();
    scene.mesh.sphere_path_b_default = true;
    let faces = scene
        .mesh
        .create_sphere(DVec3::ZERO, 200.0, 24, 16, MaterialId::new(0))
        .expect("sphere");
    let active = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("APP SPHERE returned {} face(s), {active} active", faces.len());
    assert_eq!(faces.len(), 1, "the axis-native sphere is a single face");
    assert_eq!(active, 1);

    let host = curved_face(&scene).expect("the sphere face");
    // A seam over the north pole, endpoints out near the equator.
    let seam = vec![
        DVec3::new(197.0, 0.0, 34.4),
        DVec3::new(0.0, 0.0, 200.0),
        DVec3::new(-197.0, 0.0, 34.4),
    ];
    let before = scene.scene_snapshot();
    let got = scene.draw_open_seam_on_curved(host, seam);
    let after = scene.scene_snapshot();
    println!("APP SPHERE open seam (over the pole) → {got:?}");
    // And with both endpoints ON the meridian seam, which is this form of
    // sphere rim — so the refusal cannot be blamed on where they were put.
    let r = 200.0_f64;
    let h = std::f64::consts::FRAC_1_SQRT_2 * r;
    let on_seam = vec![
        DVec3::new(h, 0.0, h),
        DVec3::new(0.0, r, 0.0),
        DVec3::new(h, 0.0, -h),
    ];
    let got2 = scene.draw_open_seam_on_curved(host, on_seam);
    println!("APP SPHERE open seam (meridian to meridian) → {got2:?}");
    assert!(got2.is_none(), "also declined: {got2:?}");

    // Whatever the answer is, the mesh must be intact and say so honestly. A
    // refusal that has already cut something is the failure ADR-298 is named
    // for.
    if got.is_none() {
        assert_eq!(before, after, "a refusal must leave the mesh untouched");
    } else {
        let inv = scene.mesh.verify_face_invariants();
        assert!(inv.is_valid(), "a split must be manifold: {:?}", inv.violations);
    }
    // Pinned as measured. If the sphere goes back to the equator form, or the
    // seam path learns the axis form, this is the line that says so.
    assert!(
        got.is_none(),
        "measured: the seam path does not apply to the app's own sphere \
         (it wants the equator form's shared rim). Got {got:?}"
    );
}

/// The cone a user gets: a base disk and a side that share the base rim — the
/// shape the seam path was built for.
#[test]
fn the_app_cone_splits_along_a_drawn_seam() {
    let mut scene = Scene::new();
    scene.mesh.cone_path_b_default = true;
    scene
        .mesh
        .create_cone(DVec3::ZERO, 200.0, 400.0, 24, MaterialId::new(0))
        .expect("cone");
    let before = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("APP CONE {before} face(s) before");
    assert_eq!(before, 2, "base disk + side");

    let host = curved_face(&scene).expect("the cone side");
    // Rim, up toward the apex, and back down to the far rim.
    let seam = vec![
        DVec3::new(200.0, 0.0, 0.0),
        DVec3::new(0.0, 100.0, 200.0),
        DVec3::new(-200.0, 0.0, 0.0),
    ];
    let got = scene.draw_open_seam_on_curved(host, seam);
    println!("APP CONE open seam → {got:?}");
    assert!(got.is_some(), "the cone side must split along the seam");

    let after = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let inv = scene.mesh.verify_face_invariants();
    println!("APP CONE {after} face(s) after, valid={}", inv.is_valid());
    assert!(inv.is_valid(), "manifold after the split: {:?}", inv.violations);
    assert_eq!(after, 3, "two side pieces + the rebuilt base");
    assert_eq!(
        scene.mesh.detect_self_intersections().count(),
        0,
        "and nothing pierces anything"
    );
}

/// How close to the rim must a click be?
///
/// The Rust coverage picks its endpoints exactly on the rim, which no mouse
/// ever does — and on a cone the rim is shared with the base disc, so a click
/// aimed at it lands on the disc as often as on the side. What a real chain can
/// offer is endpoints a little way UP the side. This asks how far up still
/// splits, because the answer decides whether the seam is reachable by clicking
/// at all or only by an API call.
#[test]
fn how_far_up_the_side_a_seam_may_start() {
    for up in [0.0_f64, 1.0, 5.0, 20.0, 50.0] {
        let mut scene = Scene::new();
        scene.mesh.cone_path_b_default = true;
        scene
            .mesh
            .create_cone(DVec3::ZERO, 200.0, 400.0, 24, MaterialId::new(0))
            .expect("cone");
        let host = curved_face(&scene).expect("the cone side");
        // At height z the cone's radius is 200·(1 − z/400).
        let r = 200.0 * (1.0 - up / 400.0);
        let seam = vec![
            DVec3::new(r, 0.0, up),
            DVec3::new(0.0, 100.0, 200.0),
            DVec3::new(-r, 0.0, up),
        ];
        let before = scene.scene_snapshot();
        let got = scene.draw_open_seam_on_curved(host, seam);
        let faces = scene.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let inv = scene.mesh.verify_face_invariants();
        println!(
            "UP {up:>5} mm → {} ({faces} faces, valid={})",
            if got.is_some() { "split" } else { "declined" },
            inv.is_valid()
        );
        if got.is_none() {
            assert_eq!(before, scene.scene_snapshot(), "a refusal must leave the mesh alone");
        } else {
            assert!(inv.is_valid(), "a split must be manifold: {:?}", inv.violations);
        }
    }
}
