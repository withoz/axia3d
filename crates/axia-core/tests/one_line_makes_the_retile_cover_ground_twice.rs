//! Draw a line across a circle and an ellipse, and 1,435 mm² ends up owned by
//! three faces at once.
//!
//! Found while establishing that sessions 40 and 73's `cached normal opposite to
//! winding` is not a geometry defect (see
//! `a_curve_bounded_face_has_no_winding_to_check.rs`). This one IS.
//!
//! ## Measured
//!
//! Everything here is sampled from `Mesh::face_tessellation` — the detector's own
//! triangles — on a 0.5 mm grid. ⚠ Outline polygons under-read a curve-bounded
//! face and are not used anywhere in this file.
//!
//! ```text
//!   circle + ellipse        3 faces   38,093 mm²   covered twice:     2
//!   + one line across       6 faces   41,058 mm²   covered twice:   187
//!                                                  covered 3×:    1,435
//! ```
//!
//! 1,435 × 2 + 187 comes to 3,057, which is the 2,965 mm² the totals differ by,
//! to within a 0.5 mm grid.
//!
//! ## Which faces
//!
//! ```text
//!   FaceId(8)    35,349   exclusive 33,926
//!   FaceId(10)    3,521   exclusive  1,939     shares 1,426 with 8, 1,504 with 4
//!   FaceId(4)     1,510   exclusive      4     shares 1,427 with 8, 1,504 with 10
//! ```
//!
//! `FaceId(4)` owns four square millimetres nobody else does. It is a duplicate
//! tile, not a region.
//!
//! ## Where it comes from
//!
//! `face_rederive`, and the flag matrix says so — every other auto behaviour
//! leaves it unchanged:
//!
//! ```text
//!   everything on          6 faces   3,019 mm² over
//!   auto_intersect off     6 faces   3,019
//!   auto_face_synth off    6 faces   3,019
//!   face_rederive off      6 faces     100     <-- the one that matters
//!   freeform_overlap off   4 faces   4,616     <-- worse, so that path corrects some
//! ```
//!
//! ⚠ Two readings I had to withdraw on the way, recorded so they are not tried
//! again:
//!
//! - *"a face survived the re-tile and duplicates the new ones"*. The face
//!   storage **recycles ids**: `FaceId(4)` before the line and `FaceId(4)` after
//!   are different faces, and the two others are gone entirely (six slots, six
//!   active). All three originals are removed and all six are new, so the
//!   re-tile emits a set that is not a partition — there is no leftover.
//! - *"the closed-freeform preserve branch does it"* — the `verts.len() == 1`
//!   arm in the removal loop that keeps a Bezier/BSpline/NURBS self-loop face
//!   because the arrangement cannot regenerate one. Mutation-checked: forcing it
//!   to always remove changes nothing at all.
//!
//! ## Fixed (2026-08-28) — the boundary curves were cut into too few chords
//!
//! Not the region extraction after all. The materialiser cut every boundary curve
//! into exactly two chords (three for a full circle) whatever it spanned, so the
//! circle's 355 degree arc became two chords lying almost on top of each other —
//! out to the far side and back. A needle, not a region.
//!
//! `split_face_by_line` works on `loop_verts`, the chord polygon, and knows
//! nothing about the arcs attached to it. Splitting a needle gave two halves that
//! each carried arcs bulging out to the true circle, so both claimed the same
//! ground. Cutting by span instead (`chord_cuts`, a quarter turn per chord) makes
//! the loop a real inscribed polygon and the split lands where it should:
//!
//! ```text
//!                        before        after
//!   covered twice        3,019 mm²     176 mm²
//!   largest face        33,949 -> 35,349 (grew)   33,949 -> 27,751 (shrank)
//! ```
//!
//! 27,751 is what the arithmetic asks for: the circle minus the ellipse is 33,948,
//! the part below y = -50 is a circular segment of 8,394 less the ellipse's lower
//! half of 2,356, so the upper piece should be about 27,910. Within 0.6%.
//!
//! ## Traced to one call (2026-08-28)
//!
//! Trapping `Face::new` — the only catch-all, since `split_face` does NOT go
//! through `add_face_with_holes` — gives the whole scene in one run, and the
//! creation order came out 1:1 with FaceId:
//!
//! ```text
//!   0 1 2   add_face_closed_curve   circle / re-derive / ellipse    removed
//!   3   5   add_face_with_holes  <- rebuild_inner                   removed
//!   4       add_face_with_holes  <- rebuild_inner   33,948 before   1,510
//!   --- the line ---
//!   6 7     split_face_by_chain  <- split_arc_face_by_line          2 / 74
//!   8       split_face           <- split_face_by_line              35,349
//!   9 10    dissolve_and_fan_split                                  602 / 3,521
//! ```
//!
//! ⚠ The arithmetic names it: the line splits FaceId(4) — 33,948 mm² — into
//! 1,510 + 35,349 = **36,859**. The two halves are 2,911 mm² bigger than the
//! whole, which is the difference the totals show. Splitting a curve-bounded
//! face leaves both halves claiming the same bulge.
//!
//! ⚠ A reading withdrawn and then restored: "a face survived the re-tile" is
//! RIGHT. FaceId(4) is created before the line and mutated in place by
//! `split_face_by_line` (loop 6 verts → 4). I replaced it with "ids recycle"
//! after seeing ids 3 and 5 vanish and the slot count drop — but removal plus
//! in-place mutation looks identical from outside. Check with a creation trap,
//! not with id-set arithmetic.
//!
//! The three tests below were `should_panic` while it stood; they are ordinary
//! assertions now, and they are what holds the fix in place.

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

fn prod(face_rederive: bool) -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = face_rederive;
    s.freeform_overlap_on_draw = true;
    s
}

/// A Ø220 circle, an ellipse across its lower left, and optionally a line at
/// y = −50 through both.
fn scene(with_line: bool, face_rederive: bool) -> Scene {
    let mut s = prod(face_rederive);
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

fn tri_area(t: &[DVec3; 3]) -> f64 {
    (t[1] - t[0]).cross(t[2] - t[0]).length() * 0.5
}

fn faces_tris(s: &Scene) -> Vec<(axia_geo::FaceId, Vec<[DVec3; 3]>)> {
    s.mesh
        .faces
        .iter()
        .filter(|(_, fa)| fa.is_active())
        .map(|(f, _)| (f, s.mesh.face_tessellation(f).unwrap_or_default()))
        .filter(|(_, t)| !t.is_empty())
        .collect()
}

fn in_tri(p: (f64, f64), t: &[DVec3; 3]) -> bool {
    let d = |a: DVec3, b: DVec3| (b.x - a.x) * (p.1 - a.y) - (b.y - a.y) * (p.0 - a.x);
    let (d1, d2, d3) = (d(t[0], t[1]), d(t[1], t[2]), d(t[2], t[0]));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// `(covered once, mm² counted more than once)`. The second number is what a
/// partition has none of.
///
/// A 1 mm grid keeps this under a second in release; the numbers in the header
/// were taken at 0.5 mm and agree to about 1%.
fn coverage(s: &Scene) -> (f64, f64) {
    let ft = faces_tris(s);
    let step = 1.0;
    let (mut once, mut extra) = (0usize, 0usize);
    let mut y = -112.0;
    while y <= 112.0 {
        let mut x = -112.0;
        while x <= 112.0 {
            let k = ft
                .iter()
                .filter(|(_, tris)| tris.iter().any(|t| in_tri((x, y), t)))
                .count();
            if k == 1 {
                once += 1;
            } else if k > 1 {
                extra += k - 1;
            }
            x += step;
        }
        y += step;
    }
    let cell = step * step;
    (once as f64 * cell, extra as f64 * cell)
}

/// The control. Without the line the same three shapes tile the plane cleanly, so
/// a failure below is about the line and not about circles and ellipses meeting.
#[test]
fn the_circle_and_the_ellipse_alone_tile_cleanly() {
    let (_, extra) = coverage(&scene(false, true));
    assert!(
        extra < 30.0,
        "circle + ellipse already covers {extra:.0} mm² more than once"
    );
}

/// The second control. With the re-derive off, the line splits them cleanly too —
/// which is what makes the re-derive the thing at fault rather than the line.
#[test]
fn without_the_re_derive_the_line_splits_them_cleanly() {
    let (_, extra) = coverage(&scene(true, false));
    assert!(
        extra < 300.0,
        "the line covers {extra:.0} mm² twice even with the re-derive off — then \
         this is not the re-derive's doing and the file's premise is wrong"
    );
}

/// With the re-derive on, the line's re-tile used to cover ~3,000 mm² twice. It
/// is 176 mm² now — the sliver where a chord still cuts a corner off its arc.
#[test]
fn one_line_across_makes_the_retile_overlap() {
    let (_, extra) = coverage(&scene(true, true));
    assert!(
        extra < 300.0,
        "{extra:.0} mm² is covered more than once after the line"
    );
}

/// The largest face, so a split can be caught growing one.
fn largest(s: &Scene) -> f64 {
    faces_tris(s)
        .iter()
        .map(|(_, t)| t.iter().map(tri_area).sum::<f64>())
        .fold(0.0f64, f64::max)
}

/// The sharpest statement of it: **a split cannot grow a face.**
///
/// Cutting a region in two gives two pieces of it. Neither can be larger than
/// what it came from, whatever the boundary is made of. Measured:
///
/// ```text
///   before the line   largest face   33,949
///   after the line    largest face   35,349      +1,401   <- was
///   after the line    largest face   27,751      -6,198   <- is
/// ```
///
/// Asserting on the largest face rather than on totals is what makes this
/// readable: the number had to come back down under the original, and it did.
#[test]
fn a_split_does_not_grow_a_face() {
    let before = largest(&scene(false, true));
    let after = largest(&scene(true, true));
    assert!(
        after <= before + 1.0,
        "after the line a face is {after:.0} mm², larger than anything it was cut \
         from (the largest before was {before:.0})"
    );
}

/// The sharper half: no tile may own almost no ground.
///
/// A region of a partition is mostly its own. The duplicate this caught was
/// 1,510 mm² of which 4 were not somebody else's, which is what a duplicate looks
/// like as a number.
#[test]
fn a_tile_is_produced_that_owns_almost_nothing() {
    let s = scene(true, true);
    let ft = faces_tris(&s);
    let step = 1.0;
    let n = ft.len();
    let mut excl = vec![0usize; n];
    let mut tot = vec![0usize; n];
    let mut y = -112.0;
    while y <= 112.0 {
        let mut x = -112.0;
        while x <= 112.0 {
            let hit: Vec<usize> = (0..n)
                .filter(|&k| ft[k].1.iter().any(|t| in_tri((x, y), t)))
                .collect();
            for &k in &hit {
                tot[k] += 1;
                if hit.len() == 1 {
                    excl[k] += 1;
                }
            }
            x += step;
        }
        y += step;
    }
    let cell = step * step;
    let worst: Vec<String> = (0..n)
        .filter(|&k| tot[k] as f64 * cell > 100.0 && (excl[k] as f64) < tot[k] as f64 * 0.1)
        .map(|k| {
            format!(
                "{:?} is {:.0} mm² with {:.0} of its own",
                ft[k].0,
                tot[k] as f64 * cell,
                excl[k] as f64 * cell
            )
        })
        .collect();
    assert!(worst.is_empty(), "{}", worst.join("; "));
}
