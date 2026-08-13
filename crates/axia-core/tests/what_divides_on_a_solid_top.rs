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

// ── Track A's remaining rows, measured before being believed ───────────────
//
// D2 and D7 come from the plan's defect table and were written down in
// 2026-08. Two of that table's rows turned out to be already fixed by the time
// they were re-measured (D8 by ADR-281, and the ellipse row by the freeform
// arm), so these are asserted rather than assumed — the same 40,000 partition
// is the judge.

/// D2 — the drawn shape SWALLOWS the host (reverse containment).
///
/// The plan's evidence: a Ø400 circle over a 200 × 200 top read 165,664 where
/// 125,664 is the truth — the disc sat ON the top instead of the top becoming
/// a hole in it, so 40,000 mm² was covered twice. Reverse containment reaches
/// neither containment detector (both look for something INSIDE) nor
/// `auto_intersect_coplanar` (which wants crossings), which is why it had no
/// path at all.
#[test]
fn a_shape_that_swallows_the_top_does_not_cover_it_twice() {
    let mut s = production_solid();
    let before = active(&s);
    // Ø400 circle centred on the 200 × 200 top: the top is strictly inside it.
    circle(&mut s, 0.0, 200.0);
    assert!(active(&s) > before, "the circle must appear");
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "swallowing circle: violations {v:?}");
    // The plane now holds the top (40,000) plus the ring around it
    // (π·200² − 40,000 = 85,663.71), and nothing twice: 125,663.71.
    let truth = std::f64::consts::PI * 200.0 * 200.0;
    let a = top_plane_area(&s);
    assert!(
        (a - truth).abs() < 1.0,
        "the plane must hold πr² = {truth:.4} once — got {a:.4} \
         (truth + 40,000 = {:.4} means the top is covered twice)",
        truth + 40_000.0
    );
}

/// D7 — drawing on a host that already has a hole.
///
/// `single_face_containing_corners` returns None once the host carries an
/// inner loop, so the interior fast-path is skipped and the draw falls through
/// to the unified pipeline. The question is only whether the result is sound.
#[test]
fn a_second_shape_on_a_holed_top_still_tiles_it() {
    let mut s = production_solid();
    circle(&mut s, 0.0, 40.0); // a disc in the middle of the top
    assert_sound_partition(&s, "first circle");
    let mid = active(&s);
    // A rect in the free part of the top, clear of the disc.
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(-60.0, -60.0, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 50.0,
        height: 50.0,
    });
    assert!(active(&s) > mid, "the rect must appear on a holed host");
    assert_sound_partition(&s, "rect on a holed top");
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

/// The outer boundary is there AND it bounds a face.
///
/// This pinned the loss while it stood: nothing survived past the host, not
/// even a wire, because the closed-shape cleanup deleted the whole hanging
/// boundary before the arrangement could use it. Both halves of the repair
/// are needed to move it — keeping a closed cycle through the cleanup, and
/// letting the arrangement's scope follow the wires out — so this asserting
/// `faced > 0` is what says both are still in place.
#[test]
fn a_corner_straddles_outer_sides_bound_a_face() {
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
        wires, 0,
        "no part of the hanging boundary may be left as a bare wire — \
         {wires} wire(s), {faced} faced"
    );
    assert!(
        faced > 0,
        "the rect's outer sides must bound the hanging piece — 0 faced edges \
         means the boundary was deleted again, or the arrangement never \
         reached it"
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
        sheet_outer, solid_outer,
        "sheet kept {sheet_outer} outer piece(s), solid kept {solid_outer} — \
         Track A wants the two hosts to answer alike"
    );
    assert_eq!(solid_outer, 1, "and the answer is one hanging piece, not none");
}

/// D1: the corner-straddle keeps its L.
///
/// Where it used to go, since the two halves that fix it are far apart and a
/// regression in either would land here:
///
/// 1. `arrange` always handled it — two squares meeting at a corner give three
///    regions, the L among them at exactly 7,500
///    (`axia-geo/tests/a_non_convex_outer_piece_is_made.rs`).
/// 2. so did the re-derive, seeded with a rect FACE — all three, 47,500.
/// 3. but `exec_draw_rect` draws four LINES first, and the synthesis that
///    follows cannot make the L: its cycle mixes free edges with the solid's
///    wall-shared rim.
/// 4. `cleanup_dangling_topological_edges` then deleted that unfaced boundary
///    as a residual, before the re-derive was ever called — which is why
///    nothing survived past the host, not even a wire, while the same four
///    sides drawn as four LINES (never reaching step 4) kept theirs.
///
/// Fixed at 3-4 rather than at 1-2: the cleanup now peels only strands with a
/// loose END and leaves closed cycles alone, and the re-derive's scope follows
/// free wires out of the affected region so the arrangement is handed the whole
/// boundary instead of the part inside the host.
///
/// ⚠ Both halves are needed and the first is gated on `face_rederive_on_draw`,
/// which production sets (ADR-176) and the engine default does not. Keeping a
/// cycle is keeping it FOR the arrangement; with no arrangement it is just an
/// orphan, and the 27-RECT stress measures 10 of them if the gate is dropped.
/// So on engine defaults this case is still open — deliberately, and the two
/// tests that pin ADR-025 P11 STRICT are what say so.
#[test]
fn a_corner_straddle_keeps_its_l_shaped_outer_piece() {
    let mut s = production_solid();
    rect(&mut s, 100.0, 100.0, 100.0); // [50,150]² — only [50,100]² is on the top
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "corner-straddle: violations {v:?}");
    let a = top_plane_area(&s);
    assert!(
        (a - 47_500.0).abs() < 1e-6,
        "the top's 40,000 plus the L's 7,500 — got {a:.4} (40,000 means the L \
         is gone again; more means something is covered twice)"
    );
    assert_eq!(pieces_past_the_edge(&s), 1, "one hanging piece, the L");
}

/// D9′ was my own misreading, and this is what the numbers actually say.
///
/// I pinned "the plane reads 40,237.88, so the union hole under-deducts by
/// ~238" from the total alone, without checking that a hole existed. It does
/// not: the ellipse in that case is centred at x = 60 with rx = 50, so it
/// reaches x = 110 and hangs 10 mm past the top's own edge at x = 100. The
/// excess is that hanging piece — the same thing D1's L is, and correct.
///
/// The instrument was never at fault either: a hole bounded by a Bezier
/// deducts its bulge to the closed form
/// (`axia-geo/tests/a_freeform_hole_deducts_its_bulge.rs`), and arcs were
/// settled in PR #124.
///
/// So the question the total cannot answer is asked properly here: what lies
/// WITHIN the top's footprint must tile it exactly once, and what reaches past
/// it is a hanging sheet, counted separately. Analytically the ellipse's cap
/// beyond x = 100 is 3,000 · ∫√(1−t²)dt over [0.8, 1] ≈ 245.
#[test]
fn a_rect_and_an_ellipse_tile_the_top_and_the_ellipse_hangs_over() {
    let mut s = production_solid();
    rect(&mut s, 0.0, 0.0, 80.0);
    let mid = active(&s);
    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(60.0, 0.0, TOP),
        ref_dir: DVec3::X, // ⚠ ref_dir BEFORE normal
        normal: DVec3::Z,
        radius_x: 50.0, // reaches x = 110 — past the top's edge at 100
        radius_y: 30.0,
    });
    assert!(active(&s) > mid, "the ellipse must divide, not vanish");
    let v = s.mesh.verify_face_invariants().violations;
    assert!(v.is_empty(), "rect-then-ellipse: violations {v:?}");

    // Split the plane's faces by whether they stay inside the top's footprint.
    let mut inside = 0.0;
    let mut hanging = 0.0;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() || f.normal().z.abs() <= 0.999 {
            continue;
        }
        let Some((lo, hi)) = s.mesh.face_bounds(fid) else { continue };
        if (lo.z - TOP).abs() > 1e-3 || (hi.z - TOP).abs() > 1e-3 {
            continue;
        }
        if hi.x > 100.0 + 1e-3 {
            hanging += s.mesh.face_area(fid);
        } else {
            inside += s.mesh.face_area(fid);
        }
    }
    assert!(
        (inside + hanging - top_plane_area(&s)).abs() < 1e-6,
        "the split must account for every face on the plane"
    );
    assert!(
        hanging > 100.0 && hanging < 400.0,
        "the ellipse's cap past x = 100 is ~245 by hand — got {hanging:.4}"
    );
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "what lies within the top must tile it exactly once — got {inside:.4} \
         (this is the assertion the old 40,237.88 pin should have made)"
    );
}
