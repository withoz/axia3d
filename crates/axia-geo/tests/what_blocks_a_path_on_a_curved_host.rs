//! 트랙 C, measured before it is planned: what actually stops a path on a
//! kernel-native (Path B) host?
//!
//! Track B's imprint divides a polygonal cylinder's 24 sides without noticing
//! they were curved-ish — a Path A solid is a box with more faces. The genuine
//! curved case is Path B, whose side is ONE face bounded by a self-loop edge
//! carrying an `AnalyticCurve`. Three guesses about that went wrong in the same
//! session, so this asks the engine rather than reasoning, and pins the answer
//! at today's numbers.
//!
//! Nothing here is a fix. It is the ground the next step stands on.

use axia_geo::mesh::Mesh;
use glam::DVec3;

/// What a Path B cylinder is MADE of — the shape of the problem.
#[test]
fn a_path_b_cylinder_has_one_side_face_and_a_curved_rim() {
    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true; // production default (LOCKED #47)
    let faces = mesh
        .create_cylinder(DVec3::ZERO, 100.0, 200.0, 24, Default::default())
        .expect("kernel-native cylinder");

    let active: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    let curved_edges = mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active() && e.curve().is_some())
        .count();
    let self_loops = mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active() && e.v_small() == e.v_large())
        .count();

    println!(
        "PATH B faces={} (created {}) curved_edges={curved_edges} self_loops={self_loops}",
        active.len(),
        faces.len()
    );

    // The canonical Path B cylinder is 3 faces / 2 edges / 2 verts (LOCKED #47).
    assert_eq!(active.len(), 3, "base, top, and ONE annular side");
    assert!(
        curved_edges >= 2,
        "its rims are curves, not polygons — {curved_edges}"
    );
    assert!(
        self_loops >= 2,
        "and each rim is a self-loop: one anchor vertex — {self_loops}"
    );
}

/// ⚠ SIXTH instrument that lies about a curved face: **its normal**.
///
/// All three faces of a Path B cylinder report `n = (0, 0, 1)` — the annular
/// side included, because its boundary is two circles whose Newell normal is
/// axial. Its shape is in the `Cylinder` SURFACE, not in the loop. Filtering
/// side faces by `normal().z.abs() < 0.5` finds ZERO of them.
///
/// Same family as `point_in_face`, `face_bounds` and `face_area` (all fixed in
/// PR #124) and the two counting instruments: a Path B face's boundary is one
/// vertex and a curve, so anything derived from the polygon is meaningless.
/// **Ask the surface.**
///
/// And that is what makes the curved host a different PROBLEM, not a harder
/// version of the same one: there is only ONE side face, so a band round it is
/// not a multi-face path at all — it is a single closed cut ON one face. The
/// imprint has nothing to route between. That is 트랙 C's actual subject, and
/// it is not what my doc comment guessed (a split refusing on a curved edge).
#[test]
fn a_curved_face_cannot_be_found_by_its_normal_and_there_is_only_one() {
    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, 100.0, 200.0, 24, Default::default())
        .expect("cylinder");

    let by_normal = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && f.normal().z.abs() < 0.5)
        .count();
    assert_eq!(
        by_normal, 0,
        "PIN: the normal cannot find a curved face — all three read +Z"
    );

    let by_surface = mesh
        .faces
        .iter()
        .filter(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| {
                    !matches!(s, axia_geo::surfaces::AnalyticSurface::Plane { .. })
                })
        })
        .count();
    assert_eq!(
        by_surface, 1,
        "PIN: asking the SURFACE finds it, and there is exactly one side. A          band round it is one closed cut on ONE face, not a multi-face path —          a different operation from the imprint"
    );
}
