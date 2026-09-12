//! THE FOUR BOUNDARY READERS ARE ONE, AND WHAT IT TOOK.
//!
//! `coplanar.rs::collect_face_boundary` was the fourth reader of a face
//! boundary and the only one that took the CHORD; the other three ask for the
//! arc by name (`mesh.rs:14872`, `face_split.rs:166`, `self_intersect.rs:351`).
//! The chord is a 29 mm blind strip along a quarter arc of r=100
//! (`the_detector_reads_the_rim_not_the_chord.rs`), so the change reads like a
//! pure refinement and is four lines.
//!
//! It took two fixes first, and this file is the record of why — kept so nobody
//! re-derives four sessions of measurement, and so the two WRONG diagnoses made
//! on the way are assertions instead of prose.
//!
//! ## What was actually in the way
//!
//! Two places where the polygon this module READ was handed to code that had
//! rebuilt that boundary a different way. Neither showed while the chord
//! reading and the vertex loop were the same polygon.
//!
//! 1. **The crossings carried indices into a polygon nobody else had.** A
//!    `Crossing`'s `face_a_edge` indexes the read polygon; every consumer
//!    applies it to one rebuilt from `collect_loop_verts`.
//!
//!    ```text
//!      chord   largest crossing edge index   2   the loop has 3 edges
//!      arcs    largest crossing edge index  34   the loop has 3 edges
//!    ```
//!
//! 2. **The split rebuilt its faces from the reading.** Whatever leaves
//!    `polygon_difference_walking` goes straight into `add_face`, so every
//!    point becomes a mesh vertex. On two 32-gon circles whose every edge
//!    carries an `Arc`:
//!
//!    ```text
//!               sub-face verts    Arc metadata
//!      loop      20 / 34 / 34     kept            sound
//!      sampled  152 / 324 / 324   all gone        148 non-manifold edges
//!    ```
//!
//! Both are fixed (`remap_to_loop_edge`; Step 3 walking the loop), and with
//! them the four-line change costs nothing:
//!
//! ```text
//!                              before the two fixes   after
//!   the 7-op reduction              5 failing         all pass
//!   two overlapping circles      148 non-manifold     sound
//!   whole workspace                 5 failing         0
//! ```
//!
//! ## ⚠ Two wrong diagnoses, kept as assertions
//!
//! Both read like the obvious culprit and both were written down before being
//! measured.
//!
//! - **Not the crossing count.** The segment is contained in the rect and
//!   shares its whole chord, so "0 crossings, containment" is what you expect,
//!   and 2 looks like arc sampling faking two crossings. Both readings say 2.
//! - **Not the lens.** It is where the difference first becomes visible — 3
//!   points against 35 — and it is a symptom of (1), not a cause.
//!
//! The assertions below hold both, so a third reader cannot reach for either.

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

/// The lens follows the rim now. It was a 3-point chord triangle while the
/// reading was the chord; it is the sampled arc region since the readers became
/// one — the 29 mm of ground the detector used to be blind to.
///
/// ⚠ This was written down as the CAUSE once. It is a symptom of the indices —
/// see `the_crossing_indices_are_only_valid_against_the_detectors_own_polygon`.
#[test]
fn the_lens_follows_the_rim_now() {
    let (m, rect, segment) = segment_and_rect();
    let ci = cop::coplanar_intersection_segments(&m, rect, segment).expect("coplanar");

    assert!(
        ci.lens_polygon.len() > 30,
        "the overlap polygon is {} points. It was 3 — the chord triangle — \
         until the readers became one, so a small number again means \
         `collect_face_boundary_owned` has stopped following arcs.",
        ci.lens_polygon.len()
    );

    // The FACE is unchanged — still a 3-vertex loop carrying two arcs. Only the
    // reading moved, which is what makes the difference a region rather than a
    // rounding.
    //
    // ⚠ 1719.8 is the segment's TRUE area, not its chord area. Measured
    // 2026-09-12, because two sessions in a row wrote down the opposite:
    //
    // ```text
    //   shoelace of the 3-vertex loop      1264.91   <- the chord
    //   face_outer_area                    1719.81   <- shoelace + arc bulge
    //   R²·acos(d/R) − d·√(R²−d²)          1719.81   <- closed form, R=110 d=90
    // ```
    //
    // `face_outer_area` adds `loop_curve_bulge` and is EXACT here, so it is not
    // lying about this face — the 1264.91 is. Both numbers are pinned so they
    // cannot be conflated again.
    let area = m.face_outer_area(segment);
    assert!(
        (area - 1719.81).abs() < 0.01,
        "the segment's true area moved: {area} — this is shoelace + bulge and \
         matches the closed form, so a drift here is the face changing, not the \
         reading"
    );

    let verts = m
        .collect_loop_verts(m.faces[segment].outer().start)
        .expect("the segment's loop");
    let pts: Vec<glam::DVec3> = verts
        .iter()
        .map(|&v| m.vertex_pos(v).expect("a position"))
        .collect();
    assert_eq!(pts.len(), 3, "the segment is still a 3-vertex loop");
    let mut area_vec = glam::DVec3::ZERO;
    for i in 1..pts.len() - 1 {
        area_vec += (pts[i] - pts[0]).cross(pts[i + 1] - pts[0]);
    }
    let chord = area_vec.length() * 0.5;
    assert!(
        (chord - 1264.91).abs() < 0.01,
        "the chord polygon measures {chord}, not 1264.91 — THIS is the number \
         that under-reports the segment by 455 mm², and it is what a reader \
         taking the chord would have handed the split"
    );
}

/// THE CAUSE, pinned separately from its symptom.
///
/// A `Crossing` carries `face_a_edge` / `face_b_edge`: indices into the polygon
/// `collect_face_boundary` built. `Scene::subtract_double_covered_faces` then
/// walks polygons built from `collect_loop_verts` — the raw loop vertices — and
/// hands those indices to `polygon_difference_by_clip`. Nothing states that the
/// two have to be the same polygon, and with today's reader they happen to be.
///
/// Measured on this pair:
///
/// ```text
///   chord   face_b_edge max  2   the clip loop has 3 edges   in range
///   arcs    face_b_edge max 34   the clip loop has 3 edges   OUT OF RANGE
/// ```
///
/// ⚠ Unifying the readers WAS blocked on this, not on the lens. The fix was for
/// the detector to translate its indices back onto the loop every consumer
/// walks (`remap_to_loop_edge`) — the same "one source" rule the unification
/// is about, one level up.
#[test]
fn the_crossing_indices_are_only_valid_against_the_detectors_own_polygon() {
    let (m, rect, segment) = segment_and_rect();
    let ci = cop::coplanar_intersection_segments(&m, rect, segment).expect("coplanar");

    // What a consumer rebuilds by hand, the way the repair does.
    let clip_loop = m
        .collect_loop_verts(m.faces[segment].outer().start)
        .expect("the segment's loop");

    let max_clip_edge = ci
        .crossings
        .iter()
        .map(|c| c.face_b_edge)
        .max()
        .expect("two crossings");

    assert!(
        max_clip_edge < clip_loop.len(),
        "a crossing indexes clip edge {max_clip_edge} but the loop a consumer \
         walks has only {} edges. This was the unification's real blocker and is \
         held by `remap_to_loop_edge`; if it fires, that translation stopped \
         happening and every consumer that rebuilds the polygon by hand is \
         reading garbage.",
        clip_loop.len()
    );
}
