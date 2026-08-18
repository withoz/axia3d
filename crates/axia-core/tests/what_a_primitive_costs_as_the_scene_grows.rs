//! What one primitive costs, and how that changes as the scene grows.
//!
//! Measured in the browser first (`web/e2e/where-the-frame-time-goes.spec.ts`):
//! submitting a frame costs about 0.1 ms and does not move with the scene, while
//! BUILDING the scene went from 10 ms to 97 ms over the same range. So the cost
//! is on this side, and this asks where.
//!
//! Every Tier-3 op takes a full `scene_snapshot()` before it runs, so Undo can
//! put the scene back byte for byte. That snapshot serialises the WHOLE scene,
//! so each primitive pays for everything already in it.
//!
//! Measured 2026-08-18, `--release`, the same primitive every time:
//!
//! ```text
//!       N    면   스냅샷(µs)  메시op(µs)   스냅샷(B)
//!       8    48         29         84       21,738
//!      32   192        143         62       89,706
//!      64   384        277        120      180,330
//!      96   576        370        185      270,954
//! ```
//!
//! The snapshot is linear in the scene — 12.8× for 12× the primitives, and its
//! bytes track exactly — so a session of N primitives pays O(N²) for Undo. By
//! N=96 it is already twice what the mesh op costs.
//!
//! ⚠ It is NOT the whole 97 ms. This calls `mesh.create_box` directly, so it
//! leaves out the WASM boundary, the transaction machinery and the scene-level
//! auto-behaviours; 96 × (370+185)/2 accounts for about 27 ms of it. What is
//! measured here is which PART scales worst, not where every millisecond goes.
//!
//! ⚠ And it is not currently hurting: 370 µs on a 576-face scene is under the
//! click budget by a factor of 90 (메타-원칙 #11). This is the thing to look at
//! FIRST if that changes — not a defect to go and fix now.
//!
//! Prints a table and asserts only the SHAPE: a wall-clock number would be a
//! flake on someone else's machine, but "the snapshot grows with the scene while
//! the mesh op barely does" is a fact about the code.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use glam::DVec3;
use std::time::Instant;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// One row: how long each part of adding the Nth primitive took.
#[derive(Debug)]
struct Row {
    n: usize,
    faces: usize,
    snapshot_us: u128,
    mesh_op_us: u128,
    snapshot_bytes: usize,
}

#[test]
fn a_primitive_pays_for_the_whole_scene_every_time() {
    let mut s = prod();
    let mut rows: Vec<Row> = Vec::new();

    for i in 0..96usize {
        let x = (i % 16) as f64 * 320.0 - 2500.0;
        let y = (i / 16) as f64 * 320.0 - 900.0;

        // What `create_sphere` and friends do first: snapshot the scene so Undo
        // can restore it.
        let t0 = Instant::now();
        let snap = s.scene_snapshot();
        let snapshot_us = t0.elapsed().as_micros();
        let snapshot_bytes = snap.len();

        // And then the mesh op itself. ⚠ The SAME primitive every time: the
        // first version of this varied by `i % 3` and the four samples landed on
        // three different shapes, so a 614 ms row read as an outlier when it was
        // just a sphere among cylinders.
        let t1 = Instant::now();
        let _ = s.mesh.create_box(DVec3::new(x, y, 100.0), 200.0, 200.0, 200.0, FORM_MATERIAL);
        let mesh_op_us = t1.elapsed().as_micros();

        if matches!(i, 7 | 31 | 63 | 95) {
            rows.push(Row {
                n: i + 1,
                faces: s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
                snapshot_us,
                mesh_op_us,
                snapshot_bytes,
            });
        }
    }

    println!("\n  프리미티브 N 번째를 더할 때의 비용\n");
    println!("      N   면    스냅샷(µs)  메시op(µs)   스냅샷 크기(B)");
    for r in &rows {
        println!(
            "    {:>3}  {:>4}  {:>10}  {:>10}  {:>15}",
            r.n, r.faces, r.snapshot_us, r.mesh_op_us, r.snapshot_bytes
        );
    }
    println!();

    let first = &rows[0];
    let last = rows.last().unwrap();

    // The shape of the answer, not the clock:
    //
    //   the snapshot carries the WHOLE scene, so its size grows with the scene
    assert!(
        last.snapshot_bytes > first.snapshot_bytes * 4,
        "the snapshot has to grow with the scene — {} B at N={} against {} B at \
         N={}; if it stops, Undo is no longer storing the whole scene and this \
         file describes something else",
        first.snapshot_bytes, first.n, last.snapshot_bytes, last.n
    );
    //   while the mesh op builds one primitive and does not care what is around it
    assert!(
        last.faces > first.faces * 4,
        "the scene has to actually be growing: {} faces against {}",
        first.faces, last.faces
    );
}
