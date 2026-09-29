//! A BOUNDARY CLICK ON A CORNER HAS NO REGION AROUND IT.
//!
//! `boundary_from_point` walks outward from a point to the closed cycle of free
//! edges enclosing it. Inside a region that works; on an edge between two
//! regions it works and picks one. Exactly ON a vertex there is no region to
//! find, and it says `NoEnclosingCycle` — the same answer it gives for a click
//! in empty space.
//!
//! Measured on two squares sharing an edge, so the shared corners are vertices
//! three regions meet at:
//!
//! ```text
//!   inside the left square       Ok(FaceId(0))          area 10000
//!   on the shared edge           Ok(FaceId(0))          area 10000
//!   exactly on a corner vertex   Err(NoEnclosingCycle)
//!   exactly on an outer corner   Err(NoEnclosingCycle)
//! ```
//!
//! ⚠ This is not a defect and nothing here asks it to change. A vertex is not
//! in the interior of anything, so refusing is right. What is pinned is that
//! the refusal is INDISTINGUISHABLE from "there is nothing here", which is why
//! the tool layer has to know before it asks:
//! `normalizeDrawInput`'s step 3 reports the vertex (via `vertexAt`, the export
//! added alongside this file), and `BoundaryTool` then says "that is a corner"
//! rather than repeating the generic refusal.
//!
//! If `boundary_from_point` ever learns to pick a region around a corner, the
//! first two assertions below fail — rewrite them and drop the tool's message.

use axia_geo::mesh::Mesh;
use axia_geo::operations::boolean_geo::Plane;
use axia_geo::operations::boundary::{boundary_from_point, BoundaryError};
use axia_geo::MaterialId;
use glam::DVec3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// Two 100×100 squares side by side on z=0, sharing the edge at x=100.
fn two_squares() -> Mesh {
    let mut m = Mesh::new();
    let p = |x: f64, y: f64| DVec3::new(x, y, 0.0);
    let vs: Vec<_> = [
        p(0.0, 0.0),
        p(100.0, 0.0),
        p(200.0, 0.0),
        p(200.0, 100.0),
        p(100.0, 100.0),
        p(0.0, 100.0),
    ]
    .iter()
    .map(|&q| m.add_vertex(q))
    .collect();
    for [a, b] in [
        [vs[0], vs[1]],
        [vs[1], vs[2]],
        [vs[2], vs[3]],
        [vs[3], vs[4]],
        [vs[4], vs[5]],
        [vs[5], vs[0]],
        [vs[1], vs[4]], // the shared wall
    ] {
        m.add_edge(a, b).expect("edge");
    }
    m
}

fn ask(pt: DVec3) -> (Mesh, Result<axia_geo::FaceId, BoundaryError>) {
    let mut m = two_squares();
    let plane = Plane { normal: DVec3::Z, dist: 0.0 };
    let r = boundary_from_point(&mut m, pt, plane, 1000.0);
    (m, r)
}

#[test]
fn inside_a_region_it_builds_the_face() {
    let (m, r) = ask(DVec3::new(50.0, 50.0, 0.0));
    let f = r.expect("a point inside the left square encloses it");
    let area = m.face_outer_area(f);
    println!("  inside: {f:?}, area {area:.0}");
    assert!((area - 10_000.0).abs() < 1.0, "the left square is 100 × 100, and it read {area:.1}");
}

#[test]
fn on_an_edge_it_still_picks_a_region() {
    let (m, r) = ask(DVec3::new(100.0, 50.0, 0.0));
    let f = r.expect("a point on the shared wall still has a region on each side");
    let area = m.face_outer_area(f);
    println!("  on the wall: {f:?}, area {area:.0}");
    assert!((area - 10_000.0).abs() < 1.0, "one of the two squares, and it read {area:.1}");
}

#[test]
fn exactly_on_a_vertex_there_is_nothing_to_enclose() {
    for (tag, pt) in [
        ("the shared corner", DVec3::new(100.0, 0.0, 0.0)),
        ("an outer corner", DVec3::new(0.0, 0.0, 0.0)),
    ] {
        let (m, r) = ask(pt);
        println!("  {tag}: {r:?}");
        assert!(
            matches!(r, Err(BoundaryError::NoEnclosingCycle)),
            "{tag} must refuse, and say so the same way empty space does: {r:?}"
        );
        assert_eq!(
            m.faces.iter().filter(|(_, f)| f.is_active()).count(),
            0,
            "{tag}: a refusal builds nothing"
        );
    }
}

/// And the vertex the tool layer would report is the one that is really there —
/// `find_existing_vertex` is what `vertexAt` exports, and it answers at the
/// engine's own dedup distance (LOCKED #5, 0.15μm).
#[test]
fn the_vertex_the_tool_layer_would_report_is_the_one_standing_there() {
    let m = two_squares();
    let corner = DVec3::new(100.0, 0.0, 0.0);
    let v = m.find_existing_vertex(corner).expect("the shared corner is a vertex");
    assert!(
        (m.vertex_pos(v).expect("pos") - corner).length() < 1e-9,
        "it must be THAT corner, not a neighbour"
    );
    // A point in the middle of a region is free, which is the case the tool
    // must let through.
    assert!(
        m.find_existing_vertex(DVec3::new(50.0, 50.0, 0.0)).is_none(),
        "the interior of a square has no vertex in it"
    );
    // Just off the corner, by more than the dedup distance, is free too.
    assert!(
        m.find_existing_vertex(DVec3::new(100.001, 0.0, 0.0)).is_none(),
        "1μm away is a different point — the match must not be loose"
    );
}
