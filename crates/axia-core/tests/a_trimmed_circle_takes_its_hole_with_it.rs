//! A CIRCLE THAT IS SOMEBODY'S HOLE TAKES THE HOLE WITH IT WHEN IT IS TRIMMED.
//!
//! Draw a circle inside a rectangle and the two of them share one rim: the
//! circle's self-loop edge carries a half-edge for the disk and its twin for
//! the rectangle's inner loop. Measured after two commands:
//!
//! ```text
//!   faces = 2, FaceId(3).inner[0] on self-loop EdgeId(13)
//! ```
//!
//! Now draw a line across the circle. `trim_circle_face_at_crossings` cuts the
//! rim into arcs, and step 3 of it removes the old self-loop edge — which is
//! also the rectangle's window frame. The rectangle was left naming a half-edge
//! that is gone, and its hole could never be walked again:
//!
//! ```text
//!   face FaceId(59): inner[0] cannot collect: HalfEdge HeId(505) not found
//! ```
//!
//! Found in fuzz session 43, operation 41, `rect(-200,150,200,140)` — a
//! rectangle reaches `exec_draw_line` (ADR-008 Axiom 2: RECT = 4 LINEs), so
//! `split_circle_face_by_line` is on the path. Backtraced:
//!
//! ```text
//!   remove_edge_and_halfedges
//!     <- trim_circle_face_at_crossings
//!       <- split_circle_face_by_chord  <- split_circle_face_by_line
//!         <- exec_draw_line <- exec_draw_rect <- exec_draw_rect_as_shape
//! ```
//!
//! The hole does not stop existing when the rim is trimmed. It is bounded by N
//! arcs instead of one loop, so the fix moves it there: the new face's boundary
//! half-edges each have a free twin, and linking those twins backwards gives the
//! loop a hole wants.
//!
//! ⚠ Only a HOLE is moved. The other shape that stands on such a rim is a face
//! whose OUTER loop is on it — a cone's side on its base rim reads
//! `[(FaceId(0), None)]` — and an outer loop is the face itself, not a window in
//! it. Declining the trim for that case broke six curved-seam tests, so it is
//! left exactly as it was; whether those come through sound is a separate
//! question this does not answer.
//!
//! Over 100 fuzz sessions × 50 operations: broken 33 → **32**, and the one
//! "cannot collect" gone.

use axia_core::{Command, Scene};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn active(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// A rectangle with a circle inside it — the circle is the rectangle's hole and
/// its own disk, and the two share the rim.
fn rect_with_a_circular_window() -> Scene {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 400.0,
        height: 400.0,
    });
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::ZERO,
        normal: DVec3::Z,
        radius: 70.0,
    });

    let on_rim: Vec<String> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .flat_map(|(fid, f)| {
            f.inners()
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    !l.start.is_null()
                        && s.mesh.hes.contains(l.start)
                        && s.mesh
                            .edges
                            .get(s.mesh.hes[l.start].edge())
                            .map(|e| e.is_self_loop())
                            .unwrap_or(false)
                })
                .map(move |(i, _)| format!("{fid:?}.inner[{i}]"))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        on_rim.len(),
        1,
        "the fixture needs exactly one hole standing on a self-loop rim, got {on_rim:?}"
    );
    assert!(s.mesh.verify_face_invariants().is_valid(), "the fixture starts sound");
    s
}

/// The disk face — the one the circle filled, which the trim will cut.
fn the_disk(s: &Scene) -> axia_geo::FaceId {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active() && f.inners().is_empty())
        .map(|(id, _)| id)
        .find(|&id| {
            s.mesh
                .collect_loop_verts(s.mesh.faces[id].outer().start)
                .map(|v| v.len() == 1)
                .unwrap_or(false)
        })
        .expect("the circle's own disk, a one-vertex self-loop face")
}

#[test]
fn a_line_across_the_window_leaves_the_hole_walkable() {
    let mut s = rect_with_a_circular_window();
    let before = active(&s);
    let disk = the_disk(&s);

    // The engine call, not the command.
    //
    // ⚠ Going through `Command::DrawLine` hides this: the draw's own
    // post-process rebuilds the rectangle afterwards and the damage never shows.
    // Measured — the trim really does see the hole (`users=[(FaceId(3),
    // Some(0))]`) and the scene still comes out valid either way, so a
    // command-level fixture passes against a build with the defect. What this
    // pins is the mesh operation's own contract.
    s.mesh
        .split_circle_face_by_line(
            disk,
            DVec3::new(-300.0, 0.0, 0.0),
            DVec3::new(300.0, 0.0, 0.0),
            axia_core::FORM_MATERIAL,
        )
        .expect("the split must not error")
        .expect("a line straight through the middle crosses it twice");

    let inv = s.mesh.verify_face_invariants();
    let orphaned: Vec<&String> =
        inv.violations.iter().filter(|v| v.contains("cannot collect")).collect();
    println!(
        "  faces {before} -> {}, {} violation(s), {} of them an unwalkable loop",
        active(&s),
        inv.violations.len(),
        orphaned.len()
    );
    assert!(
        orphaned.is_empty(),
        "the trim took the rim out from under the rectangle's hole: {orphaned:?}"
    );
    assert!(inv.is_valid(), "and nothing else broke either: {:?}", inv.violations);
    assert!(
        active(&s) > before,
        "the line should still split the disk it crosses ({before} faces before, {} after)",
        active(&s)
    );
}

/// The hole is not merely walkable — it is still the circle, now in arcs.
#[test]
fn the_rehomed_hole_still_bounds_the_circle() {
    let mut s = rect_with_a_circular_window();
    let disk = the_disk(&s);
    s.mesh
        .split_circle_face_by_line(
            disk,
            DVec3::new(-300.0, 0.0, 0.0),
            DVec3::new(300.0, 0.0, 0.0),
            axia_core::FORM_MATERIAL,
        )
        .expect("split")
        .expect("two crossings");

    let mut found = 0;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        for (i, l) in f.inners().iter().enumerate() {
            let verts = s
                .mesh
                .collect_loop_verts(l.start)
                .unwrap_or_else(|e| panic!("{fid:?}.inner[{i}] cannot be walked: {e}"));
            assert!(verts.len() >= 2, "{fid:?}.inner[{i}] has {} verts", verts.len());
            for &v in &verts {
                let p = s.mesh.vertex_pos(v).expect("vertex");
                let r = DVec3::new(p.x, p.y, 0.0).length();
                assert!(
                    (r - 70.0).abs() < 1e-6,
                    "{fid:?}.inner[{i}] has a corner at radius {r:.3}, not on the circle"
                );
            }
            println!("  {fid:?}.inner[{i}] walks {} corners, all at r = 70", verts.len());
            found += 1;
        }
    }
    assert_eq!(found, 1, "the rectangle should still have exactly one window");
}
