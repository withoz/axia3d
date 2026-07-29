//! ADR-303 de-risk SIMULATION — the two ops ADR-302 §6 left open.
//!
//! ADR-302 fixed `fillet_edge` / `chamfer_edge`. The same audit named two
//! neighbours with the same shape, and neither has been measured:
//!
//! - `chamfer_vertex_3way` (fillet.rs) tears down f1/f2/f3 at step 7, rebuilds
//!   them at step 8, and only at step 9 asserts
//!   `ensure!(uniq.len() == 3, "expected 3 unique edge trim points")`. If that
//!   fires, the three originals are gone, three replacements are in with the
//!   corner spliced out, and the closing triangle is never added.
//!
//! - `merge_coplanar_faces_geometric` (geometric_merge.rs) removes BOTH operand
//!   faces at :205-208 and only then reaches
//!   `bail!("merged loop degenerate after collinear simplification")` at :219
//!   and a fallible `add_face_with_holes(...)?` at :229. Its WASM entry has
//!   neither a restore nor a gate.
//!
//! LOCKED #88 Phase 3 classified geometric-merge as "self-reject" and left it
//! ungated — but that measurement asked whether the op corrupts *on success*,
//! not whether it leaves debris *on refusal*, so the exemption does not cover
//! this path.
//!
//! Run: `cargo test -p axia-geo --test mutate_on_failure_sim -- --nocapture`
//!
//! It prints what it found AND asserts the count is zero.
//! ADR-302's lesson applies: a sweep that finds nothing must state its inputs.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use axia_geo::{FaceId, VertId};
use glam::DVec3;

#[derive(Clone, Copy, PartialEq)]
struct Metrics {
    faces: usize,
    verts: usize,
    closed: bool,
}

fn measure(m: &Mesh) -> Metrics {
    let active: Vec<FaceId> = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    Metrics {
        faces: active.len(),
        verts: m.verts.iter().filter(|(_, v)| v.is_active()).count(),
        closed: m.face_set_manifold_info(&active).is_closed_solid,
    }
}

fn plain_box() -> Mesh {
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 100.0, 100.0, 100.0, MaterialId::new(0))
        .expect("create_box");
    m
}

fn drilled_box() -> Mesh {
    let mut m = plain_box();
    let _ = m.drill_rect_through_hole(
        DVec3::new(-25.0, -25.0, 50.0),
        DVec3::new(25.0, 25.0, 50.0),
        DVec3::Z,
    );
    m
}

fn fused_boss() -> Mesh {
    let mut m = Mesh::new();
    let a: Vec<FaceId> = m
        .create_box(DVec3::ZERO, 200.0, 200.0, 100.0, MaterialId::new(0))
        .expect("base");
    let b: Vec<FaceId> = m
        .create_box(DVec3::new(0.0, 0.0, 80.0), 80.0, 80.0, 100.0, MaterialId::new(0))
        .expect("boss");
    let _ = m.boolean_solid(
        &a,
        &b,
        axia_geo::operations::boolean::BoolOp::Union,
        MaterialId::new(0),
    );
    m
}

fn patha_cylinder() -> Mesh {
    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, 50.0, 100.0, 16, MaterialId::new(0))
        .expect("cyl");
    m
}

fn active_verts(m: &Mesh) -> Vec<VertId> {
    m.verts
        .iter()
        .filter(|(_, v)| v.is_active())
        .map(|(id, _)| id)
        .collect()
}

fn active_faces(m: &Mesh) -> Vec<FaceId> {
    m.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect()
}

/// Sweep every vertex through `chamfer_vertex_3way` at several radii.
fn sweep_vertex_chamfer(label: &str, build: fn() -> Mesh, radius: f64) -> usize {
    let verts = active_verts(&build());
    let mut errs = 0usize;
    let mut mutated = 0usize;
    let mut kinds: std::collections::HashMap<String, usize> = Default::default();

    for v in verts {
        let mut m = build();
        let before = measure(&m);
        if let Err(e) = m.chamfer_vertex_3way(v, radius) {
            errs += 1;
            let msg: String = e.to_string().chars().take(70).collect();
            *kinds.entry(msg).or_insert(0) += 1;
            let after = measure(&m);
            if after.faces != before.faces || (before.closed && !after.closed) {
                mutated += 1;
                println!(
                    "  [{label}] vert {:?}: Err(\"{}\") — faces {} -> {}, verts {} -> {}, \
                     closed {} -> {}   <<< MUTATED ON FAILURE",
                    v, e, before.faces, after.faces, before.verts, after.verts,
                    before.closed, after.closed
                );
            }
        }
    }
    println!("  [{label}] r={radius}: {errs} Err; {mutated} left the mesh mutated");
    let mut ks: Vec<_> = kinds.into_iter().collect();
    ks.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in ks.iter().take(4) {
        println!("      x{n}  {k}");
    }
    mutated
}

/// Sweep every ordered face pair through `merge_coplanar_faces_geometric`.
fn sweep_geometric_merge(label: &str, build: fn() -> Mesh, tol_deg: f64) -> usize {
    let faces = active_faces(&build());
    let mut errs = 0usize;
    let mut mutated = 0usize;
    let mut kinds: std::collections::HashMap<String, usize> = Default::default();

    for (i, &f1) in faces.iter().enumerate() {
        for &f2 in faces.iter().skip(i + 1) {
            let mut m = build();
            let before = measure(&m);
            if let Err(e) = m.merge_coplanar_faces_geometric(f1, f2, tol_deg) {
                errs += 1;
                let msg: String = e.to_string().chars().take(70).collect();
                *kinds.entry(msg).or_insert(0) += 1;
                let after = measure(&m);
                if after.faces != before.faces || (before.closed && !after.closed) {
                    mutated += 1;
                    println!(
                        "  [{label}] {:?}+{:?}: Err(\"{}\") — faces {} -> {}, closed {} -> {}   \
                         <<< MUTATED ON FAILURE",
                        f1, f2, e, before.faces, after.faces, before.closed, after.closed
                    );
                }
            }
        }
    }
    println!("  [{label}] tol={tol_deg}: {errs} Err; {mutated} left the mesh mutated");
    let mut ks: Vec<_> = kinds.into_iter().collect();
    ks.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in ks.iter().take(4) {
        println!("      x{n}  {k}");
    }
    mutated
}

#[test]
fn adr303_sim_vertex_chamfer_and_geometric_merge_mutation_on_failure() {
    println!("\n=== ADR-303 de-risk: do chamfer_vertex_3way / geometric merge mutate on failure? ===");
    let shapes: Vec<(&str, fn() -> Mesh)> = vec![
        ("box", plain_box as fn() -> Mesh),
        ("cyl-pathA", patha_cylinder),
        ("drilled-box", drilled_box),
        ("fused-boss", fused_boss),
    ];

    let mut total = 0usize;

    println!("-- chamfer_vertex_3way");
    for (name, build) in &shapes {
        for r in [5.0, 20.0, 60.0] {
            total += sweep_vertex_chamfer(name, *build, r);
        }
    }

    println!("-- merge_coplanar_faces_geometric");
    for (name, build) in &shapes {
        for tol in [0.5, 30.0] {
            total += sweep_geometric_merge(name, *build, tol);
        }
    }

    println!("=== total inputs that failed AND left the mesh mutated: {total} ===\n");

    // ADR-303 — a GATE, weaker than ADR-302's but not vacuous. Nothing in this
    // sweep reaches either post-teardown path today, so it locks in the status
    // quo rather than proving a fix. If a future change makes one reachable, or
    // makes some other refusal start leaving debris, this fires.
    assert_eq!(
        total, 0,
        "a failed chamfer_vertex_3way / geometric merge mutated the mesh — \
         see the MUTATED lines above"
    );
}
