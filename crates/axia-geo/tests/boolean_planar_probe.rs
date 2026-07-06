//! Probe (measurement, not an assertion test): does the ENGINE-level
//! `Mesh::boolean` actually CUT two overlapping planar boxes? This isolates
//! "engine can cut planar" from "the UI DCEL-multi dispatch no-ops on planar".
//! Run: cargo test -p axia-geo --test boolean_planar_probe -- --nocapture

use axia_geo::operations::boolean::BoolOp;
use axia_geo::{FaceId, MaterialId, Mesh};
use glam::DVec3;

fn active(m: &Mesh) -> Vec<FaceId> {
    m.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect()
}

#[test]
fn planar_box_box_subtract_probe() {
    let mat = MaterialId::new(0);
    let mut m = Mesh::new();
    // Box A 100^3, Box B 60^3 offset so it bites A's +x+y+top corner.
    let a = m.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, mat).unwrap();
    let b = m.create_box(DVec3::new(50.0, 50.0, 100.0), 60.0, 60.0, 60.0, mat).unwrap();
    let before = active(&m).len();

    let res = m.boolean(&a, &b, BoolOp::Subtract, mat);
    let after_ids = active(&m);
    let info = m.face_set_manifold_info(&after_ids);
    let active_verts = m.verts.iter().filter(|(_, v)| v.is_active()).count();
    println!(
        "[corner-poke] ok={} faces {}->{} closed={} nm={} => baseline dump below",
        res.is_ok(), before, after_ids.len(), info.is_closed_solid, info.non_manifold_edge_count
    );
    // A plain box = 8 verts; a corner-bitten box has MORE. This disambiguates
    // "genuine cut" from "B removed, A left an untouched box".
    println!("active_verts = {} (plain box=8; bitten>8)", active_verts);
    // Decisive: dump active vertex positions. A pure no-op leaves A's 8 corners
    // (±50,±50,{0,100}) AND B's 8 corners (20/80,20/80,{70,130}) untouched.
    // A genuine cut introduces verts on the bite seam (e.g. z=100 seam at x/y=20).
    let mut ps: Vec<(i64, i64, i64)> = m
        .verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .map(|(_, v)| {
            let p = v.pos();
            (p.x.round() as i64, p.y.round() as i64, p.z.round() as i64)
        })
        .collect();
    ps.sort();
    println!("active vert positions: {:?}", ps);
    probe_config("B pokes TOP-CENTER (blind notch, protrudes up)",
        DVec3::new(0.0, 0.0, 90.0), 40.0, 40.0, 40.0);
    probe_config("B THROUGH-SLOT (spans x, protrudes both ends)",
        DVec3::new(0.0, 0.0, 50.0), 200.0, 30.0, 30.0);
    probe_config("B fully ENCLOSED inside A (cavity)",
        DVec3::new(0.0, 0.0, 50.0), 40.0, 40.0, 40.0);
}

fn probe_config(label: &str, b_pos: DVec3, bw: f64, bh: f64, bd: f64) {
    let mat = MaterialId::new(0);
    let mut m = Mesh::new();
    let a = m.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, mat).unwrap();
    let b = m.create_box(b_pos, bw, bh, bd, mat).unwrap();
    let before = active(&m).len();
    let ok = m.boolean(&a, &b, BoolOp::Subtract, mat).is_ok();
    let after = active(&m);
    let verts = m.verts.iter().filter(|(_, v)| v.is_active()).count();
    // A cut → A's corner set changes (some original corners gone / new seam verts).
    // No-op → A's 8 corners (±50,±50,{0,100}) all still present.
    let a_corners_intact = [
        (-50.0, -50.0, 0.0), (-50.0, -50.0, 100.0), (-50.0, 50.0, 0.0), (-50.0, 50.0, 100.0),
        (50.0, -50.0, 0.0), (50.0, -50.0, 100.0), (50.0, 50.0, 0.0), (50.0, 50.0, 100.0),
    ]
    .iter()
    .all(|&(x, y, z)| {
        m.verts.iter().any(|(_, v)| {
            v.is_active() && (v.pos() - DVec3::new(x, y, z)).length() < 0.5
        })
    });
    println!(
        "[{}] ok={} faces {}->{} verts={} A_corners_intact={} => {}",
        label, ok, before, after.len(), verts, a_corners_intact,
        if a_corners_intact { "NO-OP (A uncut)" } else { "CUT" }
    );
}
