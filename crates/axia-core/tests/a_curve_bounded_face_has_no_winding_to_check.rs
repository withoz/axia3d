//! `cached normal opposite to winding` on a curve-bounded face is the check
//! being wrong, not the face.
//!
//! Fuzz sessions 40 and 73 report it and are still open. This file is what the
//! hunt for them established, pinned so the next one does not chase the same
//! thing as a geometry defect — it is not one.
//!
//! ## What the face actually is
//!
//! Draw a circle, draw an ellipse across it, draw a line: one of the three
//! regions reports `dot=-1.000`. Measured with the detector's own triangles
//! (`Mesh::face_tessellation`, which tessellates the curves):
//!
//! ```text
//!   FaceId(4)   tris=218   area=33,948   bbox x[-110,110] y[-110,110]
//! ```
//!
//! The circle is Ø220 and its area is 38,013. So that face is the circle minus
//! the lens, at full extent, and it is **correct**. Total across the three
//! regions comes to 38,093 against 37,652 with the re-derive off — 1.012.
//!
//! ## Why the check fires anyway
//!
//! I2 compares the cached normal against `compute_normal(loop_verts)`. On a
//! curve-bounded face those verts are a CHORD polygon — six points for a region
//! of a circle — and here it **crosses itself**. Newell then sums lobes of
//! opposite sign, and which one wins is an accident of where the chords fall:
//!
//! ```text
//!   circle + ellipse          6 verts   signed area   -42.6   1 self-crossing
//!   + one line across         7 verts   signed area +4,764    2 self-crossings
//! ```
//!
//! −42.6 is not a shape; it is two lobes nearly cancelling. Adding one vertex
//! moved the balance and the sign went positive, so the cached normal — which
//! had agreed — now disagreed, and I2 fired 31 operations after anything
//! happened.
//!
//! ⚠ That makes I2 another instrument that lies about a curved face, alongside
//! `point_in_face`, `face_bounds`, `face_outer_area` and `Face::normal()`. It
//! reads a chord polygon and reports on the face.
//!
//! ## What the hunt cost, so it is not repeated
//!
//! Three claims I made from the wrong instruments, each corrected by the next
//! measurement:
//!
//! - *"the circle is destroyed — 9% of its area survives"*. From
//!   `face_outline_points` (which returns the DCEL loop, not the curve) and
//!   `face_outer_area`. `face_tessellation` says 1.012.
//! - *"a self-intersecting face"*. The chord polygon self-intersects. The face
//!   does not.
//! - *"the bounding box shrinks to 190×138"*. Same outline points. The
//!   tessellation's bbox is the full ±110.
//!
//! ## Fixed (2026-08-28) — the cause, which was the chord net
//!
//! One thing had survived every instrument: on circle × ellipse the re-derive
//! handed a new face a Plane surface whose normal was **exactly opposite** the
//! face's cached normal. Flipping the face to satisfy ADR-007 Invariant 2 broke
//! four pinned reproductions in `pushing_in_three_deep_stacks_faces.rs` — measured,
//! then reverted — and setting the cached normal from the surface would only have
//! traded this complaint for an I2 one. Neither was the cause.
//!
//! The cause was upstream, in how a boundary curve goes back into the DCEL. The
//! materialiser cut every curve into exactly two chords whatever it spanned, so a
//! 355 degree arc became two chords lying almost on top of each other, and the
//! near-whole ellipse did the same. Two needles, crossing. Cut by span instead
//! (`chord_cuts` in `face_rederive.rs`, a quarter turn per chord) and the loop is
//! an ordinary inscribed polygon: no self-crossing, a signed area with a sign,
//! and the orientation is answerable again.
//!
//! Fuzz sessions 40 and 73 stopped reporting `cached normal opposite to winding`
//! in the same change — 40 had failed at op 41, 73 at op 38, and 89 at op 46. All
//! three were the `dot = -1.000` form, and all three are gone.
//!
//! ## What the fuzz aggregate does and does not say
//!
//! ⚠ Sessions are a random walk: the operation list per seed is fixed, but the
//! mesh diverges from the first curve this change touches, so session N before and
//! session N after are different experiments. Comparing them one by one misleads.
//! Two 100-session samples, each run on both sides:
//!
//! ```text
//!                       stopped   NaN   non-manifold   winding
//!    0- 99   before          49     5             36         4
//!    0- 99   after           50    10             34         4
//!   100-199  before          58     9             45         3
//!   100-199  after           58     9             39         5
//! ```
//!
//! The totals are flat and non-manifold falls in both samples. The NaN doubling in
//! the first sample is **not reproduced** in the second, the five NaN sessions the
//! first sample already had come back byte-identical (2@49, 3@30, 37@40, 41@29,
//! 68@30), and the two baselines disagree with each other 5 against 9 — so that
//! column is noise at this sample size, not a cost of the fix. Recorded because it
//! looked like a regression for an hour and the next reader deserves the numbers
//! rather than the hour.
//!
//! `no_region_of_a_circle_has_a_chord_polygon_that_crosses_itself` below is the
//! rewritten form of the test that used to assert a crossing WAS there. It is now
//! the guard on the fix, and it fires if the cutting rule goes back.

use axia_core::scene::Scene;
use axia_core::Command;
use axia_geo::AnalyticSurface;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// Circle Ø220 at the origin, an ellipse across its lower left, and (optionally)
/// a line through the lot. Session 73's thirty-seven operations reduced to these.
fn circle_and_ellipse(with_line: bool) -> Scene {
    let mut s = prod();
    s.execute(Command::DrawCircleAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        radius: 110.0,
        segments: 24,
    });
    s.execute(Command::DrawEllipseAsCurve {
        center: DVec3::new(-50.0, -50.0, 0.0),
        ref_dir: DVec3::X,
        normal: DVec3::Z,
        radius_x: 50.0,
        radius_y: 30.0,
    });
    if with_line {
        s.execute(Command::DrawLine {
            start: DVec3::new(-150.0, -50.0, 0.0),
            end: DVec3::new(150.0, -50.0, 0.0),
            surface_normal: Some(DVec3::Z),
        });
    }
    s
}

/// Area and extent from the detector's own triangles, so curves count.
fn tessellated(s: &Scene) -> (f64, DVec3, DVec3) {
    let mut area = 0.0;
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        for t in s.mesh.face_tessellation(f).unwrap_or_default() {
            area += (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5;
            for p in &t {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
    }
    (area, lo, hi)
}

/// Does a face's own outer loop, projected onto its plane, cross itself?
fn chord_crossings(s: &Scene, f: axia_geo::FaceId) -> usize {
    let pts = s.mesh.face_outline_points(f).unwrap_or_default();
    let n = pts.len();
    if n < 4 {
        return 0;
    }
    let side = |a: DVec3, b: DVec3, c: DVec3| (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    let mut hits = 0;
    for i in 0..n {
        for j in (i + 2)..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (a1, a2) = (pts[i], pts[(i + 1) % n]);
            let (b1, b2) = (pts[j], pts[(j + 1) % n]);
            let (d1, d2) = (side(a1, a2, b1), side(a1, a2, b2));
            let (d3, d4) = (side(b1, b2, a1), side(b1, b2, a2));
            if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) {
                hits += 1;
            }
        }
    }
    hits
}

/// Nothing is lost. This is the assertion that says the winding report is not
/// about a missing or shrunken region.
///
/// The circle is Ø220, so π·110² = 38,013, and the ellipse pokes 80 mm² outside
/// it. Without the line the three regions come to 38,093 — the union, to within
/// a tessellation's rounding.
#[test]
fn the_regions_still_cover_the_circle() {
    let (area, lo, hi) = tessellated(&circle_and_ellipse(false));

    assert!(
        (37_500.0..38_600.0).contains(&area),
        "the three regions cover {area:.0} mm², and the union is about 38,093"
    );
    assert!(
        lo.x < -108.0 && hi.x > 108.0 && lo.y < -108.0 && hi.y > 108.0,
        "the circle's extent is gone: x[{:.1},{:.1}] y[{:.1},{:.1}]",
        lo.x,
        hi.x,
        lo.y,
        hi.y
    );
}

/// NOT the same thing as the winding report — a second finding from the same
/// reduction, kept apart so neither is read as the other.
///
/// Adding the line used to take the total from 38,093 to **41,058**: about
/// 2,965 mm² of ground covered twice. The extent was still right and no region
/// was missing, so it was double coverage rather than loss. The chord-net fix
/// closed it; `one_line_makes_the_retile_cover_ground_twice.rs` measures the same
/// thing per-face and carries the arithmetic.
#[test]
fn one_line_across_makes_the_regions_overlap() {
    let (with_line, _, _) = tessellated(&circle_and_ellipse(true));
    let (without, _, _) = tessellated(&circle_and_ellipse(false));
    assert!(
        with_line <= without + 200.0,
        "{:.0} mm² is covered twice ({:.0} with the line, {:.0} without)",
        with_line - without,
        with_line,
        without
    );
}

/// This is the thing I2 was actually reading, and the guard on the fix for it.
///
/// ⚠ Do not weaken this to "few crossings". A chord net either is a polygon or it
/// is not; one crossing is enough to make the signed area meaningless, which is
/// how the winding report came about in the first place.
#[test]
fn no_region_of_a_circle_has_a_chord_polygon_that_crosses_itself() {
    let s = circle_and_ellipse(true);
    let crossing: Vec<String> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, fa)| fa.is_active())
        .filter(|(f, _)| chord_crossings(&s, *f) > 0)
        .map(|(f, _)| {
            let n = s.mesh.face_outline_points(f).unwrap_or_default().len();
            format!("{f:?}({n} verts, {} crossings)", chord_crossings(&s, f))
        })
        .collect();
    assert!(
        crossing.is_empty(),
        "a chord polygon crosses itself again: {crossing:?} — a boundary curve is \
         being cut into too few chords somewhere, and every instrument that reads \
         the loop instead of the curves is now reporting on a needle"
    );

    // And the extent is still the circle's. This was the whole point while the
    // crossing stood — a degenerate chord net never meant a lost region — and it
    // is worth keeping now, because a cutting rule that produced a clean polygon
    // by dropping part of the boundary would satisfy the assertion above.
    let (_, lo, hi) = tessellated(&s);
    assert!(
        lo.x < -108.0 && hi.x > 108.0,
        "the chord net is clean but the region shrank (x[{:.1},{:.1}]) — the \
         crossing was removed by losing boundary, not by cutting it properly",
        lo.x,
        hi.x
    );
}

/// The control: a circle over a circle, and a 24-gon over an ellipse, both go
/// through the same arrangement and neither produces a crossing chord net. So
/// the trigger is the circle-curve/ellipse-curve pair, not curves in general.
#[test]
fn the_pairs_that_do_not_do_it() {
    let mut cases: Vec<(&str, Scene)> = Vec::new();

    let mut s = prod();
    s.execute(Command::DrawCircleAsShape { center: DVec3::ZERO, normal: DVec3::Z, radius: 110.0, segments: 24 });
    s.execute(Command::DrawCircleAsShape { center: DVec3::new(120.0, 0.0, 0.0), normal: DVec3::Z, radius: 100.0, segments: 24 });
    cases.push(("circle × circle", s));

    let mut s = prod();
    s.execute(Command::DrawPolygonAsShape { center: DVec3::ZERO, normal: DVec3::Z, radius: 110.0, sides: 24 });
    s.execute(Command::DrawEllipseAsCurve { center: DVec3::new(-50.0, -50.0, 0.0), ref_dir: DVec3::X, normal: DVec3::Z, radius_x: 50.0, radius_y: 30.0 });
    cases.push(("24-gon × ellipse", s));

    let mut s = prod();
    s.execute(Command::DrawEllipseAsCurve { center: DVec3::ZERO, ref_dir: DVec3::X, normal: DVec3::Z, radius_x: 50.0, radius_y: 30.0 });
    s.execute(Command::DrawEllipseAsCurve { center: DVec3::new(40.0, 10.0, 0.0), ref_dir: DVec3::X, normal: DVec3::Z, radius_x: 50.0, radius_y: 30.0 });
    cases.push(("ellipse × ellipse", s));

    for (name, s) in cases {
        let bad: Vec<String> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, fa)| fa.is_active())
            .filter(|(f, _)| chord_crossings(&s, *f) > 0)
            .map(|(f, _)| format!("{f:?}"))
            .collect();
        assert!(bad.is_empty(), "{name} produced a crossing chord net: {bad:?}");
    }
}

/// ADR-007 Invariant 2 (LOCKED #2): a face and its own Plane point the same way.
///
/// On circle × ellipse a new face used to be handed a Plane pointing the opposite
/// way. Flipping the face to satisfy the invariant broke four pinned reproductions
/// in `pushing_in_three_deep_stacks_faces.rs` — measured, then reverted — because
/// the flip treats the symptom. The cached normal came from a chord net that
/// crossed itself, so it had no orientation to report; cutting the boundary curves
/// by span gave it one, and the disagreement went away without touching the face.
#[test]
fn a_new_face_is_handed_a_plane_it_points_away_from() {
    let s = circle_and_ellipse(false);
    let mut bad = Vec::new();
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        let Some(AnalyticSurface::Plane { normal, .. }) = s.mesh.face_surface(f).cloned() else {
            continue;
        };
        let (c, h) = (fa.normal().normalize_or_zero(), normal.normalize_or_zero());
        if c.length_squared() < 0.5 || h.length_squared() < 0.5 {
            continue;
        }
        if c.dot(h) < 0.0 {
            bad.push(format!("{f:?} dot={:.3}", c.dot(h)));
        }
    }
    assert!(bad.is_empty(), "faces opposite to its own Plane: {bad:?}");
}
