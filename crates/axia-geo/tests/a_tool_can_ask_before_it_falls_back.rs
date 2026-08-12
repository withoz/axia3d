//! Asking whether a straight drill would be refused as a crossing — before
//! anything is touched.
//!
//! Why this needs to exist at all: every tool that fell back to a 2D face punch
//! after a refused drill turned a careful refusal into a worse result reported
//! as a success. The punch does NOT refuse a crossing, because from its side
//! nothing is wrong — the profile fits its host face; it is the FAR side that is
//! missing. Measured 2026-08-11 in the running app on a Ø80-bored 200³ box, for
//! the circle, the rect and the polygon alike:
//!
//! ```text
//!   drill → −1 (crossing refused)   punch → 41 (succeeded)   closed → OPEN
//! ```
//!
//! So the tools ask first. The point of the query is not that it answers, but
//! that it answers **the same way the drill will** — it runs the drill's own
//! guard rather than a second opinion, and these tests hold that together.

use axia_geo::mesh::Mesh;
use axia_geo::FaceId;
use glam::DVec3;

const R: f64 = 40.0;

/// A 200³ box with a Ø80 bore along Z, centred.
fn bored() -> Mesh {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    mesh.drill_circular_through_hole(DVec3::new(0.0, 0.0, 100.0), DVec3::Z, R, 32)
        .expect("bore along Z");
    mesh
}

/// The circular rim a drill of this radius would present, on the +X wall.
fn rim_on_x_wall(centre_y: f64, centre_z: f64, radius: f64) -> Vec<DVec3> {
    (0..32)
        .map(|k| {
            let a = std::f64::consts::TAU * (k as f64) / 32.0;
            DVec3::new(100.0, centre_y + radius * a.cos(), centre_z + radius * a.sin())
        })
        .collect()
}

#[test]
fn a_profile_that_crosses_the_bore_is_reported_as_crossing() {
    let mesh = bored();
    // Rect — the two corners, which is exactly what `drill_rect_through_hole`
    // hands the guard.
    let rect = [DVec3::new(100.0, -30.0, -30.0), DVec3::new(100.0, 30.0, 30.0)];
    assert!(
        mesh.drill_profile_would_cross(&rect, DVec3::X),
        "a rect spanning the bore must be reported as a crossing",
    );

    // Polygon — the whole loop.
    let poly = [
        DVec3::new(100.0, -30.0, -30.0),
        DVec3::new(100.0, 30.0, -30.0),
        DVec3::new(100.0, 30.0, 30.0),
        DVec3::new(100.0, -30.0, 30.0),
    ];
    assert!(
        mesh.drill_profile_would_cross(&poly, DVec3::X),
        "a polygon spanning the bore must be reported as a crossing",
    );

    // Circle — the rim.
    assert!(
        mesh.drill_profile_would_cross(&rim_on_x_wall(0.0, 0.0, R), DVec3::X),
        "a circular rim spanning the bore must be reported as a crossing",
    );
}

#[test]
fn a_profile_clear_of_the_bore_is_not() {
    let mesh = bored();
    // Up in the corner of the +X wall, nowhere near the Ø80 bore on the axis.
    let rect = [DVec3::new(100.0, 60.0, 60.0), DVec3::new(100.0, 90.0, 90.0)];
    assert!(!mesh.drill_profile_would_cross(&rect, DVec3::X));

    let poly = [
        DVec3::new(100.0, 60.0, 60.0),
        DVec3::new(100.0, 90.0, 60.0),
        DVec3::new(100.0, 90.0, 90.0),
        DVec3::new(100.0, 60.0, 90.0),
    ];
    assert!(!mesh.drill_profile_would_cross(&poly, DVec3::X));

    assert!(!mesh.drill_profile_would_cross(&rim_on_x_wall(65.0, 65.0, 20.0), DVec3::X));
}

/// The whole value of the query is that it cannot reach a different conclusion
/// than the drill. A guard the drill disagrees with would send tools down the
/// very path this exists to close.
#[test]
fn the_answer_matches_what_the_drill_actually_does() {
    for (label, y, z, radius) in [
        ("across the bore", 0.0_f64, 0.0_f64, R),
        ("clear of the bore", 65.0, 65.0, 20.0),
    ] {
        let asked = bored().drill_profile_would_cross(&rim_on_x_wall(y, z, radius), DVec3::X);

        let mut mesh = bored();
        let drilled = mesh
            .drill_circular_through_hole(DVec3::new(100.0, y, z), DVec3::X, radius, 32)
            .is_ok();

        assert_eq!(
            asked, !drilled,
            "{label}: the query said crossing={asked}, the drill said ok={drilled}",
        );
    }
}

/// Read-only in the way that matters: a tool asks this *before* deciding, so an
/// answer that cost the caller a face would be a trap.
#[test]
fn asking_changes_nothing() {
    let mesh = bored();
    let before = (
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        mesh.verts.iter().filter(|(_, v)| v.is_active()).count(),
    );
    let rect = [DVec3::new(100.0, -30.0, -30.0), DVec3::new(100.0, 30.0, 30.0)];
    let _ = mesh.drill_profile_would_cross(&rect, DVec3::X);
    let after = (
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        mesh.verts.iter().filter(|(_, v)| v.is_active()).count(),
    );
    assert_eq!(before, after, "the query must not touch the mesh");
}

/// Degenerate input is not a crossing — a caller must not be told to stop
/// because it asked badly.
#[test]
fn nonsense_input_is_not_a_crossing() {
    let mesh = bored();
    assert!(!mesh.drill_profile_would_cross(&[], DVec3::X));
    assert!(!mesh.drill_profile_would_cross(
        &[DVec3::new(100.0, -30.0, -30.0), DVec3::new(100.0, 30.0, 30.0)],
        DVec3::ZERO,
    ));
    // An empty mesh has no far wall to be blocked by, so nothing crosses.
    let empty = Mesh::new();
    assert!(!empty.drill_profile_would_cross(
        &[DVec3::new(100.0, -30.0, -30.0), DVec3::new(100.0, 30.0, 30.0)],
        DVec3::X,
    ));
    let _ = FaceId::new(0); // keep the import honest
}
