//! Cross-bore step 3: two through-bores of DIFFERENT radii open into each other.
//!
//! ## What was actually in the way
//!
//! The records said this needed SSI subdivision, because
//! `cylinder_cylinder_branches` defers on unequal radii — "a genuine quartic
//! with no plane decomposition". That is true of the CURVE and irrelevant to
//! this surgery, which never wanted the curve: it wants, per station, how much
//! of the wall is standing inside the other bore. Measured 2026-09-05, that is
//! closed form for any two radii:
//!
//! ```text
//!   s² < r_other² − r_self² sin²θ          θ measured from the other axis
//!
//!   r 40×25, 40×8, 25×40 — every station's point on BOTH cylinders to 7.1e-15
//! ```
//!
//! Equal radii collapse it to `r|cos θ|`, which is what the code had.
//!
//! **The obstacle was the segmentation.** Both walls were cut at their own
//! stations, and those coincide only when the radii are equal — A's station θ
//! then lands on B's π/2 − θ. Unequal radii map through
//! `sin φ = −(r_a/r_b) sin θ`, which no pair of uniform segmentations respects.
//! Measured, at r 40 × 25:
//!
//! ```text
//!   n      8      12      16      32      64     128
//!   met  4/4    4/12    4/12    4/28    4/52   4/108
//! ```
//!
//! Four, always — the pinch points, and nothing else, at any refinement. So
//! both walls are now split at the UNION of the two station sets first
//! (`split_bore_wall_at_angles`), after which every cut edge runs between the
//! same two shared points on both sides and `add_vertex`'s position dedup welds
//! them.
//!
//! ## Three things measurement corrected on the way
//!
//! ⚠ **Rebuilding a quad from new corners opens the solid.** A tube quad's two
//! angular edges are shared with the shell's hole loop, so a corner only the
//! wall knows about leaves the shell still spanning the old chord: 8 tube faces
//! became 12, `valid = true`, `closed = FALSE`. Splitting the EDGES instead
//! updates both incident faces and the rim follows.
//!
//! ⚠ **A split piece inherits the whole barrel.** `split_face` hands each child
//! the parent's surface unchanged, and a bore quad's `u_range` is its own
//! angular slice, so both halves claimed the slice and every volume reader
//! counted it twice — 891,210 reported removed by a bore whose entire volume is
//! 392,699. Re-attaching the barrel per child fixes it. This is #112's finding
//! one level down.
//!
//! ⚠ **`SelfIntersectionReport::count()` counts contact, not damage.** The
//! crossing seam is two walls meeting at an angle, which is `Touching` — the
//! non-defect kind. Every pair this produces classifies that way (6 at n32, 11
//! at n64, 0 coplanar, 0 crossing). Asserting on `count()` would have failed a
//! sound solid; this file asks `classify_contact`.

use axia_geo::operations::cross_bore::{band_at, shared_crossing_cuts};
use axia_geo::operations::self_intersect::ContactKind;
use axia_geo::Mesh;
use glam::DVec3;

const BOX: f64 = 200.0;

fn bored(radius: f64, segments: u32) -> Mesh {
    let mut mesh = Mesh::new();
    mesh.create_box(DVec3::ZERO, BOX, BOX, BOX, Default::default()).expect("box");
    mesh.drill_circular_through_hole(DVec3::new(0.0, 0.0, BOX / 2.0), DVec3::Z, radius, segments)
        .expect("bore along Z");
    mesh
}

/// Damage-class self-intersections only, with the analytic surfaces stripped.
///
/// ⚠ Both halves matter. The detector is blind to curved analytic faces
/// (ADR-271), so a mesh that still carries its `Cylinder`s answers 0 whatever
/// is wrong with it; and `Touching` is not a defect, so counting pairs on the
/// stripped clone answers non-zero whatever is right with it.
fn damage(mesh: &Mesh) -> usize {
    let mut bare = mesh.clone();
    let ids: Vec<_> =
        bare.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
    for id in ids {
        bare.set_face_surface(id, None);
    }
    let report = bare.detect_self_intersections();
    report
        .intersecting_pairs
        .iter()
        .filter(|(a, b)| bare.classify_contact(*a, *b).is_some_and(ContactKind::is_damage))
        .count()
}

/// Any face corner standing inside BOTH bores — the wall the crossing was
/// supposed to take away.
fn intruders(mesh: &Mesh, ra: f64, rb: f64) -> usize {
    let mut n = 0;
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        for p in mesh.face_outline_points(fid).unwrap_or_default() {
            let in_a = (p.x * p.x + p.y * p.y).sqrt() < ra - 1e-6;
            let in_b = (p.y * p.y + p.z * p.z).sqrt() < rb - 1e-6;
            if in_a && in_b {
                n += 1;
            }
        }
    }
    n
}

/// The band is exact for any two radii — this is the whole of the geometry that
/// was said to need SSI.
#[test]
fn the_band_is_closed_form_for_any_two_radii() {
    let a = DVec3::Z;
    let b = DVec3::X;
    let e2 = a.cross(b);
    for (r_self, r_other) in [(40.0_f64, 40.0_f64), (40.0, 25.0), (40.0, 8.0), (25.0, 40.0)] {
        let mut worst_self: f64 = 0.0;
        let mut worst_other: f64 = 0.0;
        let mut live = 0usize;
        for k in 0..720 {
            let t = std::f64::consts::TAU * k as f64 / 720.0;
            let (sin, cos) = t.sin_cos();
            let radial = b * cos + e2 * sin;
            let s = band_at(radial, b, r_self, r_other);
            if s <= 0.0 {
                continue;
            }
            live += 1;
            let p = radial * r_self + a * s;
            worst_self = worst_self.max(((p.x * p.x + p.y * p.y).sqrt() - r_self).abs());
            worst_other = worst_other.max(((p.y * p.y + p.z * p.z).sqrt() - r_other).abs());
        }
        assert!(live > 50, "r {r_self}/{r_other}: only {live} stations have a band");
        assert!(
            worst_self < 1e-12 && worst_other < 1e-12,
            "r {r_self}/{r_other}: band points are {worst_self:e} / {worst_other:e} off the \
             two cylinders — the closed form is meant to be exact"
        );
    }
    // The wider tube loses only an arc; the narrower one is cut all the way
    // round. That asymmetry is why one of them needs the pinch points.
    let radial_side = b * (0.9_f64).cos() + e2 * (0.9_f64).sin();
    assert_eq!(band_at(radial_side, b, 40.0, 8.0), 0.0, "a r=40 wall is clear of a r=8 bore here");
    assert!(band_at(radial_side, b, 8.0, 40.0) > 0.0, "a r=8 wall is inside a r=40 bore everywhere");
}

/// The measurement that justifies splitting at all: uniform stations do not
/// sew unless the radii are equal.
#[test]
fn uniform_stations_only_sew_when_the_radii_match() {
    let meet = DVec3::ZERO;
    let met = |ra: f64, rb: f64, n: u32| -> (usize, usize) {
        let cuts =
            shared_crossing_cuts(meet, DVec3::Z, DVec3::X, ra, DVec3::X, DVec3::Y, rb, n, n)
                .expect("cuts");
        // A point is "met" by both when it is a station point of each side.
        let on_a = |c: &axia_geo::operations::cross_bore::SharedCut| {
            let step = std::f64::consts::TAU / n as f64;
            (c.theta_a / step - (c.theta_a / step).round()).abs() < 1e-6
        };
        let on_b = |c: &axia_geo::operations::cross_bore::SharedCut| {
            let step = std::f64::consts::TAU / n as f64;
            ((c.theta_b - std::f64::consts::FRAC_PI_2) / step
                - ((c.theta_b - std::f64::consts::FRAC_PI_2) / step).round())
            .abs()
                < 1e-6
        };
        (cuts.iter().filter(|c| on_a(c) && on_b(c)).count(), cuts.len())
    };

    for n in [8u32, 16, 32, 64] {
        let (both, total) = met(40.0, 40.0, n);
        assert_eq!(both, total, "equal radii, n={n}: {both} of {total} points are stations of both");
    }
    for n in [16u32, 32, 64, 128] {
        let (both, total) = met(40.0, 25.0, n);
        assert!(
            both * 4 < total,
            "unequal radii, n={n}: {both} of {total} points are stations of both. If most of \
             them now are, uniform segmentations sew after all and the splitting below is \
             unnecessary work."
        );
    }
}

/// The control. This is the case that worked before, and it has to keep
/// working exactly as well.
#[test]
fn equal_radii_still_cross() {
    let mut mesh = bored(40.0, 32);
    let before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    mesh.drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, 0.0), DVec3::X, 40.0, 32)
        .expect("equal radii cross");

    let inv = mesh.verify_face_invariants();
    assert!(inv.is_valid(), "{} violations", inv.violations.len());
    assert!(mesh.verify_outward_normals().is_closed_solid, "the solid opened");
    assert_eq!(damage(&mesh), 0, "damage-class self-intersections");
    assert_eq!(intruders(&mesh, 40.0, 40.0), 0, "a wall is standing inside both bores");
    assert!(
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count() > before,
        "the crossing bore added no faces"
    );
}

/// Step 3 itself.
#[test]
fn unequal_radii_cross() {
    for (ra, rb, seg_a, seg_b) in [
        (40.0_f64, 25.0_f64, 32u32, 32u32),
        (40.0, 25.0, 16, 32), // and the two segmentations need not match either
        (40.0, 8.0, 32, 32),
        (25.0, 40.0, 32, 32), // the NEW bore is the wider one
    ] {
        let mut mesh = bored(ra, seg_a);
        mesh.drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, 0.0), DVec3::X, rb, seg_b)
            .unwrap_or_else(|e| panic!("r {ra}/{rb} seg {seg_a}/{seg_b}: {e}"));

        let inv = mesh.verify_face_invariants();
        assert!(inv.is_valid(), "r {ra}/{rb}: {} violations", inv.violations.len());
        assert!(
            mesh.verify_outward_normals().is_closed_solid,
            "r {ra}/{rb} seg {seg_a}/{seg_b}: the solid opened — the two walls did not weld"
        );
        assert_eq!(damage(&mesh), 0, "r {ra}/{rb}: damage-class self-intersections");
        assert_eq!(
            intruders(&mesh, ra, rb),
            0,
            "r {ra}/{rb}: a wall is standing inside both bores"
        );
    }
}

/// What the second bore takes away, against the closed-form lens.
///
/// The residual is the seam's chord approximation and shrinks with refinement,
/// on equal radii exactly as on unequal — so it is the tessellation's, not this
/// change's.
#[test]
fn the_removed_volume_converges_on_the_analytic_lens() {
    // vol(cyl_R about Z ∩ cyl_r about X), axes meeting, r ≤ R: for |y| ≤ r the
    // slice is 2√(R²−y²) by 2√(r²−y²).
    let lens = |rr: f64, r: f64| -> f64 {
        let n = 200_000;
        let mut acc = 0.0;
        for i in 0..n {
            let y = -r + 2.0 * r * (i as f64 + 0.5) / n as f64;
            acc += 4.0 * (rr * rr - y * y).sqrt() * (r * r - y * y).sqrt();
        }
        acc * (2.0 * r / n as f64)
    };

    for (ra, rb) in [(40.0_f64, 40.0_f64), (40.0, 25.0), (40.0, 8.0)] {
        let want = std::f64::consts::PI * rb * rb * BOX - lens(ra.max(rb), ra.min(rb));
        let mut previous = f64::MAX;
        for seg in [16u32, 32, 64] {
            let mut mesh = bored(ra, seg);
            let before = mesh.mesh_volume();
            mesh.drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, 0.0), DVec3::X, rb, seg)
                .expect("cross");
            let removed = before - mesh.mesh_volume();
            let error = (removed - want).abs() / want;
            assert!(
                error < previous,
                "r {ra}/{rb} n={seg}: removed {removed:.1} against {want:.1} ({:.2}%), which is \
                 not closer than the coarser run",
                error * 100.0
            );
            previous = error;
        }
        assert!(
            previous < 0.03,
            "r {ra}/{rb}: still {:.2}% off the analytic lens at n=64",
            previous * 100.0
        );
    }
}

/// The splitter on its own: a wall gains stations, and the shell follows.
///
/// ⚠ `closed` is the assertion that matters. Rebuilding the quad from new
/// corners passes `valid` and fails this.
#[test]
fn splitting_a_wall_keeps_the_solid_closed() {
    let mut mesh = bored(40.0, 8);
    let tube: Vec<_> = mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && matches!(
                    mesh.face_surface(*fid),
                    Some(axia_geo::surfaces::AnalyticSurface::Cylinder { .. })
                )
        })
        .map(|(fid, _)| fid)
        .collect();
    assert_eq!(tube.len(), 8, "an 8-segment bore should have 8 wall quads");

    // Angles no 8-segment tube has a station at.
    let extra: Vec<f64> = [10.0_f64, 100.0, 200.0, 300.0].iter().map(|d| d.to_radians()).collect();
    let after = mesh
        .split_bore_wall_at_angles(&tube, DVec3::ZERO, DVec3::Z, DVec3::X, 40.0, &extra)
        .expect("split");

    assert_eq!(after.len(), 12, "four angles inside four different quads");
    assert!(mesh.verify_face_invariants().is_valid());
    assert!(
        mesh.verify_outward_normals().is_closed_solid,
        "splitting the wall opened the solid — the shell's hole loop still spans the old chord"
    );
    assert_eq!(damage(&mesh), 0);

    let mut worst: f64 = 0.0;
    for &f in &after {
        for p in mesh.face_outline_points(f).unwrap_or_default() {
            worst = worst.max(((p.x * p.x + p.y * p.y).sqrt() - 40.0).abs());
        }
    }
    assert!(worst < 1e-9, "a new corner drifted {worst:e} off the bore");
}

/// What is still refused, and how.
///
/// ⚠ The plan cannot promise the seam closes — it knows the axes meet at right
/// angles and that a shared cut set exists, but whether both walls end up
/// carrying every point of it depends on where the drill's stations actually
/// land. Measured 2026-09-05: 8, 10, 14, 16, 30, 32 and 64 close; 9, 11, 13 and
/// 15 do not. So the surgery runs on a copy and the answer is CHECKED. A
/// refusal leaves the mesh byte-identical — faces 15 → 15, volume delta
/// 0.000000 — which is what lets the caller fall back to Boolean.
#[test]
fn a_count_the_seam_cannot_close_is_refused_with_the_mesh_untouched() {
    let mut mesh = bored(40.0, 9);
    let faces_before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
    let volume_before = mesh.mesh_volume();

    let err = mesh
        .drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, 0.0), DVec3::X, 40.0, 9)
        .expect_err("9 segments is measured not to close");
    assert!(err.to_string().contains("crossing bore"), "{err}");

    assert_eq!(
        faces_before,
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        "a refusal added or removed faces"
    );
    assert!(
        (mesh.mesh_volume() - volume_before).abs() < 1e-9,
        "a refusal moved the volume"
    );
    assert!(mesh.verify_outward_normals().is_closed_solid, "a refusal left the solid open");

    // The neighbouring even counts are the control: without them this test
    // would also pass if the drill refused everything.
    for n in [8u32, 10] {
        let mut ok = bored(40.0, n);
        ok.drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, 0.0), DVec3::X, 40.0, n)
            .unwrap_or_else(|e| panic!("n={n} should still cross: {e}"));
        assert!(ok.verify_outward_normals().is_closed_solid, "n={n}");
    }
}

/// Still refused, and for the reason given: the shared set needs axes that meet
/// at right angles.
#[test]
fn a_skew_or_oblique_crossing_is_still_refused() {
    let mut mesh = bored(40.0, 32);
    // Oblique: 45° to the existing bore.
    let oblique = DVec3::new(1.0, 0.0, 1.0).normalize();
    assert!(
        mesh.drill_crossing_bore(DVec3::new(BOX / 2.0, 0.0, BOX / 2.0), oblique, 25.0, 32).is_err(),
        "an oblique crossing was accepted; the trim assumes the two branches are \
         symmetric about the meeting plane, which needs a right angle"
    );

    // Skew: parallel to X but offset in Y, so the axes never meet.
    let mut mesh2 = bored(40.0, 32);
    assert!(
        mesh2
            .drill_crossing_bore(DVec3::new(BOX / 2.0, 70.0, 0.0), DVec3::X, 25.0, 32)
            .is_err(),
        "a skew crossing was accepted"
    );
}
