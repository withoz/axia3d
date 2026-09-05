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

/// ⚠ REWRITTEN TWICE. 2026-09-03: the promotion landed and the old "no such
/// symbol" assertion fired. 2026-09-05: a caller landed and the replacement
/// fired too. Each rewrite kept the claim and changed the instrument, which is
/// what the header asks for.
///
/// The claim that has to stay alive is **cross-bore step 3 is not done**. Both
/// earlier versions asked it of the source text — first "does the promotion
/// exist", then "does anything call it" — and both went stale the moment the
/// thing they described changed, without the claim itself changing at all.
/// `cylinder_to_nurbs_surface` now has a caller (`two_cylinders_meet_as_the_
/// nurbs_they_are`, which intersects two promoted cylinders through the
/// rational SSI, including the unequal radii the closed form defers on), and
/// step 3 is still not done, because nothing routes that capability into the
/// drill.
///
/// So this version asks the drill instead. Measured 2026-09-05 on a 200 box
/// bored 40 along Z, then asked to cross on X:
///
/// ```text
///   r 40  ->  Ok, other_radius 40
///   r 39  ->  refused: "these two bores do not cut each other at shared points"
///   r 25  ->  refused, same
/// ```
///
/// A behavioural question cannot go stale the way a grep can: if someone wires
/// the SSI into `crossing_bore_plan`, the unequal case starts planning and this
/// fails, which is the signal to update the plan's table,
/// `project-cross-bore-kernel` and LOCKED #88.
#[test]
fn the_promotion_exists_and_cross_bore_step_3_still_does_not() {
    let mut hits = files_mentioning("cylinder_to_nurbs");
    hits.extend(files_mentioning("cylinder_as_nurbs"));
    assert!(
        !hits.is_empty(),
        "the Cylinder\u{2192}NURBS promotion vanished \u{2014} cross-bore step 3 lost its \
         prerequisite again"
    );

    let mut mesh = axia_geo::Mesh::new();
    mesh.create_box(glam::DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    mesh.drill_circular_through_hole(
        glam::DVec3::new(0.0, 0.0, 100.0),
        glam::DVec3::Z,
        40.0,
        32,
    )
    .expect("bore along Z");

    // The control. Without it, "unequal is refused" could pass because the
    // fixture never had a bore to cross in the first place.
    assert!(
        mesh.crossing_bore_plan(glam::DVec3::new(100.0, 0.0, 0.0), glam::DVec3::X, 40.0, 32)
            .is_ok(),
        "equal radii are supposed to be planned \u{2014} the fixture or the plan changed, \
         so the refusal below proves nothing"
    );

    for radius in [39.0_f64, 25.0] {
        let planned = mesh
            .crossing_bore_plan(glam::DVec3::new(100.0, 0.0, 0.0), glam::DVec3::X, radius, 32)
            .is_ok();
        assert!(
            !planned,
            "a crossing bore of radius {radius} against a 40 bore is now PLANNED. \
             That is cross-bore step 3 landing: the rational SSI \
             (`intersect_rational_bspline_pair`) reached the drill. Update the \
             plan's table, `project-cross-bore-kernel` and LOCKED #88, then \
             rewrite this \u{2014} do not delete it."
        );
    }
}
