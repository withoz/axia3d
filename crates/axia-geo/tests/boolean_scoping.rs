//! Scoping simulation (measurement, not assertions): for a matrix of
//! box-box configs × ops, does EACH boolean path actually CUT?
//!   - classic  `Mesh::boolean`               (planar 3-step, wired to demo_* only)
//!   - DCEL multi `boolean_dispatch_dcel_multi` (NURBS surface SSI, the UI path)
//!
//! Verdict per run: A's 8 original corners (±50,±50,{0,100}) all still
//! present => the op did NOT cut A (NO-OP); missing/new corner verts => CUT.
//! For the DCEL path we also print path_used + Ok-pair count + new/removed
//! face totals (a pair that "cuts" adds/removes faces).
//!
//! Purpose: decide the boolean fix route with data (a: classic tri-tri +
//! route non-NURBS pairs / b: extend DCEL to cut planar / c: document
//! unsupported). Run:
//!   cargo test -p axia-geo --test boolean_scoping -- --nocapture

use axia_geo::operations::boolean::BoolOp;
use axia_geo::surfaces::ssi::tolerance::BooleanTolerance;
use axia_geo::{MaterialId, Mesh};
use glam::DVec3;

/// A is always centred [0,0,50], 100^3 → corners (±50,±50,{0,100}).
fn a_corners_intact(m: &Mesh) -> bool {
    [
        (-50.0, -50.0, 0.0), (-50.0, -50.0, 100.0), (-50.0, 50.0, 0.0), (-50.0, 50.0, 100.0),
        (50.0, -50.0, 0.0), (50.0, -50.0, 100.0), (50.0, 50.0, 0.0), (50.0, 50.0, 100.0),
    ]
    .iter()
    .all(|&(x, y, z)| {
        m.verts.iter().any(|(_, v)| {
            v.is_active() && (v.pos() - DVec3::new(x, y, z)).length() < 0.5
        })
    })
}

fn active_faces(m: &Mesh) -> usize {
    m.faces.iter().filter(|(_, f)| f.is_active()).count()
}
fn active_verts(m: &Mesh) -> usize {
    m.verts.iter().filter(|(_, v)| v.is_active()).count()
}

/// Build fresh A + B for a config. Returns (mesh, a_faces, b_faces).
fn build(b_pos: DVec3, bw: f64, bh: f64, bd: f64) -> (Mesh, Vec<axia_geo::FaceId>, Vec<axia_geo::FaceId>) {
    let mat = MaterialId::new(0);
    let mut m = Mesh::new();
    let a = m.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, mat).unwrap();
    let b = m.create_box(b_pos, bw, bh, bd, mat).unwrap();
    (m, a, b)
}

const OPS: [(BoolOp, &str); 3] = [
    (BoolOp::Subtract, "SUB"),
    (BoolOp::Union, "UNI"),
    (BoolOp::Intersect, "INT"),
];

fn run_config(label: &str, b_pos: DVec3, bw: f64, bh: f64, bd: f64) {
    println!("\n### {label}  (B @ {:?} size {}x{}x{})", b_pos, bw, bh, bd);
    let mat = MaterialId::new(0);

    for (op, opn) in OPS {
        // ---- classic Mesh::boolean ----
        {
            let (mut m, a, b) = build(b_pos, bw, bh, bd);
            let before = active_faces(&m);
            let res = m.boolean(&a, &b, op, mat);
            let (verdict, detail) = match &res {
                Ok(_) => {
                    let intact = a_corners_intact(&m);
                    (
                        if intact { "NO-OP" } else { "CUT" },
                        format!("faces {}->{} verts={}", before, active_faces(&m), active_verts(&m)),
                    )
                }
                Err(e) => ("ERR", {
                    let s = e.to_string();
                    s.chars().take(60).collect::<String>()
                }),
            };
            println!("  classic {opn}: {verdict:<5} {detail}");
        }
        // ---- DCEL multi (the UI path) ----
        {
            let (mut m, a, b) = build(b_pos, bw, bh, bd);
            let before = active_faces(&m);
            let res = m.boolean_dispatch_dcel_multi(&a, &b, op, BooleanTolerance::default());
            match res {
                Ok(r) => {
                    let ok_pairs = r.per_pair.iter().filter(|p| p.result.is_ok()).count();
                    let intact = a_corners_intact(&m);
                    let verdict = if !intact || !r.all_new_faces.is_empty() || !r.all_removed_faces.is_empty() {
                        "CUT"
                    } else {
                        "NO-OP"
                    };
                    println!(
                        "  dcel    {opn}: {verdict:<5} path={:?} okPairs={}/{} new={} rem={} faces {}->{}",
                        r.path_used, ok_pairs, r.per_pair.len(),
                        r.all_new_faces.len(), r.all_removed_faces.len(),
                        before, active_faces(&m),
                    );
                }
                Err(e) => {
                    let s = e.to_string();
                    println!("  dcel    {opn}: ERR   {}", s.chars().take(60).collect::<String>());
                }
            }
        }
    }
}

#[test]
fn boolean_path_scoping_matrix() {
    println!("\n========== BOOLEAN SCOPING MATRIX ==========");
    println!("A = box [0,0,50] 100^3 (corners ±50,±50,{{0,100}})");
    println!("Verdict: NO-OP = A uncut (corners intact, no new/removed faces); CUT = geometry changed");

    // 1-4: generic overlaps (walls cross A's faces NON-coplanarly)
    run_config("corner-poke (protrudes +x+y+top)", DVec3::new(50.0, 50.0, 100.0), 60.0, 60.0, 60.0);
    run_config("top-center blind notch", DVec3::new(0.0, 0.0, 90.0), 40.0, 40.0, 40.0);
    run_config("through-slot (spans X)", DVec3::new(0.0, 0.0, 50.0), 200.0, 30.0, 30.0);
    run_config("fully-enclosed cavity", DVec3::new(0.0, 0.0, 50.0), 40.0, 40.0, 40.0);
    // 5: stacked, shares A's TOP plane z=100 (coplanar) but no volume overlap
    run_config("stacked on top (shares z=100 plane)", DVec3::new(0.0, 0.0, 150.0), 100.0, 100.0, 100.0);
    // 6: lateral half-overlap sharing z=0 & z=100 planes (coplanar) AND
    //    volume overlap (the cut plane at x=0 is NON-coplanar) — the case
    //    where coplanar detection COULD help but the cut itself is not coplanar
    run_config("lateral half-overlap (+x by 50, shares z faces)", DVec3::new(50.0, 0.0, 50.0), 100.0, 100.0, 100.0);

    println!("\n========== END MATRIX ==========\n");
}
