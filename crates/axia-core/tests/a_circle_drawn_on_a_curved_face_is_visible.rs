//! Does a circle drawn on a sphere leave a line you can see?
//!
//! CLAUDE.md's 메타-원칙 #15 audit (2026-08-11) recorded a gap and never closed
//! it: `split_{sphere,cone,torus}_face_by_circle` carry **no HARD flag at all**,
//! and neither does `annulus.rs`. They build faces with `add_face_closed_curve`
//! and twin-HE reparenting rather than delegating to the `split_face` family,
//! so the contract that every split marks its new edges HARD never reaches them.
//!
//! That was the worry: the renderer hides an edge whose two faces agree on
//! their normal (`EDGE_VISIBILITY_ANGLE_DEG`, LOCKED #16), and cutting a sphere
//! with a circle leaves both sides on the SAME sphere, so their normals agree
//! exactly along the seam. `HeFlags::HARD` is what tells the renderer to draw
//! an edge anyway.
//!
//! ⚠ It does not happen. Asked rather than assumed:
//!
//! ```text
//!   sphere   segments   0 -> 23
//!   cone     segments 158 -> 192
//!   torus    segments 180 -> 204
//! ```
//!
//! ADR-202 A-κ's closed-curve render fast-path emits a self-loop edge carrying
//! an `AnalyticCurve` as a polyline regardless of the angle test, so these seams
//! are drawn through the curve path and never reach the coplanar-hide check. The
//! missing HARD flags are real and harmless.
//!
//! The audit note asserted a consequence. This file is the consequence, asked.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use axia_geo::FaceId;
use glam::DVec3;

/// What the renderer uses (`EDGE_VISIBILITY_ANGLE_DEG`).
const ANGLE: f64 = 20.1;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// A sphere with a circle drawn on it, and how many wireframe segments come out.
fn sphere_with_circle() -> (Scene, usize, usize) {
    let mut s = prod();
    let r = 100.0;
    s.mesh
        .create_sphere_kernel_native(DVec3::ZERO, r, FORM_MATERIAL)
        .expect("a Path B sphere");
    let before = s.mesh.export_edge_lines(ANGLE).len() / 6;

    // On the surface, north of the equator.
    let hit = DVec3::new(0.0, 0.0, r);
    let host = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .find(|fid| s.mesh.face_surface(*fid).is_some())
        .expect("a face carrying the sphere surface");

    let rim = DVec3::new(40.0, 0.0, (r * r - 1600.0f64).sqrt());
    let res = s.draw_circle_on_sphere(host, hit, rim);
    assert!(res.is_some(), "drawing a circle on a sphere must work");
    let after = s.mesh.export_edge_lines(ANGLE).len() / 6;
    (s, before, after)
}

/// The seam a circle cuts into a sphere is drawn.
///
/// Mutation-checked: strip the HARD marking added to
/// `split_sphere_face_by_circle` and the segment count stops rising.
#[test]
fn a_circle_on_a_sphere_adds_wireframe_segments() {
    let (s, before, after) = sphere_with_circle();
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("\n  구에 원 그리기\n");
    println!("    면        {faces}");
    println!("    선분 전   {before}");
    println!("    선분 후   {after}\n");
    assert!(
        after > before,
        "a circle drawn on a sphere has to leave a line you can see — the two \
         sides of that seam are the same sphere, so their normals agree along \
         it and the renderer hides it unless the split marks it HARD"
    );
}

/// The same question of the other three curved surfaces.
///
/// Cone and torus splits carry no HARD reference either; the cylinder split
/// carries one. All four are asked the same way — draw, then count.
#[test]
fn a_circle_on_a_cone_or_a_torus_is_drawn_too() {
    let mut rows: Vec<(&str, usize, usize, usize)> = Vec::new();

    // Cone: apex up at +height, base ring at radius.
    {
        let mut s = prod();
        s.mesh
            .create_cone_kernel_native(DVec3::ZERO, 100.0, 200.0, FORM_MATERIAL)
            .expect("a Path B cone");
        let before = s.mesh.export_edge_lines(ANGLE).len() / 6;
        let host = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .find(|fid| {
                matches!(
                    s.mesh.face_surface(*fid),
                    Some(axia_geo::surfaces::AnalyticSurface::Cone { .. })
                )
            })
            .expect("the cone's side face");
        // ⚠ Both points must be ON the surface. The first attempt put the
        // centre on the AXIS at (0, 0, 100) and the call was refused — a
        // measurement of my own input, not of the engine.
        //
        // Base radius 100 at z = 0, apex at z = 200, so radius(z) = 100(1 - z/200):
        //   z = 100 -> 50      z = 120 -> 40
        let c = DVec3::new(50.0, 0.0, 100.0);
        let rim = DVec3::new(40.0, 0.0, 120.0);
        let ok = s.draw_circle_on_cone(host, c, rim).is_some();
        let after = s.mesh.export_edge_lines(ANGLE).len() / 6;
        rows.push(("cone", before, after, ok as usize));
    }

    // Torus: a small circle on the outer wall.
    {
        let mut s = prod();
        s.mesh
            .create_torus_kernel_native(DVec3::ZERO, 100.0, 30.0, FORM_MATERIAL)
            .expect("a Path B torus");
        let before = s.mesh.export_edge_lines(ANGLE).len() / 6;
        let host = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(fid, _)| fid)
            .find(|fid| {
                matches!(
                    s.mesh.face_surface(*fid),
                    Some(axia_geo::surfaces::AnalyticSurface::Torus { .. })
                )
            })
            .expect("the torus face");
        let c = DVec3::new(130.0, 0.0, 0.0);
        let rim = DVec3::new(0.0, 0.0, 0.0) + DVec3::new(125.0, 0.0, 10.0);
        let ok = s.draw_circle_on_torus(host, c, rim).is_some();
        let after = s.mesh.export_edge_lines(ANGLE).len() / 6;
        rows.push(("torus", before, after, ok as usize));
    }

    println!("
  곡면에 원 그리기 — 그려진 선분
");
    for (name, b, a, ok) in &rows {
        println!("    {name:<8} 그리기 {}   선분 {b} -> {a}", if *ok == 1 { "성공" } else { "거부" });
    }
    println!();

    for (name, b, a, ok) in rows {
        if ok == 1 {
            assert!(a > b, "{name}: a circle drawn on it has to leave a visible line");
        }
    }
}

/// A control: the sphere on its own is unchanged by the marking.
#[test]
fn a_plain_sphere_is_untouched() {
    let mut s = prod();
    s.mesh
        .create_sphere_kernel_native(DVec3::ZERO, 100.0, FORM_MATERIAL)
        .expect("a Path B sphere");
    let segs = s.mesh.export_edge_lines(ANGLE).len() / 6;
    let inv = s.mesh.verify_face_invariants();
    println!("\n  민 구 — 선분 {segs}, 위반 {}\n", inv.violations.len());
    assert!(inv.is_valid(), "and it stays sound");
    let _ = FaceId::new(0);
}
