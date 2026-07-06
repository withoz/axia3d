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
    // A plain box = 8 verts; a corner-bitten box has MORE. This disambiguates
    // "genuine cut" from "B removed, A left an untouched box".
    println!("active_verts = {} (plain box=8; bitten>8)", active_verts);

    println!("=== ENGINE Mesh::boolean(boxA - boxB) probe ===");
    println!("result_ok = {}", res.is_ok());
    if let Err(e) = &res {
        println!("ERR = {}", e);
    }
    println!("faces: {} -> {}", before, after_ids.len());
    println!(
        "closed={} nm={} bE={}",
        info.is_closed_solid, info.non_manifold_edge_count, info.boundary_edge_count
    );
    println!(
        "VERDICT: {}",
        if res.is_ok() && after_ids.len() != before {
            "ENGINE CUTS planar box-box (→ UI no-op is a WIRING gap)"
        } else if res.is_ok() {
            "ENGINE no-op (Ok but faces unchanged) — deeper look needed"
        } else {
            "ENGINE errored on planar box-box"
        }
    );
}
