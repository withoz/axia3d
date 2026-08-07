//! DRAWING ACROSS A SOLID'S RIM DIVIDES THE FACE IT LANDS ON.
//!
//! A rectangle drawn half on a box's top and half past its edge. The part that
//! landed must be cut out of the top, the part beyond must hang off, and the
//! edge where they meet carries three faces — the two halves of the top plane
//! and the wall below. Three faces on one edge is what a sheet meeting a rim
//! looks like and is not damage (사용자 결정 2026-08-06).
//!
//! What that decision did NOT settle is how to tell it apart from two faces
//! lying on top of each other, and the first attempt (2026-08-06, `f835ae3`)
//! asked only whether two of the faces were COPLANAR. An edge that divides one
//! plane into two regions carries two coplanar faces by construction, so this
//! draw reported a violation and `closed = false`, and that reading was written
//! down as "the inner half is left undivided". Measured 2026-08-07, it was not:
//!
//! ```text
//!   FaceId(7)  x in [70,100]  area  1800   the part that landed
//!   FaceId(9)  x in [100,130] area  1800   the part beyond the rim
//!   FaceId(8)  8-vertex L     area 38200   the top, with 1800 taken out
//! ```
//!
//! 40000 − 1800 = 38200 exactly. The division was right; the judgement was
//! wrong. What decides it is which SIDE of the shared edge each face occupies
//! (`Mesh::edge_stacked_face_pair`), measured −1.000 here and +1.000 where an
//! ellipse really does cover ground twice.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use glam::DVec3;

/// x ∈ [−100,100], y ∈ [−100,100], top at z = 100.
fn boxed() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    let f = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    s
}

fn rect_on_top(s: &mut Scene, cx: f64) -> CommandResult {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(cx, 0.0, 100.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 60.0,
        height: 60.0,
    })
}

fn top_face_areas(s: &Scene) -> Vec<f64> {
    let mut areas: Vec<f64> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && f.normal().normalize_or_zero().z > 0.9)
        .map(|(fid, _)| s.mesh.face_outer_area(fid))
        .collect();
    areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    areas
}

fn stacked(s: &Scene) -> Vec<String> {
    s.mesh
        .verify_face_invariants()
        .violations
        .into_iter()
        .filter(|v| v.contains("stacked"))
        .collect()
}

/// The control: a rectangle wholly inside the face. Nothing hangs off, so the
/// solid stays closed — if this one is not clean the test below proves nothing.
#[test]
fn a_rect_inside_the_face_leaves_the_solid_closed() {
    let mut s = boxed();
    assert!(!matches!(rect_on_top(&mut s, 0.0), CommandResult::Error(_)));

    let all: Vec<axia_geo::FaceId> =
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let mi = s.mesh.face_set_manifold_info(&all);
    assert_eq!(all.len(), 7, "the top gains the rect as a hole plus the rect itself");
    assert!(mi.is_closed_solid, "nothing left the solid: {mi:?}");
    assert_eq!(mi.boundary_edge_count, 0);
    assert!(s.mesh.verify_face_invariants().is_valid());
}

/// The part that landed is cut out of the top; the part beyond hangs off.
#[test]
fn a_rect_across_the_rim_divides_the_top_and_reports_no_stacking() {
    let mut s = boxed();
    // x ∈ [70,130] — the rim is at x = 100, so 30mm lands and 30mm goes past.
    assert!(!matches!(rect_on_top(&mut s, 100.0), CommandResult::Error(_)));

    assert_eq!(
        top_face_areas(&s),
        vec![1800.0, 1800.0, 38200.0],
        "the top keeps 40000 − 1800, the landed half is its own face, and the \
         half past the rim hangs off"
    );

    assert!(
        stacked(&s).is_empty(),
        "the two halves meet at the rim edge, they do not cover the same ground: {:?}",
        stacked(&s)
    );

    let all: Vec<axia_geo::FaceId> =
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(f, _)| f).collect();
    let mi = s.mesh.face_set_manifold_info(&all);
    assert_eq!(mi.non_manifold_edge_count, 0, "the rim edge's three faces are not stacked");

    // Deliberately asserted as it is, not as a closed solid: the piece beyond
    // the rim is a sheet with three free edges, which is what "draw past the
    // edge and keep the shape whole" means. If a future change encloses it,
    // this is the assertion that will say so.
    assert_eq!(mi.boundary_edge_count, 3, "the overhanging piece's three free edges");
    assert!(!mi.is_closed_solid, "a sheet hangs off the solid");
}
