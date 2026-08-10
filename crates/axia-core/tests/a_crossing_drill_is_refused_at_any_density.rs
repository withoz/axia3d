//! A CROSS-DRILL GUARD MUST NOT DEPEND ON HOW FINELY THE FIRST HOLE WAS CUT.
//!
//! Drilling a second through-hole whose axis passes through an existing one is
//! refused, because the straight-tube builder cannot bridge across a void. The
//! guard found the "nearest opposite face" with ONE ray from the profile centre
//! and asked whether that face could contain the projected profile — a tube wall
//! cannot, an outer shell can.
//!
//! That worked only while the ray happened to land inside a wall quad. Measured
//! 2026-08-10, a 200³ box bored along Z then drilled across on X:
//!
//! ```text
//!   segments  12  24  48  64  96   refused, mesh untouched      ✅
//!   segments  16                   ALLOWED  → 38 faces, si  88  ❌
//!   segments  32                   ALLOWED  → 70 faces, si 184  ❌
//! ```
//!
//! Identical geometry every time; only the vertex positions moved. At 16 and 32
//! the axis met the wall exactly at a quad vertex, the even-odd test said
//! "outside", the wall was skipped, and a shell 200 mm behind it became the
//! nearest opposite face — which of course contains the profile. Two tube walls
//! were then left running through each other, and `valid`/`closed` both still
//! reported fine: only the self-intersection check saw it.
//!
//! Two changes, both needed:
//!   · five rays (centre + the four rim points) instead of one, and
//!   · a boundary point counts as a hit (`point_in_face_or_on_edge`).
//!
//! ⚠ Counting crossings instead was tried and REJECTED — a legitimate drill
//! through two stacked solids meets four shells, which broke
//! `adr251_p6_multisolid_sequential_drill_works`. The original containment test
//! is right; only FINDING the wall was broken.
use axia_core::{Scene, FORM_MATERIAL};
use glam::DVec3;

fn box_bored_along_z(segments: u32) -> Scene {
    let mut s = Scene::new();
    s.mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, FORM_MATERIAL).unwrap();
    s.mesh
        .drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, 40.0, segments)
        .expect("the first bore is a clean through-drill");
    s
}

fn state(s: &Scene) -> (usize, usize, bool) {
    let a: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let si = s.mesh.detect_self_intersections().intersecting_pairs.len();
    let closed = s.mesh.face_set_manifold_info(&a).is_closed_solid;
    (a.len(), si, closed)
}

#[test]
fn a_crossing_bore_is_refused_at_every_density() {
    for segments in [12u32, 16, 24, 32, 48, 64, 96] {
        let mut s = box_bored_along_z(segments);
        let (faces_before, si_before, _) = state(&s);
        assert_eq!(si_before, 0, "segments {segments}: the first bore is clean");

        let r = s.mesh.drill_circular_through_hole(
            DVec3::new(100.0, 0.0, 0.0), DVec3::X, 40.0, segments,
        );
        let (faces, si, closed) = state(&s);
        assert!(
            r.is_err(),
            "segments {segments}: a bore crossing the first one must be refused, \
             got {faces} faces and {si} self-intersecting pairs"
        );
        assert_eq!(faces, faces_before, "segments {segments}: the mesh is untouched");
        assert_eq!(si, 0, "segments {segments}: nothing left intersecting");
        assert!(closed, "segments {segments}: still a closed solid");
    }
}

/// The other half. Refusing everything would pass the test above, so a bore that
/// genuinely misses must still go through — at the same densities.
#[test]
fn a_bore_that_misses_the_first_one_still_works() {
    for segments in [24u32, 32, 64] {
        let mut s = box_bored_along_z(segments);
        // y = 70 puts the axis outside the first tube (radius 40 about the Z axis).
        let r = s.mesh.drill_circular_through_hole(
            DVec3::new(100.0, 70.0, 0.0), DVec3::X, 20.0, segments,
        );
        assert!(r.is_ok(), "segments {segments}: a clear bore must be allowed — {r:?}");
        let (_, si, closed) = state(&s);
        assert_eq!(si, 0, "segments {segments}: two disjoint bores do not intersect");
        assert!(closed, "segments {segments}: still closed");
    }
}
