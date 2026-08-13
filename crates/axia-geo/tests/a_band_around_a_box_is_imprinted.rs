//! Track B's acceptance case: a path that runs across several faces cuts INTO
//! them, and the solid stays whole.
//!
//! `path_along_surface` worked out where such a route goes; nothing made it
//! geometry. The plan's representative case is a closed band around a box —
//! chosen because every run of it crosses its face from boundary to boundary,
//! which is the one shape v1 can divide completely.

use axia_geo::mesh::Mesh;
use axia_geo::operations::surface_path::SurfacePath;
use axia_geo::MaterialId;
use glam::DVec3;

/// A 200-cube and the four side faces in the order a band meets them.
///
/// `create_box(w, h, d)` maps w→X, h→Z, d→Y, so a cube spans ±100 on each axis
/// and its four sides are the faces whose normals lie in the XY plane.
fn cube_and_its_sides() -> (Mesh, Vec<axia_geo::FaceId>) {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    // Order them +X, +Y, −X, −Y so consecutive ones share an edge.
    let want = [DVec3::X, DVec3::Y, -DVec3::X, -DVec3::Y];
    let sides: Vec<_> = want
        .iter()
        .map(|&n| {
            mesh.faces
                .iter()
                .find(|(_, f)| f.is_active() && f.normal().normalize_or_zero().dot(n) > 0.999)
                .map(|(fid, _)| fid)
                .unwrap_or_else(|| panic!("no side facing {n:?}"))
        })
        .collect();
    assert_eq!(sides.len(), 4);
    (mesh, sides)
}

/// The band: a horizontal loop at z = 0 around the four sides, its corners on
/// the vertical edges of the cube. Each run crosses its face edge to edge.
fn band(sides: &[axia_geo::FaceId]) -> SurfacePath {
    let c = [
        DVec3::new(100.0, 100.0, 0.0),   // +X ∩ +Y
        DVec3::new(-100.0, 100.0, 0.0),  // +Y ∩ −X
        DVec3::new(-100.0, -100.0, 0.0), // −X ∩ −Y
        DVec3::new(100.0, -100.0, 0.0),  // −Y ∩ +X
    ];
    // Closed: start on the +X face, go round, come back to where it began.
    SurfacePath {
        points: vec![c[3], c[0], c[1], c[2], c[3]],
        faces: vec![sides[0], sides[1], sides[2], sides[3]],
    }
}

#[test]
fn a_closed_band_divides_every_face_it_crosses() {
    let (mut mesh, sides) = cube_and_its_sides();
    let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(before, 6);

    let report = mesh
        .imprint_surface_path(&band(&sides), MaterialId::new(0))
        .expect("the band imprints");

    assert_eq!(
        report.split_faces.len(),
        4,
        "all four sides are crossed edge to edge, so all four divide"
    );
    assert_eq!(
        report.skipped_runs, 0,
        "a closed band has no partial run — that is why it is the case v1 can \
         do whole"
    );
    let after = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    assert_eq!(after, before + 4, "each split adds one face");
    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "invariants must hold — {:?}", v.violations);
}

/// And the solid is still a solid: the band divides its skin without opening it.
#[test]
fn the_band_leaves_the_box_closed() {
    let (mut mesh, sides) = cube_and_its_sides();
    let all: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    assert!(
        mesh.face_set_manifold_info(&all).is_closed_solid,
        "the box starts closed"
    );

    let report = mesh
        .imprint_surface_path(&band(&sides), MaterialId::new(0))
        .expect("the band imprints");

    let all: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    // "Still closed" is worth nothing on its own — it also holds when the
    // imprint did nothing at all. Say what it stayed closed THROUGH.
    assert_eq!(report.split_faces.len(), 4, "four faces were actually cut");
    assert_eq!(all.len(), 10, "6 + 4, so the cuts are really there");
    let info = mesh.face_set_manifold_info(&all);
    assert!(
        info.is_closed_solid,
        "and stays closed after the imprint — {} boundary edge(s)",
        info.boundary_edge_count
    );
}

/// The refusal v1 owes the caller: a path that comes back to a face it already
/// cut. The first split replaces that face, so the second visit would name an
/// id that no longer exists — better to say so than to half-apply the path.
#[test]
fn a_path_that_revisits_a_face_is_refused_and_nothing_moves() {
    let (mut mesh, sides) = cube_and_its_sides();
    let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let revisit = SurfacePath {
        points: vec![
            DVec3::new(100.0, -100.0, 0.0),
            DVec3::new(100.0, 100.0, 0.0),
            DVec3::new(-100.0, 100.0, 0.0),
            DVec3::new(100.0, 100.0, 50.0),
        ],
        faces: vec![sides[0], sides[1], sides[0]], // +X, +Y, back to +X
    };
    let err = mesh
        .imprint_surface_path(&revisit, MaterialId::new(0))
        .expect_err("a revisit must be refused");
    assert!(
        err.to_string().contains("returns to face"),
        "and say why: {err}"
    );
    assert_eq!(
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        before,
        "a refusal leaves the mesh exactly as it was"
    );
}

/// The v1 limit, stated rather than hidden: an OPEN path's end faces.
///
/// A run that enters a face and stops inside it does not divide anything —
/// there is no second boundary point to cut to. AixiAcad measured the same
/// limit and the plan adopts it, so what matters is that the count comes back
/// rather than the run disappearing quietly.
#[test]
fn an_open_paths_end_faces_are_skipped_and_counted() {
    let (mut mesh, sides) = cube_and_its_sides();
    // Start in the middle of +X, cross onto +Y, stop in the middle of it.
    // Neither end run reaches a second boundary.
    let open = SurfacePath {
        points: vec![
            DVec3::new(100.0, 0.0, 0.0),   // interior of +X
            DVec3::new(100.0, 100.0, 0.0), // the shared edge
            DVec3::new(0.0, 100.0, 0.0),   // interior of +Y
        ],
        faces: vec![sides[0], sides[1]],
    };
    let report = mesh
        .imprint_surface_path(&open, MaterialId::new(0))
        .expect("an open path is not an error — its ends are just not divisible");
    assert_eq!(
        report.split_faces.len(),
        0,
        "neither end run crosses its face, so neither divides it"
    );
    assert_eq!(report.skipped_runs, 2, "and both are counted");
    assert!(
        report.first_refusal.is_some(),
        "with the reason carried, not just a number"
    );
    let v = mesh.verify_face_invariants();
    assert!(
        v.is_valid(),
        "and the mesh is unharmed — {:?}",
        v.violations
    );
}
