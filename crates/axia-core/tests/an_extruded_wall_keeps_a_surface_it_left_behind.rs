//! Move a face and its neighbours keep the plane they used to be on.
//!
//! Four sessions of the wide fuzz — 11, 40, 73 and 89 — reported `cached normal
//! opposite to winding`. None of them was about a normal. Underneath every one
//! were faces carrying an analytic surface they no longer sat on, by 30 to 200
//! millimetres.
//!
//! ## Where it comes from
//!
//! `push_pull_move_only` slides a face's vertices and then walks every face those
//! vertices touched, refreshing each one's cached normal because the face has
//! changed shape. It refreshed one of the two things that describe a face and
//! left the other alone.
//!
//! On a box it never shows. MoveOnly slides the top along its own normal, and the
//! side walls' planes CONTAIN that direction, so afterwards their planes are
//! exactly what they were. Push a wall of a heptagonal prism and the neighbours
//! are not parallel to the motion: their planes turn, and the surface they carry
//! is the one they had before.
//!
//! ## What the verifier could and could not see
//!
//! Nothing. `verify_face_invariants` compares a cached normal against the winding
//! of a loop, and neither of those is the surface. Session 89's neighbour wall was
//! 42.4 mm off the plane it still claimed and what got reported was `dot=0.805`,
//! which is the 36.4° the plane had turned through. Session 11's went 31
//! operations further before a draw put a vertex into one of the mislabelled loops
//! and made a five-vertex non-planar face out of it.
//!
//! ⚠ The reproductions here are built directly rather than transcribed from those
//! sessions. Shrinking a fuzz session re-resolves its face picks by centroid, and
//! the five-operation reduction of session 89 turns out to pick a different face
//! than the full session did — it reproduces *a* stale surface, but not this one.
//! A heptagonal prism with one wall pushed is the mechanism with nothing else in
//! the way, and it is measured both ways below.
//!
//! Same shape as ADR-298 (I4 walked only outer loops, so a broken hole read
//! `valid`) and ADR-304 (`NaN > 1e-10` is false, so a NaN normal read `valid`).
//!
//! ## What this file was, and is
//!
//! It was written as `should_panic` while the cause was still open, with a note
//! saying to rewrite it into an ordinary assertion the day it went red. That
//! happened in the same session, and this is the rewrite.
//!
//! ⚠ The third test is the one that keeps the fix honest. Re-synthesizing every
//! touched face unconditionally would also make the first two pass, and would
//! flatten a cylinder's walls into planes on the way. Asking whether the face is
//! still ON its surface leaves the twenty walls that never moved alone and
//! rebuilds only the three that did — which are, measurably, planes.

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

fn push_face_at(s: &mut Scene, p: DVec3, d: f64) {
    if let Some(f) = face_near(s, p) {
        s.execute(Command::CreateSolid {
            face_id: f,
            mode: CreateSolidMode::Extrude { distance: d },
        });
    }
}

/// Every face measured against the surface it carries.
fn faces_off_their_own_surface(s: &Scene) -> Vec<String> {
    let mut off = Vec::new();
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        let Some(surf) = s.mesh.face_surface(f).cloned() else { continue };
        let pts = s.mesh.face_outline_points(f).unwrap_or_default();
        if pts.is_empty() {
            continue;
        }
        let worst = pts
            .iter()
            .filter_map(|p| surf.unsigned_distance_to(*p))
            .fold(0.0f64, f64::max);
        // Well past any drift: the render chord tolerance is 0.02 mm and the
        // dedup floor is 0.15 μm.
        if worst > 1e-3 {
            off.push(format!("{f:?} off by {worst:.1} mm"));
        }
    }
    off
}

fn count_surfaces(s: &Scene) -> (usize, usize) {
    let (mut cyl, mut plane) = (0, 0);
    for (f, fa) in s.mesh.faces.iter() {
        if !fa.is_active() {
            continue;
        }
        match s.mesh.face_surface(f) {
            Some(AnalyticSurface::Cylinder { .. }) => cyl += 1,
            Some(AnalyticSurface::Plane { .. }) => plane += 1,
            _ => {}
        }
    }
    (cyl, plane)
}

/// A circle extruded into a cylinder, then one of its side quads pushed out.
fn cylinder(push_a_wall: bool) -> Scene {
    let mut s = prod();
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(-100.0, 0.0, 0.0),
        normal: DVec3::Z,
        radius: 30.0,
    });
    push_face_at(&mut s, DVec3::new(-100.0, 0.0, 0.0), 200.0);
    if push_a_wall {
        push_face_at(&mut s, DVec3::new(-119.689, 22.279, 100.0), 200.0);
    }
    s
}

/// The mechanism on its own: a heptagonal prism with one side wall pushed out.
///
/// The push reports `MODE=MoveOnly, 4 verts moved`. Two of those four belong to
/// each neighbouring wall, so the neighbours turn while the pushed wall
/// translates.
fn heptagonal_prism_with_one_wall_pushed() -> Scene {
    let mut s = prod();
    s.execute(Command::DrawPolygonAsShape {
        center: DVec3::new(0.0, 0.0, 0.0),
        normal: DVec3::Z,
        radius: 100.0,
        sides: 7,
    });
    push_face_at(&mut s, DVec3::new(0.0, 0.0, 0.0), 120.0);
    // A side wall, taken at mid-height so the pick cannot land on a cap.
    push_face_at(&mut s, DVec3::new(0.0, -97.49, 60.0), 60.0);
    s
}

/// The control. A cylinder built the ordinary way sits on its own surface, so a
/// failure below is about the push and not about how cylinders are made.
#[test]
fn a_plain_cylinder_sits_on_its_own_surface() {
    let s = cylinder(false);
    let (cyl, _) = count_surfaces(&s);
    assert!(cyl >= 20, "expected the cylinder's side quads, found {cyl}");
    let off = faces_off_their_own_surface(&s);
    assert!(off.is_empty(), "a plain cylinder is already wrong: {off:?}");
}

/// Three faces used to claim `radius: 30` from 230 away.
#[test]
fn a_pushed_wall_does_not_claim_the_cylinder_it_left() {
    let s = cylinder(true);
    let off = faces_off_their_own_surface(&s);
    assert!(off.is_empty(), "faces are off their own surface: {off:?}");
}

/// ⚠ This is what stops the fix from being "re-synthesize everything touched".
///
/// Push a cylinder's top cap along its axis and every side wall is touched — its
/// top two vertices move — while staying exactly on the cylinder it already
/// carries, with only the v_range wanting to grow. Measured both ways:
///
/// ```text
///   asking whether the face is still on it   Cylinder 23 → 23
///   re-synthesizing whatever was touched     Cylinder 23 → 0
/// ```
///
/// The second turns a cylinder into twenty-three planes, and nothing else in this
/// file would notice: the walls are still where they should be, so every other
/// assertion here still passes. This is the one that catches it.
#[test]
fn pushing_a_cylinder_taller_keeps_its_walls_curved() {
    let mut s = prod();
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 0.0, 0.0),
        normal: DVec3::Z,
        radius: 50.0,
    });
    push_face_at(&mut s, DVec3::new(0.0, 0.0, 0.0), 100.0);
    let (before, _) = count_surfaces(&s);
    assert!(before >= 20, "expected a cylinder to start from, got {before}");

    push_face_at(&mut s, DVec3::new(0.0, 0.0, 100.0), 50.0);
    let (after, plane) = count_surfaces(&s);
    assert_eq!(
        after, before,
        "the push flattened the cylinder: {before} cylindrical faces became {after}, \
         and there are now {plane} planes"
    );
}

/// ⚠ The mechanism, measured directly rather than through a fuzz session.
///
/// Before the fix, pushing one wall of a heptagonal prism out by 60 left three
/// faces off the surface they carried:
///
/// ```text
///   FaceId(9) off 60.0     the wall that moved
///   FaceId(3) off 37.4     a neighbour
///   FaceId(8) off 37.4     the other neighbour
/// ```
///
/// 37.4 is 60 × cos(51.4°), and 51.4° is a heptagon's exterior angle — the amount
/// each neighbour's plane turns through. That number is why this file claims the
/// cause rather than merely the symptom.
#[test]
fn a_pushed_wall_leaves_its_neighbours_on_their_own_planes() {
    let s = heptagonal_prism_with_one_wall_pushed();
    let off = faces_off_their_own_surface(&s);
    assert!(off.is_empty(), "neighbours left off their planes: {off:?}");

    let inv = s.mesh.verify_face_invariants();
    let winding: Vec<_> = inv
        .violations
        .iter()
        .filter(|v| v.contains("opposite to winding"))
        .collect();
    assert!(winding.is_empty(), "and the symptom it used to show as: {winding:?}");
}
