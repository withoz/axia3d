//! AN INWARD PUSH STOPS INSIDE THE SOLID, EVEN WHEN THE SOLID IS ROUND.
//!
//! `move_only_max_inward` bounds how far a MoveOnly push may travel back into
//! its own solid (ADR-196 L-196-10: "안쪽 over-push clamp — 뒤집히지 않게"). It
//! measures the thickness by looking for connecting edges PARALLEL to the face
//! normal — the four uprights under a box top.
//!
//! A cylinder has none. Its side face's neighbours run along the axis and around
//! the rim, and not one of them is parallel to the radial normal, so the search
//! found nothing and answered `None` — meaning "no limit". Measured on fuzz
//! session 12, operation 3, pushing a face of a radius-30 cylinder inward by
//! 100:
//!
//! ```text
//!   the face spans u = -31.3° .. -15.7° at radius 30
//!   afterwards its corners sit at radius 70.4, at u = 153.2° and 159.8°
//! ```
//!
//! 180° round and out the other side. The base disk's boundary then reads
//! `0°, 153°, 160°, -47°, -63° ...` — a ring with two points flung across the
//! axis, which crosses itself.
//!
//! ⚠ Nothing reports that. `verify_face_invariants` is valid and ADR-273 finds
//! 0, because a face's own boundary crossing itself is neither a topology fault
//! nor a face-against-face one. What it does is STACK: twelve operations later
//! the same plane carries two faces covering the same ground, and that is the
//! violation the fuzz eventually prints.
//!
//! So when no wall runs along the normal, the bound falls back to how deep the
//! solid is in that direction: this face's own connected component projected
//! onto the normal, stopping at its far side. For a box top that is exactly the
//! wall measure (the height), so the fallback only ever answers where the walls
//! could not.
//!
//! ## ⚠ What this does NOT do
//!
//! It does not keep a curved face on its own side. The bound is the solid's
//! depth along the normal, and a curved face's CORNERS reach the far wall before
//! its middle does. In the fuzz's own 23-gon the same push now lands them at
//! radius 30.3 rather than 70.4 — just inside the far wall instead of well
//! outside it, so the ring no longer flings points across the axis but the face
//! has still crossed to the other side. Bounding the travel so no part of the
//! face passes any other part of the solid is a ray-into-the-solid question, and
//! this is not it.
//!
//! ⚠ The 30.3 is the fuzz's cylinder, whose boundary that push had already
//! reshaped. The clean 24-segment one below reads 30.0 — the same statement,
//! measured on a shape nothing else has touched.
//!
//! Measured over 100 fuzz sessions × 50 operations: broken 32 → **30**, and the
//! earliest break moved from operation 15 to 18.

use axia_geo::mesh::Mesh;
use axia_geo::operations::push_pull::move_only_max_inward;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 30.0;
const H: f64 = 200.0;
const N: u32 = 24;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

fn cylinder() -> (Mesh, FaceId) {
    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, R, H, N, mat()).expect("cylinder");
    let side = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .find(|&id| matches!(m.face_surface(id), Some(AnalyticSurface::Cylinder { .. })))
        .expect("a side face");
    (m, side)
}

/// How far from the axis every vertex of the mesh stands.
fn radii(m: &Mesh) -> Vec<f64> {
    m.verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .map(|(_, v)| DVec3::new(v.pos().x, v.pos().y, 0.0).length())
        .collect()
}

#[test]
fn pushing_a_cylinder_wall_in_does_not_send_it_out_the_far_side() {
    let (m, side) = cylinder();
    let bound = move_only_max_inward(&m, side);
    println!("  a radius-{R} cylinder's side face may travel {bound:?} inward");
    assert!(
        bound.is_some(),
        "a round solid has a depth too — answering None means the push is unbounded"
    );
    let d = bound.expect("a bound");
    assert!(
        (d - 2.0 * R).abs() < 1.0,
        "the depth across a radius-{R} cylinder is about {}, and it answered {d:.2}",
        2.0 * R
    );

    let mut m = m;
    m.push_pull(side, -100.0, mat()).expect("the push itself must succeed");

    let worst = radii(&m).into_iter().fold(0.0f64, f64::max);
    println!("  after pushing 100 inward, the furthest vertex is at radius {worst:.1}");
    assert!(
        worst < R * 1.5,
        "a vertex ended up at radius {worst:.1} on a radius-{R} cylinder — the push \
         went through the axis and out the other side (it used to reach 70.4)"
    );
}

/// The control: a box top still measures its own height, by the walls, exactly
/// as before — the fallback must not be answering here at all.
#[test]
fn a_box_top_still_measures_its_own_height() {
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 200.0, 100.0, 200.0, mat());
    // ⚠ Pick it by its NORMAL, not by its topmost vertex: a side wall touches
    // the same z and ties are broken arbitrarily, which selects a wall whose own
    // thickness is the box's 200 and reads like a failure of this measure.
    let top = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .find(|(_, f)| f.normal().normalize_or_zero().dot(DVec3::Z) > 0.999)
        .map(|(id, _)| id)
        .expect("a top face");

    let bound = move_only_max_inward(&m, top).expect("a box top has walls under it");
    println!("  a 100-tall box's top may travel {bound:.2} inward");
    assert!(
        (bound - 100.0).abs() < 1e-6,
        "the box is 100 tall and its top answered {bound:.3}"
    );
}

/// And a sheet — a face with no solid under it — still answers `None`, because
/// there is no depth to bound it with. Clamping one would refuse a legitimate
/// push.
#[test]
fn a_lone_sheet_is_not_bounded() {
    let mut m = Mesh::new();
    let vs: Vec<_> = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
        .iter()
        .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
        .collect();
    let f = m.add_face(&vs, mat()).expect("sheet");
    let bound = move_only_max_inward(&m, f);
    println!("  a lone sheet answers {bound:?}");
    assert!(
        bound.is_none(),
        "a sheet has no far side, so there is nothing to stop it at: got {bound:?}"
    );
}
