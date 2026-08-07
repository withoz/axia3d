//! TWO DEFINITIONS OF A SPHERE, MEASURED SIDE BY SIDE.
//!
//! 사용자 지적 2026-08-07: "구를 그릴때 원으로 그리는것은 단순히 모양을 잡기
//! 위함이지 원을 구로 포함시키면 안되요. 구는 축을 중심으로 그려져야 하지
//! 않나요?" — and separately, that the volume needs only the radius, and that
//! the 4-face closure requirement is itself wrong.
//!
//! This file is the measurement those questions asked for. It asserts nothing
//! about which definition is right; it records what each one actually produces
//! so the choice is made on numbers. Deliberately assert-light — the numbers in
//! the comments are the point, and `--nocapture` prints them.
//!
//! Measured 2026-08-07, r = 50 (true area 31416, true volume 523599):
//!
//! ```text
//!                        A: equator in DCEL     B: poles on the axis
//!   verts                1 (on the equator)     2 (the poles)
//!   faces                2 (v_range halved)     1 (v_range whole)
//!   patched up with      SOFT flag + shared     nothing
//!                        surface_owner_id
//!   invariants           0                      1  "outer loop has 2 verts"
//!   tessellated tris     5112                   5112   ← identical render
//!   volume               0.0                    0.0    ← neither works
//! ```
//!
//! And the volume, by method:
//!
//! ```text
//!   analytic 4/3πr³        523598.8   exact — needs only the radius
//!   tessellation tol 0.1   520162.9   0.656%
//!   tessellation tol 0.05  521920.7   0.320%
//!   tessellation tol 0.01  523253.9   0.066%
//! ```
use axia_core::{Scene, FORM_MATERIAL};
use glam::DVec3;

fn report(s: &Scene, tag: &str) {
    let faces: Vec<axia_geo::FaceId> =
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let verts = s.mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
    let edges = s.mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
    let mi = s.mesh.face_set_manifold_info(&faces);
    let inv = s.mesh.verify_face_invariants();
    let vol = axia_core::promote::face_set_volume(&s.mesh, &faces);
    let (lines, _) = s.mesh.export_edge_lines_with_map(20.1);
    let tris: usize = faces
        .iter()
        .filter_map(|&f| s.mesh.tessellate_face_surface(f, 0.05))
        .map(|t| t.triangles.len())
        .sum();
    println!("=== {tag} ===");
    println!("  verts={verts} edges={edges} faces={}", faces.len());
    println!("  closed={} bnd={} nm={} viol={} si={}",
        mi.is_closed_solid, mi.boundary_edge_count, mi.non_manifold_edge_count,
        inv.violations.len(), s.mesh.detect_self_intersections().count());
    println!("  volume={vol:.1}   wireframe_segments={}   tessellated_tris={tris}", lines.len() / 6);
    for v in inv.violations.iter().take(3) {
        println!("    VIOL: {v}");
    }
    for (fid, f) in s.mesh.faces.iter().filter(|(_, f)| f.is_active()) {
        let n = s.mesh.collect_loop_verts(f.outer().start).map(|v| v.len()).unwrap_or(0);
        if let Some(surf) = f.surface() {
            let d = format!("{surf:?}");
            let vr = d.split("v_range: ").nth(1).unwrap_or("?").trim_end_matches(" }").to_string();
            println!("    face {fid:?} boundary_verts={n} v_range={vr}");
        }
    }
}

#[test]
fn sim_compare_two_sphere_definitions() {
    println!("(true values for r=50: area 31416, volume 523599)\n");

    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.create_sphere(DVec3::ZERO, 50.0, 16, 12, FORM_MATERIAL).unwrap();
    report(&s, "A — SHIPPED: equator in the DCEL, 2 hemispheres");

    let mut s = Scene::new();
    s.mesh.sim_create_sphere_axis_native(DVec3::ZERO, 50.0, DVec3::Z, FORM_MATERIAL).unwrap();
    report(&s, "B — SIM: poles on the axis, one seam, ONE face");
}

/// Volume from the SURFACE, not from a polygon boundary. Divergence theorem
/// over the surface's own tessellation: V = (1/6)Σ p0·(pa×pb).
fn volume_from_surface(s: &Scene, chord_tol: f64) -> f64 {
    let mut total = 0.0;
    for (fid, f) in s.mesh.faces.iter().filter(|(_, f)| f.is_active()) {
        let Some(t) = s.mesh.tessellate_face_surface(fid, chord_tol) else {
            let _ = f;
            continue;
        };
        for tri in &t.triangles {
            let (a, b, c) = (
                t.vertices[tri[0] as usize],
                t.vertices[tri[1] as usize],
                t.vertices[tri[2] as usize],
            );
            total += a.dot(b.cross(c));
        }
    }
    (total / 6.0).abs()
}

#[test]
fn sim_volume_from_the_surface() {
    let truth = 4.0 / 3.0 * std::f64::consts::PI * 50.0_f64.powi(3);
    println!("true volume r=50 = {truth:.1}");
    println!("analytic 4/3πr³ straight from the radius = {truth:.1}  (exact, no boundary needed)");
    for tol in [0.5, 0.1, 0.05, 0.01] {
        let mut s = Scene::new();
        s.mesh.sim_create_sphere_axis_native(DVec3::ZERO, 50.0, DVec3::Z, FORM_MATERIAL).unwrap();
        let v = volume_from_surface(&s, tol);
        println!("  tessellation chord_tol={tol:<5} → {v:>12.1}   error {:>7.3}%",
            (v - truth).abs() / truth * 100.0);
    }
    // and the shipped 2-face sphere, same method
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.create_sphere(DVec3::ZERO, 50.0, 16, 12, FORM_MATERIAL).unwrap();
    let v = volume_from_surface(&s, 0.05);
    println!("  (shipped 2-face sphere, tol=0.05) → {v:.1}   error {:.3}%",
        (v - truth).abs() / truth * 100.0);
}
