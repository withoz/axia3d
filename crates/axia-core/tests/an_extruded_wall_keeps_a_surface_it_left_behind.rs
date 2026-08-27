//! Push a cylinder's wall outward and the new wall still claims the cylinder.
//!
//! Fuzz session 11, seed `0x5EED000B`, reported `cached normal opposite to
//! winding` at operation 31. Shrunk from thirty-one operations to **four**, and
//! then the fourth turned out not to matter:
//!
//! ```text
//!   circle at (-100,0,0) r=30      draw
//!   extrude that disk    +200      a cylinder, 24 side quads
//!   extrude one side quad +200     the wall in question
//!   rect at (-100,50,0)  60x45     what the fuzz blamed
//! ```
//!
//! ⚠ The rect is innocent. Measured with and without it, the same three faces
//! are already 199.8 mm off the surface they carry:
//!
//! ```text
//!   without the rect   FaceId(10) r=30 off by 199.8
//!                      FaceId(11) r=30 off by 199.8
//!                      FaceId(12) r=30 off by 199.8
//!   with the rect      the same three, the same 199.8
//! ```
//!
//! Pushing a wall out by 200 gives the new faces the parent's surface --
//! `Cylinder { axis_origin: (-100,0,0), axis_dir: Z, radius: 30 }` -- while
//! their vertices sit 230 from that axis. A factor of 7.7, not a tolerance.
//!
//! ## Why the fuzz called it a winding problem
//!
//! It could not see this. `verify_face_invariants` compares a face's cached
//! normal against the winding of its loop; neither is the surface. The 199.8 mm
//! sat silently until the rect crossed the original cylinder and inserted a
//! vertex -- correctly, at r=30 where the crossing really is -- into one of the
//! mislabelled faces. Five vertices, four at r=230 and one at r=30, is not
//! planar, so the winding check finally fired, 31 operations after the cause.
//!
//! That shape is familiar: ADR-298 found `verify_face_invariants` reporting
//! `valid` over a broken hole because I4 walked only outer loops, and ADR-304
//! found it reporting `valid` over a NaN normal because `NaN > 1e-10` is false.
//! A gate that cannot see the thing that is wrong reports the next thing that
//! happens to break.
//!
//! ## What is open
//!
//! What surface an extruded curved face SHOULD carry is a decision, not an
//! oversight. Here the push is radial, so the cap really is a cylinder of radius
//! 230 about the same axis -- but an extrude along any other direction is not
//! cylindrical at all, and dropping the surface instead costs the face its
//! kernel-aware operations (ADR-087 K-ε's `NoProfileSurface`). Both answers are
//! defensible and neither is mine to pick, so this file measures rather than
//! decides.
//!
//! ⚠ `should_panic` while it stands. The day it goes red, rewrite it into an
//! ordinary assertion -- do not delete it.

use axia_core::scene::Scene;
use axia_core::Command;
use axia_geo::{AnalyticSurface, CreateSolidMode};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn face_near(s: &Scene, p: DVec3) -> Option<axia_geo::FaceId> {
    let mut best: Option<(f64, axia_geo::FaceId)> = None;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let Some(pts) = s.mesh.face_outline_points(fid) else { continue };
        if pts.is_empty() {
            continue;
        }
        let c: DVec3 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
        let d = c.distance(p);
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, fid));
        }
    }
    best.map(|(_, f)| f)
}

fn extrude_at(s: &mut Scene, p: DVec3, d: f64) {
    if let Some(f) = face_near(s, p) {
        s.execute(Command::CreateSolid {
            face_id: f,
            mode: CreateSolidMode::Extrude { distance: d },
        });
    }
}

/// A cylinder, and then one of its side quads pushed out by 200.
fn cylinder(push_a_wall: bool) -> Scene {
    let mut s = prod();
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(-100.0, 0.0, 0.0),
        normal: DVec3::Z,
        radius: 30.0,
    });
    extrude_at(&mut s, DVec3::new(-100.0, 0.0, 0.0), 200.0);
    if push_a_wall {
        extrude_at(&mut s, DVec3::new(-119.689, 22.279, 100.0), 200.0);
    }
    s
}

/// Every vertex of every cylindrical face, measured against the cylinder that
/// face claims to be part of.
fn faces_off_their_own_surface(s: &Scene) -> Vec<String> {
    let mut off = Vec::new();
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        let Some(AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, .. }) =
            s.mesh.face_surface(f).cloned()
        else {
            continue;
        };
        let pts = s.mesh.face_outline_points(f).unwrap_or_default();
        let worst = pts
            .iter()
            .map(|p| {
                let d = *p - axis_origin;
                let radial = d - axis_dir * d.dot(axis_dir);
                (radial.length() - radius).abs()
            })
            .fold(0.0f64, f64::max);
        // Well past any drift: the tessellation tolerance is 0.02 mm and the
        // dedup floor is 0.15 μm.
        if worst > 1e-3 {
            off.push(format!("{f:?} claims r={radius:.0}, off by {worst:.1} mm"));
        }
    }
    off
}

/// The control. A cylinder built the ordinary way sits on its own surface, so a
/// failure below is about the push and not about how the cylinder is made.
#[test]
fn a_plain_cylinder_sits_on_its_own_surface() {
    let s = cylinder(false);
    let n = s
        .mesh
        .faces
        .iter()
        .filter(|(f, fa)| {
            fa.is_active()
                && matches!(s.mesh.face_surface(*f), Some(AnalyticSurface::Cylinder { .. }))
        })
        .count();
    assert!(n >= 20, "expected the cylinder's side quads, found {n} cylindrical faces");

    let off = faces_off_their_own_surface(&s);
    assert!(off.is_empty(), "a plain cylinder is already wrong: {off:?}");
}

/// ⚠ Open. Three faces claim `radius: 30` from 230 away.
#[test]
#[should_panic(expected = "off by")]
fn a_pushed_wall_still_claims_the_cylinder_it_left() {
    let s = cylinder(true);
    let off = faces_off_their_own_surface(&s);
    assert!(off.is_empty(), "faces are off their own surface: {off:?}");
}
