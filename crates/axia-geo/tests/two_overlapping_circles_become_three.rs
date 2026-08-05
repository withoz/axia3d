//! TWO CIRCLES THAT CROSS ARE THREE REGIONS, AND THE RIMS STAY ROUND.
//!
//! On a sheet this already happened — the coplanar re-derive hands both curves
//! to the analytic arrangement. On a solid's face that re-derive is skipped, so
//! both circles became overlapping holes of the cap instead and the draw guard
//! rolled the whole thing back. Measured 2026-08-05: the SECOND circle drawn on
//! a box top was refused outright.
//!
//! What is pinned here is the shape of the answer rather than the route to it:
//! three regions, every boundary still an arc, the container left holding one
//! union hole, and the areas adding up. A polygonised answer would satisfy "three
//! faces" and throw away the circles, so the arcs are asserted directly.
use axia_geo::curves::AnalyticCurve;
use axia_geo::operations::annulus::split_face_by_inner_circle_generic;
use axia_geo::operations::circle_overlap::split_overlapping_circles;
use axia_geo::{FaceId, MaterialId, Mesh};
use glam::DVec3;

const R: f64 = 40.0;

fn disk(mesh: &mut Mesh, centre: DVec3, radius: f64) -> FaceId {
    let anchor = mesh.add_vertex(centre + DVec3::new(radius, 0.0, 0.0));
    mesh.add_face_closed_curve(
        anchor,
        AnalyticCurve::Circle { center: centre, radius, normal: DVec3::Z, basis_u: DVec3::X },
        MaterialId::new(0),
    )
    .expect("a circle face")
}

/// Two circles of radius R whose centres are `gap` apart, side by side.
fn two_circles(mesh: &mut Mesh, gap: f64) -> (FaceId, FaceId) {
    let a = disk(mesh, DVec3::new(-gap / 2.0, 0.0, 0.0), R);
    let b = disk(mesh, DVec3::new(gap / 2.0, 0.0, 0.0), R);
    (a, b)
}

/// The exact area of the almond between two circles of equal radius `r` whose
/// centres are `d` apart.
fn lens_area(r: f64, d: f64) -> f64 {
    2.0 * r * r * (d / (2.0 * r)).acos() - (d / 2.0) * (4.0 * r * r - d * d).sqrt()
}

/// Every edge of a loop carries an arc — nothing was flattened to a chord.
fn loop_is_all_arcs(mesh: &Mesh, start: axia_geo::HeId) -> bool {
    let hes = mesh.collect_loop_hes(start).expect("a loop");
    !hes.is_empty()
        && hes.iter().all(|&he| {
            let e = mesh.hes[he].edge();
            matches!(mesh.edges.get(e).and_then(|x| x.curve()), Some(AnalyticCurve::Arc { .. }))
        })
}

/// The true area of an arc-bounded region: the chord polygon plus each arc's
/// circular segment. (`face_area` reads the chord — a separate matter, and the
/// reason this is worked out here rather than asked for.)
fn arc_region_area(mesh: &Mesh, fid: FaceId) -> f64 {
    let start = mesh.faces[fid].outer().start;
    let verts = mesh.collect_loop_verts(start).expect("a loop");
    let hes = mesh.collect_loop_hes(start).expect("a loop");
    let mut area = 0.0;
    for i in 0..verts.len() {
        let a = mesh.vertex_pos(verts[i]).unwrap();
        let b = mesh.vertex_pos(verts[(i + 1) % verts.len()]).unwrap();
        area += a.x * b.y - b.x * a.y;
    }
    area *= 0.5;
    for &he in &hes {
        let e = mesh.hes[he].edge();
        if let Some(AnalyticCurve::Arc { radius, start_angle, end_angle, .. }) =
            mesh.edges.get(e).and_then(|x| x.curve())
        {
            // The stored span is the edge's own direction; the walk may go the
            // other way, so orient it by the half-edge's endpoints.
            let (s, t) = (*start_angle, *end_angle);
            let sweep = (t - s).abs();
            let seg = 0.5 * radius * radius * (sweep - sweep.sin());
            // Bulging out of the loop adds, bulging in takes away. The loop runs
            // counter-clockwise, so its inside is on the LEFT of the walk — an
            // arc whose midpoint falls to the RIGHT of the chord is the one that
            // adds. (Measured: taking it the other way round put every segment
            // on the wrong side and read the lens as 806 instead of 1965.)
            let p0 = mesh.vertex_pos(mesh.hes[mesh.hes[he].prev()].dst()).unwrap();
            let p1 = mesh.vertex_pos(mesh.hes[he].dst()).unwrap();
            let mid_a = (s + t) / 2.0;
            let c = match mesh.edges.get(e).and_then(|x| x.curve()) {
                Some(AnalyticCurve::Arc { center, .. }) => *center,
                _ => unreachable!(),
            };
            let m = c + DVec3::new(radius * mid_a.cos(), radius * mid_a.sin(), 0.0);
            let cross = (p1 - p0).x * (m - p0).y - (p1 - p0).y * (m - p0).x;
            area += if cross < 0.0 { seg } else { -seg };
        }
    }
    area.abs()
}

#[test]
fn a_lens_and_two_crescents_with_every_rim_still_an_arc() {
    let mut mesh = Mesh::new();
    let (a, b) = two_circles(&mut mesh, R); // centres R apart → a proper crossing
    let split = split_overlapping_circles(&mut mesh, a, b).expect("they cross");

    assert!(mesh.faces.get(a).map_or(true, |f| !f.is_active()), "the old circles are gone");
    assert!(mesh.faces.get(b).map_or(true, |f| !f.is_active()));
    assert!(split.parent.is_none(), "loose circles have no container");

    for (name, fid) in
        [("lens", split.lens), ("crescent a", split.crescent_a), ("crescent b", split.crescent_b)]
    {
        let start = mesh.faces[fid].outer().start;
        assert_eq!(
            mesh.collect_loop_verts(start).unwrap().len(),
            4,
            "{name}: two arcs, each split at its midpoint"
        );
        assert!(loop_is_all_arcs(&mesh, start), "{name}: the rim must not be flattened");
    }
    assert!(mesh.verify_face_invariants().violations.is_empty());
    assert_eq!(mesh.detect_self_intersections().intersecting_pairs.len(), 0, "nothing overlaps");
}

#[test]
fn the_three_regions_add_up_to_the_two_disks() {
    let mut mesh = Mesh::new();
    let (a, b) = two_circles(&mut mesh, R);
    let split = split_overlapping_circles(&mut mesh, a, b).expect("they cross");

    let want_lens = lens_area(R, R);
    let disk = std::f64::consts::PI * R * R;
    let got_lens = arc_region_area(&mesh, split.lens);
    let got_a = arc_region_area(&mesh, split.crescent_a);
    let got_b = arc_region_area(&mesh, split.crescent_b);

    assert!((got_lens - want_lens).abs() < want_lens * 1e-6, "lens {got_lens} vs {want_lens}");
    for (n, got) in [("a", got_a), ("b", got_b)] {
        let want = disk - want_lens;
        assert!((got - want).abs() < want * 1e-6, "crescent {n}: {got} vs {want}");
    }
    // And together they are the union, counted once.
    let union = 2.0 * disk - want_lens;
    assert!((got_lens + got_a + got_b - union).abs() < union * 1e-6);
}

/// The case that was refused: both circles are holes of a face, and that face
/// must end up with ONE hole shaped like their union.
#[test]
fn a_container_ends_up_with_one_union_hole() {
    let mut mesh = Mesh::new();
    let top: Vec<_> = [
        DVec3::new(-100.0, -100.0, 0.0),
        DVec3::new(100.0, -100.0, 0.0),
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(-100.0, 100.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    let cap = mesh.add_face(&top, MaterialId::new(0)).expect("a cap");
    let (a, b) = two_circles(&mut mesh, R);
    split_face_by_inner_circle_generic(&mut mesh, cap, a).expect("a becomes a hole");
    split_face_by_inner_circle_generic(&mut mesh, cap, b).expect("b becomes a hole");
    assert_eq!(mesh.faces[cap].inners().len(), 2, "the state that used to be refused");

    let split = split_overlapping_circles(&mut mesh, a, b).expect("they cross");
    let (old, new) = split.parent.expect("they shared a container");
    assert_eq!(old, cap);
    assert_eq!(mesh.faces[new].inners().len(), 1, "the two holes became one");

    let hole = mesh.faces[new].inners()[0].start;
    assert_eq!(mesh.collect_loop_verts(hole).unwrap().len(), 4, "two arcs, midpoint-split");
    assert!(loop_is_all_arcs(&mesh, hole), "the union hole keeps its arcs");

    // The material left is the square minus the union of the two disks.
    let disk = std::f64::consts::PI * R * R;
    let union = 2.0 * disk - lens_area(R, R);
    let want = 200.0 * 200.0 - union;
    let got = mesh.face_outer_area(new) - arc_region_area(&mesh, split.lens)
        - arc_region_area(&mesh, split.crescent_a)
        - arc_region_area(&mesh, split.crescent_b);
    assert!((got - want).abs() < want * 1e-6, "ring area {got} vs {want}");

    assert!(mesh.verify_face_invariants().violations.is_empty());
    assert_eq!(mesh.detect_self_intersections().intersecting_pairs.len(), 0, "nothing overlaps");
}

/// Another circle already sitting in the container keeps its own hole. It is a
/// one-vertex loop, so it cannot be carried across as a vertex list — it is put
/// back by the same path that put it there.
#[test]
fn a_third_circle_already_in_the_container_keeps_its_hole() {
    let mut mesh = Mesh::new();
    let top: Vec<_> = [
        DVec3::new(-200.0, -200.0, 0.0),
        DVec3::new(200.0, -200.0, 0.0),
        DVec3::new(200.0, 200.0, 0.0),
        DVec3::new(-200.0, 200.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    let cap = mesh.add_face(&top, MaterialId::new(0)).unwrap();
    let far = disk(&mut mesh, DVec3::new(0.0, 140.0, 0.0), 20.0);
    let (a, b) = two_circles(&mut mesh, R);
    for f in [far, a, b] {
        split_face_by_inner_circle_generic(&mut mesh, cap, f).expect("a hole");
    }
    assert_eq!(mesh.faces[cap].inners().len(), 3);

    let split = split_overlapping_circles(&mut mesh, a, b).expect("they cross");
    let (_, new) = split.parent.expect("a container");
    assert_eq!(mesh.faces[new].inners().len(), 2, "the union, and the far circle still there");
    assert!(mesh.faces[far].is_active(), "the far circle survives");
    assert!(mesh.verify_face_invariants().violations.is_empty());
}

/// The cases it must NOT touch, each leaving the mesh exactly as it was.
#[test]
fn it_declines_anything_that_is_not_two_crossing_circles() {
    // Apart.
    let mut mesh = Mesh::new();
    let (a, b) = two_circles(&mut mesh, R * 3.0);
    assert!(split_overlapping_circles(&mut mesh, a, b).is_none(), "disjoint");
    assert!(mesh.faces[a].is_active() && mesh.faces[b].is_active());

    // One inside the other.
    let mut mesh = Mesh::new();
    let big = disk(&mut mesh, DVec3::ZERO, R);
    let small = disk(&mut mesh, DVec3::ZERO, R / 4.0);
    assert!(split_overlapping_circles(&mut mesh, big, small).is_none(), "contained");
    assert!(mesh.faces[big].is_active() && mesh.faces[small].is_active());

    // Touching at one point — no region between them to build.
    let mut mesh = Mesh::new();
    let (a, b) = two_circles(&mut mesh, R * 2.0);
    assert!(split_overlapping_circles(&mut mesh, a, b).is_none(), "tangent");

    // On different planes.
    let mut mesh = Mesh::new();
    let a = disk(&mut mesh, DVec3::new(-R / 2.0, 0.0, 0.0), R);
    let anchor = mesh.add_vertex(DVec3::new(R / 2.0, 0.0, R));
    let b = mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle {
                center: DVec3::new(R / 2.0, 0.0, 0.0),
                radius: R,
                normal: DVec3::X,
                basis_u: DVec3::Z,
            },
            MaterialId::new(0),
        )
        .unwrap();
    assert!(split_overlapping_circles(&mut mesh, a, b).is_none(), "not coplanar");
    assert!(mesh.faces[a].is_active() && mesh.faces[b].is_active());

    // Not a closed curve at all.
    let mut mesh = Mesh::new();
    let v: Vec<_> = [
        DVec3::new(0.0, 0.0, 0.0),
        DVec3::new(10.0, 0.0, 0.0),
        DVec3::new(10.0, 10.0, 0.0),
    ]
    .iter()
    .map(|&p| mesh.add_vertex(p))
    .collect();
    let tri = mesh.add_face(&v, MaterialId::new(0)).unwrap();
    let c = disk(&mut mesh, DVec3::new(5.0, 5.0, 0.0), R);
    assert!(split_overlapping_circles(&mut mesh, tri, c).is_none(), "a polygon");
}

/// Circles of different sizes, where the crescents are lopsided — the symmetric
/// fixture above would pass with a formula that only works when the two rims
/// meet the centre line at the same angle.
#[test]
fn it_works_when_the_two_circles_are_different_sizes() {
    let mut mesh = Mesh::new();
    let a = disk(&mut mesh, DVec3::ZERO, 100.0);
    let b = disk(&mut mesh, DVec3::new(95.0, 0.0, 0.0), 20.0);
    let split = split_overlapping_circles(&mut mesh, a, b).expect("they cross");

    let big = std::f64::consts::PI * 100.0 * 100.0;
    let small = std::f64::consts::PI * 20.0 * 20.0;
    let lens = arc_region_area(&mesh, split.lens);
    let ca = arc_region_area(&mesh, split.crescent_a);
    let cb = arc_region_area(&mesh, split.crescent_b);
    assert!((lens + ca - big).abs() < big * 1e-6, "lens+crescentA = the big disk");
    assert!((lens + cb - small).abs() < small * 1e-6, "lens+crescentB = the small disk");
    assert!(mesh.verify_face_invariants().violations.is_empty());
    assert_eq!(mesh.detect_self_intersections().intersecting_pairs.len(), 0);
}
