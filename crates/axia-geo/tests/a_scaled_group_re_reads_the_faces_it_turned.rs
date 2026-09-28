//! A SMOOTH-GROUP OFFSET RE-READS THE NORMALS OF THE FACES IT TURNED.
//!
//! `create_solid(face, Extrude)` on a face carrying a Cylinder / Sphere / Cone
//! / Torus surface is a smooth-group OFFSET (ADR-080 W-2-γ): it finds every
//! face carrying the same surface, scales their vertices, and rewrites the
//! surface at the new radius. It moved geometry and left every cached normal
//! where it was.
//!
//! `add_face` caches the Newell normal at creation and nothing re-reads it
//! afterwards, so ADR-007 Invariant 2 — the cached normal agrees with the
//! winding — is the mover's to keep. The sibling `Mesh::push_pull_move_only`
//! has re-read its own moved neighbourhood since 2026-04; these four did not.
//!
//! ## It only shows when a group face is NOT on its own cylinder
//!
//! Scale a true cylinder quad about the axis and its plane turns with it — the
//! normal is still right. A face with its corners at two different radii is a
//! ramp, and scaling turns it somewhere else entirely. Fuzz session 30 offsets
//! a group of twelve by 50 and they come out standing at FOUR radii:
//!
//! ```text
//!   FaceId(46) r=[ 55,  55,  55,  55]      on it
//!   FaceId(47) r=[ 55,  55, 120, 120]      a ramp
//!   FaceId(50) r=[120, 120, 205, 205]      a ramp
//!   FaceId(59) r=[139, 139, 139, 139]      on a different one
//! ```
//!
//! Seven of the twelve then disagreed with their cached normal, by up to 62°:
//!
//! ```text
//!   face FaceId(49): cached normal opposite to winding (dot=0.463)
//!     cached   radial at u = 133.05°, which is the surface's answer
//!     newell   the plane its four corners actually span now
//! ```
//!
//! ⚠ The message says "opposite" and the check is `dot < 0.9`, so two of the
//! four sessions it fired in were 62° and 65° off rather than reversed. Read
//! the number, not the word.
//!
//! ## What this fixes and what it does not
//!
//! Re-reading the normal is what the invariant asks, and it is what the
//! pre-save reconciler (`reconcile_face_normals`) and `push_pull_move_only`
//! both already do: the winding is the truth, the normal is a cache.
//!
//! It does NOT make the surface true. A face with corners at two radii is on no
//! cylinder, and it still carries one afterwards. How such a face came to carry
//! it is a separate question, upstream of this and not answered here.
//!
//! ## ⚠ Three fixtures that did NOT reproduce it
//!
//! Each passed against a build with the defect, and the mutation said so:
//!
//! ```text
//!   one ring vertex nudged out     the quad warps; its Newell is an average
//!                                  that survives the scaling
//!   a vertical PAIR nudged out     a planar ramp — but uniform radial scaling
//!                                  about the axis is a similarity in XY, so
//!                                  EVERY vertical face keeps its normal
//!   a neighbour off to one side    turns, by 11°, and the bar is dot < 0.9
//! ```
//!
//! What reproduces it is a face standing partly on the group and partly off it,
//! positioned so the group sweeps PAST its far point — which is why the fuzz's
//! violators (44, 48, 49, 54, 61) were never in the group's own twelve.
//!
//! Measured over 100 fuzz sessions × 50 operations: broken 37 → **33**, and
//! every "opposite to winding" gone (4 sessions → 0).

use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::CreateSolidMode;
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const R: f64 = 70.0;
const H: f64 = 100.0;
const N: u32 = 8;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

/// A cylinder with one ring vertex nudged off it, so two of the side faces are
/// ramps while still carrying the group's Cylinder surface — the shape the fuzz
/// arrives at after forty-odd operations, set up in one line.
///
/// The nudge is the INPUT. Its own staleness is reconciled away before the
/// assertion, so what the test measures is the offset and nothing else.
fn cylinder_with_a_face_half_on_it() -> (Mesh, FaceId) {
    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, R, H, N, mat()).expect("cylinder");

    let side: Vec<FaceId> = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .filter(|&id| matches!(m.face_surface(id), Some(AnalyticSurface::Cylinder { .. })))
        .collect();
    assert!(side.len() >= 3, "a cylinder has side faces carrying its surface");

    // One vertical edge of a side quad, and a third point far off the cylinder.
    let verts = m.collect_loop_verts(m.faces[side[0]].outer().start).expect("loop");
    let p0 = m.vertex_pos(verts[0]).expect("pos");
    let pair: Vec<_> = verts
        .iter()
        .copied()
        .filter(|&v| {
            let p = m.vertex_pos(v).unwrap_or(DVec3::ZERO);
            (p.x - p0.x).abs() < 1e-9 && (p.y - p0.y).abs() < 1e-9
        })
        .collect();
    assert_eq!(pair.len(), 2, "a side quad has two corners at each angle");

    // The third point sits on the same radial, BETWEEN the ring's radius and
    // where the offset will put it (70 < 100 < 120). The group therefore sweeps
    // PAST it, and the triangle's plane turns right through the point — its
    // normal comes out reversed, which is the dot = -1.000 the fuzz reported.
    //
    // ⚠ A point merely off to one side is not enough: the first two versions of
    // this fixture turned the face by 11° and the invariant's bar is dot < 0.9,
    // so both passed against a build that had the defect.
    let radial = DVec3::new(p0.x, p0.y, 0.0).normalize_or_zero();
    let far = m.add_vertex(radial * (R + 30.0) + DVec3::Z * (H * 0.5));
    m.add_face(&[pair[0], pair[1], far], mat()).expect("the half-on face");

    m.reconcile_face_normals();
    let inv = m.verify_face_invariants();
    assert!(inv.is_valid(), "the fixture starts sound: {:?}", inv.violations);

    (m, side[0])
}

#[test]
fn offsetting_a_group_leaves_no_stale_normal_on_its_neighbours() {
    let (mut m, side) = cylinder_with_a_face_half_on_it();
    m.create_solid(side, CreateSolidMode::Extrude { distance: 50.0 }, mat())
        .expect("smooth-group offset");

    let inv = m.verify_face_invariants();
    let stale: Vec<&String> =
        inv.violations.iter().filter(|v| v.contains("opposite to winding")).collect();
    println!("  after the offset: {} violation(s), {} of them stale normals", inv.violations.len(), stale.len());
    assert!(
        stale.is_empty(),
        "the offset turned these faces and left their cached normals behind: {stale:?}"
    );
}

/// The control: a cylinder every face of which really is on it offsets with no
/// normal changing at all, so the refresh is not quietly rewriting sound faces.
#[test]
fn offsetting_a_true_cylinder_changes_no_normal() {
    let mut m = Mesh::new();
    m.create_cylinder(DVec3::ZERO, R, H, N, mat()).expect("cylinder");
    let side = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .find(|&id| matches!(m.face_surface(id), Some(AnalyticSurface::Cylinder { .. })))
        .expect("a side face");

    let before: Vec<(FaceId, DVec3)> = m
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(id, f)| (id, f.normal().normalize_or_zero()))
        .collect();

    m.create_solid(side, CreateSolidMode::Extrude { distance: 30.0 }, mat())
        .expect("offset");

    for (id, n) in before {
        if !m.faces.contains(id) || !m.faces[id].is_active() {
            continue;
        }
        let now = m.faces[id].normal().normalize_or_zero();
        assert!(
            n.dot(now) > 0.999,
            "face {id:?} was on the cylinder and its normal moved: {n:?} -> {now:?}"
        );
    }
    assert!(m.verify_face_invariants().is_valid(), "and the result is sound");
}
