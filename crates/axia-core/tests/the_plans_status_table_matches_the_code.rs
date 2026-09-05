//! The reconciliation of 2026-08-25, made to fail when it goes stale again.
//!
//! ⚠ In one session I went to implement three things that were already built,
//! because the records said they were next: cross-bore (the index said "step 2
//! is next", the file itself said "steps 1-4 DONE"), C-2 curved overlap (an
//! explicit **Next: splice…**, which PR #141 had already done), and C-3 a line
//! on a curved face (LOCKED #104 said it needed a state-machine change;
//! `DrawLineTool` already gathers three points and calls `drawOpenSeamOnCurved`).
//!
//! That is LOCKED #88's failure mode, and it has cost more than time — the same
//! lag once had a session call working code broken.
//!
//! So the two claims a reader most needs — *what is genuinely not built* — are
//! pinned here rather than left in prose. A grep in a document rots silently; a
//! test does not.
//!
//! ## What this holds, and what it deliberately does not
//!
//! It holds exactly the two "0 files" claims, because those are what a future
//! session would act on. It does NOT try to pin the other seven rows of the
//! plan's table: those are "✅ present", and a presence claim that goes stale
//! costs nothing — nobody re-implements something the record says is done. The
//! expensive direction is the other one.
//!
//! ⚠ When either of these lands, this test SHOULD fail. That is the signal to
//! update the plan's table, `project-engine-state-and-doc-lag`, and LOCKED #88 —
//! not to delete the assertion.

use std::path::{Path, PathBuf};

/// This file names both symbols in order to assert their absence, so the walk
/// has to skip it.
const SELF: &str = "the_plans_status_table_matches_the_code.rs";

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/axia-core.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

/// Files under `crates/` and `web/src/` whose text contains `needle`.
fn files_mentioning(needle: &str) -> Vec<PathBuf> {
    let root = repo_root();
    let mut hits = Vec::new();
    for sub in ["crates", "web/src"] {
        walk(&root.join(sub), needle, &mut hits);
    }
    hits
}

fn walk(dir: &Path, needle: &str, hits: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            // `target/` is build output and `node_modules/` is not ours.
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "node_modules" || name == "dist" {
                continue;
            }
            walk(&p, needle, hits);
            continue;
        }
        let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext != "rs" && ext != "ts" {
            continue;
        }
        // ⚠ Skip this file. It names both symbols in order to assert they are
        // absent, so counting itself made both assertions fail on the first run
        // — a guard that reports the thing it is looking for.
        if p.file_name().and_then(|s| s.to_str()) == Some(SELF) {
            continue;
        }
        if std::fs::read_to_string(&p)
            .map(|t| t.contains(needle))
            .unwrap_or(false)
        {
            hits.push(p);
        }
    }
}

/// The control. If this returns nothing the walker is broken and the two
/// assertions below would pass for the wrong reason — a search that reaches
/// nothing "finds" every absence.
#[test]
fn the_search_reaches_the_code_at_all() {
    let hits = files_mentioning("imprint_surface_path");
    assert!(
        hits.len() >= 2,
        "the walker found {} files mentioning a symbol known to be present in \
         several — every other assertion in this file is vacuous until this passes",
        hits.len()
    );
}

#[test]
fn face_lineage_is_still_not_built() {
    let hits = files_mentioning("FaceLineage");
    assert!(
        hits.is_empty(),
        "`FaceLineage` now exists in {} file(s) — 순번 5 of the expansion plan has \
         landed. Update the plan's status table, project-engine-state-and-doc-lag, \
         and LOCKED #88, then retire this assertion. Files: {:?}",
        hits.len(),
        hits
    );
}

/// REWRITTEN THREE TIMES, and this one is the other way round.
///
/// 2026-09-03: the promotion landed and the "no such symbol" assertion fired.
/// 2026-09-05 morning: a caller landed and its replacement fired. 2026-09-05
/// afternoon: **cross-bore step 3 itself landed**, and the behavioural version
/// fired too. Each rewrite kept the claim and changed the instrument; this one
/// has no claim of absence left to keep, so it guards the presence instead.
///
/// What was measured when step 3 landed, and why the earlier framing was wrong:
/// the records said it needed SSI subdivision, because
/// `cylinder_cylinder_branches` defers on unequal radii. That is true of the
/// CURVE and irrelevant to the surgery, which wants the per-station band --
/// `s^2 < r_other^2 - r_self^2 sin^2(theta)`, closed form for any two radii,
/// exact to 7.1e-15. The obstacle was the SEGMENTATION: at r 40 x 25 only four
/// points ever coincide between two uniform station sets, at any count. Both
/// walls are split at the union now.
///
/// So this asks the drill for the capability rather than for its absence. If it
/// ever goes back to refusing, that is a regression and the message says where
/// to look.
#[test]
fn cross_bore_step_3_has_landed_and_stays_landed() {
    let mut hits = files_mentioning("cylinder_to_nurbs");
    hits.extend(files_mentioning("cylinder_as_nurbs"));
    assert!(!hits.is_empty(), "the Cylinder-to-NURBS promotion vanished");

    let bored = |radius: f64, segments: u32| {
        let mut mesh = axia_geo::Mesh::new();
        mesh.create_box(glam::DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
            .expect("box");
        mesh.drill_circular_through_hole(
            glam::DVec3::new(0.0, 0.0, 100.0),
            glam::DVec3::Z,
            radius,
            segments,
        )
        .expect("bore along Z");
        mesh
    };

    // The control: the case that worked before step 3.
    let mut equal = bored(40.0, 32);
    equal
        .drill_crossing_bore(glam::DVec3::new(100.0, 0.0, 0.0), glam::DVec3::X, 40.0, 32)
        .expect("equal radii were always supposed to cross");

    for radius in [39.0_f64, 25.0, 8.0] {
        let mut mesh = bored(40.0, 32);
        mesh.drill_crossing_bore(glam::DVec3::new(100.0, 0.0, 0.0), glam::DVec3::X, radius, 32)
            .unwrap_or_else(|e| {
                panic!(
                    "a crossing bore of radius {radius} against a 40 bore is refused again: \
                     {e}. Step 3 landed on 2026-09-05 -- see \
                     `bores_of_different_sizes_can_cross.rs` for what it rests on."
                )
            });
        assert!(
            mesh.verify_outward_normals().is_closed_solid,
            "radius {radius}: the crossing was accepted but the solid is open"
        );
    }
}
