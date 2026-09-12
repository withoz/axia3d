//! WHAT STOPS THE FOUR BOUNDARY READERS BECOMING ONE.
//!
//! `coplanar.rs::collect_face_boundary` is the fourth reader of a face boundary
//! and the only one that takes the CHORD; the other three ask for the arc by
//! name — `mesh.rs:14872`, `face_split.rs:166`, `self_intersect.rs:351`, each
//! with `.following_arcs()`. The chord is a 29 mm blind strip along a quarter
//! arc of r=100 (`the_detector_reads_the_rim_not_the_chord.rs`), so unifying
//! them reads like a pure refinement.
//!
//! It is not, and this file is the reason — kept so the next attempt starts
//! here instead of re-deriving four sessions of measurement.
//!
//! ## The change, and what it costs
//!
//! The whole body collapses to one call; `loop_polygon` already handles both
//! the ≥3-vert case and the 1-vertex closed curve:
//!
//! ```text
//!   mesh.loop_polygon(outer_start,
//!       ChordTol::fixed(CURVE_BOUNDARY_CHORD_TOL).following_arcs())
//! ```
//!
//! Measured 2026-09-12 on the pinned 7-operation reduction
//! (`axia-core/tests/pushing_in_three_deep_stacks_faces.rs`), same ops, same
//! face ids, same 36 faces either way:
//!
//! ```text
//!                    seam edge faces   invariant violations
//!   chord (today)           3                  0
//!   following arcs          4                  1
//! ```
//!
//! The seam is `(-63.25, 10, 100) → (63.25, 10, 100)`, where a solid's pushed
//! cap meets the circular segment the arrangement leaves below it. ⚠ The
//! collision is **made by the change, not pre-existing** — that question was
//! open in the records until this run and is now closed.
//!
//! `face_rederive_on_draw` is the mechanism: with it off, both readers give the
//! same 9 faces and the same sound seam. On today's reader the re-derive
//! removes the pushed cap and emits a face across the segment's arcs; with the
//! arc reader it keeps the cap and leaves those arcs with one face each.
//!
//! ## What is NOT the discriminator
//!
//! ⚠ Not the crossing count. It reads like the obvious culprit — the segment is
//! contained in the rect and shares its whole top edge, so "0 crossings,
//! containment" is what you expect, and 2 looks like the arc sampling turning a
//! shared boundary into two false crossings. It is not: **both readers say 2**.
//! That was written down wrong once already; the assertion below is here so it
//! cannot be written down wrong again.
//!
//! What differs is the LENS — the overlap polygon handed to the split.

use axia_geo::curves::AnalyticCurve;
use axia_geo::mesh::ChordTol;
use axia_geo::operations::coplanar as cop;
use axia_geo::{FaceId, MaterialId, Mesh};
use glam::DVec3;

const Z: f64 = 100.0;
const R: f64 = 110.0;
const C: DVec3 = DVec3::new(0.0, 100.0, Z);
/// Where the r=110 circle centred at (0, 100) crosses y = 10. The reduction's
/// seam sits exactly here, which is why the two shapes share a whole edge.
const X: f64 = 63.245553203367585;

/// A circular segment below the chord `y = 10`, and a rectangle on the far side
/// of that chord — the pair the re-derive has to judge, with nothing else in the
/// mesh to confuse it.
fn segment_and_rect() -> (Mesh, FaceId, FaceId) {
    let mut m = Mesh::new();
    let anchor = m.add_vertex(C + DVec3::new(R, 0.0, 0.0));
    let disk = m
        .add_face_closed_curve(
            anchor,
            AnalyticCurve::Circle { center: C, radius: R, normal: DVec3::Z, basis_u: DVec3::X },
            MaterialId::new(0),
        )
        .expect("a disk");

    let pieces = m
        .split_circle_face_by_chord(
            disk,
            &[DVec3::new(-X, 10.0, Z), DVec3::new(X, 10.0, Z)],
            MaterialId::new(0),
        )
        .expect("the chord split runs")
        .expect("it yields pieces");
    assert_eq!(pieces.len(), 2, "the chord should cut the disk in two");
    let segment = *pieces
        .iter()
        .min_by(|a, b| {
            m.face_outer_area(**a)
                .partial_cmp(&m.face_outer_area(**b))
                .expect("finite areas")
        })
        .expect("a smaller piece");

    // `add_vertex` dedups at 0.15μm (LOCKED #5), so naming the same two chord
    // endpoints gives the rect the segment's own vertices — the edge is SHARED,
    // not merely coincident, which is the case that matters.
    let vids: Vec<_> = [
        DVec3::new(-X, 10.0, Z),
        DVec3::new(-X, -40.0, Z),
        DVec3::new(X, -40.0, Z),
        DVec3::new(X, 10.0, Z),
    ]
    .iter()
    .map(|&p| m.add_vertex(p))
    .collect();
    let rect = m.add_face(&vids, MaterialId::new(0)).expect("a rect");
    (m, rect, segment)
}

/// The shape of the thing: three vertices carrying two arcs, so the chord and
/// the arc disagree about the boundary by a whole region rather than a rounding.
#[test]
fn the_segment_is_three_verts_and_thirty_five_points() {
    let (m, _rect, segment) = segment_and_rect();
    let verts = m
        .collect_loop_verts(m.faces[segment].outer().start)
        .expect("the segment's loop");
    assert_eq!(verts.len(), 3, "the segment stopped being a 3-vert loop");

    let followed = m
        .loop_polygon(
            m.faces[segment].outer().start,
            ChordTol::fixed(0.02).following_arcs(),
        )
        .expect("the arc reading");
    assert!(
        followed.len() > 30,
        "following the arcs gave only {} points — the two readings have stopped \
         disagreeing, and the change this file guards may now be free",
        followed.len()
    );
}

/// ⚠ The assertion that stops the wrong diagnosis being written down a third
/// time: the crossing count is NOT what separates the two readers.
#[test]
fn both_readers_call_it_a_two_crossing_overlap() {
    let (m, rect, segment) = segment_and_rect();

    let ci = cop::coplanar_intersection_segments(&m, rect, segment)
        .expect("the pair is coplanar and readable");
    assert_eq!(
        ci.crossings.len(),
        2,
        "today's reader stopped reporting 2 crossings for a segment that shares \
         its whole chord with a rect. If this is now 0 (containment), the gate \
         in `auto_intersect_coplanar` no longer fires here and the note above \
         about crossings not being the discriminator needs re-measuring."
    );

    // And it is symmetric — neither order is the special one.
    let back = cop::coplanar_intersection_segments(&m, segment, rect).expect("the other order");
    assert_eq!(back.crossings.len(), 2, "the verdict depends on argument order");
}

/// The discriminator, pinned: the LENS. Today's chord reader hands the split a
/// 3-point triangle; following the arcs hands it 35 points that bulge past the
/// triangle by the sagitta.
///
/// ⚠ When the unification lands, THIS is the assertion that fires — not the
/// crossing one. The fix belongs wherever the lens is consumed, and the failure
/// message should be read as "the split now sees a different region", not as
/// "the detector broke".
#[test]
fn the_lens_is_the_chord_triangle_not_the_arc_region() {
    let (m, rect, segment) = segment_and_rect();
    let ci = cop::coplanar_intersection_segments(&m, rect, segment).expect("coplanar");

    assert_eq!(
        ci.lens_polygon.len(),
        3,
        "the overlap polygon is {} points, not the 3-point chord triangle. If \
         `collect_face_boundary` now follows arcs this is expected to be ~35 — \
         see this file's header for the 7-op reduction it breaks and for what \
         was already ruled out.",
        ci.lens_polygon.len()
    );

    // The triangle really is the chord one: its area is the straight-sided
    // 1719.8, below the true segment, which is what makes the difference a
    // region rather than a rounding.
    let area = m.face_outer_area(segment);
    assert!(
        (area - 1719.8).abs() < 1.0,
        "the segment's chord area moved: {area}"
    );
}
