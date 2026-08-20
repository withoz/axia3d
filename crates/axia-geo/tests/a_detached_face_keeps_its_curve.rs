//! A face detached to repair a non-manifold edge came back straight.
//!
//! `detach_face_groups` duplicates the shared vertices so two face groups on one
//! edge come apart, then rebuilds each face with `add_face(&substituted, mat)`.
//! That call takes vertices and a material and nothing else — so a boundary that
//! followed an `Arc` was rebuilt as its chord, while the face on the other side
//! (not detached) kept the bow.
//!
//! The lens between a bow and its chord is a genuine double cover, and the
//! repair was making them. Measured 2026-08-19 on a user's file: the repair
//! traded three exact-duplicate face pairs for two overlaps, 17,107 mm² and
//! 0.019 mm², each between an `Arc` edge and a straight twin joining the same
//! two positions.
//!
//! The duplicated vertices sit at the ORIGINAL positions, so the curve applies
//! to the new edge unchanged.

use axia_geo::curves::AnalyticCurve;
use axia_geo::{MaterialId, Mesh};

const FORM_MATERIAL: MaterialId = MaterialId::new(0);
use glam::DVec3;

/// Three faces on one edge, the middle one carrying an `Arc`, then repaired.
///
/// Mutation-checked: drop the curve carry-over in `detach_face_groups` and the
/// rebuilt face comes back with no `Arc` on its boundary.
#[test]
fn a_repair_that_detaches_a_face_keeps_the_arc_on_its_boundary() {
    let mut m = Mesh::new();

    // One shared edge, three faces hanging off it — a non-manifold edge the
    // repair will have to take apart.
    let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = m.add_vertex(DVec3::new(50.0, 80.0, 0.0));
    let d = m.add_vertex(DVec3::new(50.0, -80.0, 0.0));
    let e = m.add_vertex(DVec3::new(50.0, 0.0, 80.0));

    m.add_face(&[a, b, c], FORM_MATERIAL).expect("face 1");
    m.add_face(&[a, b, d], FORM_MATERIAL).expect("face 2");
    m.add_face(&[a, b, e], FORM_MATERIAL).expect("face 3");

    // Give the shared edge an Arc — a bow from a to b.
    let shared = m.find_edge(a, b).expect("the shared edge");
    m.edges
        .get_mut(shared)
        .expect("edge")
        .set_curve(Some(AnalyticCurve::Arc {
            center: DVec3::new(50.0, -50.0, 0.0),
            radius: 70.71,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 2.356,
            end_angle: 0.785,
        }));

    let nm_before = m.collect_non_manifold_edges().len();
    assert!(nm_before > 0, "the fixture has to be non-manifold to repair");

    let report = m.repair_non_manifold_edges_geometric();
    assert!(report.faces_detached > 0, "the repair has to detach something");
    assert_eq!(m.collect_non_manifold_edges().len(), 0, "and finish the job");

    // Every face that still runs between those two POSITIONS must carry the arc
    // — the detached one included. Positions, not ids: the whole point of a
    // detach is that the ids are no longer shared.
    let at = |v: axia_geo::VertId, p: DVec3| {
        m.vertex_pos(v).map(|q| (q - p).length() < 1e-6).unwrap_or(false)
    };
    let pa = DVec3::new(0.0, 0.0, 0.0);
    let pb = DVec3::new(100.0, 0.0, 0.0);
    let mut spans = 0;
    let mut curved = 0;
    for (eid, edge) in m.edges.iter().filter(|(_, e)| e.is_active()) {
        let (s, l) = (edge.v_small(), edge.v_large());
        if (at(s, pa) && at(l, pb)) || (at(s, pb) && at(l, pa)) {
            spans += 1;
            if edge.curve().is_some() {
                curved += 1;
            }
            let _ = eid;
        }
    }
    println!("\n  (0,0,0)-(100,0,0) 를 잇는 모서리 {spans} 개, 그중 호를 가진 것 {curved}\n");
    assert!(spans >= 2, "the detach has to have made a second edge there");
    assert_eq!(
        spans, curved,
        "every edge joining those two points must carry the arc — one bowing and \
         one cutting the chord is a double cover, which is what the repair used \
         to leave behind"
    );
}

/// And the repair does not lose a face's analytic surface either.
#[test]
fn a_repair_that_detaches_a_face_keeps_its_surface() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = m.add_vertex(DVec3::new(50.0, 80.0, 0.0));
    let d = m.add_vertex(DVec3::new(50.0, -80.0, 0.0));
    let e = m.add_vertex(DVec3::new(50.0, 0.0, 80.0));
    let f1 = m.add_face(&[a, b, c], FORM_MATERIAL).expect("face 1");
    m.add_face(&[a, b, d], FORM_MATERIAL).expect("face 2");
    let f3 = m.add_face(&[a, b, e], FORM_MATERIAL).expect("face 3");

    let plane = axia_geo::surfaces::AnalyticSurface::Plane {
        origin: DVec3::ZERO,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        u_range: (-1e6, 1e6),
        v_range: (-1e6, 1e6),
    };
    m.set_face_surface(f1, Some(plane.clone()));
    m.set_face_surface(f3, Some(plane));

    let with_surface_before =
        m.faces.iter().filter(|(_, f)| f.is_active() && f.surface().is_some()).count();
    m.repair_non_manifold_edges_geometric();
    let with_surface_after =
        m.faces.iter().filter(|(_, f)| f.is_active() && f.surface().is_some()).count();

    println!("\n  surface 를 가진 면 {with_surface_before} -> {with_surface_after}\n");
    assert_eq!(
        with_surface_before, with_surface_after,
        "a detached face keeps the surface it had — losing it is the ADR-089 A-χ \
         inheritance rule broken by a repair"
    );
}
