//! Cone primitive (Phase D, ADR-031).
//!
//! Right-circular cone with apex at `apex`, opening along `axis_dir` with
//! half-angle `α`. Parametric:
//!
//! ```text
//! P(u, v) = apex + v · axis + v · tan(α) · (cos(u)·ref + sin(u)·perp)
//! ```
//!
//! - `u`: angular position (radians)
//! - `v`: distance from apex along axis (mm). v=0 = apex.
//! - `α = half_angle`: half-cone-angle, in (0, π/2).
//!
//! Outward normal at (u, v):
//! `cos(α) · radial_dir - sin(α) · axis_dir`
//! where radial_dir = cos(u)·ref + sin(u)·perp.

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
    apex: DVec3,
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    u: f64,
    v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    apex + axis * v + radial * (v * half_angle.tan())
}

pub fn normal(
    _apex: DVec3,
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    u: f64,
    _v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    (radial * half_angle.cos() - axis * half_angle.sin()).normalize_or_zero()
}

pub fn derivative_u(
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    u: f64,
    v: f64,
) -> DVec3 {
    let (_axis, r, p) = basis(axis_dir, ref_dir);
    let scale = v * half_angle.tan();
    r * (-scale * u.sin()) + p * (scale * u.cos())
}

pub fn derivative_v(
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    u: f64,
    _v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    axis + radial * half_angle.tan()
}

/// ADR-263 β-1 (P3-C) — project a world point onto a cone surface, returning
/// `(surface_pt, u, v)`. Mirror of [`crate::surfaces::cylinder::project_to_cylinder`],
/// but the cone radius grows with the axial distance (`= v·tan α`) rather than
/// being constant. For a point exactly on the cone the result round-trips
/// (`evaluate(u, v) == surface_pt == p`).
///
/// `None` if the cone is degenerate (`α ∉ (0, π/2)`), the axis is degenerate,
/// the point is at/behind the apex (`v ≤ 0` — the singularity), or the point
/// lies on the axis (radial direction undefined).
pub fn project_to_cone(
    apex: DVec3,
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    p: DVec3,
) -> Option<(DVec3, f64, f64)> {
    use std::f64::consts::FRAC_PI_2;
    if !(half_angle > 0.0 && half_angle < FRAC_PI_2) {
        return None; // flat / inverted / degenerate cone
    }
    let (axis, r, perp) = basis(axis_dir, ref_dir);
    if axis.length_squared() < 0.5 {
        return None; // degenerate axis
    }
    let d = p - apex;
    let v = d.dot(axis); // axial distance from the apex
    if v <= crate::tolerances::EPSILON_LENGTH {
        return None; // at or behind the apex (singularity)
    }
    let foot = apex + axis * v; // closest axis point
    let radial = p - foot;
    let radial_dist = radial.length();
    if radial_dist < 1e-12 {
        return None; // on the axis — radial direction undefined
    }
    let radial_unit = radial / radial_dist;
    // Surface point at this (u, v): the cone radius at axial `v` is `v·tan α`.
    let surface_pt = foot + radial_unit * (v * half_angle.tan());
    // Circumferential angle in the (r, perp) basis (matches `evaluate`, where
    // the radial direction = cos(u)·r + sin(u)·perp).
    let u = radial_unit.dot(perp).atan2(radial_unit.dot(r));
    Some((surface_pt, u, v))
}

/// ADR-263 β-1 (P3-C) — generate the closed "porthole" geodesic circle drawn
/// ON a cone wall, as a polyline of surface points.
///
/// The cone is **developable** (zero Gaussian curvature), so unrolling it to a
/// flat sector is isometric: a geodesic circle of radius `rho` around the
/// `center_pt` is a true flat circle in the unrolled sector, mapped back to 3D.
/// A surface point at axial `v` (slant `L = v/cos α`), angle `u` unrolls to flat
/// polar `(L, u·sin α)` — the full sweep `u ∈ [0, 2π]` becomes the flat angle
/// `[0, 2π·sin α]`. `rho` = the flat distance from `center_pt` to `radius_pt`
/// (both projected; the angular gap uses the SHORTEST signed difference).
///
/// Returns `N` distinct surface points (closed loop — the caller connects
/// last→first), `N ∈ [24, 64]` by `chord_tol`. Every point lies exactly on the
/// cone. The curve is generally **non-planar** — it is NOT an
/// `AnalyticCurve::Circle`; hence P3-C uses a polyline boundary (mirror of
/// [`crate::surfaces::cylinder::circle_on_cylinder`]).
///
/// `None` if either point fails to project, the radius is ~0, the circle would
/// reach the apex (`rho ≥ L0`), or its angular span wraps `≥ π` in `u` (the
/// self-overlap guard for a sharp cone, mirror of L-257-7).
pub fn circle_on_cone(
    apex: DVec3,
    axis_dir: DVec3,
    half_angle: f64,
    ref_dir: DVec3,
    center_pt: DVec3,
    radius_pt: DVec3,
    chord_tol: f64,
) -> Option<Vec<DVec3>> {
    use std::f64::consts::{PI, TAU};
    let sin_a = half_angle.sin();
    let cos_a = half_angle.cos();
    if !(sin_a > 1e-9 && cos_a > 1e-9) {
        return None;
    }
    // 1. Project both points → (u, v); slant length L = v/cos α (unroll radius).
    let (_cp, u0, v0) = project_to_cone(apex, axis_dir, half_angle, ref_dir, center_pt)?;
    let (_rp, u1, v1) = project_to_cone(apex, axis_dir, half_angle, ref_dir, radius_pt)?;
    let l0 = v0 / cos_a;
    let l1 = v1 / cos_a;
    if l0 < crate::tolerances::EPSILON_LENGTH {
        return None;
    }
    // 2. Shortest angular gap, scaled to the unrolled flat sweep angle.
    let mut du = (u1 - u0) % TAU;
    if du > PI {
        du -= TAU;
    } else if du < -PI {
        du += TAU;
    }
    let dphi = du * sin_a; // flat angular gap
    // 3. Geodesic radius = flat distance (law of cosines in the flat sector,
    //    center at polar (L0, 0), radius_pt at (L1, dphi)).
    let rho = (l0 * l0 + l1 * l1 - 2.0 * l0 * l1 * dphi.cos()).max(0.0).sqrt();
    if rho < crate::tolerances::EPSILON_LENGTH {
        return None; // zero radius (coincident points)
    }
    if rho >= l0 {
        return None; // circle would reach / cross the apex (origin)
    }
    // u-span guard: the flat circle (center (L0,0), radius rho<L0) subtends a
    // half flat-angle asin(rho/L0) at the origin; in u-space that is
    // asin(rho/L0)/sin α. ≥ π ⇒ self-overlap when re-wrapped (sharp cone).
    if (rho / l0).asin() / sin_a >= PI - 1e-9 {
        return None;
    }
    // 4. Sample N points on the flat circle (center (L0, 0)); map each back.
    let n = crate::curves::circle::segment_count_for_arc(rho, TAU, chord_tol).clamp(24, 64);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let theta = TAU * (i as f64) / (n as f64);
        let fx = l0 + rho * theta.cos();
        let fy = rho * theta.sin();
        let l_back = (fx * fx + fy * fy).sqrt();
        let phi_back = fy.atan2(fx); // flat angle relative to the center (at 0)
        let u_back = u0 + phi_back / sin_a; // re-roll the angular offset
        let v_back = l_back * cos_a;
        pts.push(evaluate(apex, axis_dir, half_angle, ref_dir, u_back, v_back));
    }
    Some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_4, FRAC_PI_2};

    #[test]
    fn evaluate_apex_at_v_zero() {
        let p = evaluate(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 0.0);
        assert!((p - DVec3::ZERO).length() < 1e-12);
    }

    #[test]
    fn evaluate_axis_at_u_arbitrary_radius_zero() {
        // At v=0 (apex), all u positions collapse to apex (radius=0).
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            let p = evaluate(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, u, 0.0);
            assert!((p - DVec3::ZERO).length() < 1e-12);
        }
    }

    #[test]
    fn evaluate_at_unit_axial_distance() {
        // At v=10, half_angle=π/4, radius = 10·tan(π/4) = 10.
        let p = evaluate(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 10.0);
        // u=0 → ref direction → x = 10. axis Z contribution = 10.
        assert!((p - DVec3::new(10.0, 0.0, 10.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_axial_dist_radius_proportional() {
        // For half_angle=π/4 (tan=1), radius == v.
        for v in [1.0, 5.0, 10.0, 100.0] {
            let p = evaluate(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, v);
            let radial = DVec3::new(p.x, p.y, 0.0).length();
            assert!((radial - v).abs() < 1e-9,
                "v={}: radial={} ≠ v", v, radial);
        }
    }

    #[test]
    fn evaluate_offset_apex() {
        let apex = DVec3::new(10.0, 20.0, 30.0);
        let p = evaluate(apex, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 0.0);
        assert!((p - apex).length() < 1e-12);
    }

    #[test]
    fn normal_unit_length() {
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            let n = normal(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, u, 5.0);
            assert!((n.length() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn normal_at_45deg_cone_has_axis_component_negative() {
        // For 45° cone with axis=+Z, normals should have -Z component
        // (pointing radially outward and slightly toward apex).
        let n = normal(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 5.0);
        assert!(n.z < 0.0, "expected negative axial normal component, got {:?}", n);
    }

    #[test]
    fn normal_radial_component_aligns_with_radial() {
        let n = normal(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 5.0);
        // Radial direction at u=0 is +X.
        assert!(n.x > 0.0);
        assert!(n.y.abs() < 1e-9);
    }

    #[test]
    fn derivative_u_zero_at_apex() {
        let d = derivative_u(DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 0.0);
        assert!(d.length() < 1e-9);
    }

    #[test]
    fn derivative_u_grows_with_v() {
        let d_close = derivative_u(DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 1.0);
        let d_far = derivative_u(DVec3::Z, FRAC_PI_4, DVec3::X, 0.0, 10.0);
        assert!(d_far.length() > d_close.length() * 5.0);
    }

    #[test]
    fn derivative_v_perpendicular_to_normal() {
        for u_step in 0..6 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_3;
            let n = normal(DVec3::ZERO, DVec3::Z, FRAC_PI_4, DVec3::X, u, 5.0);
            let d = derivative_v(DVec3::Z, FRAC_PI_4, DVec3::X, u, 5.0);
            let dot = n.dot(d.normalize_or_zero()).abs();
            assert!(dot < 1e-9, "u={}: dot={}", u, dot);
        }
    }

    #[test]
    fn cone_with_small_half_angle_nearly_cylindrical() {
        // Very small half_angle → close to cylinder (radius nearly constant in v).
        let small = 0.01;  // ~0.57 deg
        let p_low = evaluate(DVec3::ZERO, DVec3::Z, small, DVec3::X, 0.0, 1.0);
        let p_high = evaluate(DVec3::ZERO, DVec3::Z, small, DVec3::X, 0.0, 100.0);
        let r_low = DVec3::new(p_low.x, p_low.y, 0.0).length();
        let r_high = DVec3::new(p_high.x, p_high.y, 0.0).length();
        // Both small, but proportional to v.
        assert!((r_low / 1.0 - r_high / 100.0).abs() < 1e-9);
    }

    #[test]
    fn cone_at_exactly_90deg_half_angle_degenerate() {
        // At π/2, cone is a flat disk. Just verify no panic.
        let p = evaluate(DVec3::ZERO, DVec3::Z, FRAC_PI_2, DVec3::X, 0.0, 5.0);
        // tan(π/2) is infinity in IEEE; depending on impl this may NaN.
        // We accept any finite or non-finite — just check no panic.
        let _ = p.length();
    }

    // ── ADR-263 β-1 (P3-C) — project_to_cone + circle_on_cone ──────────────
    use std::f64::consts::{FRAC_PI_6, TAU};

    #[test]
    fn adr263_project_to_cone_round_trips_surface_point() {
        // A point generated by `evaluate` is exactly on the cone → project
        // recovers (u, v) and the surface point round-trips.
        let apex = DVec3::new(3.0, -2.0, 5.0);
        let (axis, refd, a) = (DVec3::Z, DVec3::X, FRAC_PI_6);
        for &(u, v) in &[(0.0, 50.0), (1.3, 120.0), (-2.1, 80.0), (3.0, 200.0)] {
            let p = evaluate(apex, axis, a, refd, u, v);
            let (sp, ru, rv) = project_to_cone(apex, axis, a, refd, p).unwrap();
            assert!((sp - p).length() < 1e-7, "surface_pt round-trip u={u} v={v}");
            assert!((rv - v).abs() < 1e-7, "v round-trip u={u} v={v}: {rv}");
            // u may differ by 2π; compare via the reconstructed point.
            let p2 = evaluate(apex, axis, a, refd, ru, rv);
            assert!((p2 - p).length() < 1e-7, "u round-trip u={u}");
        }
    }

    #[test]
    fn adr263_project_to_cone_rejects_apex_behind_and_axis() {
        let (apex, axis, refd, a) = (DVec3::ZERO, DVec3::Z, DVec3::X, FRAC_PI_6);
        // At the apex.
        assert!(project_to_cone(apex, axis, a, refd, DVec3::ZERO).is_none());
        // Behind the apex (negative axial).
        assert!(project_to_cone(apex, axis, a, refd, DVec3::new(0.0, 0.0, -50.0)).is_none());
        // On the axis (radial undefined).
        assert!(project_to_cone(apex, axis, a, refd, DVec3::new(0.0, 0.0, 100.0)).is_none());
        // Degenerate half-angle.
        let p = DVec3::new(50.0, 0.0, 100.0);
        assert!(project_to_cone(apex, axis, 0.0, refd, p).is_none());
        assert!(project_to_cone(apex, axis, FRAC_PI_2, refd, p).is_none());
    }

    #[test]
    fn adr263_circle_on_cone_all_points_on_surface() {
        let apex = DVec3::new(1.0, 2.0, -3.0);
        let (axis, refd, a) = (DVec3::Z, DVec3::X, FRAC_PI_6);
        let center = evaluate(apex, axis, a, refd, 0.3, 100.0);
        let radius_pt = evaluate(apex, axis, a, refd, 0.6, 130.0);
        let pts = circle_on_cone(apex, axis, a, refd, center, radius_pt, 0.05).unwrap();
        assert!(pts.len() >= 24 && pts.len() <= 64, "N={}", pts.len());
        for (i, &p) in pts.iter().enumerate() {
            let (sp, _, _) = project_to_cone(apex, axis, a, refd, p)
                .unwrap_or_else(|| panic!("pt {i} failed to project"));
            assert!((sp - p).length() < 1e-6, "pt {i} off surface: {}", (sp - p).length());
        }
        // distinct consecutive samples.
        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            assert!((pts[i] - pts[j]).length() > 1e-6, "pts {i},{j} coincide");
        }
    }

    #[test]
    fn adr263_circle_on_cone_radius_pt_on_loop_when_aligned() {
        // du = 0 (radius_pt at the same angle, larger v) ⇒ the θ=0 sample sits
        // exactly on radius_pt (farthest point along +slant).
        let (apex, axis, refd, a) = (DVec3::ZERO, DVec3::Z, DVec3::X, FRAC_PI_6);
        let center = evaluate(apex, axis, a, refd, 0.3, 100.0);
        let radius_pt = evaluate(apex, axis, a, refd, 0.3, 130.0);
        let pts = circle_on_cone(apex, axis, a, refd, center, radius_pt, 0.05).unwrap();
        assert!((pts[0] - radius_pt).length() < 1e-7,
            "pts[0] should equal radius_pt: {}", (pts[0] - radius_pt).length());
    }

    #[test]
    fn adr263_circle_on_cone_rejects_apex_reaching_and_coincident() {
        let (apex, axis, refd, a) = (DVec3::ZERO, DVec3::Z, DVec3::X, FRAC_PI_6);
        let center = evaluate(apex, axis, a, refd, 0.0, 50.0);
        // rho = (130-50)/cosα = 80/cosα ≥ L0 = 50/cosα ⇒ encloses the apex.
        let far = evaluate(apex, axis, a, refd, 0.0, 130.0);
        assert!(circle_on_cone(apex, axis, a, refd, center, far, 0.05).is_none(),
            "apex-reaching circle must be rejected");
        // Coincident center/radius ⇒ zero radius.
        assert!(circle_on_cone(apex, axis, a, refd, center, center, 0.05).is_none());
    }

    #[test]
    fn adr263_circle_on_cone_geodesic_radius_constant() {
        // Every sample is the same geodesic (flat) distance from the center —
        // the defining property of a geodesic circle on a developable surface.
        let (apex, axis, refd, a) = (DVec3::ZERO, DVec3::Z, DVec3::X, FRAC_PI_6);
        let (u0, v0) = (0.4, 90.0);
        let center = evaluate(apex, axis, a, refd, u0, v0);
        let radius_pt = evaluate(apex, axis, a, refd, 0.4, 115.0);
        let pts = circle_on_cone(apex, axis, a, refd, center, radius_pt, 0.05).unwrap();
        // Flat-distance helper (unroll to the sector, mirror of the impl).
        let cos_a = a.cos();
        let sin_a = a.sin();
        let l0 = v0 / cos_a;
        let flat_dist = |p: DVec3| -> f64 {
            let (_sp, u, v) = project_to_cone(apex, axis, a, refd, p).unwrap();
            let l = v / cos_a;
            let mut du = (u - u0) % TAU;
            if du > std::f64::consts::PI { du -= TAU; } else if du < -std::f64::consts::PI { du += TAU; }
            let dphi = du * sin_a;
            (l0 * l0 + l * l - 2.0 * l0 * l * dphi.cos()).max(0.0).sqrt()
        };
        let rho0 = flat_dist(pts[0]);
        for (i, &p) in pts.iter().enumerate() {
            assert!((flat_dist(p) - rho0).abs() < 1e-4,
                "pt {i} geodesic radius {} ≠ {rho0}", flat_dist(p));
        }
    }
}
