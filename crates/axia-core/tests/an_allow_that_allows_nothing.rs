//! `#[allow(dead_code)]` HAS TO BE ALLOWING SOMETHING.
//!
//! An attribute that suppresses nothing is worse than no attribute: it tells the
//! next reader "this is knowingly unused" about code that is used, and it hides
//! the one case that matters when the code later stops being used.
//!
//! ## What was measured (2026-09-12, at `9ef6e2f`)
//!
//! There were 41 of them across `crates/*/src`. Stripping all 41 and rebuilding
//! the **`(lib test)`** target — the only one whose answer counts, because the
//! plain `(lib)` build has no `#[cfg(test)]` module and so reports every
//! test-only item as dead — revealed 14 warning sites. Everything else was
//! allowing nothing:
//!
//! ```text
//!   before    41 attributes      3 dead-code warnings
//!   after     15 attributes      3 dead-code warnings   (the same three)
//! ```
//!
//! The three that survive on purpose, and are the baseline this guards:
//!
//! ```text
//!   axia-geo   mesh.rs      simplify_collinear_loop   (a `_preserving` sibling took the job)
//!   axia-geo   boolean.rs   uv_holes                  (field, never read)
//!   axia-core  scene.rs     describe_overlap          (method, no caller)
//! ```
//!
//! ⚠ Three of the removed ones carried comments naming a caller —
//! *"find_intersections에서 호출 (현재 비활성 경로)"* and friends. The comments
//! were right: the callers exist, which is exactly why the attribute suppressed
//! nothing.
//!
//! ## If this test fails
//!
//! It is a count, so it fails in both directions and the direction matters.
//!
//! - **Count went UP** — someone silenced a dead-code warning instead of wiring
//!   the code or deleting it. That may be correct (a deferred hook, like
//!   `mark_faces_dirty` for ADR-193's delta pipeline), but it should be a
//!   decision someone made, not a reflex. Bump the number here and say why in
//!   the commit.
//! - **Count went DOWN** — an attribute was removed. Check that no NEW
//!   dead-code warning appeared:
//!   `cargo test -p <crate> --lib --no-run`, read only the `(lib test)`
//!   warnings, and filter by path (a dependency crate's `(lib)` warnings land in
//!   the same log and look identical).

use std::path::{Path, PathBuf};

/// This file names the attribute in prose, so the walk has to skip itself.
const SELF: &str = "an_allow_that_allows_nothing.rs";

/// What the 2026-09-12 cleanup left. Every one of these suppresses a real
/// warning — verified by removing all of them and watching 14 sites light up.
const EXPECTED: usize = 15;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/axia-core.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, usize)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name == "node_modules" || name == "dist" {
                continue;
            }
            walk(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            if p.file_name().and_then(|s| s.to_str()) == Some(SELF) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            let n = text
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    t.starts_with("#[allow(dead_code)]") || t.starts_with("#![allow(dead_code)]")
                })
                .count();
            if n > 0 {
                out.push((p, n));
            }
        }
    }
}

/// Only `crates/*/src` — a test's own unused helper is a different question and
/// is left to the compiler's own warning.
fn suppressions() -> Vec<(PathBuf, usize)> {
    let root = repo_root();
    let mut out = Vec::new();
    let Ok(crates) = std::fs::read_dir(root.join("crates")) else {
        panic!("crates/ not found — is repo_root() still right?")
    };
    for c in crates.flatten() {
        let src = c.path().join("src");
        if src.is_dir() {
            walk(&src, &mut out);
        }
    }
    out.sort();
    out
}

#[test]
fn every_suppression_is_load_bearing() {
    let found = suppressions();
    let total: usize = found.iter().map(|(_, n)| n).sum();

    let listing = found
        .iter()
        .map(|(p, n)| {
            let short = p
                .strip_prefix(repo_root())
                .unwrap_or(p)
                .to_string_lossy()
                .replace('\\', "/");
            format!("    {short}  ×{n}")
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(
        total, EXPECTED,
        "`#[allow(dead_code)]` count moved: {total} against {EXPECTED}. \
         See this file's header for which direction means what and how to \
         re-measure. Current sites:\n{listing}"
    );
}

/// The measurement that justified the number, kept so it is not re-derived by
/// hand: the cleanup did not touch any crate outside these.
#[test]
fn the_survivors_sit_where_the_measurement_put_them() {
    let found = suppressions();
    let mut files: Vec<String> = found
        .iter()
        .map(|(p, _)| {
            p.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string()
        })
        .collect();
    files.sort();
    files.dedup();

    // Measured 2026-09-12. A file joining this list means a new suppression
    // appeared somewhere the cleanup had emptied — worth a second look even if
    // the total is unchanged, because it means one was removed elsewhere.
    let expected = [
        "boolean.rs",
        "face_split.rs",
        "intersect.rs",
        "lib.rs",
        "mesh.rs",
        "region.rs",
        "robust_split.rs",
        "scene.rs",
        "trim_boolean.rs",
    ];
    assert_eq!(
        files,
        expected,
        "the set of files carrying a suppression changed"
    );
}
