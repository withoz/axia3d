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

/// The Scene's re-derive entry tiles an ellipse on a box.
///
/// ⚠ COUNT THE PIECES — this was written to bisect the draw command and every
/// version of it passed while nothing was happening, because "top = 40,000 and
/// something hanging" holds for an untouched top and an unsplit ellipse alike.
/// That vacuity is why three wrong hypotheses got as far as they did.
///
/// Instrumenting the real command is what settled it. Before the fix, this
/// replay and the production draw reported exactly the same thing:
///
/// ```text
/// RebuildReport { removed_faces: 1, created_faces: 1, coplanar_edges: 4 }
/// faces 7 → 7
/// ```
///
/// Four edges — the box top's own perimeter, the ellipse not among them. A
/// closed freeform self-loop is preserved rather than fed to the arrangement
/// unless `detect_freeform_overlaps` gives it a `curve_owner_id`, and that
/// detector read the edges it might overlap from `scope_edges`, which excludes
/// `volume_edges` — exactly a solid top's perimeter. It is now also shown
/// `solid_top_boundary`, which the arrangement was already being given.
#[test]
fn the_scenes_own_rederive_entry_tiles_an_ellipse_on_a_box() {
    use axia_geo::curves::{nurbs, AnalyticCurve};

    let mut s = Scene::new();
    s.face_rederive_on_draw = true;
    s.auto_intersect_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    // `execute()` clears the face-AABB cache before dispatching, so do that too.
    s.mesh.clear_face_aabb_cache();
    // Production opens the transaction BEFORE it makes the face, so do that.
    s.transactions.begin();
    s.transactions.set_before_snapshot(s.scene_snapshot());
    let centre = DVec3::new(100.0, 0.0, TOP);
    let (cp, w, k, deg) = nurbs::ellipse(centre, 60.0, 33.0, DVec3::X, DVec3::Y);
    let anchor = s.mesh.add_vertex(cp[0]);
    let fid = s
        .mesh
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: deg as u32 },
            axia_core::scene::FORM_MATERIAL,
        )
        .expect("ellipse face");

    // The real command registers the Shape BEFORE re-deriving, and the
    // re-derive captures and re-adopts owners, so the ownership has to be here
    // for this to be the same call.
    s.create_shape("Ellipse (kernel-native)".to_string(), vec![fid]);
    s.intersect_faces_inner(&[fid]).expect("the Scene's re-derive entry");
    s.transactions.set_after_snapshot(s.scene_snapshot());
    s.transactions.commit();

    let (mut inside, mut hanging) = (0.0, 0.0);
    for (f, face) in s.mesh.faces.iter() {
        if !face.is_active() || face.normal().z.abs() <= 0.999 {
            continue;
        }
        let Some((lo, hi)) = s.mesh.face_bounds(f) else { continue };
        if (lo.z - TOP).abs() > 1e-3 || (hi.z - TOP).abs() > 1e-3 {
            continue;
        }
        if hi.x > 100.0 + 1e-3 { hanging += s.mesh.face_area(f) } else { inside += s.mesh.face_area(f) }
    }
    let pieces = s
        .mesh
        .faces
        .iter()
        .filter(|(f, face)| {
            face.is_active()
                && face.normal().z.abs() > 0.999
                && s.mesh.face_bounds(*f).map_or(false, |(lo, hi)| {
                    (lo.z - TOP).abs() < 1e-3 && (hi.z - TOP).abs() < 1e-3
                })
        })
        .count();
    assert_eq!(
        pieces, 3,
        "three pieces — the bitten top, the half on it, the half hanging over. \
         Two means the ellipse stopped reaching the arrangement again"
    );
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "the top tiles exactly once — got {inside:.4}"
    );
    assert!(hanging > 100.0, "and the hanging half is its own face — got {hanging:.4}");

    // So the tiling is right when it leaves this call. What the draw command
    // does next is the post-draw repair, which only acts on pairs the
    // self-intersection scan reports — so ask what it would find on a result
    // that is already correct.
    let pairs = s.mesh.detect_self_intersections().intersecting_pairs;
    let sharing_an_edge = pairs
        .iter()
        .filter(|(a, b)| {
            s.mesh.edges.iter().any(|(eid, e)| {
                if !e.is_active() {
                    return false;
                }
                let (fs, _) = s.mesh.get_faces_sharing_edge(eid);
                fs.contains(a) && fs.contains(b)
            })
        })
        .count();
    // The repair only acts on COPLANAR pairs (a fold between faces at an angle
    // is a different problem and it says so). So the number that decides
    // whether it can reach these is how many of them are coplanar.
    let coplanar = pairs
        .iter()
        .filter(|(a, b)| {
            let (na, nb) = (s.mesh.faces[*a].normal(), s.mesh.faces[*b].normal());
            na.normalize_or_zero().dot(nb.normalize_or_zero()).abs() >= 0.999
        })
        .count();
    assert_eq!(
        (pairs.len(), sharing_an_edge, coplanar),
        (2, 0, 0),
        "the scan still reports two pairs — a sheet hanging past the rim TOUCHES \
         the wall without sharing an edge, and contact reads the same as \
         penetration to it (the plan's D5, still open). But NONE is coplanar \
         now, so the post-draw repair cannot reach this state: it only acts on \
         coplanar pairs. Before the ellipse reached the arrangement one of them \
         was, which is what made the repair look like the culprit"
    );

    // So the repair must not act on it. It runs on every draw, so the next
    // draw anywhere in the scene is what would reach this state — and the
    // tiling has to survive it.
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(-500.0, -500.0, TOP),
        normal: DVec3::Z,
        up: DVec3::X,
        width: 20.0,
        height: 20.0,
    });
    let mut after = 0.0;
    for (f, face) in s.mesh.faces.iter() {
        if !face.is_active() || face.normal().z.abs() <= 0.999 {
            continue;
        }
        let Some((lo, hi)) = s.mesh.face_bounds(f) else { continue };
        if (lo.z - TOP).abs() > 1e-3 || (hi.z - TOP).abs() > 1e-3 || hi.x > 100.0 + 1e-3 {
            continue;
        }
        if lo.x < -400.0 {
            continue; // the far-away rect, not the host
        }
        after += s.mesh.face_area(f);
    }
    assert!(
        (after - 40_000.0).abs() < 1.0,
        "a later draw elsewhere must not let the repair carve this tiling back \
         out — the top read {after:.4} afterwards. Without the shared-ground \
         guard the coplanar pair above is taken and the host loses the lens"
    );
    // TODAY: two pairs, neither sharing an edge, ONE of them coplanar. A
    // coplanar pair is one the post-draw repair could act on, so this looked
    // like the answer — the repair undoing correct work, i.e. the plan's D5.
    //
    // ⚠ IT IS NOT, and the correction is the point of this block. A guard
    // making the repair require real shared GROUND rather than mere contact
    // was written and tried FOUR ways — an absolute floor and a threshold
    // relative to the smaller face, each both after the circle branch and
    // ahead of every branch. The production ellipse reads the same 36,891 in
    // all four, and removing the guard again breaks nothing. So the repair is
    // not what carves, and the guard was reverted rather than landed inert.
    //
    // What this test does establish is the other half: every step of
    // `exec_draw_ellipse_as_curve` replayed by hand — the AABB cache cleared
    // as `execute` clears it, the transaction opened BEFORE the face is made,
    // the face, the Shape, then this call — tiles to exactly 40,000. The real
    // command does not. Everything between them that could differ has now been
    // checked: normal, surface, plane point, ownership, both flags, the
    // transaction, the cache, and the repair.
    //
    // Which means the next step is not another hypothesis. It is to instrument
    // the real command — count the faces either side of its own
    // `intersect_faces_inner` — because reading produces plausible stories
    // faster than the code can correct them.
    assert_eq!(
        (pairs.len(), sharing_an_edge),
        (2, 0),
        "TODAY a correctly tiled plane still reports two overlaps, neither \
         between faces that share an edge. When the scan stops counting \
         contact this reads (0, 0): retire the pin and assert emptiness"
    );
}

/// Which path is doing the damage — the re-derive, or the post-draw repair?
///
/// With `face_rederive_on_draw` ON and `auto_intersect_on_draw` OFF, the
/// re-derive is the only thing that can act on the draw. If the result matches
/// the full-production one, the re-derive is not contributing and the shape of
/// the loss belongs to whatever runs after it.
#[test]
fn the_ellipse_loss_is_the_same_with_only_the_rederive_running() {
    let measure = |rederive: bool, auto: bool| -> (f64, f64, usize) {
        let mut s = Scene::new();
        s.face_rederive_on_draw = rederive;
        s.auto_intersect_on_draw = auto;
        s.auto_face_synthesis_on_draw = true;
        s.freeform_overlap_on_draw = true;
        s.mesh
            .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
            .expect("box");
        s.execute(Command::DrawEllipseAsCurve {
            center: DVec3::new(100.0, 0.0, TOP),
            ref_dir: DVec3::X,
            normal: DVec3::Z,
            radius_x: 60.0,
            radius_y: 33.0,
        });
        let (mut inside, mut hanging) = (0.0, 0.0);
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
        let sealed = s
            .mesh
            .faces
            .iter()
            .filter(|(fid, f)| f.is_active() && s.mesh.is_face_in_volume(*fid))
            .count();
        (inside, hanging, sealed)
    };

    let production = measure(true, true);
    let rederive_only = measure(true, false);
    let neither = measure(false, false);

    assert_eq!(
        (production.2, rederive_only.2),
        (7, 7),
        "the re-derive alone gives what production gives — inside {:.1}/{:.1}, \
         hanging {:.1}/{:.1}",
        production.0,
        rederive_only.0,
        production.1,
        rederive_only.1
    );
    assert!(
        (production.0 - rederive_only.0).abs() < 1.0
            && (production.0 - 40_000.0).abs() < 1.0,
        "and both tile the top exactly once: {:.4} vs {:.4}",
        production.0,
        rederive_only.0
    );
    // ⚠ WITH BOTH OFF THE CLASSIFIER READS 4 — with nothing allowed to touch
    // the draw at all. Nothing has happened to the box in that run: its six
    // faces are the ones `create_box` made, and the ellipse is a floating sheet
    // lying over the top. So `is_face_in_volume` is not reporting damage there,
    // it is reporting that a coplanar sheet over a solid's face confuses it —
    // the fourth instrument this area has caught doing that (the other three:
    // `point_in_face`, `face_bounds`, `face_area`, PR #124).
    //
    // That matters beyond this test: `draw_freely_matrix`'s soundness grid uses
    // exactly this count and calls a drop "the solid was opened". Its ellipse
    // rows were partly this artifact — the real defect was that the ellipse
    // never reached the arrangement, and with that fixed the count moves to 5
    // above while the geometry is right.
    assert_eq!(neither.2, 4, "the same 4 with every auto behaviour off");
    assert!(
        (neither.0 - 36_891.08).abs() < 1.0,
        "and the top is carved to the SAME 36,891 with every auto behaviour \
         off — got {:.4}. So neither the re-derive nor the auto-intersect is \
         doing it: the post-draw repair is ungated and runs regardless",
        neither.0
    );
}

/// The post-draw repair is ungated — it carves with every flag off.
///
/// Written to show that a floating ellipse leaves the box alone, and it does
/// not: one of the six faces `create_box` made is gone even with the re-derive
/// and the auto-intersect both switched off. Nothing else in the draw touches
/// the host, so this is `subtract_double_covered_faces` — which no flag
/// controls — taking the overlap out of the top.
#[test]
fn the_post_draw_repair_carves_the_host_with_every_flag_off() {
    let mut s = Scene::new();
    s.face_rederive_on_draw = false;
    s.auto_intersect_on_draw = false;
    s.auto_face_synthesis_on_draw = true;
    s.freeform_overlap_on_draw = true;
    let box_faces: Vec<_> = s
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    assert!(
        s.mesh.face_set_manifold_info(&box_faces).is_closed_solid,
        "the box starts closed"
    );

    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(100.0, 0.0, TOP),
        ref_dir: DVec3::X,
        normal: DVec3::Z,
        radius_x: 60.0,
        radius_y: 33.0,
    });

    let still_there: Vec<_> = box_faces
        .iter()
        .copied()
        .filter(|f| s.mesh.faces.get(*f).map_or(false, |x| x.is_active()))
        .collect();
    assert_eq!(
        still_there.len(),
        5,
        "TODAY the top is replaced even with every flag off — the repair is \
         not gated by them. At 6 the repair has learned to leave a host alone \
         and this pin retires"
    );
    assert!(
        s.mesh.verify_face_invariants().is_valid(),
        "the carve at least leaves the invariants intact"
    );
}

/// The ellipse straddling a solid's edge — the six openings in the grid.
///
/// `draw_freely_matrix`'s soundness grid measures rect and circle crossing or
/// straddling a solid's edge at 6 → 8 faces, sealed; the ELLIPSE at 6 → 7 with
/// the solid opened. Six of twenty-seven, all the ellipse, on all three solid
/// hosts. The re-derive handles this shape correctly on its own
/// (`axia-geo/tests/an_ellipse_hanging_off_a_solid.rs`), so the loss is in the
/// Scene path — which is where D1's was too.
#[test]
fn an_ellipse_straddling_the_edge_leaves_its_piece_and_the_solid_sealed() {
    let mut s = production_solid();
    let sealed_before = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && s.mesh.is_face_in_volume(*fid))
        .count();
    assert_eq!(sealed_before, 6, "the box starts sealed");

    // Centred on the top's edge at x = 100, so half hangs over.
    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(100.0, 0.0, TOP),
        ref_dir: DVec3::X,
        normal: DVec3::Z,
        radius_x: 60.0,
        radius_y: 33.0,
    });

    let (mut inside, mut hanging) = (0.0, 0.0);
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
    let sealed = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| f.is_active() && s.mesh.is_face_in_volume(*fid))
        .count();

    // Before the fix this read 36,891 inside and 6,210 hanging — the top bitten
    // by the ellipse's inner half and the whole ellipse lying across it, the
    // signature of a re-derive that never saw the ellipse at all.
    assert!(
        (inside - 40_000.0).abs() < 1.0,
        "the top tiles exactly once — got {inside:.4}. 36,891 means the ellipse \
         is being carved out of it again instead of cut at the rim"
    );
    assert!(
        hanging > 100.0,
        "and the half past the edge is its own face — got {hanging:.4}"
    );
    // ⚠ FIVE, not six, and that is the classifier rather than the box.
    // `is_face_in_volume` stops counting a solid's face once a coplanar sheet
    // sits on it, whether or not anything was done to the solid — measured on
    // an untouched box in `the_post_draw_repair_...` below. What matters is
    // that the plane tiles once and the invariants hold; whole-scene closure is
    // the wrong question with a sheet hanging off the rim.
    assert_eq!(
        sealed, 7,
        "seven faces bound the volume — the box's five untouched ones plus both          halves of the split top. Five meant the ellipse's pieces were running          beside the rim instead of sharing it"
    );
}

/// The ellipse now splits like the others — but the volume classifier still
/// tells them apart, and that difference is pinned rather than waved away.
///
/// On the soundness grid the ellipse moved from 6 → 7 faces to 6 → 8, the same
/// as rect and circle in the same placements. Its plane tiles to exactly
/// 40,000 and its invariants are clean. But `is_face_in_volume` reads 6 of 6
/// for rect and circle and 5 of 6 for the ellipse, so something about how the
/// ellipse's pieces meet the solid's rim is still not what a polygon's are —
/// most likely duplicated rim edges (a T-junction) where the others share.
///
/// The classifier is unreliable on its own (it drops on an UNTOUCHED box the
/// moment a coplanar sheet lies over it — see the flag comparison above), so
/// this is not evidence of damage. It is evidence of a DIFFERENCE, and rect and
/// circle are the control that makes it worth keeping.
#[test]
fn an_ellipse_splits_like_a_rect_but_its_pieces_meet_the_rim_differently() {
    let count = |shape: &str| -> (usize, usize) {
        let mut s = production_solid();
        match shape {
            "rect" => {
                s.execute(Command::DrawRectAsShape {
                    center: DVec3::new(100.0, 0.0, TOP),
                    normal: DVec3::Z,
                    up: DVec3::X,
                    width: 120.0,
                    height: 66.0,
                });
            }
            "circle" => circle(&mut s, 100.0, 60.0),
            _ => {
                s.execute(Command::DrawEllipseAsCurve {
                    center: DVec3::new(100.0, 0.0, TOP),
                    ref_dir: DVec3::X,
                    normal: DVec3::Z,
                    radius_x: 60.0,
                    radius_y: 33.0,
                });
            }
        }
        let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let sealed = s
            .mesh
            .faces
            .iter()
            .filter(|(fid, f)| f.is_active() && s.mesh.is_face_in_volume(*fid))
            .count();
        (faces, sealed)
    };
    let (rect_faces, rect_sealed) = count("rect");
    let (circle_faces, circle_sealed) = count("circle");
    let (ellipse_faces, ellipse_sealed) = count("ellipse");

    assert_eq!(
        (rect_faces, circle_faces, ellipse_faces),
        (8, 8, 8),
        "all three straddles now make the same number of faces"
    );
    assert_eq!(
        (rect_sealed, circle_sealed),
        (7, 7),
        "a polygon and a circle leave seven faces bounding the volume — the \
         box's five untouched ones plus both halves of the split top"
    );
    assert_eq!(
        ellipse_sealed, 7,
        "and the ellipse now leaves seven too — its top pieces share the rim \
         with the wall instead of running beside it. This read 5 while the \
         ellipse skipped the pre-split"
    );
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

/// WHERE the ellipse's pieces lose the rim — counted, not guessed.
///
/// `is_face_in_volume` asks one thing: does every boundary half-edge of this
/// face have another active face across its edge. So a top piece drops out of
/// the volume when one of its edges has no neighbour. This counts those edges
/// for a rect and for an ellipse in the same straddle, restricted to pieces
/// that lie WITHIN the host's footprint — the hanging piece outside legitimately
/// has free edges in both cases and is not the question.
#[test]
fn an_ellipses_top_pieces_are_missing_a_neighbour_where_a_rects_are_not() {
    let orphan_edges = |shape: &str| -> usize {
        let mut s = production_solid();
        if shape == "rect" {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(100.0, 0.0, TOP),
                normal: DVec3::Z,
                up: DVec3::X,
                width: 120.0,
                height: 66.0,
            });
        } else {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(100.0, 0.0, TOP),
                ref_dir: DVec3::X,
                normal: DVec3::Z,
                radius_x: 60.0,
                radius_y: 33.0,
            });
        }
        let mut orphans = 0usize;
        for (fid, f) in s.mesh.faces.iter() {
            if !f.is_active() || f.normal().z.abs() <= 0.999 {
                continue;
            }
            // On the top plane AND inside the footprint (skip the hanging one).
            let Some((lo, hi)) = s.mesh.face_bounds(fid) else { continue };
            if (lo.z - TOP).abs() > 1e-3 || hi.x > 100.0 + 1e-3 {
                continue;
            }
            let Ok(hes) = s.mesh.collect_loop_hes(f.outer().start) else { continue };
            for he in hes {
                let eid = s.mesh.hes[he].edge();
                let (faces, _) = s.mesh.get_faces_sharing_edge(eid);
                let neighbours = faces
                    .iter()
                    .filter(|&&x| x != fid && s.mesh.faces.get(x).map_or(false, |t| t.is_active()))
                    .count();
                if neighbours == 0 {
                    orphans += 1;
                }
            }
        }
        orphans
    };
    let rect = orphan_edges("rect");
    let ellipse = orphan_edges("ellipse");
    assert_eq!(
        rect, 0,
        "a rect's top pieces have a neighbour across every edge"
    );
    assert_eq!(
        ellipse, 0,
        "and so do an ellipse's, now that it breaks the rim before drawing. \
         This read 2 while `exec_draw_ellipse_as_curve` skipped the pre-split \
         a rect gets from `exec_draw_line` and a circle from \
         `split_edges_at_circle_crossings` — the re-derive never splits a \
         PRESERVED edge (`edges_to_remove` excludes `volume_edges`), so the \
         wall kept the whole rim while the arrangement built fresh edges for \
         the pieces, and two faces ran along one line without sharing it"
    );
}
