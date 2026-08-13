//! A split line on a curved face shows.
//!
//! 메타-원칙 #15: same split op, same topological contract — split-induced
//! edges carry `HARD` so the wireframe draws them. Every planar split honours
//! it (`Mesh::split_face`, chains, coplanar, ADR-101 Amendment 10). The curved
//! splits do not — measured on this branch before the fix, the N seam edges of
//! a porthole on a cylinder wall mapped to **0** wireframe segments, because
//! cap and remainder inherit the SAME surface instance (ADR-089 A-χ) and the
//! smooth-group hide (A-τ) sees one surface, not a user's cut.
//!
//! The one curved split that already showed is the sphere × analytic circle:
//! its seam is a self-loop `Circle`, and the closed-curve fast-path (A-κ)
//! draws those before any flag or smooth-group is consulted. That is pinned
//! here as the control, so the two rendering routes cannot drift apart again.
//! (D6 of FACE-PIPELINE-PLAN-2026-08-13; docs/plans, step C-1.)

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

/// How many wireframe segments the cap's boundary edges produce.
fn seam_segments(mesh: &Mesh, cap: FaceId) -> usize {
    let start = mesh.faces.get(cap).expect("cap").outer().start;
    let seam: std::collections::HashSet<u32> = mesh
        .collect_loop_hes(start)
        .expect("cap loop")
        .iter()
        .map(|&he| mesh.hes[he].edge().raw())
        .collect();
    let (_lines, edge_map) = mesh.export_edge_lines_with_map(20.1);
    edge_map.iter().filter(|e| seam.contains(e)).count()
}

#[test]
fn a_porthole_on_a_cylinder_wall_shows_its_rim() {
    use axia_geo::surfaces::cylinder;
    let mut mesh = Mesh::new();
    mesh.set_cylinder_path_b_default(true);
    let faces = mesh
        .create_cylinder(DVec3::ZERO, 10.0, 20.0, 16, MaterialId::new(0))
        .expect("cylinder");
    let annulus = faces[2];
    let (ax_o, ax_d, rad, refd, vlo, vhi) = match mesh.face_surface(annulus).unwrap() {
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, v_range, .. } => {
            (*axis_origin, *axis_dir, *radius, *ref_dir, v_range.0, v_range.1)
        }
        _ => panic!("Cylinder"),
    };
    let vmid = 0.5 * (vlo + vhi);
    let cp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.0, vmid);
    let rp = cylinder::evaluate(ax_o, ax_d, rad, refd, 0.4, vmid);
    let samples =
        cylinder::circle_on_cylinder(ax_o, ax_d, rad, refd, cp, rp, 0.05).expect("geodesic");
    let (cap, _host) = mesh.split_cylinder_face_by_circle(annulus, &samples).expect("split");
    let segs = seam_segments(&mesh, cap);
    assert!(
        segs > 0,
        "the porthole's rim must reach the wireframe — 0 segments means the \
         smooth-group hide swallowed a user's cut"
    );
}

#[test]
fn a_porthole_on_a_cone_wall_shows_its_rim() {
    use axia_geo::surfaces::cone;
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_cone_kernel_native(DVec3::ZERO, 10.0, 20.0, MaterialId::new(0))
        .expect("cone");
    let side = faces
        .iter()
        .copied()
        .find(|&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Cone { .. })))
        .expect("cone side");
    let (apex, ad, ha, rd, vlo, vhi) = match mesh.face_surface(side).cloned().unwrap() {
        AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. } => {
            (apex, axis_dir, half_angle, ref_dir, v_range.0, v_range.1)
        }
        _ => unreachable!(),
    };
    let vmid = 0.5 * (vlo + vhi);
    let cp = cone::evaluate(apex, ad, ha, rd, 0.0, vmid);
    let rp = cone::evaluate(apex, ad, ha, rd, 0.4, vmid);
    let samples = cone::circle_on_cone(apex, ad, ha, rd, cp, rp, 0.05).expect("geodesic");
    let (cap, _host) = mesh.split_cone_face_by_circle(side, &samples).expect("split");
    assert!(seam_segments(&mesh, cap) > 0, "the cone porthole's rim must reach the wireframe");
}

#[test]
fn a_rect_on_a_torus_shows_its_outline() {
    use axia_geo::surfaces::torus;
    let mut mesh = Mesh::new();
    let face = mesh
        .create_torus_kernel_native(DVec3::ZERO, 10.0, 3.0, MaterialId::new(0))
        .expect("torus");
    let (c, ax, refd, rmaj, rmin) = match mesh.face_surface(face).cloned().unwrap() {
        AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } => {
            (center, axis_dir, ref_dir, major_radius, minor_radius)
        }
        _ => panic!("Torus"),
    };
    let corners: Vec<DVec3> = [(-0.2, -0.3), (0.2, -0.3), (0.2, 0.3), (-0.2, 0.3)]
        .iter()
        .map(|&(u, v)| torus::evaluate(c, ax, refd, rmaj, rmin, u, v))
        .collect();
    let samples =
        torus::polyline_on_torus(c, ax, refd, rmaj, rmin, &corners, true, 0.3).expect("polyline");
    let (cap, _host) = mesh.split_torus_face_by_circle(face, &samples).expect("split");
    assert!(seam_segments(&mesh, cap) > 0, "the torus outline must reach the wireframe");
}

#[test]
fn a_rect_on_a_sphere_shows_its_outline() {
    use axia_geo::surfaces::sphere;
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_sphere_kernel_native(DVec3::ZERO, 10.0, MaterialId::new(0))
        .expect("sphere");
    let hemi = faces[0];
    let (c, rad, ax, refd) = match mesh.face_surface(hemi).cloned().unwrap() {
        AnalyticSurface::Sphere { center, radius, axis_dir, ref_dir, .. } => {
            (center, radius, axis_dir, ref_dir)
        }
        _ => panic!("Sphere"),
    };
    let corners: Vec<DVec3> = [(-0.2, 0.3), (0.2, 0.3), (0.2, 0.6), (-0.2, 0.6)]
        .iter()
        .map(|&(u, v)| sphere::evaluate(c, rad, ax, refd, u, v))
        .collect();
    let samples =
        sphere::polyline_on_sphere(c, rad, ax, refd, &corners, true, 0.3).expect("polyline");
    let (cap, _host) = mesh.split_sphere_face_by_polyline(hemi, &samples).expect("split");
    assert!(seam_segments(&mesh, cap) > 0, "the sphere outline must reach the wireframe");
}

#[test]
fn an_open_seam_across_a_hemisphere_shows() {
    // A freehand stroke rim-to-rim over the pole divides the hemisphere in
    // two. Both pieces inherit the same Sphere, so without HARD the seam is
    // a smooth-group interior edge and the stroke the user just drew is gone.
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_sphere_kernel_native(DVec3::ZERO, 10.0, MaterialId::new(0))
        .expect("sphere");
    let seam = [
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(7.0, 0.0, 7.2),
        DVec3::new(0.0, 0.0, 10.0),
        DVec3::new(-7.0, 0.0, 7.2),
        DVec3::new(-10.0, 0.0, 0.0),
    ];
    let (piece_a, piece_b, _) = faces
        .iter()
        .find_map(|&f| mesh.split_curved_face_by_open_seam(f, &seam))
        .expect("one hemisphere takes the seam");
    // The seam = every edge bordering BOTH pieces.
    let seam_edges: std::collections::HashSet<u32> = mesh
        .collect_loop_hes(mesh.faces.get(piece_a).expect("piece a").outer().start)
        .expect("loop")
        .iter()
        .filter_map(|&he| {
            let eid = mesh.hes[he].edge();
            let twin = mesh.hes[he].next_rad();
            (!twin.is_null() && mesh.hes[twin].face() == piece_b).then(|| eid.raw())
        })
        .collect();
    assert!(!seam_edges.is_empty(), "the two pieces share the seam");
    let (_lines, edge_map) = mesh.export_edge_lines_with_map(20.1);
    let segs = edge_map.iter().filter(|e| seam_edges.contains(e)).count();
    assert!(segs > 0, "the stroke the user drew must reach the wireframe");
}

/// The control — the one curved split that always showed. A circle on a
/// sphere is a self-loop `Circle` seam, drawn by the closed-curve fast-path
/// before flags or smooth-groups are consulted. If this ever goes dark, the
/// fast-path itself changed, which is a different conversation than D6.
#[test]
fn a_circle_on_a_sphere_already_shows_and_keeps_showing() {
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_sphere_kernel_native(DVec3::ZERO, 10.0, MaterialId::new(0))
        .expect("sphere");
    // Latitude circle at z = 5 on the r = 10 sphere: radius √75.
    let circle = AnalyticCurve::Circle {
        center: DVec3::new(0.0, 0.0, 5.0),
        radius: 75.0_f64.sqrt(),
        normal: DVec3::Z,
        basis_u: DVec3::X,
    };
    let (cap, _host) = faces
        .iter()
        .find_map(|&f| mesh.split_sphere_face_by_circle(f, circle.clone()))
        .expect("one hemisphere holds the circle");
    assert!(
        seam_segments(&mesh, cap) > 0,
        "the self-loop circle seam is drawn by the closed-curve fast-path"
    );
}
