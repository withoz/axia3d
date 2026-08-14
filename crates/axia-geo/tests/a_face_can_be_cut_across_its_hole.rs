//! Cutting a face across one of its own holes.
//!
//! The engine has described this topology since 2026-04-28 and refused it ever
//! since — "not yet supported (would need face-with-hole split)", with the
//! answer written in the comment beside it: *one simple face (chain area) +
//! one face-with-hole (rest)*. It is what C-2's materializer needs, because a
//! cap's rim IS its host's hole, so cutting the host means cutting across that
//! hole.
//!
//! A chord from one point on the hole back to another divides an annulus in
//! two: a lune closed by one arc of the hole, and the rest — same outer loop,
//! with a hole made of the chord and the hole's other arc.

use axia_geo::operations::face_split::split_face_by_chain;
use axia_geo::{MaterialId, Mesh, VertId};
use glam::DVec3;

/// A square annulus: a 200 × 200 outer square with a 100 × 100 hole.
fn annulus() -> (Mesh, Vec<VertId>, Vec<VertId>, axia_geo::FaceId) {
    let mut mesh = Mesh::new();
    let mat = MaterialId::new(0);
    let outer: Vec<VertId> = [
        (-100.0, -100.0),
        (100.0, -100.0),
        (100.0, 100.0),
        (-100.0, 100.0),
    ]
    .iter()
    .map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, 0.0)))
    .collect();
    // Clockwise, as a hole is.
    let hole: Vec<VertId> = [
        (-50.0, -50.0),
        (-50.0, 50.0),
        (50.0, 50.0),
        (50.0, -50.0),
    ]
    .iter()
    .map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, 0.0)))
    .collect();
    let f = mesh
        .add_face_with_holes(&outer, &[&hole], mat)
        .expect("annulus");
    (mesh, outer, hole, f)
}

#[test]
fn a_chord_across_the_hole_divides_the_annulus() {
    let (mut mesh, _outer, hole, f) = annulus();
    let mat = MaterialId::new(0);
    let before_area = mesh.face_outer_area(f);
    println!("HOLE annulus area (outer only) {before_area:.1}");

    // A chord from one hole corner to the one diagonally opposite, routed
    // through the material by way of a point outside the hole. Two segments,
    // so it is a chain and not just an edge of the hole.
    let via = mesh.add_vertex(DVec3::new(0.0, 80.0, 0.0));
    let chain = vec![hole[1], via, hole[2]]; // (-50,50) -> (0,80) -> (50,50)
    for w in chain.windows(2) {
        mesh.add_edge(w[0], w[1]);
    }

    let res = split_face_by_chain(&mut mesh, f, &chain, mat);
    let res = match res {
        Ok(r) => r,
        Err(e) => panic!("a chord across the hole should divide the annulus: {e}"),
    };
    let active: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(_, x)| x.is_active())
        .map(|(fid, x)| {
            let mut loops = vec![mesh.collect_loop_verts(x.outer().start).unwrap().len()];
            for l in x.inners() {
                loops.push(mesh.collect_loop_verts(l.start).unwrap().len());
            }
            (fid.raw(), loops, mesh.face_outer_area(fid))
        })
        .collect();
    println!("HOLE after: {active:?}");

    assert_eq!(res.new_faces.len(), 2, "the lune and the rest");
    assert_eq!(active.len(), 2, "and nothing else survives");
    // One piece is simple, the other keeps the outer loop and a hole.
    let simple = active.iter().filter(|(_, l, _)| l.len() == 1).count();
    let holed = active.iter().filter(|(_, l, _)| l.len() == 2).count();
    assert_eq!((simple, holed), (1, 1), "one simple face, one face-with-hole");

    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);
    assert_eq!(
        mesh.detect_self_intersections().count(),
        0,
        "and nothing pierces anything"
    );
}

/// The area adds up: the lune plus the rest is the annulus, because the chord
/// moved material from one side of the hole to the other and created none.
#[test]
fn the_two_pieces_are_the_annulus() {
    let (mut mesh, _outer, hole, f) = annulus();
    let mat = MaterialId::new(0);
    // outer 200×200 minus a 100×100 hole
    let annulus_area = 200.0 * 200.0 - 100.0 * 100.0;
    let hole_area = mesh
        .faces
        .get(f)
        .map(|x| {
            let l = mesh.collect_loop_verts(x.inners()[0].start).unwrap();
            let pts: Vec<DVec3> = l.iter().map(|&v| mesh.vertex_pos(v).unwrap()).collect();
            let n = pts.len();
            0.5 * (0..n)
                .map(|i| {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    a.x * b.y - b.x * a.y
                })
                .sum::<f64>()
                .abs()
        })
        .unwrap();
    assert!((hole_area - 10_000.0).abs() < 1.0, "the hole is 100 × 100");

    let via = mesh.add_vertex(DVec3::new(0.0, 80.0, 0.0));
    let chain = vec![hole[1], via, hole[2]];
    for w in chain.windows(2) {
        mesh.add_edge(w[0], w[1]);
    }
    split_face_by_chain(&mut mesh, f, &chain, mat).expect("split");

    // `face_outer_area` reads the OUTER loop, so the holed piece over-reports
    // by its hole. Subtract it back and the two must be the annulus.
    let mut total = 0.0;
    for (fid, face) in mesh.faces.iter().filter(|(_, x)| x.is_active()) {
        let mut a = mesh.face_outer_area(fid);
        for l in face.inners() {
            let pts: Vec<DVec3> = mesh
                .collect_loop_verts(l.start)
                .unwrap()
                .iter()
                .map(|&v| mesh.vertex_pos(v).unwrap())
                .collect();
            let n = pts.len();
            a -= 0.5
                * (0..n)
                    .map(|i| {
                        let (p, q) = (pts[i], pts[(i + 1) % n]);
                        p.x * q.y - q.x * p.y
                    })
                    .sum::<f64>()
                    .abs();
        }
        total += a;
    }
    println!("HOLE pieces sum {total:.1} (annulus {annulus_area:.1})");
    assert!(
        (total - annulus_area).abs() < 1.0,
        "the two pieces are the annulus: {total:.1} vs {annulus_area:.1}"
    );
}
