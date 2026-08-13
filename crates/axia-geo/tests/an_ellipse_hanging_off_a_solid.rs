//! An ellipse hanging off a solid's edge leaves a piece, and the solid closed.
//!
//! Measured 2026-08-13 on `draw_freely_matrix`'s soundness grid: rect and
//! circle crossing or straddling a solid's edge go 6 → 8 faces and stay sealed;
//! the ELLIPSE goes 6 → 7 and the solid opens (3 or 4 faces still sealed of 6).
//! Six of the grid's twenty-seven solid cases, all of them the ellipse. On a
//! SHEET the same ellipse works (1 → 3), so it is solid-specific.
//!
//! `DrawEllipseAsCurve` does not go through `exec_draw_rect`'s four-line
//! pipeline, so D1's mechanism (synthesis + the closed-shape cleanup) cannot be
//! the cause here. Two levels, so a failure says which.

use axia_geo::curves::nurbs;
use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const TOP: f64 = 100.0;

/// A 200-cube with a Path B ellipse face on its top plane, ready for the
/// re-derive. `cx` places the centre; rx/ry are the radii.
fn box_with_ellipse_on_top(cx: f64, rx: f64, ry: f64) -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let centre = DVec3::new(cx, 0.0, TOP);
    let (cp, w, k, deg) = nurbs::ellipse(centre, rx, ry, DVec3::X, DVec3::Y);
    let anchor = mesh.add_vertex(cp[0]);
    let fid = mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: deg as u32 },
            MaterialId::new(0),
        )
        .expect("ellipse face");
    (mesh, vec![fid])
}

fn rederive_at(mesh: &mut Mesh, seed: &[FaceId], plane_origin: DVec3) {
    axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        mesh,
        plane_origin,
        DVec3::Z,
        1e-3,
        true,
        Some(seed),
    )
    .expect("re-derive");
}

fn rederive(mesh: &mut Mesh, seed: &[FaceId]) {
    rederive_at(mesh, seed, DVec3::new(0.0, 0.0, TOP));
}

/// Faces on the top plane, split by whether they stay inside the top's
/// footprint (|x| ≤ 100) or reach past it.
fn inside_and_hanging(mesh: &Mesh) -> (f64, f64) {
    let (mut inside, mut hanging) = (0.0, 0.0);
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() || f.normal().z.abs() <= 0.999 {
            continue;
        }
        let Some((lo, hi)) = mesh.face_bounds(fid) else { continue };
        if (lo.z - TOP).abs() > 1e-3 || (hi.z - TOP).abs() > 1e-3 {
            continue;
        }
        if hi.x > 100.0 + 1e-3 {
            hanging += mesh.face_area(fid);
        } else {
            inside += mesh.face_area(fid);
        }
    }
    (inside, hanging)
}

/// The Scene derives the re-derive's PLANE from the drawn face — its normal
/// and its first loop vertex — so a Path B ellipse must carry a usable normal.
/// A self-loop face has one vertex and no polygon to take a normal from, which
/// is exactly the assumption that has misled three instruments in this area
/// already, so it is asserted rather than assumed.
#[test]
fn a_path_b_ellipse_face_knows_which_way_it_faces() {
    let (mesh, seed) = box_with_ellipse_on_top(100.0, 40.0, 25.0);
    let f = mesh.faces.get(seed[0]).expect("ellipse face");
    let n = f.normal();
    assert!(
        n.length() > 0.5,
        "the face must have a unit-ish normal, got {n:?} — a zero normal would \
         send the Scene's re-derive to a plane that matches nothing"
    );
    assert!(
        n.z.abs() > 0.999,
        "and it must be the drawing plane's, got {n:?}"
    );
    // And its SURFACE must read as planar. The Scene's `intersect_faces_inner`
    // drops every face whose surface is curved before it re-derives (ADR-197 —
    // a Path B sphere must not be collapsed into a disk), so an ellipse that
    // came out carrying anything but `Plane` would be filtered out and never
    // re-derived at all.
    use axia_geo::surfaces::AnalyticSurface as S;
    match mesh.face_surface(seed[0]) {
        None | Some(S::Plane { .. }) => {}
        other => panic!(
            "a planar ellipse must carry Plane or nothing, got {other:?} — the \
             Scene's planar filter would drop it"
        ),
    }
}

/// The Scene does not pass the shape's CENTRE as the plane point — it reads the
/// first vertex of the drawn face's loop, which for a Path B face is the anchor
/// on the RIM. The plane is the same either way (both points lie on z = TOP),
/// so this must not change the answer. It is asked because it is the one input
/// that provably differs between the working call one layer down and the
/// failing one in the Scene.
#[test]
fn the_plane_point_does_not_have_to_be_the_centre() {
    let (mut mesh, seed) = box_with_ellipse_on_top(100.0, 40.0, 25.0);
    // The anchor of a NURBS ellipse is cp[0] = centre + rx·u — on the rim, and
    // in this placement OUTSIDE the host's footprint (x = 140 > 100).
    let anchor_pos = mesh
        .collect_loop_verts(mesh.faces[seed[0]].outer().start)
        .ok()
        .and_then(|v| v.first().and_then(|&x| mesh.vertex_pos(x).ok()))
        .expect("the anchor");
    rederive_at(&mut mesh, &seed, anchor_pos);
    let (inside, hanging) = inside_and_hanging(&mesh);
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "the top tiles exactly once whichever point names the plane — got \
         {inside:.4} from origin {anchor_pos:?}"
    );
    assert!(hanging > 100.0, "and the half past the edge is its own face");
}

/// The control: a circle in the same place, which the grid says works. If this
/// ever fails, the ellipse is not the special one and the whole reading changes.
#[test]
fn a_circle_straddling_the_edge_leaves_a_piece() {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let anchor = mesh.add_vertex(DVec3::new(140.0, 0.0, TOP));
    let fid = mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle {
                center: DVec3::new(100.0, 0.0, TOP),
                radius: 40.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("circle face");
    rederive(&mut mesh, &[fid]);
    let (inside, hanging) = inside_and_hanging(&mesh);
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "the top tiles exactly once — got {inside:.4}"
    );
    assert!(
        hanging > 100.0,
        "half the disc hangs past the edge — got {hanging:.4}"
    );
}

#[test]
fn an_ellipse_straddling_the_edge_leaves_a_piece() {
    // Centre on the top's edge at x = 100, so half the ellipse hangs over.
    let (mut mesh, seed) = box_with_ellipse_on_top(100.0, 40.0, 25.0);
    rederive(&mut mesh, &seed);
    let (inside, hanging) = inside_and_hanging(&mesh);
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "the top must tile exactly once — got {inside:.4} (less means the \
         ellipse ate part of it, more means double cover)"
    );
    assert!(
        hanging > 100.0,
        "half the ellipse hangs past the edge and must be its own face — got \
         {hanging:.4}"
    );
    assert!(
        mesh.verify_face_invariants().is_valid(),
        "invariants must hold"
    );
}
