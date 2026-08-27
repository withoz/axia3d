//! A vertex whose outgoing half-edge is gone — seed 0x5EED0004, shrunk.
//!
//! The wide fuzz panicked at `storage.rs:139`, `Entity HeId(965) not found in
//! storage`. Fifty operations is not a bug report, so this is the twenty that
//! still do it, by plain delta debugging to a fixpoint (a second pass found
//! five more; a third found none).
//!
//! ⚠ It is NOT the defect the records pointed at. `remove_edge_and_halfedges`
//! genuinely never looks at faces — that gap is real and pinned separately by
//! `v_ring_cleans_up_on_edge_removal` in `mesh.rs` — but it is not this.
//! Measured immediately before the last operation:
//!
//! ```text
//!   BEFORE  faces=115  valid=true  violations=0
//!   BEFORE  broken loops = 0
//! ```
//!
//! The mesh going in is sound. One `DrawRectAsShape` then panics.
//!
//! ## Where, exactly
//!
//! ```text
//!   exec_draw_rect_as_shape → intersect_faces_inner → rederive_coplanar_on_draw
//!     → rebuild_coplanar_faces_analytic_scoped → rebuild_inner
//!       → remove_edge_and_halfedges → remove_from_v_ring → PANIC
//! ```
//!
//! Inside `remove_from_v_ring`, walking a vertex's v_next ring:
//!
//! ```ignore
//!   let mut cur = start;                  // start = verts[v].outgoing()
//!   loop {
//!       let nxt = self.hes[cur].v_next(); // indexes `cur` UNGUARDED
//!       if nxt == he { pred = Some(cur); break; }
//!       if nxt.is_null() || !self.hes.contains(nxt) { break; }   // nxt IS guarded
//!       cur = nxt;
//! ```
//!
//! Every half-edge in that walk is checked before being indexed **except the
//! first**. The function guards its `he` argument on entry too; only `start` is
//! missed. So a vertex whose `outgoing` names a dead half-edge panics on
//! iteration one.
//!
//! ## How `outgoing` goes dead
//!
//! `remove_edge_and_halfedges`'s F1 repoints `outgoing` for the edge's OWN two
//! endpoints. A half-edge being removed can be some THIRD vertex's `outgoing`,
//! and nothing repoints that one.
//!
//! ⚠ `verify_face_invariants` cannot see it — it walks faces, and a stale
//! v_ring is not a face. That is why the mesh reads `valid=true` right up to
//! the panic, and why the fuzz only finds this by falling over.
//!
//! ## What this file is for
//!
//! It pins the reproduction, not a fix. `#[should_panic]` so it stays green
//! while the defect stands and goes RED the day someone closes it — that is the
//! signal to rewrite it into an ordinary assertion, not to delete it.
//!
//! ⚠ The list is coordinate-based on purpose. The fuzz picks faces by INDEX
//! into the active list, so dropping any operation shifts every later choice —
//! delta debugging on the raw log is meaningless. Each push here re-resolves its
//! face by centroid, which survives a drop.
//!
//! ⚠ Also recorded, because it cost a wrong conclusion: a delta-debug script
//! must WRITE BACK the final list. Mine left the last FAILED trial in the file,
//! and the first "the mesh is clean before the last op" reading was taken from a
//! list that does not panic at all. The reading happened to survive re-doing on
//! the real list; it was luck, not method.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::CreateSolidMode;
use glam::DVec3;

#[derive(Clone, Copy, Debug)]
enum Op {
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Circle { x: f64, y: f64, z: f64, w: f64 },
    CircleCurve { x: f64, y: f64, z: f64, w: f64 },
    Ellipse { x: f64, y: f64, z: f64, w: f64 },
    Polygon { x: f64, y: f64, z: f64, w: f64, n: u32 },
    Line { x: f64, y: f64, z: f64 },
    PushFace { cx: f64, cy: f64, cz: f64, d: f64 },
    Box { x: f64, y: f64, z: f64, w: f64 },
    Punch { x: f64, y: f64, z: f64 },
}

const OPS: &[Op] = &[
    Op::Circle { x: 50.0, y: -150.0, z: 100.0, w: 220.0 },
    Op::Circle { x: 50.0, y: 200.0, z: 0.0, w: 100.0 },
    Op::CircleCurve { x: 200.0, y: -100.0, z: 200.0, w: 140.0 },
    Op::PushFace { cx: 50.0, cy: -149.99999999999997, cz: 100.0, d: 50.0 },
    Op::Rect { x: 100.0, y: -50.0, z: 100.0, w: 180.0 },
    Op::Box { x: -100.0, y: 200.0, z: 0.0, w: 220.0 },
    Op::Circle { x: 150.0, y: -100.0, z: 0.0, w: 220.0 },
    Op::Box { x: 50.0, y: 150.0, z: 0.0, w: 60.0 },
    Op::Box { x: -50.0, y: -150.0, z: 0.0, w: 60.0 },
    Op::Circle { x: 100.0, y: -100.0, z: 100.0, w: 140.0 },
    Op::Box { x: -50.0, y: -200.0, z: 0.0, w: 180.0 },
    Op::Box { x: 50.0, y: -100.0, z: 100.0, w: 180.0 },
    Op::Rect { x: -150.0, y: 0.0, z: 100.0, w: 220.0 },
    Op::Box { x: -150.0, y: 0.0, z: 0.0, w: 220.0 },
    Op::Box { x: 50.0, y: 150.0, z: 100.0, w: 100.0 },
    Op::PushFace { cx: 86.49365779310823, cy: -47.3166881653416, cz: 125.0, d: -100.0 },
    Op::PushFace { cx: 191.59075191683885, cy: -39.63409257117852, cz: 0.0, d: 200.0 },
    Op::PushFace { cx: 53.005696576009626, cy: -141.54278037722366, cz: 125.0, d: -100.0 },
    Op::Line { x: 0.0, y: -100.0, z: 100.0 },
    // …and this one brings it down.
    Op::Rect { x: -100.0, y: 100.0, z: 100.0, w: 220.0 },
];

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The active face whose centroid is nearest `p`, re-resolved every time.
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

fn apply(s: &mut Scene, op: Op) {
    match op {
        Op::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                up: DVec3::X,
                width: w,
                height: w * 0.75,
            });
        }
        Op::Circle { x, y, z, w } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: w * 0.5,
                segments: 24,
            });
        }
        Op::CircleCurve { x, y, z, w } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: w * 0.5,
            });
        }
        Op::Ellipse { x, y, z, w } => {
            s.execute(Command::DrawEllipseAsCurve {
                center: DVec3::new(x, y, z),
                ref_dir: DVec3::X,
                normal: DVec3::Z,
                radius_x: w * 0.5,
                radius_y: w * 0.3,
            });
        }
        Op::Polygon { x, y, z, w, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: w * 0.5,
                sides: n,
            });
        }
        Op::Line { x, y, z } => {
            s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            });
        }
        Op::PushFace { cx, cy, cz, d } => {
            if let Some(f) = face_near(s, DVec3::new(cx, cy, cz)) {
                s.execute(Command::CreateSolid {
                    face_id: f,
                    mode: CreateSolidMode::Extrude { distance: d },
                });
            }
        }
        Op::Box { x, y, z, w } => {
            let _ = s
                .mesh
                .create_box(DVec3::new(x, y, z + 60.0), w, 120.0, w, FORM_MATERIAL);
        }
        Op::Punch { x, y, z } => {
            let _ = s.punch_rect_hole(
                DVec3::new(x - 30.0, y - 30.0, z),
                DVec3::new(x + 30.0, y + 30.0, z),
                DVec3::Z,
            );
        }
    }
}

/// The nineteen operations before the last one leave a mesh the verifier calls
/// sound. This is the half of the report that says the last draw is the CAUSE
/// and not a victim — and it is an ordinary passing test, so it keeps saying so.
#[test]
fn the_mesh_is_sound_right_up_to_the_last_draw() {
    let mut s = prod();
    for op in &OPS[..OPS.len() - 1] {
        apply(&mut s, *op);
    }

    let inv = s.mesh.verify_face_invariants();
    assert!(
        inv.is_valid(),
        "already invalid before the last op: {:?}",
        inv.violations
    );

    let mut broken = Vec::new();
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        if s.mesh.collect_loop_verts(f.outer().start).is_err() {
            broken.push(format!("{fid:?} outer"));
        }
        for (i, l) in f.inners().iter().enumerate() {
            if s.mesh.collect_loop_verts(l.start).is_err() {
                broken.push(format!("{fid:?} inner[{i}]"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "loops already broken before the last op: {broken:?}"
    );
}

/// And then one rect used to bring it down. It does not any more.
///
/// This was `#[should_panic(expected = "not found in storage")]` with a note
/// saying that when it stopped panicking the defect was fixed and the test
/// should be rewritten rather than deleted. That is what happened, in the same
/// session, and this is the rewrite.
#[test]
fn the_last_rect_draws_and_the_mesh_stays_sound() {
    let mut s = prod();
    for op in OPS {
        apply(&mut s, *op);
    }

    let inv = s.mesh.verify_face_invariants();
    assert!(inv.is_valid(), "invalid after the last op: {:?}", inv.violations);

    // The verifier walks faces, so it cannot see the thing that actually broke.
    // Ask the v_rings directly: every vertex's anchor must be a live half-edge
    // that leaves THAT vertex — the invariant F1 was violating.
    let mut bad = Vec::new();
    for (v, vt) in s.mesh.verts.iter() {
        let Some(h) = vt.outgoing() else { continue };
        if !s.mesh.hes.contains(h) {
            bad.push(format!("{v:?} anchors dead {h:?}"));
            continue;
        }
        let e = s.mesh.hes[h].edge();
        if !s.mesh.edges.contains(e) {
            bad.push(format!("{v:?} anchors {h:?} on a dead edge"));
            continue;
        }
        let (a, b) = (s.mesh.edges[e].v_small(), s.mesh.edges[e].v_large());
        if a != v && b != v {
            bad.push(format!("{v:?} anchors {h:?}, an edge it is not on"));
        }
    }
    assert!(bad.is_empty(), "v_ring anchors are wrong: {bad:?}");
}

