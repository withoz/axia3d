//! A LINE THAT IS THICK WITH SOMETHING.
//!
//! 사용자 (2026-08-03): `선도 시민권자로 승격할까요? 우리엔진에 필요한가?`
//!
//! Measuring answered it: a line was already a citizen of both layers — a form
//! Shape by way of its standalone edge, and a `Linear` Xia once given a
//! material. Nothing needed promoting. What was hollow was the section: every
//! promoted line reported `cross_section_area = 1.0`, a placeholder that made
//! quantities silently wrong instead of honestly unknown.
//!
//! So the line can now carry a real one. The section rides on the EDGE, which
//! is what lets it survive promotion and demotion without anyone carrying it
//! across — both citizenships point at the same edge.
use axia_core::profile::Profile;
use axia_core::{Command, CommandResult, Scene};
use axia_geo::MaterialId;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// A drawn line and the edge it owns.
fn line(s: &mut Scene, from: DVec3, to: DVec3) -> (axia_core::ShapeId, axia_geo::EdgeId) {
    let r = s.execute(Command::DrawLineAsShape { start: from, end: to, surface_normal: None });
    let sid = match r {
        CommandResult::ShapeCreated(id) => axia_core::ShapeId::new(id),
        other => panic!("expected a Shape, got {other:?}"),
    };
    let eid = s.shapes.get(&sid).and_then(|sh| sh.standalone_edge_id).expect("line owns an edge");
    (sid, eid)
}

#[test]
fn a_line_without_a_section_says_so_rather_than_guessing() {
    let mut s = prod();
    let (sid, _) = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    let ok = s.promote_shape_to_xia(sid, MaterialId::new(7)).expect("a line is a member");
    match ok.kind {
        axia_core::promote::XiaKind::Linear { length, cross_section_area } => {
            assert!((length - 5000.0).abs() < 1e-6, "{length}");
            assert_eq!(
                cross_section_area, None,
                "no section was given — that is an absence, not a 1.0"
            );
        }
        other => panic!("expected Linear, got {other:?}"),
    }
}

#[test]
fn a_line_with_a_section_reports_its_area() {
    let mut s = prod();
    let (sid, eid) = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    s.set_edge_profile(eid, Some(Profile::Rectangular { width: 200.0, height: 400.0 }))
        .expect("a 200×400 column section");

    let ok = s.promote_shape_to_xia(sid, MaterialId::new(7)).expect("promote");
    match ok.kind {
        axia_core::promote::XiaKind::Linear { length, cross_section_area } => {
            assert_eq!(cross_section_area, Some(80_000.0), "200 × 400");
            // What the whole thing is for: length × section is a real volume.
            let volume = length * cross_section_area.unwrap();
            assert!((volume - 400_000_000.0).abs() < 1e-3, "{volume}");
        }
        other => panic!("expected Linear, got {other:?}"),
    }
}

/// The section belongs to the line, not to whichever citizenship it happens to
/// hold at the moment.
#[test]
fn the_section_survives_promotion_and_demotion() {
    let mut s = prod();
    let (sid, eid) = line(&mut s, DVec3::ZERO, DVec3::new(3000.0, 0.0, 0.0));
    s.set_edge_profile(eid, Some(Profile::Circular { radius: 50.0 })).unwrap();

    let ok = s.promote_shape_to_xia(sid, MaterialId::new(7)).unwrap();
    assert!(matches!(
        ok.kind,
        axia_core::promote::XiaKind::Linear { cross_section_area: Some(_), .. }
    ));
    assert!(s.edge_profile(eid).is_some(), "still there as a member");

    // Demotion is triggered by taking the material away (ADR-091), so that is
    // how the round trip is made.
    if let Some(x) = s.xias.get_mut(&ok.xia_id) {
        x.material = axia_core::FORM_MATERIAL;
    }
    s.demote_xia_to_shape(ok.xia_id).expect("back to a form");
    assert!(s.edge_profile(eid).is_some(), "and still there as a form");
}

/// It comes back with the file.
#[test]
fn a_section_survives_a_save_and_load() {
    let mut s = prod();
    let (_, eid) = line(&mut s, DVec3::ZERO, DVec3::new(3000.0, 0.0, 0.0));
    let section = Profile::Polygon {
        points: vec![(-100.0, -200.0), (100.0, -200.0), (100.0, 200.0), (-100.0, 200.0)],
    };
    s.set_edge_profile(eid, Some(section.clone())).unwrap();
    let snapshot = s.scene_snapshot();

    let mut back = prod();
    back.restore_scene_snapshot(&snapshot);
    assert_eq!(back.edge_profile(eid), Some(&section), "the section came back");
}

/// A section that measures nothing is a stated mistake, and it is refused where
/// the mistake was made rather than much later at promotion.
#[test]
fn a_section_with_no_area_is_refused_on_the_spot() {
    let mut s = prod();
    let (sid, eid) = line(&mut s, DVec3::ZERO, DVec3::new(3000.0, 0.0, 0.0));
    assert!(s
        .set_edge_profile(eid, Some(Profile::Rectangular { width: 200.0, height: 0.0 }))
        .is_err());
    assert!(s.edge_profile(eid).is_none(), "nothing was stored");
    // And the line still promotes — it simply has no section yet.
    assert!(s.promote_shape_to_xia(sid, MaterialId::new(7)).is_ok());
}

#[test]
fn a_section_can_be_taken_away_again() {
    let mut s = prod();
    let (_, eid) = line(&mut s, DVec3::ZERO, DVec3::new(3000.0, 0.0, 0.0));
    s.set_edge_profile(eid, Some(Profile::Circular { radius: 25.0 })).unwrap();
    assert!(s.edge_profile(eid).is_some());
    s.set_edge_profile(eid, None).unwrap();
    assert!(s.edge_profile(eid).is_none());
}

/// The line where two faces meet is a line like any other, so it can be given a
/// section and become a member — a beam along a fold, say.
#[test]
fn a_contact_line_can_be_given_a_section_too() {
    let mut s = prod();
    s.execute(Command::DrawRectAsShape {
        center: DVec3::ZERO, normal: DVec3::Z, up: DVec3::Y, width: 1000.0, height: 1000.0,
    });
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(0.0, 0.0, 150.0), normal: DVec3::Y, up: DVec3::Z,
        width: 300.0, height: 300.0,
    });
    let (sid, eid) = s
        .shapes
        .iter()
        .find_map(|(id, sh)| sh.standalone_edge_id.map(|e| (*id, e)))
        .expect("the crossing named a line");
    s.set_edge_profile(eid, Some(Profile::Rectangular { width: 100.0, height: 200.0 })).unwrap();
    let ok = s.promote_shape_to_xia(sid, MaterialId::new(7)).expect("promote");
    match ok.kind {
        axia_core::promote::XiaKind::Linear { length, cross_section_area } => {
            assert!((length - 300.0).abs() < 1e-6, "{length}");
            assert_eq!(cross_section_area, Some(20_000.0));
        }
        other => panic!("expected Linear, got {other:?}"),
    }
}

/// The member reaches the file. Before this it did not: the exporter skipped any
/// element with no faces, so a column that was a line simply was not there.
///
/// The engine test can only reach as far as the scene; the emitted STEP is
/// checked at the wasm layer where the exporter lives (see `ifc_line_member`).
/// What this pins is that the scene hands over everything that layer needs: a
/// line member whose edge is alive, whose endpoints are readable, and whose
/// section has an area.
#[test]
fn a_line_member_carries_everything_the_exporter_needs() {
    let mut s = prod();
    let (sid, eid) = line(&mut s, DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 0.0, 3000.0));
    s.set_edge_profile(eid, Some(Profile::Rectangular { width: 300.0, height: 300.0 })).unwrap();
    s.promote_shape_to_xia(sid, MaterialId::new(7)).unwrap();

    let xia = s.xias.values().next().expect("the member");
    let edge_id = xia.standalone_edge_id.expect("a line member owns its edge");
    let edge = s.mesh.edges.get(edge_id).expect("the edge is alive");
    assert!(edge.is_active());
    let a = s.mesh.vertex_pos(edge.v_small()).expect("start");
    let b = s.mesh.vertex_pos(edge.v_large()).expect("end");
    assert!((b - a).length() > 0.0, "a member of no length is not a member");
    assert_eq!(s.edge_profile(edge_id).and_then(|p| p.area()), Some(90_000.0));
}
