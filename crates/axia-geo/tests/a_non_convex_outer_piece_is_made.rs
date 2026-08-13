//! A shape hanging past a face's CORNER keeps its outer piece.
//!
//! D1 of FACE-PIPELINE-PLAN-2026-08-13, narrowed by the 2026-08-13 divergence
//! measurement (`axia-core/tests/what_divides_on_a_solid_top.rs`): a rect
//! straddling a solid top's EDGE leaves its hanging half behind (45,000 = the
//! top's 40,000 + a 5,000 rectangle), while the same rect straddling a CORNER
//! loses its hanging piece entirely (40,000, nothing past x = 100). The
//! difference between the two outer regions is that the corner's is an L —
//! non-convex, with a reflex vertex at the top's own corner.
//!
//! Two levels, so a failure says WHERE:
//!
//!   - the 2D arrangement (`arrange`) on two squares overlapping at a corner
//!     must report three regions — that is planar subdivision, nothing to do
//!     with solids or the DCEL;
//!   - the same draw through the engine must leave a face for the third one.
//!
//! The region-1 piece (the top minus the corner bite) is ALSO non-convex and
//! is made, so "non-convex" alone was never the whole story — which is why the
//! arrangement level is asserted separately rather than assumed.

use axia_geo::boundary_kernel::analytic_arrange::{arrange, InputCurve, SubCurve};
use axia_geo::boundary_kernel::geom2::Vec2;
use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;

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

/// The area a region's boundary encloses (shoelace over the sub-curve corners —
/// every input here is straight, so the corners are the whole story).
fn region_area(outer: &[SubCurve]) -> f64 {
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
        let p = pts[i];
        let q = pts[(i + 1) % n];
        two_a += p.x * q.y - q.x * p.y;
    }
    (two_a * 0.5).abs()
}

fn areas(curves: &[InputCurve]) -> Vec<f64> {
    let mut a: Vec<f64> = arrange(curves, 1e-4)
        .iter()
        .map(|f| region_area(&f.outer))
        .collect();
    a.sort_by(|x, y| x.partial_cmp(y).unwrap());
    a
}

/// The control: overlapping at an EDGE. Both outer regions are rectangles.
#[test]
fn two_squares_overlapping_at_an_edge_arrange_into_three() {
    let mut input = square(-100.0, -100.0, 100.0, 100.0); // the "top"
    input.extend(square(50.0, -50.0, 150.0, 50.0)); // the drawn rect
    let a = areas(&input);
    assert_eq!(a.len(), 3, "top-only + overlap + hanging piece — got {a:?}");
    // overlap 50×100 = 5,000; hanging 50×100 = 5,000; top minus overlap = 35,000.
    assert!((a[0] - 5_000.0).abs() < 1e-6, "{a:?}");
    assert!((a[1] - 5_000.0).abs() < 1e-6, "{a:?}");
    assert!((a[2] - 35_000.0).abs() < 1e-6, "{a:?}");
}

/// The case under test: overlapping at a CORNER. The hanging region is an L.
#[test]
fn two_squares_overlapping_at_a_corner_arrange_into_three() {
    let mut input = square(-100.0, -100.0, 100.0, 100.0); // the "top"
    input.extend(square(50.0, 50.0, 150.0, 150.0)); // the drawn rect
    let a = areas(&input);
    assert_eq!(
        a.len(),
        3,
        "top-minus-bite + overlap + the L — got {a:?} (if this is 2, the \
         arrangement itself drops the non-convex region and the fix belongs \
         in the kernel, not in the re-derive)"
    );
    // overlap 50×50 = 2,500; the L = 10,000 − 2,500 = 7,500; the bitten top =
    // 40,000 − 2,500 = 37,500.
    assert!((a[0] - 2_500.0).abs() < 1e-6, "{a:?}");
    assert!((a[1] - 7_500.0).abs() < 1e-6, "the L must be 7,500 — got {a:?}");
    assert!((a[2] - 37_500.0).abs() < 1e-6, "{a:?}");
}

// ── the re-derive, one level below the Scene ────────────────────────────────

const TOP: f64 = 100.0;

/// A 200-cube plus a loose rect drawn on its top plane, ready for the re-derive.
fn box_with_rect_on_top(x0: f64, y0: f64, x1: f64, y1: f64) -> (Mesh, Vec<axia_geo::FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    let v: Vec<_> = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .iter()
        .map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, TOP)))
        .collect();
    let rect = mesh.add_face(&v, MaterialId::new(0)).expect("rect");
    (mesh, vec![rect])
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

fn rederive(mesh: &mut Mesh, seed: &[axia_geo::FaceId]) {
    axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped(
        mesh,
        DVec3::new(0.0, 0.0, TOP),
        DVec3::Z,
        1e-3,
        true,
        Some(seed),
    )
    .expect("re-derive");
}

/// The control again, one level down: the convex hanging piece survives the
/// re-derive, so the DCEL half of the pipeline can carry an outer region.
#[test]
fn the_rederive_keeps_a_convex_hanging_piece() {
    let (mut mesh, seed) = box_with_rect_on_top(50.0, -50.0, 150.0, 50.0);
    rederive(&mut mesh, &seed);
    let a = top_plane_areas(&mesh);
    assert_eq!(a.len(), 3, "top-minus-overlap + overlap + hanging — got {a:?}");
    assert!((a.iter().sum::<f64>() - 45_000.0).abs() < 1e-6, "{a:?}");
}

#[test]
fn the_rederive_keeps_a_non_convex_hanging_piece() {
    let (mut mesh, seed) = box_with_rect_on_top(50.0, 50.0, 150.0, 150.0);
    rederive(&mut mesh, &seed);
    let a = top_plane_areas(&mesh);
    assert_eq!(
        a.len(),
        3,
        "bitten top + overlap + the L — got {a:?} (2 means the re-derive \
         dropped the L that `arrange` handed it)"
    );
    assert!(
        (a.iter().sum::<f64>() - 47_500.0).abs() < 1e-6,
        "40,000 of top plus the 7,500 L — got {a:?}"
    );
}

/// And the piece is not bought with a broken mesh.
///
/// ⚠ TWO instruments were tried for "is the box still the box" and BOTH lie
/// once a sheet hangs off the solid — recorded so the next reader does not
/// spend the same afternoon:
///
/// - a hand-rolled `is_face_in_volume` set + `face_set_manifold_info` reported
///   the box OPEN with 10 boundary edges for BOTH straddles, including the one
///   that ships and measures sound. A re-tiled top piece that now borders a
///   hanging sheet stops classifying into the volume, so the set was missing
///   the top and its own perimeter read as boundary.
/// - `mesh_volume` reads **8,166,666.67** for the edge straddle: the fan counts
///   the hanging 50×100 sheet at z=100, contributing exactly 100 × 5,000 / 3 =
///   166,666.67. Predicted to the decimal, so it is the instrument, not damage.
///
/// A hanging sheet shares its rim edge with the wall AND the top piece — three
/// faces on one edge, a T-junction the draw guard explicitly allows (SketchUp
/// makes exactly this). Whole-scene closure is therefore the wrong question
/// here; what IS well defined is that the invariants hold, and that is what
/// this asserts. "The solid is still sound" belongs at the Scene layer, where
/// `volume_face_count` exists for precisely this reason (scene.rs).
#[test]
fn a_hanging_piece_leaves_the_invariants_intact() {
    for (name, r) in [
        ("edge", (50.0, -50.0, 150.0, 50.0)),
        ("corner", (50.0, 50.0, 150.0, 150.0)),
    ] {
        let (mut mesh, seed) = box_with_rect_on_top(r.0, r.1, r.2, r.3);
        rederive(&mut mesh, &seed);
        let report = mesh.verify_face_invariants();
        assert!(
            report.is_valid(),
            "{name}: invariants must hold — {:?}",
            report.violations
        );
    }
}
