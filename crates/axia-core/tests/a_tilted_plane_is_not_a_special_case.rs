//! Track D — the axis nobody measured.
//!
//! Every one of the 36 overlap combinations was drawn at z = 0 or z = 100,
//! normal +Z. The expansion plan says so plainly: *"36조합은 전부 z=0 또는
//! z=100 축정렬이었다"*. Meanwhile `tool-plane` (three-point workplane),
//! `sketch-offset` and `sketch-tilt` all ship, so a user can put the drawing
//! plane anywhere — and nothing has ever checked that the arrangement behaves
//! there.
//!
//! ## Why four planes, and why 45° is not enough
//!
//! ⚠ A 45° tilt proves less than it looks. `sin θ = cos θ` there, so a formula
//! with the two swapped reads correct — that is exactly how `plane_cylinder`
//! carried an inverted semi-major for a whole session, because 45° was the only
//! oblique angle any test covered. So the oblique row here is `(1, 2, 3)`: no
//! two components equal, no symmetry to hide behind.
//!
//! 1. `+Z`      the axis every prior measurement used — the control
//! 2. `+X`      still cardinal, different axis: does anything assume Z?
//! 3. `45°`     the common case, kept because users do it, not because it proves
//! 4. `(1,2,3)` the one that can actually fail
//!
//! ## Why the criteria are minimal, and what that costs
//!
//! Areas do not compare across planes without care, and counts alone lie — the
//! expansion plan's own judgment standard became "the top tiles to exactly
//! 40,000" only after face counts had misled twice. What CAN be asked
//! identically on all four planes is:
//!
//!   - a face was created at all
//!   - its normal is the one that was asked for (`|n·N| > 0.999`)
//!   - two overlapping shapes give the same region count as the control plane
//!   - `verify_face_invariants` is clean
//!
//! That is deliberately less than the 36-combination grid asks on +Z, and the
//! shortfall is the point: this file does not re-prove the arrangement, it
//! proves the arrangement does not CARE about the axis. Only a question asked
//! identically on all four planes can find a claim that holds on one and not
//! another. The control decides the expected number; the other three must match
//! it. Pinning an absolute instead would quietly turn this into another
//! arrangement test and stop measuring the axis.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::operations::coplanar::face_world_normal;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The four planes, each as (name, normal, up). `up` is a unit vector lying in
/// the plane; rect needs one and the others ignore it.
fn planes() -> Vec<(&'static str, DVec3, DVec3)> {
    let obl = DVec3::new(1.0, 2.0, 3.0).normalize();
    // Any vector not parallel to the normal, projected into the plane.
    let obl_up = (DVec3::X - obl * DVec3::X.dot(obl)).normalize();
    let t45 = DVec3::new(0.0, 1.0, 1.0).normalize();
    vec![
        ("+Z (control)", DVec3::Z, DVec3::X),
        ("+X", DVec3::X, DVec3::Y),
        ("45 deg", t45, DVec3::X),
        ("(1,2,3)", obl, obl_up),
    ]
}

fn active_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// Active faces whose normal agrees with `n`, either way round.
fn faces_on_plane(s: &Scene, n: DVec3) -> usize {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(fid, _)| {
            // `face_world_normal` is the non-destructive reader (LOCKED #41).
            // There is no `Mesh::face_normal` — a stored `Face::normal` exists
            // but reads (0,0,1) on a Path B cylinder's side, so it is the wrong
            // instrument for asking which plane a face sits on.
            face_world_normal(&s.mesh, *fid)
                .map(|fna| fna.dot(n).abs() > 0.999)
                .unwrap_or(false)
        })
        .count()
}

/// How many active faces lie on the EXACT plane `n·p = offset`, and how much
/// area they carry between them.
///
/// ⚠ Both numbers are needed, and finding that out took three mutations:
///
///   - a face COUNT delta cannot tell "divided the face" from "made a sheet in
///     mid-air" — both are +1
///   - the count ON THE PLANE cannot tell "divided" from "a coplanar sheet
///     lying beside it" — both are 2
///   - the AREA cannot tell "divided" from "the rect missed the plane entirely"
///     — both leave 40,000
///
/// Together they can: divided is (2, 40_000); beside is (2, 43_600); off-plane
/// is (1, 40_000). This is the expansion plan's own standard — *the top must
/// tile to exactly 40,000* — which it reached only after face counts had misled
/// it twice.
fn plane_census(s: &Scene, n: DVec3, offset: f64) -> (usize, f64) {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(fid, _)| {
            let Some(fna) = face_world_normal(&s.mesh, *fid) else { return false };
            if fna.dot(n).abs() <= 0.999 {
                return false;
            }
            let Some(pts) = s.mesh.face_outline_points(*fid) else { return false };
            if pts.is_empty() {
                return false;
            }
            let c: DVec3 = pts.iter().copied().sum::<DVec3>() / pts.len() as f64;
            (c.dot(n) - offset).abs() < 1e-6
        })
        .map(|(fid, _)| (1usize, s.mesh.face_area(fid)))
        .fold((0usize, 0.0f64), |(n0, a0), (n1, a1)| (n0 + n1, a0 + a1))
}

/// Two rects overlapping by half, laid out in the plane's own basis.
fn two_rects(s: &mut Scene, c: DVec3, n: DVec3, up: DVec3) {
    let right = n.cross(up).normalize();
    for k in [-25.0_f64, 25.0] {
        s.execute(Command::DrawRectAsShape {
            center: c + right * k,
            normal: n,
            up,
            width: 100.0,
            height: 100.0,
        });
    }
}

/// Two circles overlapping, same idea.
fn two_circles(s: &mut Scene, c: DVec3, n: DVec3, up: DVec3) {
    let right = n.cross(up).normalize();
    for k in [-25.0_f64, 25.0] {
        s.execute(Command::DrawCircleAsShape {
            center: c + right * k,
            normal: n,
            radius: 50.0,
            segments: 48,
        });
    }
}

#[test]
fn a_single_shape_lands_on_whatever_plane_it_was_asked_for() {
    for (name, n, up) in planes() {
        let mut s = prod();
        s.execute(Command::DrawRectAsShape {
            center: DVec3::ZERO,
            normal: n,
            up,
            width: 100.0,
            height: 100.0,
        });
        let total = active_faces(&s);
        let on_plane = faces_on_plane(&s, n);
        assert!(total >= 1, "{name}: no face was created at all");
        assert_eq!(
            on_plane, total,
            "{name}: {on_plane} of {total} faces carry the requested normal — \
             a face landed on a plane other than the one asked for"
        );
        assert!(
            s.mesh.verify_face_invariants().is_valid(),
            "{name}: invariants dirty after a single rect"
        );
    }
}

#[test]
fn two_overlapping_rects_give_the_control_planes_answer_on_every_plane() {
    let mut seen = Vec::new();
    for (name, n, up) in planes() {
        let mut s = prod();
        two_rects(&mut s, DVec3::ZERO, n, up);
        seen.push((name, active_faces(&s), faces_on_plane(&s, n)));
        assert!(
            s.mesh.verify_face_invariants().is_valid(),
            "{name}: invariants dirty after two overlapping rects"
        );
    }
    for (name, total, on_plane) in &seen {
        println!("  {name:<14} regions {total}  on-plane {on_plane}");
    }
    let (_, want, want_on) = seen[0];
    assert_eq!(want, 3, "the control plane stopped giving three regions");
    for (name, total, on_plane) in &seen {
        assert_eq!(
            *total, want,
            "{name}: {total} regions where the control plane gives {want} — \
             the arrangement depends on the axis"
        );
        assert_eq!(
            *on_plane, want_on,
            "{name}: {on_plane} regions carry the plane normal, control has {want_on}"
        );
    }
}

#[test]
fn two_overlapping_circles_give_the_control_planes_answer_on_every_plane() {
    let mut seen = Vec::new();
    for (name, n, up) in planes() {
        let mut s = prod();
        two_circles(&mut s, DVec3::ZERO, n, up);
        seen.push((name, active_faces(&s)));
        assert!(
            s.mesh.verify_face_invariants().is_valid(),
            "{name}: invariants dirty after two overlapping circles"
        );
    }
    let (_, want) = seen[0];
    assert_eq!(want, 3, "the control plane stopped giving three regions");
    for (name, total) in &seen {
        assert_eq!(
            *total, want,
            "{name}: {total} regions where the control plane gives {want}"
        );
    }
}

#[test]
fn a_rect_drawn_on_a_solids_face_divides_it_whichever_face_it_is() {
    // The solid stays axis-aligned; what changes is WHICH of its faces is drawn
    // on. A side must divide exactly as a top does — the axis question asked of
    // a solid rather than a sheet.
    for (name, n, up) in [
        ("+Z top", DVec3::Z, DVec3::X),
        ("+X side", DVec3::X, DVec3::Z),
        ("-Y side", -DVec3::Y, DVec3::Z),
    ] {
        let mut s = prod();
        s.mesh
            .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, FORM_MATERIAL);
        let before = active_faces(&s);
        s.execute(Command::DrawRectAsShape {
            center: n * 100.0,
            normal: n,
            up,
            width: 60.0,
            height: 60.0,
        });
        let after = active_faces(&s);
        assert!(
            after > before,
            "{name}: {before} -> {after} — the rect did not divide the face"
        );
        assert!(
            s.mesh.verify_face_invariants().is_valid(),
            "{name}: invariants dirty"
        );
    }
}


#[test]
fn a_rect_lands_on_a_genuinely_TILTED_solid_face() {
    // The three faces above are all cardinal — the box never left its axes, so
    // "+X side" tests a different axis, not an oblique one. This rotates the
    // whole solid so its top is a real oblique plane, then draws on it.
    //
    // ⚠ The angle is 37°, not 45°: at 45° a swapped sin/cos reads correct, and
    // this file's header records what that cost once already.
    let axis = DVec3::new(1.0, 1.0, 0.0).normalize();
    let ang = 37.0_f64.to_radians();

    let mut s = prod();
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, FORM_MATERIAL);
    let all: Vec<_> = s.mesh.verts.iter().map(|(v, _)| v).collect();
    s.mesh
        .rotate_verts(&all, DVec3::ZERO, axis, ang)
        .expect("rotating the box");
    assert!(
        s.mesh.verify_face_invariants().is_valid(),
        "rotating the box alone already dirtied the invariants"
    );

    // Where the top went, and the point at its centre.
    let q = glam::DQuat::from_axis_angle(axis, ang);
    let n = q * DVec3::Z;
    let up = q * DVec3::X;
    let c = q * (DVec3::Z * 100.0);
    assert!(
        n.dot(DVec3::Z).abs() < 0.999,
        "the test plane is still cardinal — nothing oblique was measured"
    );

    let before = active_faces(&s);
    s.execute(Command::DrawRectAsShape {
        center: c,
        normal: n,
        up,
        width: 60.0,
        height: 60.0,
    });
    let after = active_faces(&s);

    // The same draw on the SAME box, untilted. `after > before` alone would
    // pass at 7 and at 8 alike, so it cannot tell "divided" from "divided
    // differently" — the control has to name the number, as it does for the
    // sheet rows above.
    let mut ctl = prod();
    ctl.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, FORM_MATERIAL);
    let ctl_before = active_faces(&ctl);
    ctl.execute(Command::DrawRectAsShape {
        center: DVec3::Z * 100.0,
        normal: DVec3::Z,
        up: DVec3::X,
        width: 60.0,
        height: 60.0,
    });
    let ctl_after = active_faces(&ctl);

    println!("  cardinal top   faces {ctl_before} -> {ctl_after}");
    println!("  tilted   top   faces {before} -> {after}");
    assert!(
        ctl_after > ctl_before,
        "the control stopped dividing ({ctl_before} -> {ctl_after}) — this test\
         can no longer say anything about the tilt"
    );
    assert_eq!(
        after - before,
        ctl_after - ctl_before,
        "tilted top gained {} faces where the cardinal top gains {} — \
         the axis mattered",
        after - before,
        ctl_after - ctl_before
    );

    // …and the pieces are on the tilted top itself, not floating beside it.
    let (on_top, area) = plane_census(&s, n, c.dot(n));
    let (ctl_on_top, ctl_area) = plane_census(&ctl, DVec3::Z, 100.0);
    println!("  cardinal top plane: {ctl_on_top} faces, area {ctl_area:.1}");
    println!("  tilted   top plane: {on_top} faces, area {area:.1}");
    assert_eq!(
        ctl_on_top, 2,
        "the control's top should be two pieces (rect + remainder), not {ctl_on_top}"
    );
    assert!(
        (ctl_area - 40_000.0).abs() < 1.0,
        "the control top tiles to {ctl_area:.1}, not 40,000 — something is \
         doubled or missing before the tilt is even asked about"
    );
    assert_eq!(
        on_top, ctl_on_top,
        "the tilted top holds {on_top} faces where the cardinal top holds \
         {ctl_on_top} — either it did not divide, or the rect landed elsewhere"
    );
    assert!(
        (area - ctl_area).abs() < 1.0,
        "the tilted top tiles to {area:.1} where the cardinal top tiles to \
         {ctl_area:.1} — the axis mattered"
    );
    assert!(
        s.mesh.verify_face_invariants().is_valid(),
        "invariants dirty after drawing on a tilted solid face"
    );
}
