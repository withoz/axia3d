//! What the arrange makes of the two scenes — and the mechanism it found.
//!
//! Session 10's reduced break and defect 3's look alike on the plane the draw
//! lands on, and three ways of counting bodies could not tell them apart. So the
//! question moved from how they look to what is MADE of them, and the answer is
//! smaller than the side rule:
//!
//! ```text
//!   A  session 10   circle 110 over the rectangles   1 new face, whole,
//!                                                    one 7,500 hole   4 violations
//!   B  defect 3     circle 70 off to the side        1 new face, whole,
//!                                                    no hole          0 violations
//! ```
//!
//! B is sound because its circle does not overlap the rectangles at all. A's
//! does, and comes out WHOLE with a hole punched for the inner rectangle —
//! nothing divided it against the ring's outer boundary, which it also crosses.
//!
//! Take the box away and the same draw divides properly: 2 faces to 10, sound.
//! So the box is not being drawn on; its only role is to trip this, in
//! `face_rederive.rs`:
//!
//! ```text
//!   if !volume_edges.is_empty() && !retile_is_planar {
//!       if region_touches_solid { return Ok(RebuildReport::default()); }
//!   }
//! ```
//!
//! The side rule drops the box's perimeter (its body is on the near side), which
//! leaves `solid_top_boundary` empty, which makes `retile_is_planar` false, and
//! the whole re-derive returns — so the SHEETS are not arranged either. A's
//! violation is sheet-against-sheet: the circle and the inner rectangle. The box
//! is only what silences the arrange.
//!
//! Narrowing that skip is the next piece of work, and it is small: turning the
//! early return off breaks exactly two tests —
//! `a_shape_overlapping_a_drawn_solid_splits_three_ways` and
//! `a_sheet_may_meet_a_solid_along_a_shared_edge`.

//! ── Where downstream it stacks, located ──────────────────────────────────
//!
//! Probed through the whole draw wiring on defect 3's scene, with the
//! re-derive's early return turned off so it runs:
//!
//! ```text
//!   guard_imprint entry              si=4  viol=0
//!   intersect_faces_inner entry      si=7  viol=0
//!   ...re-derive                     si=7  viol=0
//!   ...containment post-process      si=7  viol=0
//!   ...end of rebuild                si=7  viol=0
//!   before subtract_double_covered   si=7  viol=0
//!   AFTER  subtract_double_covered   si=6  viol=3   ← here
//! ```
//!
//! Every stage of the rebuild leaves the plane clean by both instruments. The
//! post-draw repair in `guard_imprint` fixes one overlap and makes three
//! violations — it rebuilds a face wound against the draw. That is the step, and
//! it is the same function session 9's rollback went into; that rollback only
//! fires when a face that bounded a solid stops bounding one, which this does
//! not do.
//!
//! ── The fix that follows, measured, and its cost ─────────────────────────
//!
//! Two rollbacks, each reading the instrument that can see its own damage:
//! the re-derive undone when self-intersections rise, the repair undone when
//! invariant violations rise. Measured with both in place:
//!
//! ```text
//!   defect 3                                    ok
//!   session 10's four-operation repro           SOUND (was stacked)
//!   the two solid-sharing guards                ok
//!   axia-core --lib                             467 passed, 0 failed
//!   every other scene suite                     ok
//! ```
//!
//! And the cost, which is why it is not in this commit: the fuzz says **session
//! 3 was sound and is not any more**. That is the harness working — a newly
//! broken session is a regression, not an inventory item, and it wants reducing
//! and pinning the way the other four did before this can land.
//!
//! ── The comparison this file started as ──────────────────────────────────
//!
use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const TOP: f64 = 100.0;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// Session 10's reduced break: a box that merely SITS on the plane.
fn scene_a() -> Scene {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });
    s.mesh
        .create_box(DVec3::new(-50.0, 50.0, TOP + 60.0), 180.0, 120.0, 180.0, FORM_MATERIAL)
        .expect("box");
    s
}

/// Defect 3: the same start, but the inner rectangle is pushed into a solid.
fn scene_b() -> Scene {
    let mut s = scene_a();
    s.execute(Command::CreateSolid {
        face_id: FaceId::new(10),
        mode: CreateSolidMode::Extrude { distance: 50.0 },
    });
    s
}

/// Each scene's OWN final draw. They are not the same circle, and using one for
/// both is the first thing this comparison got wrong: session 10's is a big one
/// centred on the rectangles, defect 3's is a small one off to the side.
fn draw_the_circle(s: &mut Scene, center: DVec3, radius: f64) {
    s.execute(Command::DrawCircleAsCurve { center, normal: DVec3::Z, radius });
}

fn describe(s: &Scene, f: FaceId) -> String {
    let Some(face) = s.mesh.faces.get(f) else { return format!("{f:?}(gone)") };
    let vv = s.mesh.collect_loop_verts(face.outer().start).map(|v| v.len()).unwrap_or(0);
    let z = s
        .mesh
        .collect_loop_verts(face.outer().start)
        .ok()
        .and_then(|v| v.first().and_then(|x| s.mesh.verts.get(*x)).map(|p| p.pos().z));
    format!(
        "{f:?}(area={:8.0} verts={vv:>2} holes={} {} z={:?})",
        s.mesh.face_outer_area(f),
        face.inners().len(),
        if s.mesh.is_face_in_volume(f) { "solid" } else { "sheet" },
        z.map(|v| format!("{v:.0}"))
    )
}

/// Both scenes, through the draw, reported face by face.
#[test]
fn what_each_draw_makes() {
    let cases: Vec<(&str, Scene, DVec3, f64)> = vec![
        ("A 세션 10 — 상자만 놓임", scene_a(), DVec3::new(0.0, 100.0, TOP), 110.0),
        ("B 결함 3 — 밀어 세운 솔리드도 있음", scene_b(), DVec3::new(0.0, -50.0, TOP), 70.0),
    ];
    let mut results: Vec<(&str, usize, usize)> = Vec::new();
    for (name, mut s, c, r) in cases {
        let before: Vec<FaceId> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(i, _)| i)
            .collect();
        println!("\n═══ {name} ═══\n  그리기 전 {}면", before.len());
        for f in &before {
            println!("    {}", describe(&s, *f));
        }

        draw_the_circle(&mut s, c, r);

        let after: Vec<FaceId> = s
            .mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(i, _)| i)
            .collect();
        println!("  그린 뒤 {}면", after.len());
        for f in &after {
            let tag = if before.contains(f) { "   " } else { "NEW" };
            println!("    {tag} {}", describe(&s, *f));
            if let Some(face) = s.mesh.faces.get(*f) {
                for (k, inner) in face.inners().iter().enumerate() {
                    let n = s.mesh.collect_loop_verts(inner.start).map(|v| v.len()).unwrap_or(0);
                    let pts: Vec<DVec3> = s
                        .mesh
                        .collect_loop_verts(inner.start)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|v| s.mesh.verts.get(*v).map(|p| p.pos()))
                        .collect();
                    // Shoelace in the plane the loop lies on (all z equal here).
                    let mut a2 = 0.0;
                    for i in 0..pts.len() {
                        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                        a2 += p.x * q.y - q.x * p.y;
                    }
                    println!("          구멍 {k}: verts={n} area={:.0}", (a2 / 2.0).abs());
                }
            }
        }
        for f in &before {
            if !after.contains(f) {
                println!("    GONE {f:?}");
            }
        }
        let v: Vec<String> = s
            .mesh
            .verify_face_invariants()
            .violations
            .iter()
            .map(|x| format!("{x:?}"))
            .collect();
        println!("  위반 {}건", v.len());
        for t in v.iter().take(3) {
            println!("    ✗ {t}");
        }
        results.push((name, after.len() - before.len(), v.len()));
    }

    // The measured difference, pinned. One new face each — the circle survives
    // whole in both — and only the one whose circle overlaps the rectangles
    // leaves anything stacked.
    assert_eq!(results[0].1, 1, "A adds one face: the circle, undivided");
    assert_eq!(results[1].1, 1, "B adds one face too");
    assert!(results[0].2 > 0, "A stacks — its circle overlaps the rectangles");
    assert_eq!(results[1].2, 0, "B does not — its circle is off to the side");
}

/// No solid at all: a ring-shaped sheet and a circle drawn over it.
///
/// The comparison above says the two scenes differ in something simpler than
/// the side rule. In A the circle overlaps the ring and comes out WHOLE with one
/// hole — the inner rectangle's, area 7,500 — and nothing divided it against the
/// ring's OUTER boundary, which it also crosses. In B the circle is off to the
/// side and overlaps neither.
///
/// So the box may be beside the point. Three operations, no solid, no push.
#[test]
fn a_circle_over_a_ring_shaped_sheet() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });
    let before: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .collect();
    println!("\n  링 + 안쪽 면 {}개", before.len());
    for f in &before {
        println!("    {}", describe(&s, *f));
    }

    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        radius: 110.0,
    });

    let after: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .collect();
    println!("  그린 뒤 {}면", after.len());
    for f in &after {
        println!(
            "    {} {}",
            if before.contains(f) { "   " } else { "NEW" },
            describe(&s, *f)
        );
    }
    let v: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    println!("  위반 {}건", v.len());
    for t in v.iter().take(3) {
        println!("    ✗ {t}");
    }

    // This is the finding. The SAME draw that leaves the circle whole when a box
    // shares the plane divides it properly when no box does — and the box is not
    // being drawn on. Its only role is to make the re-derive return early.
    assert!(
        after.len() > 5,
        "without a solid on the plane the circle is divided against the ring:          {} faces",
        after.len()
    );
    assert!(v.is_empty(), "and nothing is left stacked: {v:?}");
}

/// A fourth attempt, and what it measured.
///
/// The comparison above localises the break to one early return: the side rule
/// drops a near-side solid's perimeter, `solid_top_boundary` goes empty, and
/// `rebuild_coplanar_faces_analytic_scoped` returns without arranging anything —
/// so sheets that merely share the plane with a box are left overlapping.
///
/// The obvious narrowing is to separate two things that set is doing at once:
/// an edge in it is FED to the arrange *and* its face is removed and re-derived.
/// Telling the arrange where a solid's face is does not require re-tiling it, so
/// feed every on-plane volume edge, keep `solid_top_boundary` for the removal
/// decision alone, and let the return fire only when the arrange would be
/// working blind.
///
/// Measured: **the two are not separable that way**. Feeding a boundary is how
/// the arrange is told to rebuild along it, so handing it the solid's perimeter
/// makes it produce faces there — seven tests in the scene suite, plus defect 3,
/// the two-rim carry and the second-solid guard. Reverted.
///
/// So the next attempt is not another way to choose what to feed. It is either
/// to make the arrange able to receive a boundary it must respect but not
/// rebuild, or to let the re-derive run and undo it when it covers a preserved
/// solid face — the shape of the repair rollback in `guard_imprint`.
///
/// This test holds the boundary of the finding: the early return is what
/// silences the arrange, and it fires exactly when a solid shares the plane.
#[test]
fn the_early_return_is_what_silences_it() {
    // With a box on the plane: the circle survives whole.
    let mut with_box = scene_a();
    with_box.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        radius: 110.0,
    });
    let a = with_box.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();

    // Without one: the same draw divides.
    let mut no_box = prod();
    no_box.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 100.0, height: 75.0,
    });
    no_box.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z, up: DVec3::X, width: 180.0, height: 135.0,
    });
    no_box.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        radius: 110.0,
    });
    let b = no_box.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();

    println!("\n  상자 있음 {a}면, 상자 없음 {b}면");
    assert!(
        b > a,
        "the same draw makes MORE faces when no solid shares the plane — the          box is not being drawn on, it is silencing the arrange"
    );
}

/// The fifth attempt: run the re-derive and undo it if the plane came out
/// worse. Measured at two depths, and refused at both.
///
/// The comparison says neither answer — declining or running — is right for both
/// scenes, and that the input cannot tell them apart. The result can, so: let it
/// run, compare the plane before and after, put it back if it is worse. Two
/// instruments, because each damage is invisible to the other's — faces covering
/// each other (`detect_self_intersections`) for a shape tiled across a solid,
/// invariant violations for tiles wound against the draw.
///
/// ```text
///                                        session 10     defect 3
///   around the re-derive itself          FIXED          back
///   after the containment post-process   FIXED          back
/// ```
///
/// So in defect 3's scene the damage does not exist yet at either point. The
/// re-derive leaves the plane clean by both instruments, the containment
/// post-process leaves it clean, and the stacked faces appear somewhere further
/// down — the XIA and shape reconciles, the auto-intersect, or the repair in
/// `guard_imprint`, which has a rollback of its own that restores when a solid
/// opens and could be undoing a repair that would have fixed this.
///
/// That is where the next attempt starts: not another place to put a rollback,
/// but finding which downstream step turns a clean plane into a stacked one.
///
/// This test holds the ground the attempts have to keep: both scenes as they
/// stand today, so a fix that trades one for the other fails here rather than
/// looking like progress.
#[test]
fn neither_scene_may_be_traded_for_the_other() {
    // Session 10's reduced break: still stacks, and is inventoried as such.
    let mut a = scene_a();
    a.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 100.0, TOP),
        normal: DVec3::Z,
        radius: 110.0,
    });
    let a_bad = a.mesh.verify_face_invariants().violations.len();

    // Defect 3: fixed, and must stay fixed.
    let mut b = scene_b();
    b.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, -50.0, TOP),
        normal: DVec3::Z,
        radius: 70.0,
    });
    let b_bad = b.mesh.verify_face_invariants().violations.len();

    println!("\n  세션 10 위반 {a_bad}, 결함 3 위반 {b_bad}");
    assert!(a_bad > 0, "session 10 still stacks — if it stops, say what fixed it");
    assert_eq!(b_bad, 0, "defect 3 must stay fixed; trading it away is not progress");
}
