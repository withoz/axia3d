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
//! Flags are the production ones (ADR-176 / face-rederive), not `Scene::new()`
//! defaults, because that is the configuration the report came from.
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

    assert_eq!(i3, 0, "drawing beside a solid left {i3} invariant violation(s)");
    assert_eq!(a3, 0, "drawing beside a solid left {a3} damaging contact(s)");
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

    assert!(dmg.is_empty(), "a draw left {} damaging contact(s) standing for the next op", dmg.len());
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

    assert!(dmg.is_empty(), "left {} stacked pair(s)", dmg.len());
    assert_eq!(solid_faces(&s), solids_before, "the repair opened the solid");
    assert_eq!(s.mesh.verify_face_invariants().violations.len(), 0);
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

    assert!(after.is_empty(), "asked directly, the repair still left {}", after.len());
}
