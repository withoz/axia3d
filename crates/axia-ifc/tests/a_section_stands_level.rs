//! A MEMBER'S SECTION STANDS LEVEL: WIDTH ACROSS, HEIGHT UPRIGHT.
//!
//! Measured 2026-08-04: a 200 wide × 400 deep beam laid along +X came out of the
//! exporter **on its side** — 200 tall, 400 across. A brace at 45° was worse, its
//! width tilted 45° out of horizontal.
//!
//! The profile lies in the placement's local XY with width along RefDir, so
//! RefDir is the one that has to be horizontal. The old code took `up` squared to
//! the axis, which is the OTHER one — its own comment said it wanted a section
//! that "stays level", and it achieved that only for a vertical member, where
//! both section axes are horizontal anyway.
//!
//! Every reading below comes out of the emitted STEP. The first version of this
//! probe re-implemented the exporter's arithmetic to check it, so it happily
//! reported the old answer after the fix — which is the whole reason to read what
//! was written rather than what one believes was written.
use axia_ifc::ifc_extrude::{emit_line_member, LineMember, SectionProfile};
use axia_ifc::step_writer::StepWriter;
use glam::DVec3;
use std::collections::HashMap;

/// The placement's (Axis, RefDir), read back out of the emitted file.
fn emitted_frame(start: DVec3, end: DVec3, w: f64, h: f64) -> (DVec3, DVec3) {
    let mut sw = StepWriter::new();
    let m = LineMember {
        start,
        end,
        profile: SectionProfile::Rectangular { width: w, height: h },
    };
    emit_line_member(&mut sw, &m, 0.001).expect("a member with a section is emitted");
    let step: String = sw.build();

    let mut dirs: HashMap<String, DVec3> = HashMap::new();
    let mut placement: Option<(String, String)> = None;
    for line in step.lines() {
        let Some(rest) = line.trim().strip_prefix('#') else { continue };
        let Some((id, body)) = rest.split_once('=') else { continue };
        if let Some(inner) = body.strip_prefix("IFCDIRECTION((") {
            let nums: Vec<f64> = inner
                .trim_end_matches(");")
                .trim_end_matches(')')
                .split(',')
                .filter_map(|t| t.trim().trim_end_matches('.').parse::<f64>().ok())
                .collect();
            if nums.len() == 3 {
                dirs.insert(id.to_string(), DVec3::new(nums[0], nums[1], nums[2]));
            }
        }
        if let Some(inner) = body.strip_prefix("IFCAXIS2PLACEMENT3D(") {
            let parts: Vec<&str> = inner.trim_end_matches(");").split(',').collect();
            if parts.len() == 3 {
                placement = Some((
                    parts[1].trim().trim_start_matches('#').to_string(),
                    parts[2].trim().trim_start_matches('#').to_string(),
                ));
            }
        }
    }
    let (a, r) = placement.expect("the sweep is placed");
    (dirs[&a], dirs[&r])
}

/// How much of each section dimension ends up vertical.
fn standing(start: DVec3, end: DVec3, w: f64, h: f64) -> (f64, f64) {
    let (axis, ref_dir) = emitted_frame(start, end, w, h);
    let local_y = axis.cross(ref_dir);
    (w * ref_dir.z.abs(), h * local_y.z.abs())
}

const BEAM_W: f64 = 200.0;
const BEAM_H: f64 = 400.0;

#[test]
fn a_horizontal_beam_carries_its_depth_upright() {
    for (name, end) in [
        ("+X", DVec3::new(5000.0, 0.0, 0.0)),
        ("-X", DVec3::new(-5000.0, 0.0, 0.0)),
        ("+Y", DVec3::new(0.0, 5000.0, 0.0)),
        ("-Y", DVec3::new(0.0, -5000.0, 0.0)),
        ("diagonal", DVec3::new(3000.0, 4000.0, 0.0)),
    ] {
        let (w_up, h_up) = standing(DVec3::ZERO, end, BEAM_W, BEAM_H);
        assert!(w_up < 1e-9, "along {name}: the width must lie flat, got {w_up} of it upright");
        assert!(
            (h_up - BEAM_H).abs() < 1e-9,
            "along {name}: the full depth {BEAM_H} stands up, got {h_up}"
        );
    }
}

/// A column has no upright section dimension — both of its section axes are
/// horizontal, and which is which is arbitrary until the user says.
#[test]
fn a_column_lies_flat_in_both_directions() {
    for end in [DVec3::new(0.0, 0.0, 3000.0), DVec3::new(0.0, 0.0, -3000.0)] {
        let (w_up, h_up) = standing(DVec3::ZERO, end, BEAM_W, BEAM_H);
        assert!(w_up < 1e-9 && h_up < 1e-9, "a column's section is level: {w_up}, {h_up}");
        let (axis, ref_dir) = emitted_frame(DVec3::ZERO, end, BEAM_W, BEAM_H);
        assert!(ref_dir.z.abs() < 1e-9, "and its RefDir is horizontal: {ref_dir:?}");
        assert!(axis.z.abs() > 0.999, "{axis:?}");
    }
}

/// A brace leans, and its section leans with it — the width stays horizontal
/// while the height tilts by exactly the member's own angle.
#[test]
fn a_brace_tilts_by_its_own_angle() {
    let end = DVec3::new(3000.0, 0.0, 3000.0); // 45°
    let (axis, ref_dir) = emitted_frame(DVec3::ZERO, end, BEAM_W, BEAM_H);
    assert!(ref_dir.z.abs() < 1e-9, "the width still lies flat: {ref_dir:?}");
    let local_y = axis.cross(ref_dir);
    let want = std::f64::consts::FRAC_1_SQRT_2;
    assert!(
        (local_y.z.abs() - want).abs() < 1e-9,
        "the height leans 45° with the member: {local_y:?}"
    );
}

/// The frame that gets written has to be a frame: unit, square, right-handed.
#[test]
fn the_written_frame_is_orthonormal() {
    for end in [
        DVec3::new(5000.0, 0.0, 0.0),
        DVec3::new(0.0, 0.0, 3000.0),
        DVec3::new(3000.0, 4000.0, 1200.0),
    ] {
        let (axis, ref_dir) = emitted_frame(DVec3::ZERO, end, BEAM_W, BEAM_H);
        assert!((axis.length() - 1.0).abs() < 1e-9, "{axis:?}");
        assert!((ref_dir.length() - 1.0).abs() < 1e-9, "{ref_dir:?}");
        assert!(axis.dot(ref_dir).abs() < 1e-9, "square: {axis:?} · {ref_dir:?}");
        assert!((axis.cross(ref_dir).length() - 1.0).abs() < 1e-9, "and the third is unit too");
    }
}

/// A member with no length has no frame to write, and says so rather than
/// emitting a placement built on a zero direction.
#[test]
fn a_member_of_no_length_is_refused() {
    let mut sw = StepWriter::new();
    let m = LineMember {
        start: DVec3::ZERO,
        end: DVec3::ZERO,
        profile: SectionProfile::Rectangular { width: 200.0, height: 400.0 },
    };
    assert!(emit_line_member(&mut sw, &m, 0.001).is_none());
}
