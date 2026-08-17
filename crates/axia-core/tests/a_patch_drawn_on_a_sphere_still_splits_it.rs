//! The sphere patch from `curved-surface-osnap.spec.ts`, measured in the engine.
//!
//! That end-to-end test builds a Path B sphere, draws a closed polyline patch
//! on its +X side, then clicks near a rim vertex and far from everything. It
//! caught a change to `face_rederive`'s `onp_ve` filter — but what it asserts
//! at the point of failure is how deep the far click sits inside the analytic
//! sphere, which is the render tessellation's chord sag and not the split.
//!
//! So this asks the engine the question the E2E cannot: does the patch still
//! divide the sphere, and does the mesh stay sound?
//!
//! Measured, with the filter and with it reverted: **identical** — 2 faces to
//! 3, the same two FaceIds returned, and not one vertex off the sphere. The
//! E2E failure that started this was flakiness: the spec passes three times out
//! of three in isolation on the same build, and a full-suite run fails a
//! different spec each time (`adr-190-p3-repeat-last` on the next one), which
//! also passes three of three alone. Kept because the question is worth being
//! able to answer in one command instead of a stash, two rebuilds and a guess.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use axia_geo::FaceId;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

#[test]
fn a_closed_patch_on_a_sphere_divides_it() {
    const R: f64 = 100.0;
    let mut s = prod();
    s.mesh.create_sphere_kernel_native(DVec3::ZERO, R, FORM_MATERIAL).unwrap();
    let before = active(&s);

    // The same patch the E2E draws: a square on the +X side, in the plane x=R.
    let d = R * 0.4;
    let host = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .next()
        .expect("a sphere face to draw on");
    let pts = vec![
        DVec3::new(R, -d, -d),
        DVec3::new(R, d, -d),
        DVec3::new(R, d, d),
        DVec3::new(R, -d, d),
    ];
    let res = s.draw_polyline_on_sphere(host, pts, true);

    let after = active(&s);
    println!("\n  구 면 {before} → {after} (패치 {res:?})");
    let inv = s.mesh.verify_face_invariants();
    for v in inv.violations.iter() {
        println!("    ✗ {v:?}");
    }

    assert!(res.is_some(), "the patch has to be drawn: {res:?}");
    assert_eq!(
        after,
        before + 1,
        "a closed patch takes the sphere from {before} faces to {} — cap plus \
         what is left of the hemisphere it was drawn on",
        before + 1
    );
    assert!(inv.is_valid(), "and leaves the mesh sound: {:?}", inv.violations);
}

/// The patch's own rim is what the E2E snaps to, so it has to be there.
#[test]
fn the_patch_rim_sits_on_the_sphere() {
    const R: f64 = 100.0;
    let mut s = prod();
    s.mesh.create_sphere_kernel_native(DVec3::ZERO, R, FORM_MATERIAL).unwrap();
    let d = R * 0.4;
    let host: FaceId = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .next()
        .unwrap();
    let pts = vec![
        DVec3::new(R, -d, -d),
        DVec3::new(R, d, -d),
        DVec3::new(R, d, d),
        DVec3::new(R, -d, d),
    ];
    s.draw_polyline_on_sphere(host, pts, true).expect("the patch");

    let off: Vec<f64> = s
        .mesh
        .verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .map(|(_, v)| (v.pos().length() - R).abs())
        .filter(|e| *e > 1e-6)
        .collect();
    println!("\n  구 반지름을 벗어난 정점 {}개", off.len());
    for e in off.iter().take(5) {
        println!("    {e:.6} mm");
    }
    assert!(
        off.is_empty(),
        "every vertex of a sphere with a patch on it is ON the sphere — {} are \
         not, by up to {:.4} mm",
        off.len(),
        off.iter().cloned().fold(0.0, f64::max)
    );
}
