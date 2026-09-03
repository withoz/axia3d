//! WHAT OVERLAPPING DRAWS LEAVE ON THE GROUND, AND WHETHER A PUSH CAN FOLLOW.
//!
//! Reported 2026-09-01 from the running app: after drawing several overlapping
//! circles and a rectangle on the ground and pushing one of the pieces, the
//! console said
//!
//! ```text
//!   create_solid_extrude REJECTED by integrity gate:
//!     invariants: 3 violation(s)
//!       - edge EdgeId(85): shared by 4 active faces (non-manifold)
//!         — FaceId(..) / FaceId(31) cover the same ground (stacked)
//!     geometric cracks: 37 edge(s)
//! ```
//!
//! and the shape on screen had a seam across it. Measured in that live scene:
//! 22 faces, `is_closed_solid = false`, one non-manifold stacked pair, and
//! **44 self-intersecting face pairs**.
//!
//! The gate did its job — it refused an extrude on a mesh that was already
//! broken. This file asks the question the gate cannot: **what broke it.**
//!
//! ## 2026-09-02 — the filter is not the lever, and here is why
//!
//! `face_rederive`'s on-plane volume-edge filter keeps an edge when
//! `!saw_a_wall || reaches_below`, so a solid STANDING on the plane (walls only
//! above) has its footprint dropped. Widening that does fix this file: the three
//! damaging contacts below go to **0**. It was measured three ways and all three
//! ended in the same place.
//!
//! ```text
//!   keep every on-plane volume edge        bug fixed; gate 12x20 breaks 3
//!   keep it when the plane also holds an   bug fixed; gate breaks 1
//!     on-plane face of that solid
//!   ...and stand down when the plane also  bug fixed; gate GREEN
//!     cuts a solid BELOW it
//! ```
//!
//! The third looked like the answer — gate green, `KNOWN_BREAKS` still empty,
//! both clauses mutation-checked and each caught by a different guard. Then the
//! full suite: **three pinned scenes gained stacked pairs**
//! (`a_vertex_whose_outgoing_half_edge_is_gone` twice, and
//! `the_fifty_operation_inventory`'s MoveOnly push, 0 -> 6). Same class as the
//! defect. Traded, not fixed, so it was reverted.
//!
//! ⚠ **The structural reason, which is the part worth keeping.** The filter does
//! not decide what to imprint — it decides whether the solid's own on-plane face
//! stays PROTECTED. `face_rederive.rs`'s `part_of_solid && solid_top_boundary
//! .is_empty()` skips it, so making the boundary non-empty un-protects it and the
//! face is removed and re-tiled (ADR-281 beta-1, written for a solid's TOP). The
//! new clause fires only when an adjacent face lies wholly in the plane — the
//! solid's bottom — so **every time it fires, something already covers that
//! ground**, and the re-tile has to reproduce it exactly or leave a stack.
//!
//! And it cannot be guarded on the way out: the rollback branches in
//! `guard_imprint` were removed by 사용자 결정 2026-08-06, *A DRAW IS NEVER
//! REFUSED*. Rolling a draw back for a stacked pair is the thing that decision
//! forbids.
//!
//! So the fix is not in this predicate. It is in the re-tile producing a tiling
//! that covers the footprint exactly once — or in the repair being able to clear
//! what it leaves, which the three tests below measure it cannot.
//!
//! ⚠ One more thing the same session measured, because it changes how the fuzz
//! gate should be read: run the SAME 12 gate seeds ten operations deeper on
//! today's main and **session 6 fails at op 20 and session 9 at op 25**, both
//! "cover the same ground (stacked)". The gate is green because it stops at op
//! 20, not because the class is absent. Across two independent seed ranges (98
//! sessions, 12..109) narrow and widened failed **4 and 4** — the same
//! population, differing only in which seeds reach it.
//!
//! Flags are the production ones (ADR-176 / face-rederive), not `Scene::new()`
//! defaults, because that is the configuration the report came from.
//!
//! ## 2026-09-03 — where the repair actually stops, and why the obvious lever is barred
//!
//! `where_each_standing_pair_falls_out_of_the_repair` reports two of the three
//! as `Err(coplanar clipping requires convex faces)`. That is honest, not an
//! instrument fault: the corner cross products were read directly and the
//! refused faces really are reflex —
//!
//! ```text
//!   FaceId( 2) verts 23  solid true    corner signs +++++++++++++++++++++++
//!   FaceId(30) verts  4  solid false   corner signs +-++     worst -1670.5
//!   FaceId(32) verts  7  solid false   corner signs -+++++-  worst -3579.9
//! ```
//!
//! Sutherland-Hodgman needs a convex CLIP, and an arrangement's leftovers are
//! crescents. `the_same_pairs_with_the_sheet_as_subject_and_the_solid_as_clip`
//! already measured that swapping the roles makes every pair readable — the
//! solid's cap is the convex one.
//!
//! Two levers were built from that and both were reverted:
//!
//!   - **Sheet as subject by rule.** Fixes nothing here and BREAKS a case that
//!     worked: a rect's draw stopped clearing the six standing contacts it used
//!     to clear (`drawing_again_after_a_push_is_where_faces_stack`, 6 -> 0).
//!   - **Try the other order when the first errors.** Safe — everything from
//!     the intersection call to the first `add_vertex` is read-only — and it
//!     earns nothing: axia-core 826/826 identical with and without it, no test
//!     differing. Nothing reaches it, because the pair that fails, fails LATER.
//!
//! ⚠ Where it actually stops, measured by instrumenting the gates:
//!
//! ```text
//!   [PROBE] diff walk Err FaceId(2)/FaceId(28):
//!           polygon_difference_by_clip: clip polygon has fewer than 3 vertices
//! ```
//!
//! The clip is a kernel-native closed curve — ADR-089 Path B, one anchor vertex
//! and one self-loop edge — so there is no polygon to walk. That is exactly what
//! this file's own note says, and the only way to give it one is
//! `polygonize_closed_curve_face`, which turns the user's circle into a polygon.
//! ADR-189 moved AWAY from that by 사용자 결재 ("다각형화 제거 + 자동 분할
//! 유지"), so reaching for it here would walk a decision back.
//!
//! So the repair is not the lever either. Between this and
//! `face_rederive`'s filter (see the note above), both obvious levers are
//! measured and barred, and what is left is the one neither touches: the
//! arrangement producing a tiling that covers the footprint exactly once.

use axia_core::{Command, CommandResult, Scene};
use glam::DVec3;

fn production() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn circle(s: &mut Scene, x: f64, y: f64, r: f64) -> CommandResult {
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(x, y, 0.0),
        normal: DVec3::Z,
        radius: r,
    })
}

fn rect(s: &mut Scene, cx: f64, cy: f64, w: f64, h: f64) -> CommandResult {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(cx, cy, 0.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: w,
        height: h,
    })
}

/// faces, invariant violations, self-intersecting pairs.
fn health(s: &Scene) -> (usize, usize, usize) {
    let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let inv = s.mesh.verify_face_invariants().violations.len();
    let si = s.mesh.detect_self_intersections().intersecting_pairs.len();
    (faces, inv, si)
}

#[test]
fn overlapping_circles_and_a_rect_on_the_ground() {
    let mut s = production();

    // Roughly the reported arrangement: two large circles that cross, a
    // smaller one over both, and a rectangle laid across them.
    circle(&mut s, 0.0, 0.0, 100.0);
    let (f1, i1, x1) = health(&s);
    circle(&mut s, 120.0, 0.0, 100.0);
    let (f2, i2, x2) = health(&s);
    circle(&mut s, 60.0, 40.0, 55.0);
    let (f3, i3, x3) = health(&s);
    rect(&mut s, 60.0, 0.0, 140.0, 90.0);
    let (f4, i4, x4) = health(&s);

    println!("  after circle A   faces {f1:3}  invariant-violations {i1}  self-intersections {x1}");
    println!("  after circle B   faces {f2:3}  invariant-violations {i2}  self-intersections {x2}");
    println!("  after circle C   faces {f3:3}  invariant-violations {i3}  self-intersections {x3}");
    println!("  after rect       faces {f4:3}  invariant-violations {i4}  self-intersections {x4}");

    // A flat arrangement of coplanar shapes has nothing to intersect in 3D and
    // no edge can carry more than two faces. Whatever the drawing leaves, it
    // must at least be readable by the next operation.
    assert_eq!(i4, 0, "drawing alone left {i4} invariant violation(s)");
    assert_eq!(x4, 0, "drawing alone left {x4} self-intersecting face pair(s)");
}

#[test]
fn a_push_after_those_draws() {
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);
    rect(&mut s, 60.0, 0.0, 140.0, 90.0);

    let (before_f, before_i, before_x) = health(&s);
    println!("  before push  faces {before_f}  inv {before_i}  si {before_x}");

    // Push whichever piece is first — the report did not say which, and any of
    // them should be pushable.
    let fid = s
        .mesh
        .faces
        .iter()
        .find(|(_, f)| f.is_active())
        .map(|(id, _)| id)
        .expect("a face to push");

    let r = s.execute(Command::CreateSolid {
        face_id: fid,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    let (after_f, after_i, after_x) = health(&s);
    println!("  push result  {r:?}");
    println!("  after push   faces {after_f}  inv {after_i}  si {after_x}");

    assert!(
        after_i <= before_i,
        "the push added {} invariant violation(s)",
        after_i - before_i
    );
}

#[test]
fn which_faces_the_push_puts_through_each_other() {
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);
    rect(&mut s, 60.0, 0.0, 140.0, 90.0);

    let before: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
    let fid = before[0];
    let _ = s.execute(Command::CreateSolid {
        face_id: fid,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });

    // For each intersecting pair, say whether the face existed before the push
    // (a neighbouring sheet) or was made by it (a wall or a cap), and where it sits.
    // ⚠ `face_tessellation`, not `face_outline_points`: an outline polygon is one
    // of the instruments that misreports a curve-bounded face, and half of this
    // scene is circles.
    let z_of = |s: &Scene, id: axia_geo::FaceId| -> String {
        match s.mesh.face_tessellation(id) {
            Some(tris) if !tris.is_empty() => {
                let zs = tris.iter().flatten().map(|v| v.z);
                let lo = zs.clone().fold(f64::INFINITY, f64::min);
                let hi = zs.fold(f64::NEG_INFINITY, f64::max);
                format!("z {lo:.0}..{hi:.0}")
            }
            _ => "z ?".into(),
        }
    };
    let rep = s.mesh.detect_self_intersections();
    println!("  pushed FaceId({}) ; {} pairs", fid.raw(), rep.intersecting_pairs.len());
    for (a, b) in &rep.intersecting_pairs {
        let oa = if before.contains(a) { "old" } else { "NEW" };
        let ob = if before.contains(b) { "old" } else { "NEW" };
        println!("    {:>3}({}) {:<10}  x  {:>3}({}) {:<10}",
            a.raw(), oa, z_of(&s, *a), b.raw(), ob, z_of(&s, *b));
    }
}

#[test]
fn are_those_contacts_damage_or_a_wall_standing_on_a_floor() {
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);
    rect(&mut s, 60.0, 0.0, 140.0, 90.0);
    let fid = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: fid,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });

    // ⚠ `intersecting_pairs` counts contact; `damaging_contacts` counts damage.
    // ContactKind::Touching is documented "not damage — a wall standing on a
    // floor", which is exactly the shape a push makes beside its neighbours.
    let rep = s.mesh.detect_self_intersections();
    let bad = s.mesh.damaging_contacts();
    println!("  contacts {}  damaging {}", rep.intersecting_pairs.len(), bad.len());
    for (a, b) in &rep.intersecting_pairs {
        println!("    {:>3} x {:>3}  {:?}", a.raw(), b.raw(), s.mesh.classify_contact(*a, *b));
    }
    assert!(bad.is_empty(), "the push left {} damaging contact(s): {:?}", bad.len(), bad);
}

#[test]
fn drawing_again_after_a_push_is_where_faces_stack() {
    // The reported screenshot has a cylinder ALREADY standing among the flat
    // shapes, so the order was not draw-then-push. It was draw, push, and then
    // keep drawing — onto a ground that now has a solid on it.
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    let fid = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: fid,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    let after_push = s.mesh.damaging_contacts().len();
    let inv_push = s.mesh.verify_face_invariants().violations.len();
    println!("  after push        damaging {after_push}  invariants {inv_push}");

    // Now keep drawing on the ground, overlapping where the solid stands.
    circle(&mut s, 120.0, 0.0, 100.0);
    let (a1, i1) = (s.mesh.damaging_contacts().len(), s.mesh.verify_face_invariants().violations.len());
    println!("  + circle B        damaging {a1}  invariants {i1}");
    circle(&mut s, 60.0, 40.0, 55.0);
    let (a2, i2) = (s.mesh.damaging_contacts().len(), s.mesh.verify_face_invariants().violations.len());
    println!("  + circle C        damaging {a2}  invariants {i2}");
    rect(&mut s, 60.0, 0.0, 140.0, 90.0);
    let (a3, i3) = (s.mesh.damaging_contacts().len(), s.mesh.verify_face_invariants().violations.len());
    println!("  + rect            damaging {a3}  invariants {i3}");
    for v in s.mesh.verify_face_invariants().violations.iter().take(4) {
        println!("      {v}");
    }

    // The invariants hold throughout — stacked faces are not a manifold fault —
    // which is why the fuzz gate does not see this and the integrity gate only
    // meets it later, at the push.
    assert_eq!(i3, 0, "drawing beside a solid left {i3} invariant violation(s)");
    // ⚠ Damage rises to 3 and then the rect's draw clears it. The repair fixes
    // the PREVIOUS draw's stacking, never its own, so the mesh is left damaged
    // between draws.
    assert_eq!(a3, 0, "the rect's draw no longer clears what came before");
}

#[test]
fn a_push_that_follows_a_draw_instead_of_another_draw() {
    // The sequence from the report. The repair that clears a draw's stacked
    // faces runs on the NEXT draw — so it never runs if the next thing you do
    // is push.
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    let first = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: first,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);

    let dmg = s.mesh.damaging_contacts();
    println!("  standing damage before the push: {}", dmg.len());
    for (a, b) in dmg.iter().take(4) {
        println!("    {:>3} x {:>3}  {:?}", a.raw(), b.raw(), s.mesh.classify_contact(*a, *b));
    }

    // Push a flat piece that is still on the ground.
    let flat = s.mesh.faces.iter()
        .filter(|(_, f)| f.is_active())
        .find(|(id, _)| s.mesh.face_tessellation(*id)
            .map_or(false, |t| t.iter().flatten().all(|v| v.z.abs() < 1e-6)))
        .map(|(id, _)| id)
        .expect("a flat piece to push");
    let r = s.execute(Command::CreateSolid {
        face_id: flat,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 150.0 },
    });
    println!("  push -> {r:?}");
    println!("  after: damaging {}  invariants {}",
        s.mesh.damaging_contacts().len(),
        s.mesh.verify_face_invariants().violations.len());

    // ⚠ THREE, and that is the defect. A draw over a standing solid's footprint
    // leaves stacked faces behind, and the repair that would clear them runs on
    // the NEXT draw — so it never runs when the next thing is a push.
    //
    // Asserted as measured, not as wanted: when this starts coming back empty,
    // this line is what says so. Tighten it to `dmg.is_empty()` then.
    assert_eq!(dmg.len(), 3, "the standing damage changed: {dmg:?}");
}

#[test]
fn what_the_repair_asks_a_circle_face_for_its_normal() {
    // `subtract_double_covered_faces` decides coplanarity with `Face::normal()`
    // and skips the pair when the dot product is under 0.999. A circle drawn as
    // a curve is ONE anchor vertex and one self-loop edge, so a boundary-walk
    // normal has no polygon to walk.
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    rect(&mut s, 400.0, 0.0, 100.0, 100.0); // far away, untouched, as a control

    for (id, f) in s.mesh.faces.iter().filter(|(_, f)| f.is_active()) {
        let n = f.normal();
        // The tessellation is the instrument that does not lie about a
        // curve-bounded face; use it to say what the normal SHOULD be.
        let tri = s.mesh.face_tessellation(id).and_then(|t| t.first().copied());
        let truth = tri.map(|[a, b, c]| (b - a).cross(c - a).normalize_or_zero());
        println!(
            "  FaceId({:>2})  Face::normal() = ({:.3},{:.3},{:.3})  len {:.3}   tessellation says {:?}",
            id.raw(), n.x, n.y, n.z, n.length(),
            truth.map(|v| format!("({:.3},{:.3},{:.3})", v.x, v.y, v.z))
        );
    }
}

/// The scene the report came from: a solid standing on the ground, then more
/// drawing over its footprint.
fn solid_then_draw_over_it() -> Scene {
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    let first = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: first,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);
    s
}

fn solid_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(fid, f)| f.is_active() && s.mesh.is_face_in_volume(*fid)).count()
}

#[test]
fn a_draw_over_a_solids_footprint_leaves_nothing_stacked() {
    // BOTH halves of this are the contract, and the second is why the repair
    // used to decline: rebuilding the face a solid's walls stand on takes their
    // edges away and the solid opens. Repairing by opening the solid would be
    // worse than the overlap, so the fix is not "repair harder" — it is to make
    // the SHEET the side that gives up the region, since a sheet has no walls
    // to lose.
    let mut s = solid_then_draw_over_it();
    let solids_before = solid_faces(&s);

    let n = s.subtract_double_covered_faces(&std::collections::HashSet::new());
    let dmg = s.mesh.damaging_contacts();
    println!("  repair fixed {n}; damage left {}; solid faces {} -> {}",
        dmg.len(), solids_before, solid_faces(&s));
    for (a, b) in dmg.iter() {
        println!("    {:>3} x {:>3}  {:?}", a.raw(), b.raw(), s.mesh.classify_contact(*a, *b));
    }

    // ⚠ Zero fixed, three left — measured, and the second assertion is why.
    // Two of the three fail the clipper outright (the crescents an arrangement
    // leaves are non-convex, and a non-convex CLIP is refused), and the third is
    // solid × solid, where rebuilding either side takes away the edges the walls
    // stand on, so the whole pass rolls back.
    //
    // The second line is the one that must never loosen. Opening the solid to
    // clear an overlap is a worse answer than the overlap.
    assert_eq!(dmg.len(), 3, "the standing damage changed: {dmg:?}");
    assert_eq!(solid_faces(&s), solids_before, "the repair opened the solid");
}

#[test]
fn can_the_repair_clear_it_when_asked_directly() {
    let mut s = production();
    circle(&mut s, 0.0, 0.0, 100.0);
    let first = s.mesh.faces.iter().find(|(_, f)| f.is_active()).map(|(id, _)| id).unwrap();
    let _ = s.execute(Command::CreateSolid {
        face_id: first,
        mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    circle(&mut s, 120.0, 0.0, 100.0);
    circle(&mut s, 60.0, 40.0, 55.0);

    let before = s.mesh.damaging_contacts();
    println!("  standing damage: {}", before.len());
    for (a, b) in before.iter() {
        let (na, nb) = (s.mesh.faces[*a].normal(), s.mesh.faces[*b].normal());
        println!("    {:>3} x {:>3}  dot {:.4}  solid? {} / {}",
            a.raw(), b.raw(),
            na.normalize_or_zero().dot(nb.normalize_or_zero()).abs(),
            s.mesh.is_face_in_volume(*a), s.mesh.is_face_in_volume(*b));
    }

    // The doc says: pass an empty set and it repairs them all.
    let n = s.subtract_double_covered_faces(&std::collections::HashSet::new());
    let after = s.mesh.damaging_contacts();
    println!("  repair reported {n} fix(es); damage now {}", after.len());

    // Even asked directly, with nothing excluded — the doc says an empty set
    // "repairs them all" — it fixes none of these.
    assert_eq!(after.len(), 3, "asked directly, the outcome changed: {after:?}");
}

#[test]
fn why_the_repair_cannot_express_the_fix() {
    // The repair's own note: "a closed curve is a single anchor vertex, so the
    // containment punch and the difference walk both have no polygon to work
    // with". Count the boundary of each face in the standing damage and see
    // whether that is what is happening here.
    let s = solid_then_draw_over_it();
    let vcount = |f: axia_geo::FaceId| {
        s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).map(|v| v.len()).unwrap_or(0)
    };
    for (a, b) in s.mesh.damaging_contacts() {
        println!(
            "    {:>3} (verts {}, solid {})  x  {:>3} (verts {}, solid {})",
            a.raw(), vcount(a), s.mesh.is_face_in_volume(a),
            b.raw(), vcount(b), s.mesh.is_face_in_volume(b),
        );
    }
}

#[test]
fn where_each_standing_pair_falls_out_of_the_repair() {
    // The straddling walk needs `crossings >= 2 && even`; below that only the
    // containment punch can act, and it needs the small one genuinely inside
    // the big one. Measure which case each standing pair actually is.
    use axia_geo::operations::coplanar as cop;
    let s = solid_then_draw_over_it();
    for (a, b) in s.mesh.damaging_contacts() {
        let big_first = s.mesh.face_outer_area(a) >= s.mesh.face_outer_area(b);
        let (big, small) = if big_first { (a, b) } else { (b, a) };
        match cop::coplanar_intersection_segments(&s.mesh, big, small) {
            Ok(ci) => println!(
                "    {:>3} x {:>3}   crossings {}   lens pts {}   areas {:.0} / {:.0}",
                big.raw(), small.raw(), ci.crossings.len(), ci.lens_polygon.len(),
                s.mesh.face_outer_area(big), s.mesh.face_outer_area(small)),
            Err(e) => println!("    {:>3} x {:>3}   Err({e})", big.raw(), small.raw()),
        }
    }
}

#[test]
fn the_same_pairs_with_the_sheet_as_subject_and_the_solid_as_clip() {
    // LOCKED #105: the clipper needs the CLIP convex; a concave SUBJECT is fine.
    // The solid's cap is a polygonised circle and convex; the sheets left by the
    // arrangement are crescents and are not. So the ordering that fails is the
    // one that hands the crescent in as the clip.
    use axia_geo::operations::coplanar as cop;
    let s = solid_then_draw_over_it();
    for (a, b) in s.mesh.damaging_contacts() {
        let (solid, sheet) = if s.mesh.is_face_in_volume(a) { (a, b) } else { (b, a) };
        for (label, subject, clip) in [("solid-as-subject", solid, sheet), ("SHEET-as-subject", sheet, solid)] {
            match cop::coplanar_intersection_segments(&s.mesh, subject, clip) {
                Ok(ci) => println!("    {:>3} x {:>3}  {label:<16} crossings {}  lens {}",
                    subject.raw(), clip.raw(), ci.crossings.len(), ci.lens_polygon.len()),
                Err(e) => println!("    {:>3} x {:>3}  {label:<16} Err({})",
                    subject.raw(), clip.raw(), e.to_string().chars().take(58).collect::<String>()),
            }
        }
    }
}

#[test]
fn what_surfaces_sit_on_the_solids_ground_boundary() {
    // ADR-281 β-1's re-tile is the imprint that would fix this: feed the solid's
    // on-plane perimeter to the arrange as INPUT and do not remove it, so the
    // walls keep the edges they stand on. It is gated on `retile_is_planar`,
    // which requires every face touching that boundary to be Plane or None.
    //
    // A solid made by pushing a CIRCLE has a Cylinder wall. Measure whether that
    // is what closes the gate here.
    let s = solid_then_draw_over_it();
    let kind = |f: axia_geo::FaceId| -> String {
        match s.mesh.faces.get(f).and_then(|x| x.surface()) {
            None => "None".into(),
            Some(su) => format!("{:?}", std::mem::discriminant(su)),
        }
    };
    // the ground-plane faces of the solid, and what shares their edges
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() || !s.mesh.is_face_in_volume(fid) { continue; }
        let flat = s.mesh.face_tessellation(fid)
            .map_or(false, |t| t.iter().flatten().all(|v| v.z.abs() < 1e-6));
        if !flat { continue; }
        println!("  solid ground face {:>3}  surface {}", fid.raw(), kind(fid));
        if let Ok(edges) = s.mesh.face_outer_edges(fid) {
            let mut seen = std::collections::BTreeSet::new();
            for e in edges.iter().take(40) {
                let (faces, _) = s.mesh.get_faces_sharing_edge(*e);
                for nf in faces { if nf != fid { seen.insert((nf.raw(), kind(nf))); } }
            }
            for (r, k) in seen.iter().take(8) { println!("      neighbour {r:>3}  surface {k}"); }
        }
    }
}

/// The two predicates, simulated on both shapes, WITHOUT touching the engine.
///
/// `retile_is_planar` today asks: is every face touching the solid's on-plane
/// boundary planar? A cylinder's cap fails it because the WALLS are curved,
/// even though the wall is not re-tiled — it only stands on the rim, and
/// keeping its edges is exactly what ADR-281 β-1's re-tile is for.
///
/// The proposed question is narrower: are the faces that LIE IN the plane —
/// the ones a planar re-tile would actually rebuild — planar, and is there at
/// least one? That must stay false for a sphere, whose equator bounds two
/// curved faces and no planar one; re-tiling there deleted both (2026-08-06).
#[test]
fn simulate_both_predicates_on_a_cylinder_and_on_a_sphere() {
    let probe = |s: &Scene, label: &str| {
        let plane_z = 0.0;
        let in_plane = |f: axia_geo::FaceId| {
            s.mesh.face_tessellation(f)
                .map_or(false, |t| !t.is_empty() && t.iter().flatten().all(|v| (v.z - plane_z).abs() < 1e-6))
        };
        let planar_surface = |f: axia_geo::FaceId| {
            match s.mesh.faces.get(f).and_then(|x| x.surface()) {
                None => true,
                Some(su) => matches!(su, axia_geo::surfaces::AnalyticSurface::Plane { .. }),
            }
        };
        // on-plane edges of volume faces
        let mut onp: Vec<axia_geo::EdgeId> = Vec::new();
        for (fid, f) in s.mesh.faces.iter() {
            if !f.is_active() || !s.mesh.is_face_in_volume(fid) { continue; }
            if let Ok(es) = s.mesh.face_outer_edges(fid) {
                for e in es {
                    if let Some(ed) = s.mesh.edges.get(e) {
                        let (a, b) = (s.mesh.vertex_pos(ed.v_small()), s.mesh.vertex_pos(ed.v_large()));
                        if let (Ok(a), Ok(b)) = (a, b) {
                            if (a.z - plane_z).abs() < 1e-6 && (b.z - plane_z).abs() < 1e-6 && !onp.contains(&e) { onp.push(e); }
                        }
                    }
                }
            }
        }
        let old = !onp.is_empty() && onp.iter().all(|&e| {
            let (fs, _) = s.mesh.get_faces_sharing_edge(e);
            fs.iter().all(|&f| s.mesh.faces.get(f).map_or(true, |x| !x.is_active() || planar_surface(f)))
        });
        let new = !onp.is_empty() && onp.iter().all(|&e| {
            let (fs, _) = s.mesh.get_faces_sharing_edge(e);
            let ip: Vec<_> = fs.iter().copied()
                .filter(|&f| s.mesh.faces.get(f).map_or(false, |x| x.is_active()) && in_plane(f))
                .collect();
            !ip.is_empty() && ip.iter().all(|&f| planar_surface(f))
        });
        println!("  {label:<22} on-plane solid edges {:>3}   old {old:<5}  new {new}", onp.len());
    };

    // a) the reported shape: a circle pushed into a cylinder
    let mut cyl = production();
    circle(&mut cyl, 0.0, 0.0, 100.0);
    let f = cyl.mesh.faces.iter().find(|(_, x)| x.is_active()).map(|(id, _)| id).unwrap();
    let _ = cyl.execute(Command::CreateSolid {
        face_id: f, mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    probe(&cyl, "cylinder (cap+walls)");

    // b) the shape the guard exists for: a sphere sitting on the plane, its
    //    equator an on-plane closed curve between two curved faces.
    let mut sph = production();
    let _ = sph.mesh.create_sphere(glam::DVec3::ZERO, 100.0, 24, 12, axia_core::FORM_MATERIAL);
    probe(&sph, "sphere (two caps)");
}

/// `onp_ve`'s filter, simulated, before the engine is touched.
///
/// The engine keeps an on-plane volume edge when `!saw_a_wall || reaches_below`.
/// `reaches_below` asks whether an adjacent face has a vertex on the NEGATIVE
/// side of the plane — which is true when the plane is a solid's TOP, and false
/// when the solid stands ON the plane, because then its walls go up.
///
/// That is why a draw beside a standing cylinder never reaches the re-tile:
/// measured 2026-09-01, `onp_ve` came out 0 on every draw.
#[test]
fn simulate_widening_the_on_plane_volume_edge_filter() {
    let probe = |s: &Scene, plane_z: f64, label: &str| {
        let on_plane = |p: glam::DVec3| (p.z - plane_z).abs() < 1e-6;
        let (mut kept_now, mut kept_widened, mut total) = (0usize, 0usize, 0usize);
        for (fid, f) in s.mesh.faces.iter() {
            if !f.is_active() || !s.mesh.is_face_in_volume(fid) { continue; }
            let Ok(edges) = s.mesh.face_outer_edges(fid) else { continue };
            for e in edges {
                let Some(ed) = s.mesh.edges.get(e) else { continue };
                let (Ok(a), Ok(b)) = (s.mesh.vertex_pos(ed.v_small()), s.mesh.vertex_pos(ed.v_large()))
                    else { continue };
                if !(on_plane(a) && on_plane(b)) { continue; }
                total += 1;
                let (adj, _) = s.mesh.get_faces_sharing_edge(e);
                let (mut saw_a_wall, mut below, mut above) = (false, false, false);
                for nf in adj {
                    let Some(face) = s.mesh.faces.get(nf) else { continue };
                    if !face.is_active() { continue; }
                    let Ok(vv) = s.mesh.collect_loop_verts(face.outer().start) else { continue };
                    for vid in vv {
                        if let Ok(p) = s.mesh.vertex_pos(vid) {
                            if !on_plane(p) {
                                saw_a_wall = true;
                                if p.z < plane_z { below = true; } else { above = true; }
                            }
                        }
                    }
                }
                if !saw_a_wall || below { kept_now += 1; }
                if !saw_a_wall || below || above { kept_widened += 1; }
            }
        }
        println!("  {label:<32} on-plane {total:>3}   kept now {kept_now:>3}   kept widened {kept_widened:>3}");
    };

    // 1) the reported case — a cylinder standing ON the plane
    let mut cyl = production();
    circle(&mut cyl, 0.0, 0.0, 100.0);
    let f = cyl.mesh.faces.iter().find(|(_, x)| x.is_active()).map(|(id, _)| id).unwrap();
    let _ = cyl.execute(Command::CreateSolid {
        face_id: f, mode: axia_geo::CreateSolidMode::Extrude { distance: 200.0 },
    });
    probe(&cyl, 0.0, "cylinder on the plane (z=0)");

    // 2) the case this filter was built for — the same solid's TOP
    probe(&cyl, 200.0, "the same solid's top (z=200)");

    // 3) the case the `!saw_a_wall` arm exists for
    let mut sph = production();
    let _ = sph.mesh.create_sphere(glam::DVec3::ZERO, 100.0, 24, 12, axia_core::FORM_MATERIAL);
    probe(&sph, 0.0, "sphere through its middle");

    // 4) a plain box, both ways
    let mut bx = production();
    let _ = bx.mesh.create_box(glam::DVec3::new(0.0, 0.0, 100.0), 200.0, 200.0, 200.0, axia_core::FORM_MATERIAL);
    probe(&bx, 0.0, "box sitting on z=0");
    probe(&bx, 200.0, "box's own top (z=200)");
}
