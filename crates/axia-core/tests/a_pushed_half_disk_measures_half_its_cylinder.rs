//! A CIRCLE CUT BY A LINE, ONE HALF PUSHED, MEASURES HALF A CYLINDER.
//!
//! The path a user reaches: draw a circle, draw a line across it, push one half.
//! Its walls are arc quads, and until 2026-09-23 each of them carried the WHOLE
//! cylinder the circle stands on, so the solid measured **×2.8789** its volume.
//! Reading each face over the patch it stands on left ×0.8789 — the rest was its
//! top cap, whose 141,371.7 mm² of area entered the volume as the 90,000 mm² of
//! the chord triangle its three vertices trace.
//!
//! Both are readers, and the Inspector reads the same numbers.

use axia_core::{Command, Scene};
use axia_geo::CreateSolidMode;
use glam::DVec3;

const R: f64 = 300.0;
const H: f64 = 120.0;

/// Draw the circle, cut it with a line, push one half by `dist`, and measure the
/// solid — the other half stays a flat sheet and is left out of the sum.
fn pushed_half(dist: f64) -> (f64, bool) {
    let mut s = Scene::new();
    s.face_rederive_on_draw = true;
    s.auto_intersect_on_draw = true;
    s.execute(Command::DrawCircleAsCurve { center: DVec3::ZERO, normal: DVec3::Z, radius: R });
    s.execute(Command::DrawLineAsShape {
        start: DVec3::new(-400.0, 0.0, 0.0),
        end: DVec3::new(400.0, 0.0, 0.0),
        surface_normal: None,
    });
    let halves: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
    assert_eq!(halves.len(), 2, "a line across a circle leaves two halves");
    let (pushed, sheet) = (halves[0], halves[1]);
    s.execute(Command::CreateSolid {
        face_id: pushed,
        mode: CreateSolidMode::Extrude { distance: dist },
    });
    let flux: f64 = s
        .mesh
        .faces
        .iter()
        .filter(|(id, f)| f.is_active() && *id != sheet)
        .filter_map(|(id, _)| s.mesh.face_outward_flux(id))
        .sum();
    (flux / 3.0, s.mesh.verify_face_invariants().is_valid())
}

#[test]
fn a_half_disk_pushed_either_way_measures_half_a_cylinder() {
    let truth = std::f64::consts::PI * R * R / 2.0 * H;
    for dist in [H, -H] {
        let (v, valid) = pushed_half(dist);
        assert!(valid, "pushing {dist:+} leaves a sound mesh");
        assert!(
            (v / truth - 1.0).abs() < 1e-9,
            "half a cylinder is {truth:.1}; pushed {dist:+} it read {v:.1} ({:.4} of it)",
            v / truth
        );
    }
}
