//! A shape drawn right around its host makes the host a hole, not a lid.
//!
//! D2 of FACE-PIPELINE-PLAN-2026-08-13. Reverse containment — the drawn shape
//! CONTAINS the face it was drawn over — reaches neither containment detector
//! (`detect_circle_containment` / `detect_polygon_containment` both look for
//! something INSIDE the outer) nor `auto_intersect_coplanar` (which wants
//! crossings), so the disc used to sit ON the top: a Ø400 circle over a
//! 200 × 200 solid top measured 165,663.71 where πr² = 125,663.71 is the truth,
//! covering 40,000 mm² twice.
//!
//! Two levels, so a failure says WHERE — the same split that located D1.

use axia_geo::boundary_kernel::analytic_arrange::{arrange, InputCurve, SubCurve};
use axia_geo::boundary_kernel::geom2::Vec2;
use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;
use std::f64::consts::PI;

const TOP: f64 = 100.0;

/// The four sides of an axis-aligned box, as arrangement input.
fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<InputCurve> {
    let c = [
        Vec2::new(x0, y0),
        Vec2::new(x1, y0),
        Vec2::new(x1, y1),
        Vec2::new(x0, y1),
    ];
    (0..4)
        .map(|i| InputCurve::Line { a: c[i], b: c[(i + 1) % 4] })
        .collect()
}

/// Area enclosed by one region's outer boundary, arcs included.
fn outer_area(outer: &[SubCurve]) -> f64 {
    // A lone full circle is one Arc spanning 2π.
    if outer.len() == 1 {
        if let SubCurve::Arc { radius, a0, a1, .. } = &outer[0] {
            if (a1 - a0).abs() >= 2.0 * PI - 1e-6 {
                return PI * radius * radius;
            }
        }
    }
    let pts: Vec<Vec2> = outer
        .iter()
        .map(|sc| match sc {
            SubCurve::Line { a, .. } => *a,
            SubCurve::Arc { center, radius, a0, .. } => {
                Vec2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin())
            }
            SubCurve::Freeform { f2d, t0, .. } => f2d.eval(*t0),
        })
        .collect();
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut two_a = 0.0;
    for i in 0..n {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        two_a += p.x * q.y - q.x * p.y;
    }
    (two_a * 0.5).abs()
}

#[test]
fn a_circle_around_a_square_arranges_into_two() {
    let mut input = square(-100.0, -100.0, 100.0, 100.0);
    input.push(InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 200.0 });
    let faces = arrange(&input, 1e-4);
    assert_eq!(
        faces.len(),
        2,
        "the square, and the ring around it — got {}",
        faces.len()
    );
    // One region is the square; the other is the disc WITH the square as a hole.
    let mut areas: Vec<f64> = faces.iter().map(|f| outer_area(&f.outer)).collect();
    areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((areas[0] - 40_000.0).abs() < 1.0, "{areas:?}");
    assert!((areas[1] - PI * 200.0 * 200.0).abs() < 1.0, "{areas:?}");
    let holes: usize = faces.iter().map(|f| f.holes.len()).sum();
    assert_eq!(holes, 1, "the ring carries the square as its hole");
}

/// Areas of the faces the re-derive left on the top plane, smallest first.
fn top_plane_areas(mesh: &Mesh) -> Vec<f64> {
    let mut a: Vec<f64> = mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && f.normal().z.abs() > 0.999
                && mesh.face_bounds(*fid).map_or(false, |(lo, hi)| {
                    (lo.z - TOP).abs() < 1e-3 && (hi.z - TOP).abs() < 1e-3
                })
        })
        .map(|(fid, _)| mesh.face_area(fid))
        .collect();
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    a
}

#[test]
fn the_rederive_turns_a_swallowed_host_into_a_hole() {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    // A Path B disc right around the top: one anchor, one self-loop Circle.
    let anchor = mesh.add_vertex(DVec3::new(200.0, 0.0, TOP));
    let disc = mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle {
                center: DVec3::new(0.0, 0.0, TOP),
                radius: 200.0,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            },
            MaterialId::new(0),
        )
        .expect("disc");
    axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        &mut mesh,
        DVec3::new(0.0, 0.0, TOP),
        DVec3::Z,
        1e-3,
        true,
        Some(&[disc]),
    )
    .expect("re-derive");

    let a = top_plane_areas(&mesh);
    let total: f64 = a.iter().sum();
    let truth = PI * 200.0 * 200.0;
    assert!(
        (total - truth).abs() < 1.0,
        "the plane must hold the disc's {truth:.4} exactly once — got \
         {total:.4} from {a:?} ({:.4} means the top is still covered twice)",
        truth + 40_000.0
    );
    assert!(
        mesh.verify_face_invariants().is_valid(),
        "invariants must hold after swallowing"
    );
}
