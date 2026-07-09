//! Cylinder primitive (Phase D, ADR-031).
//!
//! Right-circular cylinder. Parametric:
//!
//! ```text
//! P(u, v) = axis_origin + R · (cos(u)·ref + sin(u)·perp) + v · axis
//! ```
//!
//! where `ref = ortho(ref_dir, axis)` and `perp = axis × ref` (right-handed).
//!
//! - `u`: angle in radians (0 = ref direction, π/2 = perp direction)
//! - `v`: distance along axis (mm)
//!
//! Outward normal: `cos(u)·ref + sin(u)·perp` (radial unit vector).

use glam::DVec3;

use super::orthonormal_ref;

#[inline]
fn basis(axis_dir: DVec3, ref_dir: DVec3) -> (DVec3, DVec3, DVec3) {
    let axis = axis_dir.normalize_or_zero();
    let r = orthonormal_ref(axis, ref_dir);
    let p = axis.cross(r).normalize_or_zero();
    (axis, r, p)
}

pub fn evaluate(
    axis_origin: DVec3,
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    u: f64,
    v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    axis_origin + r * (radius * u.cos()) + p * (radius * u.sin()) + axis * v
}

pub fn normal(
    _axis_origin: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    u: f64,
    _v: f64,
) -> DVec3 {
    let (_axis, r, p) = basis(axis_dir, ref_dir);
    r * u.cos() + p * u.sin()
}

pub fn derivative_u(
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    u: f64,
    _v: f64,
) -> DVec3 {
    let (_axis, r, p) = basis(axis_dir, ref_dir);
    r * (-radius * u.sin()) + p * (radius * u.cos())
}

/// ADR-257 β-1 (P3-B cylinder wall circle sketching) — project a world
/// point onto the cylinder lateral surface (closest point along the radial
/// direction from the axis) and return its surface parameters.
///
/// Returns `(surface_pt, u, v)` where `v` is the signed height along the
/// axis and `u` is the circumferential angle (matching `evaluate`'s
/// convention: 0 = `ref`, π/2 = `perp`). Closed-form (no Newton), mirror of
/// `sphere::project_to_surface`. Every returned `surface_pt` lies exactly on
/// the cylinder: `|surface_pt - foot| == radius` and
/// `evaluate(.., u, v) == surface_pt`.
///
/// `None` if the cylinder is degenerate (radius ≤ 0 or zero axis) or the
/// point is on the axis (radial direction undefined).
#[inline]
pub fn project_to_cylinder(
    axis_origin: DVec3,
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    p: DVec3,
) -> Option<(DVec3, f64, f64)> {
    if !(radius > 0.0) {
        return None;
    }
    let (axis, r, perp) = basis(axis_dir, ref_dir);
    if axis.length_squared() < 0.5 {
        return None; // degenerate axis
    }
    let d = p - axis_origin;
    let v = d.dot(axis); // signed height along the axis
    let foot = axis_origin + axis * v; // closest axis point
    let radial = p - foot;
    let radial_dist = radial.length();
    if radial_dist < 1e-12 {
        return None; // on the axis — radial direction undefined
    }
    let radial_unit = radial / radial_dist;
    let surface_pt = foot + radial_unit * radius;
    // Circumferential angle in the (r, perp) basis (matches `evaluate`,
    // where the radial direction = cos(u)·r + sin(u)·perp).
    let u = radial_unit.dot(perp).atan2(radial_unit.dot(r));
    Some((surface_pt, u, v))
}

/// ADR-257 β-2 (P3-B) — generate the closed "porthole" geodesic circle drawn
/// ON a cylinder wall, as a polyline of surface points.
///
/// The cylinder is developable (zero Gaussian curvature), so unrolling it to
/// a flat strip is isometric: a geodesic circle of radius `rho` around the
/// `center_pt` is a true flat circle in unroll `(R·u, v)` space, mapped back
/// to 3D. `rho` = the flat (geodesic) distance from `center_pt` to
/// `radius_pt` (both projected to the surface; the angular gap uses the
/// SHORTEST signed difference so the circle is centered correctly).
///
/// Returns `N` distinct surface points (closed loop — the caller connects
/// last→first), `N ∈ [24, 64]` by `chord_tol`. Every point lies exactly on
/// the cylinder. The curve is generally **non-planar** (skew/helical) — it is
/// NOT an `AnalyticCurve::Circle`; hence P3-B uses a polyline boundary.
///
/// `None` if either point fails to project, the radius is ~0, or
/// `rho >= π·R` (the flat circle would wrap past the half-circumference and
/// self-overlap when re-wrapped — the MVP self-overlap guard, L-257-7).
pub fn circle_on_cylinder(
    axis_origin: DVec3,
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    center_pt: DVec3,
    radius_pt: DVec3,
    chord_tol: f64,
) -> Option<Vec<DVec3>> {
    use std::f64::consts::{PI, TAU};
    // 1. Project both points to surface params (u angle, v height).
    let (_cp, u0, v0) = project_to_cylinder(axis_origin, axis_dir, radius, ref_dir, center_pt)?;
    let (_rp, u1, v1) = project_to_cylinder(axis_origin, axis_dir, radius, ref_dir, radius_pt)?;
    // 2. Unroll: geodesic radius = flat distance. The angular gap wraps, so
    //    use the SHORTEST signed difference in (-π, π].
    let mut du = (u1 - u0) % TAU;
    if du > PI {
        du -= TAU;
    } else if du < -PI {
        du += TAU;
    }
    let flat_du = radius * du; // arc-length offset along the circumference
    let flat_dv = v1 - v0;
    let rho = (flat_du * flat_du + flat_dv * flat_dv).sqrt();
    // 3. Guards.
    if rho < crate::tolerances::EPSILON_LENGTH {
        return None; // zero-radius (coincident points)
    }
    if rho >= PI * radius {
        return None; // would wrap past half-circumference → self-overlap (L-257-7)
    }
    // 4. Sample N points on the flat circle around (u0, v0); map each back.
    let n = crate::curves::circle::segment_count_for_arc(rho, TAU, chord_tol).clamp(24, 64);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let theta = TAU * (i as f64) / (n as f64);
        let u_back = u0 + (rho * theta.cos()) / radius; // un-unroll the angular offset
        let v_back = v0 + rho * theta.sin();
        pts.push(evaluate(axis_origin, axis_dir, radius, ref_dir, u_back, v_back));
    }
    Some(pts)
}

/// ADR-284 β-1 — project a drawn POLYLINE (arbitrary world points: rect corners,
/// polygon verts, freehand / bezier tessellation) onto this cylinder and
/// geodesically sample it into on-surface points, ready for
/// [`crate::mesh::Mesh::split_cylinder_face_by_circle`] (the shape-agnostic
/// closed-polyline splitter). Generalizes [`circle_on_cylinder`] from a circle
/// to any polyline.
///
/// `closed` appends the closing edge (last → first). The cylinder is developable,
/// so a geodesic edge = a straight segment in flat (u·radius, v) space; each edge
/// is chord-sampled. `u` is unrolled continuously across points so a shape
/// spanning the ref_dir seam is handled.
///
/// `None` if fewer than 2 points, any point is not on this cylinder (> 1e-3 mm),
/// the loop wraps ≥ full circumference (ill-defined inside/outside), or fewer
/// than 3 output samples.
pub fn polyline_on_cylinder(
    axis_origin: DVec3,
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    pts: &[DVec3],
    closed: bool,
    chord_tol: f64,
) -> Option<Vec<DVec3>> {
    use std::f64::consts::{PI, TAU};
    if pts.len() < 2 {
        return None;
    }
    // Project every point to (u, v), unrolling u continuously from the first.
    let mut uv: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    let mut u_prev = 0.0;
    for (i, &p) in pts.iter().enumerate() {
        let (sp, u, v) = project_to_cylinder(axis_origin, axis_dir, radius, ref_dir, p)?;
        if (sp - p).length() > 1e-3 {
            return None; // not on this cylinder (lenient tol for drawn points)
        }
        let u_cont = if i == 0 {
            u
        } else {
            let mut uu = u;
            while uu - u_prev > PI {
                uu -= TAU;
            }
            while uu - u_prev < -PI {
                uu += TAU;
            }
            uu
        };
        u_prev = u_cont;
        uv.push((u_cont, v));
    }
    let n = uv.len();
    // Wrap guard. CLOSED: the loop must not ENCIRCLE the axis — a simple
    // non-encircling loop has total signed angular winding ≈ 0, an encircling
    // one ≈ ±2π (no in-between for a simple curve), so reject |winding| > π.
    // OPEN: the vertex angular span must stay under a full turn.
    if closed {
        let mut winding = 0.0;
        for e in 0..n {
            let mut du = (uv[(e + 1) % n].0 - uv[e].0) % TAU;
            if du > PI {
                du -= TAU;
            } else if du < -PI {
                du += TAU;
            }
            winding += du;
        }
        if winding.abs() > PI {
            return None;
        }
    } else {
        let (umin, umax) = uv
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), &(u, _)| (a.min(u), b.max(u)));
        if umax - umin >= TAU - 1e-6 {
            return None;
        }
    }
    // Chord-sample each edge in flat UV; map back with `evaluate`. Each edge
    // emits its START plus interior points (NOT its end) so the concatenation
    // has no duplicate vertices (the split expects a clean loop).
    let n = uv.len();
    let edge_count = if closed { n } else { n - 1 };
    let mut out: Vec<DVec3> = Vec::new();
    for e in 0..edge_count {
        let (u0, v0) = uv[e];
        let (u1, v1) = uv[(e + 1) % n];
        let flat = (((u1 - u0) * radius).powi(2) + (v1 - v0).powi(2)).sqrt();
        let k = ((flat / chord_tol.max(1e-6)).ceil() as usize).clamp(1, 64);
        for s in 0..k {
            let t = s as f64 / k as f64;
            let u = u0 + (u1 - u0) * t;
            let v = v0 + (v1 - v0) * t;
            out.push(evaluate(axis_origin, axis_dir, radius, ref_dir, u, v));
        }
    }
    if out.len() < 3 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn evaluate_at_u_zero_v_zero_is_axis_origin_plus_radius_ref() {
        let p = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, 0.0, 0.0);
        assert!((p - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-12);
    }

    #[test]
    fn evaluate_quarter_angle_is_perp_direction() {
        // axis = Z, ref = X → perp = Z × X = Y
        let p = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, FRAC_PI_2, 0.0);
        assert!((p - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-12);
    }

    #[test]
    fn evaluate_v_translates_along_axis() {
        let p = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, 0.0, 10.0);
        assert!((p - DVec3::new(5.0, 0.0, 10.0)).length() < 1e-12);
    }

    #[test]
    fn evaluate_offset_axis_origin() {
        let p = evaluate(DVec3::new(1.0, 2.0, 3.0), DVec3::Z, 5.0, DVec3::X, 0.0, 0.0);
        assert!((p - DVec3::new(6.0, 2.0, 3.0)).length() < 1e-12);
    }

    #[test]
    fn evaluate_radius_invariant_at_any_height() {
        // Distance from axis should equal radius regardless of v.
        for v in [0.0, 5.0, -3.0, 100.0] {
            for u_step in 0..8 {
                let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
                let p = evaluate(DVec3::ZERO, DVec3::Z, 7.0, DVec3::X, u, v);
                let radial = DVec3::new(p.x, p.y, 0.0).length();
                assert!((radial - 7.0).abs() < 1e-12,
                    "u={}, v={}: radial={} ≠ 7", u, v, radial);
            }
        }
    }

    // ── ADR-257 β-1 — project_to_cylinder ─────────────────────────────────
    /// Radial distance of `pt` from the axis line through `origin` along `dir`.
    fn radial_from_axis(pt: DVec3, origin: DVec3, dir: DVec3) -> f64 {
        let axis = dir.normalize();
        let d = pt - origin;
        (d - axis * d.dot(axis)).length()
    }

    #[test]
    fn project_to_cylinder_point_lands_on_surface() {
        // Off-surface point (radial 10) projects to radial == R (5) at v=3.
        let (sp, u, v) = project_to_cylinder(
            DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, DVec3::new(10.0, 0.0, 3.0),
        )
        .expect("must project");
        assert!((radial_from_axis(sp, DVec3::ZERO, DVec3::Z) - 5.0).abs() < 1e-9,
            "surface point must be at radius 5 (got {})", sp);
        assert!((v - 3.0).abs() < 1e-9, "v (height) must be 3 (got {})", v);
        assert!(u.abs() < 1e-9, "u must be 0 (radial = +X = ref) (got {})", u);
        assert!((sp - DVec3::new(5.0, 0.0, 3.0)).length() < 1e-9);
    }

    #[test]
    fn project_to_cylinder_roundtrip_evaluate() {
        // evaluate(u, v) must reproduce the projected surface point exactly.
        let (sp, u, v) = project_to_cylinder(
            DVec3::new(1.0, 2.0, 3.0), DVec3::Z, 7.0, DVec3::X,
            DVec3::new(9.0, 6.0, 8.0),
        )
        .expect("must project");
        let back = evaluate(DVec3::new(1.0, 2.0, 3.0), DVec3::Z, 7.0, DVec3::X, u, v);
        assert!((back - sp).length() < 1e-9,
            "evaluate(u,v) {} must equal surface_pt {}", back, sp);
    }

    #[test]
    fn project_to_cylinder_on_axis_returns_none() {
        // A point ON the axis has no defined radial direction.
        assert!(project_to_cylinder(
            DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, DVec3::new(0.0, 0.0, 4.0),
        )
        .is_none());
        // Degenerate radius rejected too.
        assert!(project_to_cylinder(
            DVec3::ZERO, DVec3::Z, 0.0, DVec3::X, DVec3::new(5.0, 0.0, 0.0),
        )
        .is_none());
    }

    #[test]
    fn project_to_cylinder_idempotent_on_surface() {
        // A point already on the surface projects to itself.
        let on = DVec3::new(5.0, 0.0, 3.0);
        let (sp, _u, _v) = project_to_cylinder(
            DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, on,
        )
        .expect("must project");
        assert!((sp - on).length() < 1e-9, "idempotent (got {})", sp);
    }

    #[test]
    fn project_to_cylinder_non_axis_aligned() {
        // axis = Y (not Z): surface point must still be at radius R from axis.
        let (sp, u, v) = project_to_cylinder(
            DVec3::ZERO, DVec3::Y, 5.0, DVec3::X, DVec3::new(8.0, 2.0, 1.0),
        )
        .expect("must project");
        assert!((radial_from_axis(sp, DVec3::ZERO, DVec3::Y) - 5.0).abs() < 1e-9,
            "radius from Y-axis must be 5 (got {})", sp);
        assert!((v - 2.0).abs() < 1e-9, "v along Y must be 2 (got {})", v);
        let back = evaluate(DVec3::ZERO, DVec3::Y, 5.0, DVec3::X, u, v);
        assert!((back - sp).length() < 1e-9, "roundtrip on Y-axis cylinder");
    }

    // ── ADR-257 β-2 — circle_on_cylinder (geodesic porthole) ─────────────
    #[test]
    fn circle_on_cylinder_samples_lie_on_surface() {
        // center on wall (u=0), radius point at angle 0.4 → rho = R·0.4 = 4.
        let pts = circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 10.0, DVec3::X,
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.0 * 0.4_f64.cos(), 10.0 * 0.4_f64.sin(), 0.0),
            0.05,
        )
        .expect("must generate");
        for p in &pts {
            assert!((radial_from_axis(*p, DVec3::ZERO, DVec3::Z) - 10.0).abs() < 1e-9,
                "every sample on cylinder surface (radius 10), got {}", p);
        }
    }

    #[test]
    fn circle_on_cylinder_geodesic_radius_preserved() {
        // Defining property: every sample is at flat (geodesic) distance rho
        // from the center. rho = R·0.4 = 4 for R=10.
        use std::f64::consts::{PI, TAU};
        let (origin, axis, refd, r) = (DVec3::ZERO, DVec3::Z, DVec3::X, 10.0);
        let rho = r * 0.4;
        let pts = circle_on_cylinder(
            origin, axis, r, refd,
            DVec3::new(10.0, 0.0, 0.0),
            DVec3::new(10.0 * 0.4_f64.cos(), 10.0 * 0.4_f64.sin(), 0.0),
            0.05,
        )
        .expect("must generate");
        // center params: u0=0, v0=0.
        for p in &pts {
            let (_sp, u, v) = project_to_cylinder(origin, axis, r, refd, *p).unwrap();
            let mut du = u % TAU;
            if du > PI { du -= TAU; } else if du < -PI { du += TAU; }
            let flat_dist = ((r * du).powi(2) + v.powi(2)).sqrt();
            assert!((flat_dist - rho).abs() < 1e-6,
                "geodesic distance {} must equal rho {}", flat_dist, rho);
        }
    }

    #[test]
    fn circle_on_cylinder_sample_count_bounded() {
        let pts = circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 10.0, DVec3::X,
            DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 4.0), 0.05,
        )
        .expect("must generate");
        assert!(pts.len() >= 24 && pts.len() <= 64, "N clamped 24..64 (got {})", pts.len());
    }

    #[test]
    fn circle_on_cylinder_self_overlap_clamped() {
        // rho >= pi*R (R=10 → 31.4) must be rejected. radius_pt far in v.
        assert!(circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 10.0, DVec3::X,
            DVec3::new(10.0, 0.0, 0.0), DVec3::new(10.0, 0.0, 35.0), 0.05,
        )
        .is_none(), "rho=35 >= pi*R=31.4 must clamp to None");
    }

    #[test]
    fn circle_on_cylinder_zero_radius_none() {
        // Coincident center/radius points → zero radius → None.
        assert!(circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 10.0, DVec3::X,
            DVec3::new(10.0, 0.0, 5.0), DVec3::new(10.0, 0.0, 5.0), 0.05,
        )
        .is_none());
    }

    #[test]
    fn polyline_on_cylinder_rect_samples_on_surface() {
        // ADR-284 β-1 — a rect (4 world corners) projects to a closed on-surface
        // loop; every sample lies on the cylinder + the loop is non-empty.
        let (ax_o, ax_d, rad, refd) = (DVec3::ZERO, DVec3::Z, 10.0, DVec3::X);
        let corners: Vec<DVec3> = [(-0.3, -3.0), (0.3, -3.0), (0.3, 3.0), (-0.3, 3.0)]
            .iter()
            .map(|&(u, v)| evaluate(ax_o, ax_d, rad, refd, u, v))
            .collect();
        let samples = polyline_on_cylinder(ax_o, ax_d, rad, refd, &corners, true, 0.5)
            .expect("rect projects onto cylinder");
        assert!(samples.len() >= 4, "closed rect loop has ≥4 samples");
        for &p in &samples {
            let (sp, _u, _v) = project_to_cylinder(ax_o, ax_d, rad, refd, p).unwrap();
            assert!((sp - p).length() < 1e-9, "every sample lies on the cylinder");
        }
    }

    #[test]
    fn polyline_on_cylinder_rejects_full_wrap() {
        // A loop spanning the whole circumference (4 corners at 0, π/2, π, 3π/2)
        // wraps → ill-defined inside/outside → None.
        let (ax_o, ax_d, rad, refd) = (DVec3::ZERO, DVec3::Z, 10.0, DVec3::X);
        use std::f64::consts::FRAC_PI_2;
        let corners: Vec<DVec3> = [0.0, FRAC_PI_2, 2.0 * FRAC_PI_2, 3.0 * FRAC_PI_2]
            .iter()
            .map(|&u| evaluate(ax_o, ax_d, rad, refd, u, 0.0))
            .collect();
        assert!(
            polyline_on_cylinder(ax_o, ax_d, rad, refd, &corners, true, 0.5).is_none(),
            "full-circumference loop → None (wrap guard)"
        );
    }

    #[test]
    fn circle_on_cylinder_belt_is_planar_but_off_axis_is_not() {
        // A belt circle (radius point at same v, pure angular offset) — every
        // sample at constant... no: even a pure-u circle straddles a u-range,
        // so it is generally non-planar. Sanity: a small porthole has a
        // non-zero z (axial) extent (it is not a flat ring).
        let pts = circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 10.0, DVec3::X,
            DVec3::new(10.0, 0.0, 5.0),
            DVec3::new(10.0 * 0.4_f64.cos(), 10.0 * 0.4_f64.sin(), 5.0),
            0.05,
        )
        .expect("must generate");
        let zmin = pts.iter().map(|p| p.z).fold(f64::INFINITY, f64::min);
        let zmax = pts.iter().map(|p| p.z).fold(f64::NEG_INFINITY, f64::max);
        // porthole spans v=5±rho → z extent ~2*rho = 8.
        assert!((zmax - zmin) > 1.0, "porthole has axial extent (got {})", zmax - zmin);
    }

    #[test]
    fn normal_unit_length() {
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, u, 0.0);
            assert!((n.length() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn normal_is_radial_outward() {
        let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 0.0, 0.0);
        assert!((n - DVec3::X).length() < 1e-12);
        let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, FRAC_PI_2, 0.0);
        assert!((n - DVec3::Y).length() < 1e-12);
    }

    #[test]
    fn derivative_u_perpendicular_to_radius() {
        // At any u, ∂P/∂u should be tangential (perpendicular to radial dir).
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, u, 0.0);
            let d = derivative_u(DVec3::Z, 5.0, DVec3::X, u, 0.0);
            assert!(d.normalize().dot(n).abs() < 1e-9,
                "u={}: derivative not perpendicular to normal", u);
        }
    }

    #[test]
    fn derivative_u_magnitude_equals_radius() {
        let d = derivative_u(DVec3::Z, 5.0, DVec3::X, 0.0, 0.0);
        assert!((d.length() - 5.0).abs() < 1e-12);
    }

    #[test]
    fn nonparallel_ref_dir_orthogonalized() {
        // ref_dir not perpendicular to axis — should still produce valid evaluation.
        // axis = Z, ref = X + 0.5Z (has component along axis)
        let p = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::new(1.0, 0.0, 0.5), 0.0, 0.0);
        // After orthogonalization, ref reduces to +X.
        assert!((p - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn full_circle_period_returns_to_start() {
        let p0 = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, 0.0, 0.0);
        let p1 = evaluate(DVec3::ZERO, DVec3::Z, 5.0, DVec3::X, std::f64::consts::TAU, 0.0);
        assert!((p0 - p1).length() < 1e-9);
    }
}
