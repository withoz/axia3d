//! DRAWING A SECOND CIRCLE THAT OVERLAPS THE FIRST, ON A SOLID.
//!
//! Measured 2026-08-05, in this order:
//!
//! ```text
//!   sheet   two circles that cross -> 3 faces, rims still arcs
//!   solid   two circles apart      -> 2 holes in the cap, accepted
//!   solid   two circles that cross -> REFUSED, "도형이 기존 면과 같은 자리를 덮습니다"
//! ```
//!
//! On a sheet the coplanar re-derive hands both curves to the analytic
//! arrangement, which handles circle × circle exactly. On a solid that re-derive
//! is deliberately skipped — re-arranging a region a solid stands on re-cuts the
//! edges its walls are on — so both circles became holes of the cap instead, and
//! two hole loops that overlap each other are not something earcut, the overlap
//! detector, or either repair can make sense of.
//!
//! What this file pins is the user-facing half: the draw survives, the cap ends
//! up with one hole, the solid is still closed, and the three regions are things
//! the user owns and can push. The geometry itself — the four arcs, the areas —
//! is pinned in axia-geo's `two_overlapping_circles_become_three`.
use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use axia_geo::curves::AnalyticCurve;
use axia_geo::FaceId;
use glam::DVec3;

const R: f64 = 40.0;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn box_scene() -> Scene {
    let mut s = prod();
    let f = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
    s
}

fn circle(s: &mut Scene, x: f64) -> CommandResult {
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(x, 0.0, 100.0),
        normal: DVec3::Z,
        radius: R,
    })
}

fn active(s: &Scene) -> Vec<FaceId> {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(fid, _)| fid).collect()
}

/// Faces lying on the top plane, apart from the cap itself.
fn on_top(s: &Scene) -> Vec<FaceId> {
    active(s)
        .into_iter()
        .filter(|&f| {
            s.mesh.faces[f].normal().normalize_or_zero().z > 0.9
                && s.mesh
                    .collect_loop_verts(s.mesh.faces[f].outer().start)
                    .map(|vs| {
                        vs.iter().all(|&v| {
                            s.mesh.vertex_pos(v).map_or(false, |p| (p.z - 100.0).abs() < 1e-6)
                        })
                    })
                    .unwrap_or(false)
        })
        .collect()
}

fn all_arcs(s: &Scene, start: axia_geo::HeId) -> bool {
    let hes = s.mesh.collect_loop_hes(start).expect("a loop");
    !hes.is_empty()
        && hes.iter().all(|&he| {
            let e = s.mesh.hes[he].edge();
            matches!(s.mesh.edges.get(e).and_then(|x| x.curve()), Some(AnalyticCurve::Arc { .. }))
        })
}

#[test]
fn the_second_circle_is_accepted_and_the_solid_stays_closed() {
    let mut s = box_scene();
    assert!(!matches!(circle(&mut s, -R / 2.0), CommandResult::Error(_)));
    let second = circle(&mut s, R / 2.0);
    assert!(!matches!(second, CommandResult::Error(_)), "second circle: {second:?}");

    // The cap, plus a lens and two crescents.
    let top = on_top(&s);
    assert_eq!(top.len(), 4, "cap + three regions, got {}", top.len());
    let cap = *top
        .iter()
        .find(|&&f| !s.mesh.faces[f].inners().is_empty())
        .expect("the cap holds the hole");
    assert_eq!(s.mesh.faces[cap].inners().len(), 1, "one hole, not two overlapping ones");
    assert!(all_arcs(&s, s.mesh.faces[cap].inners()[0].start), "the union hole keeps its arcs");
    for &f in top.iter().filter(|&&f| f != cap) {
        assert!(all_arcs(&s, s.mesh.faces[f].outer().start), "a region's rim must stay round");
    }

    assert!(s.mesh.verify_face_invariants().violations.is_empty());
    assert_eq!(s.mesh.detect_self_intersections().intersecting_pairs.len(), 0);
    assert!(s.mesh.verify_outward_normals().is_closed_solid, "the solid is still closed");
}

/// Each region is the user's — something to select, colour and push. A face that
/// came out of the split unowned would look right and behave like nothing.
#[test]
fn every_region_has_an_owner_and_a_surface_to_push_from() {
    let mut s = box_scene();
    circle(&mut s, -R / 2.0);
    circle(&mut s, R / 2.0);
    let top = on_top(&s);
    // Without this the test also passes on the REFUSED state, where the top is
    // just the cap and one whole disk and both are owned already — measured,
    // when the repair was unwired to check this test could fail at all.
    assert_eq!(top.len(), 4, "the split has to have happened for this to mean anything");
    for f in top {
        assert!(
            s.face_to_shape.contains_key(&f) || s.get_xia_for_face(f).is_some(),
            "face {f:?} belongs to nobody"
        );
        assert!(s.mesh.faces[f].surface().is_some(), "face {f:?} has no surface to push from");
    }
}

/// Undo takes the whole thing back, split and all — it is one draw to the user.
#[test]
fn one_undo_puts_the_first_circle_back() {
    let mut s = box_scene();
    circle(&mut s, -R / 2.0);
    let before = active(&s).len();
    circle(&mut s, R / 2.0);
    assert_ne!(active(&s).len(), before, "the second draw changed something");
    s.execute(Command::Undo);
    assert_eq!(active(&s).len(), before, "one undo, back to one circle");
    assert!(s.mesh.verify_outward_normals().is_closed_solid);
}

/// Drawing them apart still just makes two holes — the split must not fire where
/// there is nothing to resolve.
#[test]
fn circles_drawn_apart_are_left_alone() {
    let mut s = box_scene();
    assert!(!matches!(circle(&mut s, -60.0), CommandResult::Error(_)));
    assert!(!matches!(circle(&mut s, 60.0), CommandResult::Error(_)));
    let top = on_top(&s);
    let cap = *top
        .iter()
        .find(|&&f| !s.mesh.faces[f].inners().is_empty())
        .expect("the cap holds the holes");
    assert_eq!(s.mesh.faces[cap].inners().len(), 2, "two separate holes");
    assert_eq!(top.len(), 3, "cap + two whole disks");
    for &f in top.iter().filter(|&&f| f != cap) {
        assert_eq!(
            s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start).unwrap().len(),
            1,
            "a disk nothing crosses stays a single closed curve"
        );
    }
    assert!(s.mesh.verify_outward_normals().is_closed_solid);
}

/// An ellipse could not be drawn on a solid's face AT ALL — measured, even a
/// small one well inside the top, while the same draw on a sheet was fine.
///
/// The freeform containment that handles it on a sheet (ADR-186 A2) lives inside
/// the coplanar re-derive, and that re-derive is skipped wherever the region
/// touches a solid. So the ellipse simply lay on the cap and the guard rolled
/// the draw back. Circles had their own path out of this; nothing else did.
#[test]
fn an_ellipse_can_be_drawn_on_a_solid_face() {
    for (rx, ry) in [(40.0, 25.0), (20.0, 10.0)] {
        let mut s = box_scene();
        let r = s.execute(Command::DrawEllipseAsCurve {
            center: DVec3::new(0.0, 0.0, 100.0),
            normal: DVec3::Z,
            ref_dir: DVec3::X,
            radius_x: rx,
            radius_y: ry,
        });
        assert!(!matches!(r, CommandResult::Error(_)), "ellipse {rx}x{ry}: {r:?}");
        let top = on_top(&s);
        assert_eq!(top.len(), 2, "the cap with a hole, and the ellipse filling it");
        let cap = *top
            .iter()
            .find(|&&f| !s.mesh.faces[f].inners().is_empty())
            .expect("the cap holds the hole");
        assert_eq!(s.mesh.faces[cap].inners().len(), 1);
        assert_eq!(
            s.mesh.collect_loop_verts(s.mesh.faces[cap].inners()[0].start).unwrap().len(),
            1,
            "and it is still one closed curve, not flattened into a polygon"
        );
        assert!(s.mesh.verify_face_invariants().violations.is_empty());
        assert!(s.mesh.verify_outward_normals().is_closed_solid);
    }
}

/// And a circle drawn right around a smaller one still nests, rather than being
/// read as a crossing.
#[test]
fn a_circle_around_a_smaller_one_still_nests() {
    let mut s = box_scene();
    assert!(!matches!(
        s.execute(Command::DrawCircleAsCurve {
            center: DVec3::new(0.0, 0.0, 100.0),
            normal: DVec3::Z,
            radius: 30.0,
        }),
        CommandResult::Error(_)
    ));
    assert!(!matches!(
        s.execute(Command::DrawCircleAsCurve {
            center: DVec3::new(0.0, 0.0, 100.0),
            normal: DVec3::Z,
            radius: 70.0,
        }),
        CommandResult::Error(_)
    ));
    assert_eq!(on_top(&s).len(), 3, "cap, ring, disk");
    assert!(s.mesh.verify_outward_normals().is_closed_solid);
}
