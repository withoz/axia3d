//! Phase 3 gate-coverage SIMULATION harness (MEASUREMENT ONLY — no production
//! code changed). For each unguarded topology-mutating engine op, apply a
//! CORRUPTING input against a fresh CLOSED box and empirically measure whether
//! the op can open the solid / make it non-manifold / self-intersecting /
//! violate invariants.
//!
//! Run: `cargo test -p axia-geo --test phase3_gate_sim -- --nocapture`
//!
//! The test itself always PASSES — it prints a table of before→after metrics.

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use axia_geo::{FaceId, VertId};
use axia_geo::operations::slice::SlicePlane;
use axia_geo::operations::boolean::CurvedCutMode;
use glam::DVec3;

/// Four scalar metrics + validity flag captured before/after each op.
#[derive(Clone, Copy)]
struct Metrics {
    closed: bool,
    boundary_edges: usize,
    non_manifold: usize,
    self_intersections: usize,
    invariants_valid: bool,
}

fn measure(mesh: &Mesh) -> Metrics {
    let active: Vec<FaceId> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .collect();
    let info = mesh.face_set_manifold_info(&active);
    let si = mesh.detect_self_intersections();
    let inv = mesh.verify_face_invariants();
    Metrics {
        closed: info.is_closed_solid,
        boundary_edges: info.boundary_edge_count,
        non_manifold: info.non_manifold_edge_count,
        self_intersections: si.count(),
        invariants_valid: inv.is_valid(),
    }
}

/// Fresh closed box (100×100×100 at origin). Returns (mesh, faces).
/// faces[0]=Bottom, faces[1]=Top, faces[2]=Front, [3]=Back, [4]=Right, [5]=Left.
fn fresh_box() -> (Mesh, Vec<FaceId>) {
    let mut mesh = Mesh::new();
    let faces = mesh
        .create_box(DVec3::ZERO, 100.0, 100.0, 100.0, MaterialId::new(0))
        .expect("create_box");
    (mesh, faces)
}

fn top_face_verts(mesh: &Mesh, faces: &[FaceId]) -> Vec<VertId> {
    let start = mesh.faces[faces[1]].outer().start;
    mesh.collect_loop_verts(start).unwrap_or_default()
}

/// Classify: any metric worsened → CORRUPTS. Err before mutate → SELF-REJECTS.
/// Else SAFE.
fn classify(before: &Metrics, after: &Metrics, op_ok: bool, rejected: bool) -> &'static str {
    if rejected {
        return "SELF-REJECTS";
    }
    let worsened = (before.closed && !after.closed)
        || after.boundary_edges > before.boundary_edges
        || after.non_manifold > before.non_manifold
        || after.self_intersections > before.self_intersections
        || (before.invariants_valid && !after.invariants_valid);
    let _ = op_ok;
    if worsened {
        "CORRUPTS"
    } else {
        "SAFE"
    }
}

fn print_row(name: &str, ok: bool, rejected: bool, b: &Metrics, a: &Metrics) {
    let verdict = classify(b, a, ok, rejected);
    let ok_str = if rejected {
        "Err"
    } else if ok {
        "Ok"
    } else {
        "Err"
    };
    println!(
        "[{name:<28}] result={ok_str:<3}  closed: {}→{}  bE: {}→{}  nm: {}→{}  SI: {}→{}  invariants: {}→{}   **{verdict}**",
        tf(b.closed), tf(a.closed),
        b.boundary_edges, a.boundary_edges,
        b.non_manifold, a.non_manifold,
        b.self_intersections, a.self_intersections,
        vi(b.invariants_valid), vi(a.invariants_valid),
    );
}

fn tf(x: bool) -> &'static str { if x { "T" } else { "F" } }
fn vi(x: bool) -> &'static str { if x { "valid" } else { "INVALID" } }

/// Measure only the ORIGINAL box's faces that are still active (distinguishes
/// "tore the original solid" from "added a disjoint object to the scene").
fn measure_subset(mesh: &Mesh, orig: &[FaceId]) -> Metrics {
    let active: Vec<FaceId> = orig
        .iter()
        .copied()
        .filter(|&id| mesh.faces.get(id).map(|f| f.is_active()).unwrap_or(false))
        .collect();
    let info = mesh.face_set_manifold_info(&active);
    let si = mesh.detect_self_intersections(); // SI is whole-mesh scoped
    let inv = mesh.verify_face_invariants();
    Metrics {
        closed: info.is_closed_solid,
        boundary_edges: info.boundary_edge_count,
        non_manifold: info.non_manifold_edge_count,
        self_intersections: si.count(),
        invariants_valid: inv.is_valid(),
    }
}

/// Run one op: takes a closure that mutates the box and returns Result<()>.
/// If the box is not closed at start, panics (setup guard).
/// `subset`=true → measure only original box faces (for additive ops that add
/// disjoint geometry, so we ask "did the ORIGINAL solid stay closed?").
fn run_op_impl<F>(name: &str, subset: bool, apply: F)
where
    F: FnOnce(&mut Mesh, &[FaceId]) -> anyhow::Result<()>,
{
    let (mut mesh, faces) = fresh_box();
    let before = measure(&mesh);
    assert!(before.closed, "SETUP: box must be closed before op {name}");

    let res = apply(&mut mesh, &faces);
    let (ok, rejected) = match &res {
        Ok(_) => (true, false),
        Err(_) => (false, true),
    };
    let after = if subset {
        measure_subset(&mesh, &faces)
    } else {
        measure(&mesh)
    };
    print_row(name, ok, rejected, &before, &after);
}

fn run_op<F>(name: &str, apply: F)
where
    F: FnOnce(&mut Mesh, &[FaceId]) -> anyhow::Result<()>,
{
    run_op_impl(name, false, apply);
}

/// For additive ops: measure the original box's faces only.
fn run_op_subset<F>(name: &str, apply: F)
where
    F: FnOnce(&mut Mesh, &[FaceId]) -> anyhow::Result<()>,
{
    run_op_impl(name, true, apply);
}

#[test]
fn phase3_gate_simulation() {
    println!("\n=== Phase 3 Gate Coverage Simulation (closed box, corrupting inputs) ===\n");

    // ── Transform: apply to a SUBSET (top face) with large delta/rotation ──
    run_op("translate_faces(top,+80z)", |m, f| {
        m.translate_faces(&[f[1]], DVec3::new(0.0, 0.0, 80.0)).map(|_| ())
    });
    run_op("translate_verts(top,+80z)", |m, f| {
        let vs = top_face_verts(m, f);
        m.translate_verts(&vs, DVec3::new(0.0, 0.0, 80.0)).map(|_| ())
    });
    run_op("rotate_verts(top,90deg)", |m, f| {
        let vs = top_face_verts(m, f);
        m.rotate_verts(&vs, DVec3::ZERO, DVec3::Z, std::f64::consts::FRAC_PI_2).map(|_| ())
    });
    run_op("scale_verts(top,3x-nonuniform)", |m, f| {
        let vs = top_face_verts(m, f);
        m.scale_verts(&vs, DVec3::ZERO, DVec3::new(3.0, 1.0, 1.0)).map(|_| ())
    });
    run_op("scale_verts(top,NEGATIVE-x)", |m, f| {
        let vs = top_face_verts(m, f);
        m.scale_verts(&vs, DVec3::ZERO, DVec3::new(-1.0, 1.0, 1.0)).map(|_| ())
    });
    run_op("rotate_faces(top,90deg)", |m, f| {
        m.rotate_faces(&[f[1]], DVec3::ZERO, DVec3::Z, std::f64::consts::FRAC_PI_2).map(|_| ())
    });
    run_op("scale_faces(top,NEGATIVE-x)", |m, f| {
        m.scale_faces(&[f[1]], DVec3::ZERO, DVec3::new(-1.0, 1.0, 1.0)).map(|_| ())
    });

    // ── Deform: large parameter over the WHOLE box ──
    run_op("bend_verts(all,180deg)", |m, _f| {
        let vs: Vec<VertId> = m.verts.iter().map(|(id, _)| id).collect();
        m.bend_verts(&vs, DVec3::X, DVec3::Z, DVec3::new(0.0, 0.0, -50.0),
            std::f64::consts::PI, 100.0).map(|_| ())
    });
    run_op("twist_verts(all,large)", |m, _f| {
        let vs: Vec<VertId> = m.verts.iter().map(|(id, _)| id).collect();
        // ~180deg per 100 units of axial distance
        m.twist_verts(&vs, DVec3::ZERO, DVec3::Z,
            std::f64::consts::PI / 100.0).map(|_| ())
    });
    run_op("taper_verts(all,3x)", |m, _f| {
        let vs: Vec<VertId> = m.verts.iter().map(|(id, _)| id).collect();
        m.taper_verts(&vs, DVec3::new(0.0, 0.0, -50.0), DVec3::Z, 1.0, 3.0, 100.0).map(|_| ())
    });

    // ── Merge: mis-merge two parallel/opposite faces of the box ──
    run_op("merge_coplanar_faces_geometric(top,bottom)", |m, f| {
        m.merge_coplanar_faces_geometric(f[1], f[0], 5.0).map(|_| ())
    });
    run_op("merge_coplanar_containing(top,bottom)", |m, f| {
        m.merge_coplanar_containing(f[1], f[0], 5.0).map(|_| ())
    });

    // ── Trim / cut: plane grazing/partially intersecting the box ──
    run_op("trim_volume_by_plane(z=0)", |m, f| {
        let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
        m.trim_volume_by_plane(f, plane, true, MaterialId::new(0)).map(|_| ())
    });
    run_op("cut_curved_by_z_plane(z=0)", |m, f| {
        match m.cut_curved_by_z_plane(f, 0.0, CurvedCutMode::Slice, MaterialId::new(0)) {
            None => anyhow::bail!("dispatch: not a curved primitive"),
            Some(r) => r.map(|_| ()),
        }
    });
    run_op("trim_curved_by_plane(z=0)", |m, f| {
        match m.trim_curved_by_plane(f, DVec3::ZERO, DVec3::Z, MaterialId::new(0)) {
            None => anyhow::bail!("dispatch: not a curved primitive"),
            Some(r) => r.map(|_| ()),
        }
    });

    // ── Topology: split one edge / flip one face ──
    run_op("split_edge(one edge,midpoint)", |m, f| {
        let edges = m.face_outer_edges(f[1])?;
        let eid = edges[0];
        // midpoint of that edge
        let (va, vb) = {
            let e = &m.edges[eid];
            (e.v_small(), e.v_large())
        };
        let pa = m.verts[va].pos();
        let pb = m.verts[vb].pos();
        let mid = (pa + pb) * 0.5;
        m.split_edge(eid, mid).map(|_| ())
    });
    run_op("flip_faces(one face)", |m, f| {
        let n = m.flip_faces(&[f[1]]);
        if n == 0 { anyhow::bail!("flip produced 0"); }
        Ok(())
    });

    // ── Additive (expected safe): mirror / array / subdivide.
    //    NOTE: mirror/array add DISJOINT new faces, so we measure only the
    //    ORIGINAL box faces — asking "did the original solid stay closed?" ──
    run_op_subset("mirror_faces(top across z=200)", |m, f| {
        m.mirror_faces(&[f[1]], DVec3::new(0.0, 0.0, 200.0), DVec3::Z).map(|_| ())
    });
    run_op_subset("array_linear_faces(top x3)", |m, f| {
        m.array_linear_faces(&[f[1]], 3, DVec3::new(200.0, 0.0, 0.0)).map(|_| ())
    });
    run_op("subdivide_catmull_clark(whole)", |m, _f| {
        m.subdivide_catmull_clark().map(|_| ())
    });

    // ═══════════════════════════════════════════════════════════════════
    // ADVERSARIAL re-test — "SAFE"-classified UNGATED ops with EXTREME inputs.
    // Lesson from P3-B: translate was SAFE for +z but CORRUPTS on overshoot.
    // taper is the one ungated deform op (bend/twist corrupted → were gated).
    // ═══════════════════════════════════════════════════════════════════
    run_op("ADV taper_verts(pinch end=0.02)", |m, _f| {
        let vs: Vec<VertId> = m.verts.iter().map(|(id, _)| id).collect();
        m.taper_verts(&vs, DVec3::new(0.0, 0.0, -50.0), DVec3::Z, 1.0, 0.02, 100.0).map(|_| ())
    });
    run_op("ADV taper_verts(neg start=-1 end=10)", |m, _f| {
        let vs: Vec<VertId> = m.verts.iter().map(|(id, _)| id).collect();
        m.taper_verts(&vs, DVec3::new(0.0, 0.0, -50.0), DVec3::Z, -1.0, 10.0, 100.0).map(|_| ())
    });
    run_op("ADV subdivide x2", |m, _f| {
        m.subdivide_catmull_clark()?;
        m.subdivide_catmull_clark().map(|_| ())
    });

    println!("\n=== end of simulation table ===\n");
}
