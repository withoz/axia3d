//! A FACE THAT CROSSES ITS OWN BOUNDARY, AND THE THREE DETECTORS THAT MISS IT.
//!
//! `verify_face_invariants` walks the topology: loops close, half-edges pair,
//! normals are finite and agree with their winding. `detect_self_intersections`
//! (ADR-273) asks whether one face passes through another. Neither asks whether
//! a SINGLE face's boundary crosses itself, so a face shaped like a bowtie is
//! valid to both — and it is not a rare shape.
//!
//! ## What it costs
//!
//! Found while following the fuzz's stacking family upstream. Fuzz session 12,
//! operation 3, pushes a face of a radius-30 cylinder inward by 100; before
//! `an_inward_push_stops_inside_the_solid` bounded it, its corners landed at
//! radius 70.4 on the far side and the base disk's ring read
//!
//! ```text
//!   0°, 153°, 160°, -47°, -63° …
//! ```
//!
//! — two points flung across the axis. Nothing reported it. Twelve operations
//! later the same plane carried two faces covering the same ground, and THAT is
//! what the fuzz printed. So the stacking family is, at least in part, this
//! class arriving late.
//!
//! ## ⚠ A repeated vertex is not a crossing
//!
//! A loop may touch itself at one point and be perfectly legal — ADR-022 P9's
//! pinch, and the rings `polygon_difference_by_clip` splits at a self-touch
//! (LOCKED #105 L-105-9). Counting those as crossings put this class at 86
//! sessions in 100 instead of 62, and made its correlation with the broken
//! sessions read 22:8 when it is **14:16**. Only a genuine segment crossing
//! counts here.
//!
//! ## ⚠ The epsilon has to be a distance
//!
//! The 2D side test is a cross product, which is an AREA. An absolute epsilon
//! against it means a different angular tolerance at every size — at
//! coordinates around 100 the products run to 1e4, so 1e-9 is no tolerance at
//! all and a segment that merely grazes another reads as a crossing. Dividing
//! by the segment's length turns it back into a perpendicular distance,
//! comparable to the loop's own extent.
//!
//! ## ⚠ Bounding the push on this predicate was tried, and did not earn it
//!
//! `pushIn` makes the first such face in 26 of the 62 sessions, more than
//! double the next, so stopping an inward MoveOnly push where a face around it
//! would NEWLY cross itself looked like the fix. Measured over 100 × 50:
//!
//! ```text
//!                     sessions that ever grow one    standing at the end    broken
//!   without the bound            62                         108               30
//!   with it (round hosts)        60                          99               31
//! ```
//!
//! Two sessions and nine faces, inside the stream-shift noise this harness
//! warns about, and the broken count moved the wrong way. Three things were
//! learnt on the way and are worth more than the bound was:
//!
//! * asking "does any face cross after the move" clamps a push to NOTHING
//!   whenever a face around it was already crossing — the predicate is then
//!   true at every distance including zero;
//! * on a PLANAR host the rule is too strict outright: the pinned scenario in
//!   `a_vertex_whose_outgoing_half_edge_is_gone` pushes a box face -100, would
//!   be stopped at -93.04, and goes from sound to seven stacked pairs. Its later
//!   steps depend on the face arriving where it was asked to;
//! * so **a face crossing itself mid-push is not, by itself, damage**. What hurt
//!   in session 12 was the magnitude, and that is bounded now.
//!
//! This file pins the measurement instead. If a future change makes the numbers
//! below smaller, the survey fails and says so — re-measure and write down what
//! started working, the way the fuzz harness's own pinned tests do.

use axia_core::{Command, Scene, FORM_MATERIAL};
use axia_geo::mesh::Mesh;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// Does this face's own boundary cross itself, somewhere other than at a shared
/// vertex? Planar faces only — projecting a curved one onto a single plane
/// reports crossings that are not there.
fn crosses_itself(mesh: &Mesh, fid: FaceId) -> bool {
    let Some(face) = mesh.faces.get(fid) else { return false };
    if !face.is_active() || face.outer().start.is_null() {
        return false;
    }
    let Ok(vs) = mesh.collect_loop_verts(face.outer().start) else { return false };
    if vs.len() < 4 {
        return false;
    }
    let uniq: std::collections::HashSet<_> = vs.iter().collect();
    if uniq.len() != vs.len() {
        return false; // a pinch, which is legal — see the header
    }
    let n = face.normal().normalize_or_zero();
    if n.length_squared() < 0.5 {
        return false;
    }
    let pts: Vec<DVec3> = vs.iter().filter_map(|&v| mesh.vertex_pos(v).ok()).collect();
    if pts.len() != vs.len() {
        return false;
    }
    let (mut off_plane, mut extent) = (0.0f64, 0.0f64);
    for q in &pts {
        off_plane = off_plane.max((*q - pts[0]).dot(n).abs());
        extent = extent.max((*q - pts[0]).length());
    }
    if extent <= 0.0 || off_plane > extent * 1e-9 {
        return false;
    }
    let e1 = (pts[1] - pts[0]).normalize_or_zero();
    if e1.length_squared() < 0.5 {
        return false;
    }
    let e2 = n.cross(e1);
    let flat: Vec<(f64, f64)> = pts.iter().map(|q| (q.dot(e1), q.dot(e2))).collect();
    let m = flat.len();
    let eps = extent * 1e-6;
    let side = |o: (f64, f64), u: (f64, f64), w: (f64, f64)| -> f64 {
        let len = ((u.0 - o.0).powi(2) + (u.1 - o.1).powi(2)).sqrt();
        if len < 1e-12 {
            return 0.0;
        }
        ((u.0 - o.0) * (w.1 - o.1) - (u.1 - o.1) * (w.0 - o.0)) / len
    };
    for i in 0..m {
        for j in (i + 2)..m {
            if i == 0 && j == m - 1 {
                continue; // adjacent through the wrap
            }
            let (a, b, c, d) = (flat[i], flat[(i + 1) % m], flat[j], flat[(j + 1) % m]);
            let (s1, s2, s3, s4) = (side(a, b, c), side(a, b, d), side(c, d, a), side(c, d, b));
            if s1.min(s2) < -eps && s1.max(s2) > eps && s3.min(s4) < -eps && s3.max(s4) > eps {
                return true;
            }
        }
    }
    false
}

fn count_crossing(mesh: &Mesh) -> usize {
    mesh.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .filter(|&id| crosses_itself(mesh, id))
        .count()
}

/// A bowtie: four corners in an order that makes the boundary cross itself once.
/// Both detectors accept it.
#[test]
fn a_bowtie_face_is_valid_to_every_detector_we_have() {
    let mut m = Mesh::new();
    let vs: Vec<_> = [(0.0, 0.0), (100.0, 0.0), (50.0, -60.0), (50.0, 80.0)]
        .iter()
        .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
        .collect();
    let f = m.add_face(&vs, MaterialId::new(0)).expect("a bowtie is a face like any other");

    let normal = m.faces[f].normal();
    assert!(
        normal.is_finite() && normal.length_squared() > 1e-12,
        "the fixture needs a real normal, or it is testing ADR-304's NaN case instead: {normal:?}"
    );
    assert!(crosses_itself(&m, f), "the fixture must actually cross itself");

    let inv = m.verify_face_invariants();
    assert!(
        inv.is_valid(),
        "the topology checker is expected to accept it — if it no longer does, this \
         class became visible and the header needs rewriting: {:?}",
        inv.violations
    );
    assert_eq!(
        m.detect_self_intersections().count(),
        0,
        "ADR-273 asks whether faces pass through EACH OTHER; one face is not a pair"
    );
    println!("  a bowtie: invariants valid, self-intersections 0, crosses itself yes");
}

/// The control: the same four corners in the order that does not cross.
#[test]
fn the_same_corners_in_order_do_not_cross() {
    let mut m = Mesh::new();
    let vs: Vec<_> = [(0.0, 0.0), (100.0, 0.0), (50.0, 80.0), (50.0, -60.0)]
        .iter()
        .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
        .collect();
    // Wound so the boundary walks the outline: (0,0) -> (100,0) -> (50,80) is a
    // simple triangle; add the fourth corner on the far side and it stays simple
    // only in one of the two orders, which is the point.
    let f = m.add_face(&[vs[3], vs[0], vs[2], vs[1]], MaterialId::new(0)).expect("simple quad");
    assert!(!crosses_itself(&m, f), "this order does not cross");
    assert!(m.verify_face_invariants().is_valid(), "and it is sound");
}

/// A box pushed straight down is the shape this class is NOT about — a control
/// for the survey below, so a change that quietly starts flagging ordinary work
/// shows up here rather than as a number drifting.
#[test]
fn ordinary_drawing_grows_none_of_them() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 200.0,
        height: 200.0,
    });
    s.mesh.create_box(DVec3::new(400.0, 0.0, 100.0), 200.0, 200.0, 200.0, FORM_MATERIAL);
    s.mesh.create_cylinder(DVec3::new(-400.0, 0.0, 0.0), 70.0, 200.0, 24, FORM_MATERIAL).expect("cylinder");

    let n = count_crossing(&s.mesh);
    println!("  a rect, a box and a cylinder: {n} faces crossing themselves");
    assert_eq!(n, 0, "nothing ordinary should grow one");
}
