//! What divides on a solid top — measured 2026-08-13, pinned so it stays known.
//!
//! The expansion plan (docs/plans/SHAPE-DRAWING-EXPANSION-PLAN-2026-08-13.html)
//! numbered D8 "rect first, then a circle, on a solid top — REFUSED" from
//! `describe_overlap`'s matrix and LOCKED #103's leftovers. Both were measured
//! 2026-08-05 — one day BEFORE the solid-top re-tile went live (ADR-281 β-1,
//! 2026-08-06). Re-measured here on the production entries with the production
//! flags: **D8 divides.** So does circle-then-rect, circle-then-circle, and —
//! with the freeform flag production sets — rect-then-ellipse. D8 is retired,
//! not fixed by this file; this file is what stops it being "found" again.
//!
//! Counting faces is not the judgement — D2's 165,664 taught that a tiling can
//! look right and cover ground twice. Every case here also checks the PARTITION:
//! the hole-deducted areas of the faces on the top plane must sum to the top's
//! 40,000 exactly. The instruments read arc and freeform bulge since PR #124,
//! which is what makes that sum trustworthy.
//!
//! Two things are still open, pinned at today's numbers so the fix announces
//! itself by turning them red:
//!
//! - `the_outer_piece_of_a_corner_straddle_is_still_missing` — a rect over the
//!   top's EDGE leaves its hanging piece (45,000 total, outer exists); the same
//!   rect over the CORNER loses it (40,000, outer 0). The outer region of an
//!   edge-straddle is a rectangle; a corner-straddle's is an L — the divide
//!   drops the non-convex outer. This is the plan's D1, narrowed.
//! - `an_ellipse_union_hole_still_drops_its_curve` — the pieces of a
//!   rect × ellipse divide carry their freeform boundary (measured by bulge),
//!   but the host's union HOLE loop does not, so the hole deducts ~5% short
//!   and the plane sums to 40,237.88, not 40,000.

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

const TOP: f64 = 100.0; // the 200-cube spans ±100

fn production_solid() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true; // production (B6-2a TS flips it ON)
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    s
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// Hole-deducted area of every face lying ON the top plane (z = 100).
fn top_plane_area(s: &Scene) -> f64 {
    s.mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && f.normal().z.abs() > 0.999
                && s.mesh.face_bounds(*fid).map_or(false, |(lo, hi)| {
                    (lo.z - TOP).abs() < 1e-3 && (hi.z - TOP).abs() < 1e-3
                })
        })
        .map(|(fid, _)| s.mesh.face_area(fid))
        .sum()
}

/// Faces on the top plane that reach past the top's x = 100 boundary.
fn pieces_past_the_edge(s: &Scene) -> usize {
    s.mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && f.normal().z.abs() > 0.999
                && s.mesh.face_bounds(*fid).map_or(false, |(lo, hi)| {
                    (lo.z - TOP).abs() < 1e-3 && hi.x > 100.0 + 1e-3
                })
        })
        .count()
}

fn rect(s: &mut Scene, cx: f64, cy: f64, w: f64) {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(cx, cy, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: w,
        height: w,
    });
}

fn circle(s: &mut Scene, cx: f64, r: f64) {
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(cx, 0.0, TOP),
        normal: DVec3::Z,
        radius: r,
    });
}

fn assert_sound_partition(s: &Scene, label: &str) {
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "{label}: violations {v:?}");
    let a = top_plane_area(s);
    assert!(
        (a - 40_000.0).abs() < 1e-6,
        "{label}: the top must tile to exactly 40,000 — got {a:.4} \
         (more is double cover, less is a gap)"
    );
}

#[test]
fn a_rect_then_a_circle_divide_a_solid_top() {
    // The order describe_overlap's 2026-08-05 matrix called REFUSED.
    let mut s = production_solid();
    rect(&mut s, 0.0, 0.0, 80.0);
    let mid = active(&s);
    circle(&mut s, 60.0, 40.0); // overlaps the rect's x = 40 edge
    assert!(active(&s) > mid, "the circle must divide, not vanish or stack");
    assert_sound_partition(&s, "rect-then-circle");
}

#[test]
fn a_circle_then_a_rect_divide_a_solid_top() {
    let mut s = production_solid();
    circle(&mut s, 60.0, 40.0);
    let mid = active(&s);
    rect(&mut s, 0.0, 0.0, 80.0);
    assert!(active(&s) > mid, "the rect must divide, not vanish or stack");
    assert_sound_partition(&s, "circle-then-rect");
}

#[test]
fn two_circles_divide_a_solid_top() {
    let mut s = production_solid();
    circle(&mut s, 0.0, 40.0);
    let mid = active(&s);
    circle(&mut s, 60.0, 40.0);
    assert!(active(&s) > mid, "the second circle must divide, not vanish");
    assert_sound_partition(&s, "circle-then-circle");
}

#[test]
fn a_rect_straddling_the_tops_edge_keeps_its_outer_piece() {
    // The convex case works and this holds it: the top divides (40,000 kept
    // whole) and the hanging half lives on the plane beyond it.
    let mut s = production_solid();
    rect(&mut s, 100.0, 0.0, 100.0); // [50,150]×[-50,50]
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "edge-straddle: violations {v:?}");
    let a = top_plane_area(&s);
    assert!(
        (a - 45_000.0).abs() < 1e-6,
        "top 40,000 + hanging 5,000 — got {a:.4}"
    );
    assert_eq!(pieces_past_the_edge(&s), 1, "the hanging piece exists");
}

/// What KIND of loss is it — was the outer boundary never drawn, or drawn and
/// left without a face?
///
/// The answer decides where the fix goes. Edges past the host with no face on
/// them mean the geometry arrived and only the face synthesis / re-derive
/// missed it; no edges at all would mean the draw itself stopped at the host's
/// rim, which is a different repair entirely.
#[test]
fn a_corner_straddles_outer_sides_survive_as_wires_without_a_face() {
    let mut s = production_solid();
    rect(&mut s, 100.0, 100.0, 100.0); // [50,150]² — only [50,100]² is on the top

    let past = |lo: DVec3, hi: DVec3| hi.x > 100.0 + 1e-3 || hi.y > 100.0 + 1e-3;
    let mut wires = 0usize; // edges past the host bounding NO active face
    let mut faced = 0usize; // edges past the host that do bound one
    for (eid, e) in s.mesh.edges.iter() {
        if !e.is_active() {
            continue;
        }
        let (Ok(a), Ok(b)) = (s.mesh.vertex_pos(e.v_small()), s.mesh.vertex_pos(e.v_large()))
        else {
            continue;
        };
        if (a.z - TOP).abs() > 1e-3 || (b.z - TOP).abs() > 1e-3 {
            continue;
        }
        if !past(a.min(b), a.max(b)) {
            continue;
        }
        let (adj, _) = s.mesh.get_faces_sharing_edge(eid);
        if adj
            .iter()
            .any(|&f| s.mesh.faces.get(f).map_or(false, |x| x.is_active()))
        {
            faced += 1;
        } else {
            wires += 1;
        }
    }
    assert_eq!(
        (wires, faced),
        (0, 0),
        "TODAY nothing of the rect survives past the host — not even a wire. \
         So the loss is not 'a face was not synthesized', it is 'the boundary \
         was removed'. The companion test says who removes it. When either \
         count turns non-zero, retire this pin"
    );
}

/// Who removed it: the closed-shape cleanup.
///
/// `exec_draw_rect` ends with `cleanup_dangling_topological_edges` (ADR-025
/// P11 Phase 7), which deactivates leftover topological edges after a CLOSED
/// shape on the grounds that they are synthesis artifacts. Drawing the same
/// four sides as four LINES never reaches that call — and there the outer
/// sides survive as bare wires. Same geometry, same host, same flags; the only
/// difference is the cleanup.
///
/// So the L is lost in two steps: synthesis does not make it (its cycle mixes
/// free edges with the solid's wall-shared rim), and the cleanup then removes
/// the evidence — before the re-derive, which CAN tile it, ever runs
/// (`axia-geo/tests/a_non_convex_outer_piece_is_made.rs` proves it tiles when
/// the boundary is still there).
#[test]
fn drawing_the_same_corner_as_four_lines_keeps_the_outer_sides() {
    let mut s = production_solid();
    let c = [
        DVec3::new(50.0, 50.0, TOP),
        DVec3::new(150.0, 50.0, TOP),
        DVec3::new(150.0, 150.0, TOP),
        DVec3::new(50.0, 150.0, TOP),
    ];
    for i in 0..4 {
        s.execute(Command::DrawLine {
            start: c[i],
            end: c[(i + 1) % 4],
            surface_normal: Some(DVec3::Z),
        });
    }
    let survivors = s
        .mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .filter(|(_, e)| {
            let (Ok(a), Ok(b)) = (s.mesh.vertex_pos(e.v_small()), s.mesh.vertex_pos(e.v_large()))
            else {
                return false;
            };
            (a.z - TOP).abs() < 1e-3
                && (b.z - TOP).abs() < 1e-3
                && (a.x.max(b.x) > 100.0 + 1e-3 || a.y.max(b.y) > 100.0 + 1e-3)
        })
        .count();
    assert!(
        survivors > 0,
        "four lines must leave the outer sides behind — if this is 0 the \
         cleanup is NOT the difference and the companion test's conclusion \
         needs rewriting"
    );
}

/// The same corner-straddle on a SHEET host — is losing the L a solid-only
/// thing, or does the draw path lose it wherever it happens?
///
/// The plan's Track A asks the 36 combinations to answer alike on a sheet and
/// on a solid, so the pair is worth holding even while one of them is wrong.
#[test]
fn a_sheet_host_and_a_solid_host_answer_alike_for_a_corner_straddle() {
    let sheet_outer = {
        let mut s = Scene::new();
        s.auto_intersect_on_draw = true;
        s.auto_face_synthesis_on_draw = true;
        s.face_rederive_on_draw = true;
        s.freeform_overlap_on_draw = true;
        s.execute(Command::DrawRectAsShape {
            center: DVec3::new(0.0, 0.0, 0.0),
            normal: DVec3::Z,
            up: DVec3::X,
            width: 200.0,
            height: 200.0,
        });
        s.execute(Command::DrawRectAsShape {
            center: DVec3::new(100.0, 100.0, 0.0),
            normal: DVec3::Z,
            up: DVec3::X,
            width: 100.0,
            height: 100.0,
        });
        s.mesh
            .faces
            .iter()
            .filter(|(fid, f)| {
                f.is_active()
                    && f.normal().z.abs() > 0.999
                    && s.mesh.face_bounds(*fid).map_or(false, |(_, hi)| hi.x > 100.0 + 1e-3)
            })
            .count()
    };
    let solid_outer = {
        let mut s = production_solid();
        rect(&mut s, 100.0, 100.0, 100.0);
        pieces_past_the_edge(&s)
    };
    assert_eq!(
        (sheet_outer, solid_outer),
        (1, 0),
        "TODAY the sheet keeps its L and the solid does not. Track A wants the \
         two hosts to answer alike — when they do, this reads (1, 1): retire \
         this pin and assert equality instead"
    );
}

/// D1's mechanism, in one place, so the fix does not have to be re-derived:
///
/// 1. `arrange` handles it — two squares meeting at a corner give three
///    regions, the L among them at exactly 7,500
///    (`axia-geo/tests/a_non_convex_outer_piece_is_made.rs`).
/// 2. the re-derive handles it — seeded with a rect FACE on a box top it
///    produces all three, 47,500, invariants intact (same file).
/// 3. but `exec_draw_rect` draws four LINES first, and the synthesis that
///    follows does not make the L: its cycle mixes free edges with the solid's
///    wall-shared rim.
/// 4. `cleanup_dangling_topological_edges` (ADR-025 P11 Phase 7) then removes
///    every scoped edge bounding no active face — closed cycle or not — so the
///    L's boundary is gone before the re-derive is ever called.
///
/// Steps 3 and 4 are why nothing survives past the host, and why four separate
/// lines (which never reach step 4) leave wires behind.
///
/// The fix therefore needs BOTH: let the arrangement run before the cleanup,
/// AND widen its scope to the drawn shape's own extent — today's scope is the
/// affected FACES' boxes (ADR-186 Option A, a perf mechanism), which stop at
/// the host and so never reach the far sides at x = 150. Neither half alone is
/// enough, which is why this is its own change and not a line in this one.
#[test]
fn the_outer_piece_of_a_corner_straddle_is_still_missing() {
    // D1, narrowed: same rect, straddling the CORNER — its outer region is an
    // L, and the divide drops it. Pinned at today's answer; when the divide
    // learns non-convex outer regions this goes red, and the fix should flip
    // it to assert the L exists (40,000 + 7,500).
    let mut s = production_solid();
    rect(&mut s, 100.0, 100.0, 100.0); // [50,150]² — only [50,100]² is on the top
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "corner-straddle: violations {v:?}");
    let a = top_plane_area(&s);
    assert!(
        (a - 40_000.0).abs() < 1e-6,
        "the top itself still tiles exactly — got {a:.4}"
    );
    assert_eq!(
        pieces_past_the_edge(&s),
        0,
        "TODAY the L-shaped outer piece is dropped — if one exists now, the \
         divide learned non-convex outer regions: retire this pin and assert \
         45,000 + the L (7,500) instead"
    );
}

#[test]
fn an_ellipse_union_hole_still_drops_its_curve() {
    // Rect × ellipse divides (it did not before the freeform arm), and the
    // pieces carry their freeform boundary — the bulge-aware instruments read
    // them near-exactly. The host's union HOLE loop does not carry it, so the
    // hole deducts the chord polygon (~5% small) and the plane over-counts by
    // ~238. Pinned at today's number; when hole loops keep their curves this
    // goes red — retire it and fold the case into assert_sound_partition.
    let mut s = production_solid();
    rect(&mut s, 0.0, 0.0, 80.0);
    let mid = active(&s);
    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(60.0, 0.0, TOP),
        ref_dir: DVec3::X, // ⚠ ref_dir BEFORE normal
        normal: DVec3::Z,
        radius_x: 50.0,
        radius_y: 30.0,
    });
    assert!(active(&s) > mid, "the ellipse must divide, not vanish");
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "rect-then-ellipse: violations {v:?}");
    let a = top_plane_area(&s);
    assert!(
        a > 40_000.0 + 100.0 && a < 40_000.0 + 400.0,
        "TODAY the union hole under-deducts and the plane reads ~40,238 — \
         got {a:.4}. At exactly 40,000 the hole kept its curve: retire this \
         pin and use assert_sound_partition"
    );
}
