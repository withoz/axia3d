//! WHAT SEPARATES "ONE SOLID" FROM "TWO SOLIDS IN THE SAME PLACE".
//!
//! The coplanar re-tile (ADR-281 β-1) feeds a solid's on-plane perimeter to the
//! arrange so a crossing shape divides that face without opening it. It can do
//! that for ONE solid. When the drawn region reaches the perimeters of two, the
//! arrange divides the shared ground and there is no way to hand each solid the
//! pieces it needs — a face has one owner.
//!
//! Counting the perimeter components cannot decide it: a solid whose top has a
//! hole yields two of them (outer rim + hole rim) and has to keep working. What
//! decides it is whether those components are REACHABLE from each other across
//! shared edges. Two boxes that merely interpenetrate share no edge at all; a
//! drilled box's hole rim is reached from its outer rim through the tube walls.
use axia_geo::entities::MaterialId;
use axia_geo::Mesh;
use glam::DVec3;

const MAT: MaterialId = MaterialId::new(0);

fn distinct_shells(m: &Mesh) -> usize {
    let ids = m.face_shell_ids();
    let mut roots: std::collections::HashSet<_> = std::collections::HashSet::new();
    for (_, r) in ids {
        roots.insert(r);
    }
    roots.len()
}

/// Two 120³ boxes overlapping in space and sharing the z=0 plane. Overlapping is
/// not touching: they have no edge in common, so they are two shells.
#[test]
fn two_interpenetrating_boxes_are_two_shells() {
    let mut m = Mesh::new();
    m.create_box(DVec3::new(0.0, 0.0, 60.0), 120.0, 120.0, 120.0, MAT).unwrap();
    assert_eq!(distinct_shells(&m), 1, "one box is one shell");
    m.create_box(DVec3::new(80.0, 80.0, 60.0), 120.0, 120.0, 120.0, MAT).unwrap();
    assert_eq!(
        distinct_shells(&m),
        2,
        "two boxes that overlap in space still share no edge"
    );
    // and they really do overlap — this is the fixture the re-tile used to break
    assert!(
        m.detect_self_intersections().count() > 0,
        "the fixture must actually interpenetrate, or it proves nothing"
    );
}

/// A box with a hole drilled through it: the hole's rim and the outer rim are
/// two perimeter components of ONE solid, and the walk reaches between them.
#[test]
fn a_drilled_box_is_one_shell() {
    let mut m = Mesh::new();
    m.create_box(DVec3::new(0.0, 0.0, 60.0), 200.0, 120.0, 200.0, MAT).unwrap();
    let before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
    m.drill_rect_through_hole(
        DVec3::new(-30.0, -30.0, 120.0),
        DVec3::new(30.0, 30.0, 120.0),
        DVec3::Z,
    )
    .expect("the drill must succeed for this fixture to mean anything");
    let after = m.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert!(after > before, "the drill must add the tube's walls");
    assert_eq!(
        distinct_shells(&m),
        1,
        "the hole rim is reached from the outer rim through the tube"
    );
}

/// Faces that touch along an edge are one shell even when they are separate
/// pieces of work — that is the whole point of asking about edges rather than
/// about owners, and it is what keeps a sheet abutting a solid out of trouble.
#[test]
fn faces_sharing_an_edge_are_one_shell() {
    let mut m = Mesh::new();
    let a = m.add_vertex(DVec3::ZERO);
    let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
    let c = m.add_vertex(DVec3::new(100.0, 100.0, 0.0));
    let d = m.add_vertex(DVec3::new(0.0, 100.0, 0.0));
    let e = m.add_vertex(DVec3::new(200.0, 0.0, 0.0));
    let f = m.add_vertex(DVec3::new(200.0, 100.0, 0.0));
    m.add_face(&[a, b, c, d], MAT).unwrap();
    m.add_face(&[b, e, f, c], MAT).unwrap();
    assert_eq!(distinct_shells(&m), 1, "they share the b–c edge");

    // a third quad well away from both shares nothing
    let g = m.add_vertex(DVec3::new(500.0, 0.0, 0.0));
    let h = m.add_vertex(DVec3::new(600.0, 0.0, 0.0));
    let i = m.add_vertex(DVec3::new(600.0, 100.0, 0.0));
    let j = m.add_vertex(DVec3::new(500.0, 100.0, 0.0));
    m.add_face(&[g, h, i, j], MAT).unwrap();
    assert_eq!(distinct_shells(&m), 2, "the far quad is its own shell");
}
