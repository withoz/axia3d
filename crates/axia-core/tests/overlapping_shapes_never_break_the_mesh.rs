//! Every way a drawn shape can overlap its host, on a sheet and on a solid.
//!
//! The question this answers, asked 2026-08-13: drawing a shape that overlaps —
//! a solid's face, or past the solid altogether — can a face disappear, can one
//! fail to appear, can the mesh break? Two hosts × three shapes × six relations
//! = 36 combinations, run with the auto behaviours ON because that is what
//! production runs (ADR-176 turned them on; the engine default stays off so the
//! older regression corpus keeps its expectations).
//!
//! Measured before the fix in this file's sibling commit:
//!
//! ```text
//!   never lost a face                 36/36
//!   never failed to create one        36/36
//!   invariants intact                 34/36   ← rect and polygon "covering"
//! ```
//!
//! The two failures were a validator defect, not damaged geometry:
//! `edge_stacked_face_pair` read a hole loop's winding as if it were an outer
//! loop, so a ring and the face filling its hole came out on the same side of
//! the rim and were called stacked. See `a_ring_does_not_cover_its_own_hole`.
//!
//! What is still open is recorded in the second test rather than asserted away.

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

const TOP: f64 = 100.0; // the 200-cube spans ±100, so its top plane is z = 100

/// (name, centre x, centre y, size) — the host face is 200×200 about the origin.
const RELATIONS: [(&str, f64, f64, f64); 6] = [
    ("disjoint", 300.0, 0.0, 80.0),    // clear of the host
    ("inside", 0.0, 0.0, 100.0),       // strictly within it
    ("partial", 100.0, 0.0, 100.0),    // straddling one edge
    ("covering", 0.0, 0.0, 400.0),     // swallowing it
    ("exact", 0.0, 0.0, 200.0),        // the same footprint
    ("corner", 100.0, 100.0, 100.0),   // straddling a corner
];

struct Outcome {
    before: usize,
    after: usize,
    violations: Vec<String>,
}

fn draw_over(host: &str, kind: &str, cx: f64, cy: f64, size: f64) -> Outcome {
    let mut scene = Scene::new();
    scene.auto_intersect_on_draw = true;
    scene.auto_face_synthesis_on_draw = true;
    scene.face_rederive_on_draw = true;

    let z = if host == "solid" {
        scene
            .mesh
            .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
            .expect("box");
        TOP
    } else {
        scene.execute(Command::DrawRectAsShape {
            center: DVec3::ZERO,
            normal: DVec3::Z,
            up: DVec3::X,
            width: 200.0,
            height: 200.0,
        });
        0.0
    };

    let active = |s: &Scene| s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let before = active(&scene);

    let center = DVec3::new(cx, cy, z);
    match kind {
        "rect" => {
            scene.execute(Command::DrawRectAsShape {
                center,
                normal: DVec3::Z,
                up: DVec3::X,
                width: size,
                height: size,
            });
        }
        "circle" => {
            scene.execute(Command::DrawCircleAsShape {
                center,
                normal: DVec3::Z,
                radius: size * 0.5,
                segments: 24,
            });
        }
        "polygon" => {
            scene.execute(Command::DrawPolygonAsShape {
                center,
                normal: DVec3::Z,
                radius: size * 0.5,
                sides: 6,
            });
        }
        other => panic!("unknown shape {other}"),
    }

    Outcome {
        before,
        after: active(&scene),
        violations: scene.mesh.verify_face_invariants().violations,
    }
}

#[test]
fn no_overlap_loses_a_face_fails_to_make_one_or_breaks_the_mesh() {
    let mut lost = Vec::new();
    let mut missing = Vec::new();
    let mut broken = Vec::new();

    for host in ["sheet", "solid"] {
        for kind in ["rect", "circle", "polygon"] {
            for (rel, cx, cy, size) in RELATIONS {
                let case = format!("{host}/{kind}/{rel}");
                let got = draw_over(host, kind, cx, cy, size);

                if got.after < got.before {
                    lost.push(format!("{case}: {} → {}", got.before, got.after));
                }
                // "exact" may legitimately reuse the host face rather than add
                // one — the same footprint drawn twice is one region, not two.
                if got.after == got.before && rel != "exact" {
                    missing.push(format!("{case}: stayed at {}", got.before));
                }
                if !got.violations.is_empty() {
                    broken.push(format!("{case}: {:?}", got.violations));
                }
            }
        }
    }

    assert!(lost.is_empty(), "a face disappeared: {lost:#?}");
    assert!(missing.is_empty(), "no face was created: {missing:#?}");
    assert!(broken.is_empty(), "the mesh broke: {broken:#?}");
}

#[test]
fn a_disk_swallowing_a_solids_face_still_covers_it_twice() {
    // KNOWN OPEN, pinned so that closing it is noticed rather than silent.
    //
    // A rect or hexagon drawn over the whole top of a box becomes a RING whose
    // hole is that top — the plane ends up covered exactly once. A circle does
    // not: a Path B closed-curve face is one anchor vertex and one self-loop
    // edge, it shares no edge with the top face, and the coplanar arrangement
    // never cuts it. The disk simply lies on top.
    //
    // Measured 2026-08-13, a Ø400 circle over a 200-cube's top:
    //
    // ```text
    //   rect    covering   +Z area 160000   ring 120000 + top 40000   once  ✓
    //   circle  covering   +Z area 165664   disk 125664 + top 40000   twice ✗
    // ```
    //
    // Nothing else reports it: the two faces share no edge, so the non-manifold
    // check cannot see them, and they are coplanar, so triangle-triangle
    // intersection does not either. Only the area does.
    let mut scene = Scene::new();
    scene.auto_intersect_on_draw = true;
    scene.auto_face_synthesis_on_draw = true;
    scene.face_rederive_on_draw = true;
    scene
        .mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    scene.execute(Command::DrawCircleAsShape {
        center: DVec3::new(0.0, 0.0, TOP),
        normal: DVec3::Z,
        radius: 200.0,
        segments: 24,
    });

    let upward: f64 = scene
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && f.normal().normalize_or_zero().z > 0.9)
        .map(|(id, _)| scene.mesh.face_area(id))
        .sum();

    let disk = std::f64::consts::PI * 200.0 * 200.0;
    assert!(
        (upward - (disk + 40_000.0)).abs() < 1.0,
        "the box top is covered twice; expected {} while this is open, got {upward}",
        disk + 40_000.0,
    );
    // When the arrangement learns to cut a closed-curve face against its host,
    // this is what it should read instead — update the assertion above then.
    assert!(
        (upward - disk).abs() > 1.0,
        "the plane is now covered once — the gap above is closed, retire this test",
    );
}
