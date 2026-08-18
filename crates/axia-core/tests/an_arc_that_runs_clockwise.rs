//! A clockwise arc could not be intersected — it crashed.
//!
//! Widening the fuzz to 100 sessions x 50 operations (the size its own header
//! names as the pre-release run) found this on session 2, and it is not an
//! invariant violation — it is a PANIC, so the harness contract never got to
//! speak:
//!
//! ```text
//!   panicked at core/src/num/f64.rs:1432
//!   min > max, or either was NaN.  min = 9.433953182702224, max = 6.746832916180392
//!            curves/intersect.rs:144   newton_refine
//!            face_rederive.rs:1129     detect_freeform_overlaps
//! ```
//!
//! `f64::clamp(min, max)` panics when min > max, and the bounds came straight
//! from `AnalyticCurve::parameter_range()`, which for an arc returns
//! `(start_angle, end_angle)` — in curve order, not sorted. An arc whose
//! `start_angle > end_angle` is a legal CLOCKWISE arc; the type says so itself:
//!
//! ```text
//!   /// Direction: positive (CCW around `normal`) when `end_angle > start_angle`.
//!   AnalyticCurve::Arc { radius, start_angle, end_angle, .. } =>
//!       Ok(radius * (end_angle - start_angle).abs())     <- arc_length takes |d|
//! ```
//!
//! So the arc is fine and the reading of it was not.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult, FORM_MATERIAL};
use axia_geo::curves::{AnalyticCurve, CurveOps};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

// The fuzz harness generator, verbatim, so session 2 replays exactly.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n.max(1)
    }
    fn coord(&mut self) -> f64 {
        (self.below(9) as f64 - 4.0) * 50.0
    }
    fn size(&mut self) -> f64 {
        60.0 + self.below(5) as f64 * 40.0
    }
}

fn active_faces(s: &Scene) -> Vec<FaceId> {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(fid, _)| fid).collect()
}

fn step(s: &mut Scene, r: &mut Lcg) -> String {
    let z = [0.0, 100.0, 200.0][r.below(3)];
    let (x, y) = (r.coord(), r.coord());
    let c = DVec3::new(x, y, z);
    let w = r.size();
    match r.below(10) {
        0 => {
            let e = matches!(s.execute(Command::DrawRectAsShape {
                center: c, normal: DVec3::Z, up: DVec3::X, width: w, height: w * 0.75,
            }), CommandResult::Error(_));
            format!("rect({x},{y},{z},{w}){}", if e { " REFUSED" } else { "" })
        }
        1 => {
            let e = matches!(s.execute(Command::DrawCircleAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, segments: 24,
            }), CommandResult::Error(_));
            format!("circle({x},{y},{z},r={}){}", w * 0.5, if e { " REFUSED" } else { "" })
        }
        2 => {
            let e = matches!(s.execute(Command::DrawCircleAsCurve {
                center: c, normal: DVec3::Z, radius: w * 0.5,
            }), CommandResult::Error(_));
            format!("circleCurve({x},{y},{z},r={}){}", w * 0.5, if e { " REFUSED" } else { "" })
        }
        3 => {
            let e = matches!(s.execute(Command::DrawEllipseAsCurve {
                center: c, ref_dir: DVec3::X, normal: DVec3::Z,
                radius_x: w * 0.5, radius_y: w * 0.3,
            }), CommandResult::Error(_));
            format!("ellipse({x},{y},{z},rx={},ry={}){}", w * 0.5, w * 0.3, if e { " REFUSED" } else { "" })
        }
        4 => {
            let sides = 3 + r.below(5) as u32;
            let e = matches!(s.execute(Command::DrawPolygonAsShape {
                center: c, normal: DVec3::Z, radius: w * 0.5, sides,
            }), CommandResult::Error(_));
            format!("polygon({x},{y},{z},r={},n={}){}", w * 0.5, sides, if e { " REFUSED" } else { "" })
        }
        5 => {
            let e = matches!(s.execute(Command::DrawLine {
                start: DVec3::new(x - 150.0, y, z),
                end: DVec3::new(x + 150.0, y, z),
                surface_normal: Some(DVec3::Z),
            }), CommandResult::Error(_));
            format!("line({x},{y},{z},len=300){}", if e { " REFUSED" } else { "" })
        }
        6 => {
            let faces = active_faces(s);
            if faces.is_empty() { return "extrude(skip: nothing there)".into(); }
            let f = faces[r.below(faces.len())];
            let d = 50.0 + r.below(4) as f64 * 50.0;
            let e = matches!(s.execute(Command::CreateSolid {
                face_id: f, mode: CreateSolidMode::Extrude { distance: d },
            }), CommandResult::Error(_));
            format!("extrude({f:?},{d}){}", if e { " REFUSED" } else { "" })
        }
        7 => {
            let faces = active_faces(s);
            if faces.is_empty() { return "pushIn(skip: nothing there)".into(); }
            let f = faces[r.below(faces.len())];
            let d = -(50.0 + r.below(3) as f64 * 50.0);
            let e = matches!(s.execute(Command::CreateSolid {
                face_id: f, mode: CreateSolidMode::Extrude { distance: d },
            }), CommandResult::Error(_));
            format!("pushIn({f:?},{d}){}", if e { " REFUSED" } else { "" })
        }
        8 => {
            let ok = s.mesh.create_box(c + DVec3::new(0.0, 0.0, 60.0), w, 120.0, w, FORM_MATERIAL).is_ok();
            format!("box({x},{y},{z},{w},{})", if ok { "ok" } else { "FAILED" })
        }
        _ => {
            let a = DVec3::new(x - 30.0, y - 30.0, z);
            let b = DVec3::new(x + 30.0, y + 30.0, z);
            let ok = s.punch_rect_hole(a, b, DVec3::Z).is_ok();
            format!("punch({x},{y},{z},{})", if ok { "ok" } else { "refused" })
        }
    }
}

/// Every live edge whose analytic curve reports a range running backwards.
fn backwards_ranges(s: &Scene) -> Vec<(u32, f64, f64)> {
    let mut out = Vec::new();
    for (eid, e) in s.mesh.edges.iter() {
        if !e.is_active() {
            continue;
        }
        if let Some(c) = s.mesh.edge_curve(eid) {
            let (a, b) = c.parameter_range();
            if a > b {
                out.push((eid.raw(), a, b));
            }
        }
    }
    out
}

/// Which operation of fuzz session 2 first leaves a backwards-running range.
///
/// Printed, not asserted on a count — a clockwise arc is legal geometry and the
/// point of this run is to name the operation that makes one, so the reduction
/// can start from something small.
#[test]
fn session_2_says_which_operation_makes_a_clockwise_arc() {
    let mut r = Lcg(0x5EED_0000 + 2);
    let mut s = prod();
    println!("\n  fuzz session 2 — which operation leaves a backwards range\n");
    for op in 0..50 {
        let before = backwards_ranges(&s).len();
        let desc = step(&mut s, &mut r);
        let after = backwards_ranges(&s);
        println!("    op {op:>2}  {desc:<44}  backwards {}", after.len());
        if !after.is_empty() && before == 0 {
            println!("\n  first at op {op} — {desc}");
            println!("  ranges: {after:?}\n");
            return;
        }
    }
    println!("\n  no backwards range in 50 operations\n");
}

/// A clockwise arc can be asked where it meets a line, and answers correctly.
///
/// The arc of the crash: 9.4340 rad down to 6.7468 rad, radius 50 about the
/// origin. Taken modulo 2π that is 180.5 degrees down to 26.6 degrees, so it
/// crosses the X axis once — just inside its far end — at (-50, 0, 0).
///
/// Asserting the POINT, not merely that nothing panicked. `f64::clamp` could be
/// silenced by sorting the range at its source instead, and that passes a
/// "does not crash" test while putting every Newton seed at the wrong end of
/// the arc: `intersect_curves` maps polyline index to parameter linearly and
/// `tessellate` walks an arc start -> end, so the backwards range is what makes
/// the two line up.
///
/// Mutation-checked three ways:
///   - restore `t1.clamp(r1_min, r1_max)`      -> panics, as it did
///   - sort the range inside `parameter_range` -> 0 intersections, silently
///   - keep the fix                            -> 1, at (-50, 0, 0)
#[test]
fn a_clockwise_arc_can_be_asked_where_it_meets_a_line() {
    let m = axia_geo::Mesh::new();
    let cw = AnalyticCurve::Arc {
        center: DVec3::ZERO,
        radius: 50.0,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        start_angle: 9.433953182702224,
        end_angle: 6.746832916180392,
    };
    let (a, b) = cw.parameter_range();
    let tau = std::f64::consts::TAU;
    println!("
  clockwise arc range = ({a:.4}, {b:.4})   min>max? {}", a > b);
    println!(
        "  i.e. {:.1} deg down to {:.1} deg",
        (a % tau).to_degrees(),
        (b % tau).to_degrees()
    );
    println!("  arc length = {:.3}", cw.arc_length(&m).unwrap_or(f64::NAN));

    // A straight two-point Bezier stands in for a line: `AnalyticCurve::Line`
    // names mesh vertices, and this needs no mesh.
    let line = AnalyticCurve::Bezier {
        control_pts: vec![DVec3::new(-100.0, 0.0, 0.0), DVec3::new(100.0, 0.0, 0.0)],
    };
    let hits = axia_geo::curves::intersect::intersect_curves(&cw, &line, &m, 1e-6)
        .expect("asking a legal clockwise arc where it meets a line must not fail");
    for h in &hits {
        println!("  meets at ({:.3}, {:.3}, {:.3})", h.point.x, h.point.y, h.point.z);
    }
    println!();

    assert_eq!(hits.len(), 1, "the arc crosses the X axis exactly once");
    let p = hits[0].point;
    assert!(
        (p - DVec3::new(-50.0, 0.0, 0.0)).length() < 1e-3,
        "and it crosses at (-50, 0, 0), not {p:?} — a seed at the wrong end of \n         the arc converges somewhere else or not at all"
    );
}

/// A clockwise arc can now be split at a point.
///
/// The same conflation, one layer over, and it was recorded here as "measured,
/// not fixed" when the crash was: `parameter_at_3d_point` walked the angle into
/// `[start_angle, end_angle]` with two `while` loops that fight each other when
/// start > end, so it returned `PointOffCurve` for a point exactly ON the arc,
/// and `split_at` refused the same way — `t < start || t > end` is true for
/// every t on a backwards arc.
///
/// Both now order the interval first. Neither reverses anything: the two halves
/// a split produces each keep the endpoint they started from, so a clockwise arc
/// splits into two clockwise arcs.
///
/// Mutation-checked: restore either `[start_angle, end_angle]` comparison and
/// this fails — `parameter_at_3d_point` with `PointOffCurve`, `split_at` with
/// "outside arc range".
#[test]
fn a_clockwise_arc_can_be_split_at_a_point() {
    let m = axia_geo::Mesh::new();
    let (sa, ea) = (9.433953182702224, 6.746832916180392);
    let cw = AnalyticCurve::Arc {
        center: DVec3::ZERO,
        radius: 50.0,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        start_angle: sa,
        end_angle: ea,
    };
    // Dead centre of the arc, by its own midpoint angle.
    let mid = (sa + ea) * 0.5;
    let on_it = DVec3::new(50.0 * f64::cos(mid), 50.0 * f64::sin(mid), 0.0);

    let t = cw
        .parameter_at_3d_point(on_it, &m)
        .expect("a point exactly on the arc has a parameter");
    println!("
  point on the arc: ({:.3}, {:.3})", on_it.x, on_it.y);
    println!("  parameter        {t:.6}   (arc runs {sa:.4} -> {ea:.4})");

    let (a, b) = cw
        .split_at(t, axia_geo::VertId::new(0))
        .expect("and can be split there");
    let (AnalyticCurve::Arc { start_angle: a0, end_angle: a1, .. }, 
         AnalyticCurve::Arc { start_angle: b0, end_angle: b1, .. }) = (&a, &b)
    else {
        panic!("two arcs, not {a:?} / {b:?}");
    };
    println!("  halves           {a0:.4} -> {a1:.4}   and   {b0:.4} -> {b1:.4}
");

    assert!((t - mid).abs() < 1e-6, "the parameter is the midpoint angle: {t}");
    assert!((*a0 - sa).abs() < 1e-12 && (*a1 - t).abs() < 1e-12, "first half starts where the arc did");
    assert!((*b0 - t).abs() < 1e-12 && (*b1 - ea).abs() < 1e-12, "second half ends where the arc did");
    assert!(a0 > a1 && b0 > b1, "and BOTH halves still run clockwise");
}

/// A counter-clockwise arc is unchanged by the ordering.
///
/// The control the fix needs: whatever the backwards case gained, the ordinary
/// case has to behave exactly as it did.
#[test]
fn a_counter_clockwise_arc_splits_as_it_always_did() {
    let m = axia_geo::Mesh::new();
    let ccw = AnalyticCurve::Arc {
        center: DVec3::ZERO,
        radius: 50.0,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        start_angle: 0.5,
        end_angle: 2.5,
    };
    let mid = 1.5;
    let on_it = DVec3::new(50.0 * f64::cos(mid), 50.0 * f64::sin(mid), 0.0);
    let t = ccw.parameter_at_3d_point(on_it, &m).expect("on the arc");
    let (a, b) = ccw.split_at(t, axia_geo::VertId::new(0)).expect("splits");
    let (AnalyticCurve::Arc { start_angle: a0, end_angle: a1, .. },
         AnalyticCurve::Arc { start_angle: b0, end_angle: b1, .. }) = (&a, &b)
    else {
        panic!("two arcs");
    };
    println!("
  ccw halves  {a0:.4} -> {a1:.4}   and   {b0:.4} -> {b1:.4}
");
    assert!((t - mid).abs() < 1e-6);
    assert!(a0 < a1 && b0 < b1, "still counter-clockwise");
}
