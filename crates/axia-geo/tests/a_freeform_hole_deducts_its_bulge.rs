//! A hole bounded by a curve deducts what the CURVE encloses.
//!
//! D9′ (found 2026-08-13 while measuring the plan's rows): a rect × ellipse
//! divide on a solid top tiles to 40,237.88 where 40,000 is the truth. The
//! pieces carry their freeform boundary and measure right; the ~238 is the
//! host's union HOLE deducting less than it should.
//!
//! Arcs were checked when the instruments were fixed
//! (`a_hole_bounded_by_arcs_deducts_its_arcs`, PR #124) and are exact. This
//! isolates the freeform kinds — Bezier, B-spline, NURBS — so a failure says
//! whether the instrument is short or the union loop simply has no curves on
//! it, which are different repairs.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::Mesh;
use axia_geo::{EdgeId, FaceId, MaterialId, VertId};
use glam::DVec3;

/// The edge between two vertices on any of a face's loops.
fn edge_between(mesh: &Mesh, fid: FaceId, a: VertId, b: VertId) -> EdgeId {
    let face = &mesh.faces[fid];
    let mut starts = vec![face.outer().start];
    starts.extend(face.inners().iter().map(|lr| lr.start));
    for start in starts {
        let Ok(hes) = mesh.collect_loop_hes(start) else { continue };
        for he in hes {
            let Ok(src) = mesh.he_src(he) else { continue };
            if (src == a && mesh.hes[he].dst() == b) || (src == b && mesh.hes[he].dst() == a) {
                return mesh.hes[he].edge();
            }
        }
    }
    panic!("no edge between {a:?} and {b:?}");
}

/// A 300 × 300 square with a triangular hole, one side of which bows out into
/// the hole as a quadratic Bezier. The bow encloses exactly 2/3 of its control
/// triangle over the chord — a closed form the sampler cannot copy, so the
/// number is an independent check of the instrument rather than of itself.
///
/// Hole corners are wound opposite the outer loop, and the bow points AWAY
/// from the hole's interior, so the hole encloses `triangle + 2/3 · h · w / 2`
/// and the face is `90,000 −` that.
#[test]
fn a_hole_with_a_bezier_side_deducts_the_bulge() {
    let mut mesh = Mesh::new();
    let o: Vec<VertId> = [(-150.0, -150.0), (150.0, -150.0), (150.0, 150.0), (-150.0, 150.0)]
        .iter()
        .map(|&(x, y): &(f64, f64)| mesh.add_vertex(DVec3::new(x, y, 0.0)))
        .collect();
    // Hole triangle (0,0) → (100,0) → (50,80), wound CW seen from +Z.
    let h0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let h1 = mesh.add_vertex(DVec3::new(50.0, 80.0, 0.0));
    let h2 = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let fid = mesh
        .add_face_with_holes(&o, &[&[h0, h1, h2]], MaterialId::new(0))
        .expect("square with a triangular hole");

    let triangle = 100.0 * 80.0 * 0.5; // 4,000
    let plain = mesh.face_area(fid);
    assert!(
        (plain - (90_000.0 - triangle)).abs() < 1e-6,
        "control: a straight-sided hole deducts its triangle — got {plain:.4}"
    );

    // Now bow the h2 → h0 side (the base, y = 0) downward, away from the hole.
    let eid = edge_between(&mesh, fid, h2, h0);
    mesh.edges[eid].set_curve(Some(AnalyticCurve::Bezier {
        control_pts: vec![
            DVec3::new(100.0, 0.0, 0.0),
            DVec3::new(50.0, -60.0, 0.0), // control apex below the chord
            DVec3::new(0.0, 0.0, 0.0),
        ],
    }));
    let bulge = (2.0 / 3.0) * (100.0 * 60.0 * 0.5); // 2,000
    let got = mesh.face_area(fid);
    assert!(
        (got - (90_000.0 - triangle - bulge)).abs() < 1.0,
        "the hole grew by its bow: expected {:.4}, got {got:.4} \
         (a result of {:.4} means the bulge was ignored, {:.4} means it was \
         added to the hole instead of taken out of the face)",
        90_000.0 - triangle - bulge,
        90_000.0 - triangle,
        90_000.0 - triangle + bulge
    );
}

/// The same shape as an OUTER boundary, so the two readings can be compared.
/// If this is exact and the hole above is not, the instrument treats a hole
/// loop differently — which is the thing to fix.
#[test]
fn the_same_bow_read_as_an_outer_boundary_is_exact() {
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(50.0, 80.0, 0.0));
    let fid = mesh.add_face(&[v0, v1, v2], MaterialId::new(0)).expect("triangle");
    let eid = edge_between(&mesh, fid, v0, v1);
    mesh.edges[eid].set_curve(Some(AnalyticCurve::Bezier {
        control_pts: vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(50.0, -60.0, 0.0), // bows away from the triangle
            DVec3::new(100.0, 0.0, 0.0),
        ],
    }));
    let expected = 100.0 * 80.0 * 0.5 + (2.0 / 3.0) * (100.0 * 60.0 * 0.5);
    let got = mesh.face_area(fid);
    assert!(
        (got - expected).abs() < 1.0,
        "outer bow: expected {expected:.4}, got {got:.4}"
    );
}
