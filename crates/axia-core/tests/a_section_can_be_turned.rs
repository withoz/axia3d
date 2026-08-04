//! WHICH WAY THE SECTION FACES IS THE USER'S TO SAY.
//!
//! A line says which way it runs; it does not say which way its section is
//! turned about itself. The exporter picks level — width across, height upright
//! — and that is right for a rectangular beam and meaningless for a round one,
//! but wrong for anything the user wants at an angle: an I-beam laid flat is a
//! quarter turn from one stood upright, and until now there was no way to say so.
//!
//! The roll lives in a map beside the sections rather than as a field on
//! `EdgeSection`, because that struct is bincode-encoded into snapshot section
//! 13 and bincode is positional — a new field would make every saved file's
//! section fail to decode and every line quietly lose its thickness. That is
//! ADR-091 §E L1, and the cost of obeying it is the two sync points this file
//! pins: taking a section away takes its roll, and crossing a line hands the
//! roll to both pieces along with the section.
use axia_core::profile::Profile;
use axia_core::{Command, CommandResult, Scene};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn line(s: &mut Scene, from: DVec3, to: DVec3) -> axia_geo::EdgeId {
    let r = s.execute(Command::DrawLineAsShape { start: from, end: to, surface_normal: None });
    let sid = match r {
        CommandResult::ShapeCreated(id) => axia_core::ShapeId::new(id),
        other => panic!("expected a Shape, got {other:?}"),
    };
    s.shapes.get(&sid).and_then(|sh| sh.standalone_edge_id).expect("a line owns an edge")
}

const QUARTER: f64 = std::f64::consts::FRAC_PI_2;

#[test]
fn a_section_starts_level_and_can_be_turned() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    s.set_edge_profile(e, Some(Profile::Rectangular { width: 200.0, height: 400.0 })).unwrap();

    assert_eq!(s.edge_roll(e), 0.0, "level until someone says otherwise");
    s.set_edge_roll(e, QUARTER).expect("turn it on its side");
    assert!((s.edge_roll(e) - QUARTER).abs() < 1e-12);
}

/// A full turn is no turn, and it is stored as the absence of a choice rather
/// than as a number that happens to mean the same thing.
#[test]
fn a_full_turn_is_no_turn() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    s.set_edge_profile(e, Some(Profile::Circular { radius: 50.0 })).unwrap();

    s.set_edge_roll(e, std::f64::consts::TAU).unwrap();
    assert_eq!(s.edge_roll(e), 0.0);
    assert!(s.edge_roll.is_empty(), "and nothing is stored: {:?}", s.edge_roll);

    // Past a full turn wraps rather than accumulating.
    s.set_edge_roll(e, std::f64::consts::TAU + QUARTER).unwrap();
    assert!((s.edge_roll(e) - QUARTER).abs() < 1e-12);

    // Backwards is the same place the other way round.
    s.set_edge_roll(e, -QUARTER).unwrap();
    assert!((s.edge_roll(e) - (std::f64::consts::TAU - QUARTER)).abs() < 1e-12);
}

/// Nothing to turn, nothing to store — otherwise the entry outlives the line it
/// was meant for and lands on whatever gets that id next.
#[test]
fn a_line_with_no_section_cannot_be_turned() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    assert!(s.set_edge_roll(e, QUARTER).is_err());
    assert!(s.edge_roll.is_empty());

    // And a number that is not one is refused rather than stored.
    s.set_edge_profile(e, Some(Profile::Circular { radius: 50.0 })).unwrap();
    assert!(s.set_edge_roll(e, f64::NAN).is_err());
    assert!(s.edge_roll.is_empty());
}

/// Sync point one: the roll goes when the section goes.
#[test]
fn taking_the_section_away_takes_its_roll() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    s.set_edge_profile(e, Some(Profile::Rectangular { width: 200.0, height: 400.0 })).unwrap();
    s.set_edge_roll(e, QUARTER).unwrap();
    assert!(!s.edge_roll.is_empty());

    s.set_edge_profile(e, None).unwrap();
    assert!(s.edge_roll.is_empty(), "an orphan roll would land on the next line to get this id");
}

/// Sync point two: a crossed beam faces the same way in both halves.
#[test]
fn crossing_a_line_leaves_its_roll_on_both_halves() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(1000.0, 0.0, 0.0));
    s.set_edge_profile(e, Some(Profile::Rectangular { width: 300.0, height: 300.0 })).unwrap();
    s.set_edge_roll(e, QUARTER).unwrap();

    // Straight through the middle.
    s.execute(Command::DrawLineAsShape {
        start: DVec3::new(500.0, -500.0, 0.0),
        end: DVec3::new(500.0, 500.0, 0.0),
        surface_normal: None,
    });

    assert!(!s.mesh.edges.get(e).map(|x| x.is_active()).unwrap_or(false), "the line was replaced");
    let rolled: Vec<f64> = s.edge_roll.values().copied().collect();
    assert_eq!(rolled.len(), 2, "both halves, and only them: {:?}", s.edge_roll);
    for r in rolled {
        assert!((r - QUARTER).abs() < 1e-12, "each piece faces the way the beam faced, got {r}");
    }
}

/// It comes back with the file, and a file saved before there was such a thing
/// still loads — every section simply at the level default.
#[test]
fn a_roll_survives_a_save_and_load() {
    let mut s = prod();
    let e = line(&mut s, DVec3::ZERO, DVec3::new(5000.0, 0.0, 0.0));
    s.set_edge_profile(e, Some(Profile::Rectangular { width: 200.0, height: 400.0 })).unwrap();
    s.set_edge_roll(e, QUARTER).unwrap();
    let snapshot = s.scene_snapshot();

    let mut back = prod();
    back.restore_scene_snapshot(&snapshot);
    assert!((back.edge_roll(e) - QUARTER).abs() < 1e-12, "the turn came back");
    assert!(back.edge_profile(e).is_some(), "and so did the section");

    // A snapshot cut before section 14 — what a file written yesterday looks
    // like. The sections still load; only the turns are gone.
    let roll_data = bincode::serialize(&back.edge_roll).unwrap();
    let mut legacy = snapshot.clone();
    legacy.truncate(legacy.len() - (8 + roll_data.len()));
    let mut old = prod();
    old.restore_scene_snapshot(&legacy);
    assert_eq!(old.edge_roll(e), 0.0, "no turn survives, because none was written");
    assert!(old.edge_profile(e).is_some(), "but the section does — section 13 is untouched");
}
