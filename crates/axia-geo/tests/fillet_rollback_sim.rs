//! ADR-302 — a failed fillet/chamfer must leave the mesh untouched.
//!
//! `fillet_edge` tears down its operand faces at fillet.rs:292
//! (`remove_face` on f1, f2, f3_a, f3_b, then a hard `faces.remove`) and only
//! afterwards runs eight fallible steps — five `add_face_with_holes(...)?`, two
//! `orient_arc_for_f3(...)?`, two `splice_vertex_replacement(...)?` and a
//! `bail!`. The WASM entry's Err arm calls `transactions.cancel()`, which sets
//! `is_recording = false` and clears the frame — it restores NOTHING
//! (axia-transaction/src/lib.rs:135-139). The closure-preserving gate runs only
//! on the Ok arm.
//!
//! So the shape is certain from reading. What is NOT certain, and what this
//! measures, is whether any real input reaches an Err *after* the teardown —
//! an adversarial review of this claim corrected the originally-proposed
//! reproduction (a drilled hole rim bails EARLY, at fillet.rs:184, which is
//! safe) and pointed at material-convex geometry instead.
//!
//! Run: `cargo test -p axia-geo --test fillet_rollback_sim -- --nocapture`
//!
//! It prints what it found AND asserts the count is zero (ADR-302).

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use axia_geo::{EdgeId, FaceId};
use glam::DVec3;

fn active_faces(m: &Mesh) -> usize {
    m.faces.iter().filter(|(_, f)| f.is_active()).count()
}

fn is_closed(m: &Mesh) -> bool {
    let active: Vec<FaceId> = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    m.face_set_manifold_info(&active).is_closed_solid
}

fn all_edges(m: &Mesh) -> Vec<EdgeId> {
    m.edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .map(|(id, _)| id)
        .collect()
}

/// One box. The control: every edge of a plain cube should fillet cleanly.
fn plain_box() -> Mesh {
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 100.0, 100.0, 100.0, MaterialId::new(0))
        .expect("create_box");
    m
}

/// Path B (kernel-native) primitives. The source comment at fillet.rs:316-317
/// names this shape as the one that trips the post-teardown `bail!`:
/// "single face wrapping both ends of the edge — e.g., a 2-face cylinder mesh".
fn pathb_cylinder() -> Mesh {
    let mut m = Mesh::new();
    m.create_cylinder_kernel_native_clean(DVec3::ZERO, 50.0, 100.0, MaterialId::new(0))
        .expect("path b cylinder");
    m
}

fn pathb_sphere() -> Mesh {
    let mut m = Mesh::new();
    m.create_sphere_kernel_native(DVec3::ZERO, 50.0, MaterialId::new(0))
        .expect("path b sphere");
    m
}

fn pathb_cone() -> Mesh {
    let mut m = Mesh::new();
    m.create_cone_kernel_native(DVec3::ZERO, 50.0, 100.0, MaterialId::new(0))
        .expect("path b cone");
    m
}

fn pathb_torus() -> Mesh {
    let mut m = Mesh::new();
    m.create_torus_kernel_native(DVec3::ZERO, 80.0, 25.0, MaterialId::new(0))
        .expect("path b torus");
    m
}

/// Polygonal (Path A) cylinder — the control for the Path B cases.
fn patha_cylinder() -> Mesh {
    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, 50.0, 100.0, 16, MaterialId::new(0))
        .expect("path a cylinder");
    m
}

/// A box with a rectangular through-hole. Tube walls meet the entry/exit faces
/// at an inner rim, so an edge's two endpoints can sit on the same third face.
fn drilled_box() -> Mesh {
    let mut m = plain_box();
    let _ = m.drill_rect_through_hole(
        DVec3::new(-25.0, -25.0, 50.0),
        DVec3::new(25.0, 25.0, 50.0),
        DVec3::Z,
    );
    m
}

/// Two boxes genuinely FUSED by a union — a real material-convex boss, unlike
/// two independent `create_box` calls (which are merely disjoint solids).
fn fused_boss() -> Mesh {
    let mut m = Mesh::new();
    let a: Vec<FaceId> = m
        .create_box(DVec3::ZERO, 200.0, 200.0, 100.0, MaterialId::new(0))
        .expect("base");
    let b: Vec<FaceId> = m
        .create_box(DVec3::new(0.0, 0.0, 80.0), 80.0, 80.0, 100.0, MaterialId::new(0))
        .expect("boss");
    let _ = m.boolean_solid(&a, &b, axia_geo::operations::boolean::BoolOp::Union, MaterialId::new(0));
    m
}

/// Sweep every edge of `build()` through `op`, and report any case where the op
/// returned Err but the mesh lost faces anyway.
fn sweep(
    label: &str,
    build: fn() -> Mesh,
    op: fn(&mut Mesh, EdgeId) -> anyhow::Result<()>,
) -> usize {
    let edges = all_edges(&build());
    let mut errs = 0usize;
    let mut mutated_on_err = 0usize;
    let mut msgs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for eid in edges {
        let mut m = build();
        let before_faces = active_faces(&m);
        let before_closed = is_closed(&m);

        match op(&mut m, eid) {
            Ok(()) => {}
            Err(e) => {
                errs += 1;
                let m0 = e.to_string();
                let short: String = m0.chars().take(72).collect();
                *msgs.entry(short).or_insert(0) += 1;
                let after_faces = active_faces(&m);
                let after_closed = is_closed(&m);
                if after_faces != before_faces || (before_closed && !after_closed) {
                    mutated_on_err += 1;
                    println!(
                        "  [{label}] edge {:?}: Err(\"{}\") — faces {before_faces} -> {after_faces}, \
                         closed {before_closed} -> {after_closed}   <<< MUTATED ON FAILURE",
                        eid, e
                    );
                }
            }
        }
    }
    println!("  [{label}] {errs} Err out of the swept edges; {mutated_on_err} of them left the mesh mutated");
    let mut kinds: Vec<(String, usize)> = msgs.into_iter().collect();
    kinds.sort_by(|a, b| b.1.cmp(&a.1));
    for (msg, n) in kinds {
        println!("      x{n}  {msg}");
    }
    mutated_on_err
}

#[test]
fn adr302_sim_fillet_chamfer_mutation_on_failure() {
    println!("
=== ADR-302 de-risk: does a FAILED fillet/chamfer still mutate? ===");

    let cases: Vec<(&str, fn() -> Mesh)> = vec![
        ("box", plain_box as fn() -> Mesh),
        ("cyl-pathA", patha_cylinder),
        ("cyl-pathB", pathb_cylinder),
        ("sphere-pathB", pathb_sphere),
        ("cone-pathB", pathb_cone),
        ("torus-pathB", pathb_torus),
        ("drilled-box", drilled_box),
        ("fused-boss", fused_boss),
    ];

    let mut total = 0usize;
    for (name, build) in cases {
        let m = build();
        println!(
            "-- {name}: {} active faces, {} edges, closed={}",
            active_faces(&m),
            all_edges(&m).len(),
            is_closed(&m)
        );
        total += sweep(&format!("{name}/fillet"), build, |m, e| {
            m.fillet_edge(e, 10.0, 4).map(|_| ())
        });
        total += sweep(&format!("{name}/chamfer"), build, |m, e| {
            m.chamfer_edge(e, 10.0).map(|_| ())
        });
    }

    println!("=== total inputs that failed AND left the mesh mutated: {total} ===
");
    // ADR-302 — this is a GATE, not a report. Before the fix, chamfering any
    // of the four hole-rim edges of `drilled-box` failed with "endpoint N not
    // in F3 loop" AFTER the teardown and left the mesh at 8 faces / open, with
    // no undo frame (the WASM Err arm calls transactions.cancel(), which
    // restores nothing). A failed fillet/chamfer must leave the mesh untouched.
    assert_eq!(
        total, 0,
        "a failed fillet/chamfer mutated the mesh — see the MUTATED lines above"
    );
}
