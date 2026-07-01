//! Torus primitive (Phase D, ADR-031).
//!
//! Standard torus parametric form:
//!
//! ```text
//! P(u, v) = center + (R + r·cos(v)) · (cos(u)·ref + sin(u)·perp) + r·sin(v) · axis
//! ```
//!
//! where:
//! - `R` = `major_radius`: distance from torus center to tube center
//! - `r` = `minor_radius`: tube radius
//! - `u`: angle around major axis (longitude)
//! - `v`: angle around tube circle (latitude on tube)
//! - `axis = axis_dir.normalize()`: torus symmetry axis (perpendicular to torus plane)
//! - `ref / perp`: orthonormal basis perpendicular to axis
//!
//! Outward normal: `cos(v)·radial + sin(v)·axis`,
//! where `radial = cos(u)·ref + sin(u)·perp`.

use glam::DVec3;

use super::orthonormal_ref;

#[inline]
fn basis(axis_dir: DVec3, ref_dir: DVec3) -> (DVec3, DVec3, DVec3) {
    let axis = axis_dir.normalize_or_zero();
    let r = orthonormal_ref(axis, ref_dir);
    let p = axis.cross(r).normalize_or_zero();
    (axis, r, p)
}

#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    u: f64,
    v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    center
        + radial * (major_radius + minor_radius * v.cos())
        + axis * (minor_radius * v.sin())
}

#[allow(clippy::too_many_arguments)]
pub fn normal(
    _center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    _major_radius: f64,
    _minor_radius: f64,
    u: f64,
    v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    radial * v.cos() + axis * v.sin()
}

pub fn derivative_u(
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    u: f64,
    v: f64,
) -> DVec3 {
    let (_axis, r, p) = basis(axis_dir, ref_dir);
    let scale = major_radius + minor_radius * v.cos();
    r * (-scale * u.sin()) + p * (scale * u.cos())
}

pub fn derivative_v(
    axis_dir: DVec3,
    ref_dir: DVec3,
    minor_radius: f64,
    u: f64,
    v: f64,
) -> DVec3 {
    let (axis, r, p) = basis(axis_dir, ref_dir);
    let radial = r * u.cos() + p * u.sin();
    radial * (-minor_radius * v.sin()) + axis * (minor_radius * v.cos())
}

/// ADR-263 β-4 (P3-C) — project a world point onto a torus surface, returning
/// `(surface_pt, u, v)` where `u` = major (longitude) angle, `v` = minor (tube)
/// angle. For a point exactly on the torus the result round-trips
/// (`evaluate(u, v) == surface_pt == p`).
///
/// `None` if the torus is degenerate (`R ≤ 0` or `r ≤ 0`), the axis is
/// degenerate, or the point projects onto the symmetry axis (`u` undefined).
pub fn project_to_torus(
    center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    p: DVec3,
) -> Option<(DVec3, f64, f64)> {
    if !(major_radius > 0.0 && minor_radius > 0.0) {
        return None;
    }
    let (axis, r_basis, p_basis) = basis(axis_dir, ref_dir);
    if axis.length_squared() < 0.5 {
        return None; // degenerate axis
    }
    let d = p - center;
    let axial = d.dot(axis); // = r·sin(v) on the surface
    let in_plane = d - axis * axial;
    let in_plane_len = in_plane.length(); // = R + r·cos(v) on the surface
    if in_plane_len < 1e-9 {
        return None; // on the symmetry axis — longitude undefined
    }
    let radial_unit = in_plane / in_plane_len;
    // Longitude (matches `evaluate`: radial = cos(u)·r + sin(u)·perp).
    let u = radial_unit.dot(p_basis).atan2(radial_unit.dot(r_basis));
    // Tube latitude: R + r·cos v = in_plane_len, r·sin v = axial.
    let v = axial.atan2(in_plane_len - major_radius);
    let surface_pt = evaluate(center, axis_dir, ref_dir, major_radius, minor_radius, u, v);
    Some((surface_pt, u, v))
}

/// ADR-263 β-4 (P3-C) — generate the closed "porthole" circle drawn ON a torus
/// wall, as a polyline of surface points.
///
/// Unlike the cylinder / cone (developable), the torus has **non-zero Gaussian
/// curvature** — it does NOT unroll isometrically, so there is no exact flat
/// geodesic circle. Instead this samples a **metric-scaled parameter-space
/// circle** around `(u0, v0)`: with the local first fundamental form
/// `ds² = (R + r·cos v0)² du² + r² dv²`, a circle of geodesic-radius `rho` maps
/// to the parameter ellipse `du = ρ·cos θ / (R + r·cos v0)`, `dv = ρ·sin θ / r`.
/// This passes exactly through `radius_pt` and is an excellent first-order
/// geodesic circle for an MVP sketch (ADR-263 §2.2 "quad-param"). `rho` is the
/// local-metric distance from `center_pt` to `radius_pt` (shortest signed
/// param diffs — both `u` and `v` wrap on a torus).
///
/// Returns `N` distinct surface points (closed loop). `None` if either point
/// fails to project, the radius is ~0, the local major-circle radius collapses
/// (`R + r·cos v0 ≈ 0`), or the parameter circle wraps `≥ π` in `u` or `v`
/// (self-overlap guard).
#[allow(clippy::too_many_arguments)]
pub fn circle_on_torus(
    center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    center_pt: DVec3,
    radius_pt: DVec3,
    chord_tol: f64,
) -> Option<Vec<DVec3>> {
    use std::f64::consts::{PI, TAU};
    let (_cp, u0, v0) =
        project_to_torus(center, axis_dir, ref_dir, major_radius, minor_radius, center_pt)?;
    let (_rp, u1, v1) =
        project_to_torus(center, axis_dir, ref_dir, major_radius, minor_radius, radius_pt)?;
    // Shortest signed param diffs (both u and v wrap).
    let shortest = |mut d: f64| -> f64 {
        d %= TAU;
        if d > PI {
            d -= TAU;
        } else if d < -PI {
            d += TAU;
        }
        d
    };
    let du_diff = shortest(u1 - u0);
    let dv_diff = shortest(v1 - v0);
    // Local first-fundamental-form coefficients at the center.
    let m = major_radius + minor_radius * v0.cos(); // √g_uu
    if m < 1e-9 {
        return None; // major-circle radius collapsed (inner pinch)
    }
    let rho = ((m * du_diff).powi(2) + (minor_radius * dv_diff).powi(2)).sqrt();
    if rho < crate::tolerances::EPSILON_LENGTH {
        return None; // zero radius (coincident points)
    }
    // Self-overlap guards: the param circle must not wrap u or v (max param
    // extent is rho/m in u, rho/r in v).
    if rho / m >= PI - 1e-9 || rho / minor_radius >= PI - 1e-9 {
        return None;
    }
    let n = crate::curves::circle::segment_count_for_arc(rho, TAU, chord_tol).clamp(24, 64);
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let theta = TAU * (i as f64) / (n as f64);
        let du = (rho * theta.cos()) / m;
        let dv = (rho * theta.sin()) / minor_radius;
        pts.push(evaluate(
            center, axis_dir, ref_dir, major_radius, minor_radius, u0 + du, v0 + dv,
        ));
    }
    Some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    #[test]
    fn evaluate_outermost_equator_u_zero_v_zero() {
        // At v=0 (outer equator), distance from center = R+r.
        let p = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, 0.0);
        assert!((p - DVec3::new(6.0, 0.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_innermost_equator_u_zero_v_pi() {
        // At v=π (inner equator), distance from center = R-r.
        let p = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, PI);
        assert!((p - DVec3::new(4.0, 0.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_top_of_tube_u_zero_v_pi_half() {
        // At v=π/2, tube top: z = r above the equatorial plane.
        let p = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, FRAC_PI_2);
        assert!((p - DVec3::new(5.0, 0.0, 1.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_radial_distance_outer_equator() {
        // For all u at v=0, distance from torus center should equal R+r.
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            let p = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, u, 0.0);
            let dist = p.length();
            assert!((dist - 6.0).abs() < 1e-9, "u={}: |p|={}, expected 6", u, dist);
        }
    }

    #[test]
    fn evaluate_offset_center() {
        let c = DVec3::new(10.0, 20.0, 30.0);
        let p = evaluate(c, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, 0.0);
        assert!((p - DVec3::new(16.0, 20.0, 30.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_full_u_period_returns_to_start() {
        let p0 = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, 0.0);
        let p1 = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, TAU, 0.0);
        assert!((p0 - p1).length() < 1e-9);
    }

    #[test]
    fn evaluate_full_v_period_returns_to_start() {
        let p0 = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.5, 0.0);
        let p1 = evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.5, TAU);
        assert!((p0 - p1).length() < 1e-9);
    }

    #[test]
    fn normal_unit_length() {
        for u_step in 0..6 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_3;
            for v_step in 0..6 {
                let v = (v_step as f64) * std::f64::consts::FRAC_PI_3;
                let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, u, v);
                assert!((n.length() - 1.0).abs() < 1e-9,
                    "u={}, v={}: n.length={}", u, v, n.length());
            }
        }
    }

    #[test]
    fn normal_outer_equator_outward_radial() {
        // At outer equator (v=0), normal should point outward radially.
        let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, 0.0);
        assert!((n - DVec3::X).length() < 1e-9);
    }

    #[test]
    fn normal_inner_equator_inward_radial() {
        // At inner equator (v=π), normal should point INWARD (toward axis).
        let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, PI);
        assert!((n - (-DVec3::X)).length() < 1e-9);
    }

    #[test]
    fn normal_top_points_up_axis() {
        let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, 0.0, FRAC_PI_2);
        assert!((n - DVec3::Z).length() < 1e-9);
    }

    #[test]
    fn derivative_u_perpendicular_to_normal() {
        for u in [0.5, 1.0, 2.0] {
            for v in [0.5, 1.0, 2.5] {
                let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, u, v);
                let d = derivative_u(DVec3::Z, DVec3::X, 5.0, 1.0, u, v);
                if d.length() > 1e-9 {
                    let dot = n.dot(d.normalize()).abs();
                    assert!(dot < 1e-9, "u={}, v={}: dot={}", u, v, dot);
                }
            }
        }
    }

    #[test]
    fn derivative_v_perpendicular_to_normal() {
        for u in [0.5, 1.0, 2.0] {
            for v in [0.5, 1.0, 2.5] {
                let n = normal(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.0, u, v);
                let d = derivative_v(DVec3::Z, DVec3::X, 1.0, u, v);
                if d.length() > 1e-9 {
                    let dot = n.dot(d.normalize()).abs();
                    assert!(dot < 1e-9, "u={}, v={}: dot={}", u, v, dot);
                }
            }
        }
    }

    // ── ADR-263 β-4 (P3-C) — project_to_torus + circle_on_torus ────────────
    const TR_R: f64 = 50.0;
    const TR_r: f64 = 10.0;

    #[test]
    fn adr263_project_to_torus_round_trips_surface_point() {
        let center = DVec3::new(2.0, -3.0, 1.0);
        let (axis, refd) = (DVec3::Z, DVec3::X);
        for &(u, v) in &[(0.0, 0.0), (1.3, 0.7), (-2.1, 2.5), (3.0, -1.0)] {
            let p = evaluate(center, axis, refd, TR_R, TR_r, u, v);
            let (sp, ru, rv) = project_to_torus(center, axis, refd, TR_R, TR_r, p).unwrap();
            assert!((sp - p).length() < 1e-7, "surface_pt round-trip u={u} v={v}");
            let p2 = evaluate(center, axis, refd, TR_R, TR_r, ru, rv);
            assert!((p2 - p).length() < 1e-7, "(u,v) round-trip u={u} v={v}");
        }
    }

    #[test]
    fn adr263_project_to_torus_rejects_degenerate() {
        let (axis, refd) = (DVec3::Z, DVec3::X);
        // On the symmetry axis (longitude undefined).
        assert!(project_to_torus(DVec3::ZERO, axis, refd, TR_R, TR_r, DVec3::ZERO).is_none());
        assert!(project_to_torus(DVec3::ZERO, axis, refd, TR_R, TR_r, DVec3::new(0.0, 0.0, 7.0)).is_none());
        // Degenerate radii.
        let p = DVec3::new(60.0, 0.0, 0.0);
        assert!(project_to_torus(DVec3::ZERO, axis, refd, 0.0, TR_r, p).is_none());
        assert!(project_to_torus(DVec3::ZERO, axis, refd, TR_R, 0.0, p).is_none());
    }

    #[test]
    fn adr263_circle_on_torus_all_points_on_surface() {
        let center = DVec3::new(1.0, 2.0, -3.0);
        let (axis, refd) = (DVec3::Z, DVec3::X);
        let cp = evaluate(center, axis, refd, TR_R, TR_r, 0.3, 0.5);
        let rp = evaluate(center, axis, refd, TR_R, TR_r, 0.5, 0.8);
        let pts = circle_on_torus(center, axis, refd, TR_R, TR_r, cp, rp, 0.05).unwrap();
        assert!(pts.len() >= 24 && pts.len() <= 64, "N={}", pts.len());
        for (i, &p) in pts.iter().enumerate() {
            let (sp, _, _) = project_to_torus(center, axis, refd, TR_R, TR_r, p)
                .unwrap_or_else(|| panic!("pt {i} failed to project"));
            assert!((sp - p).length() < 1e-6, "pt {i} off surface: {}", (sp - p).length());
        }
        for i in 0..pts.len() {
            let j = (i + 1) % pts.len();
            assert!((pts[i] - pts[j]).length() > 1e-6, "pts {i},{j} coincide");
        }
    }

    #[test]
    fn adr263_circle_on_torus_radius_pt_on_loop_when_aligned() {
        // du>0, dv=0 (radius_pt at the same latitude, larger longitude) ⇒ the
        // θ=0 sample sits exactly on radius_pt.
        let (center, axis, refd) = (DVec3::ZERO, DVec3::Z, DVec3::X);
        let cp = evaluate(center, axis, refd, TR_R, TR_r, 0.3, 0.5);
        let rp = evaluate(center, axis, refd, TR_R, TR_r, 0.5, 0.5);
        let pts = circle_on_torus(center, axis, refd, TR_R, TR_r, cp, rp, 0.05).unwrap();
        assert!((pts[0] - rp).length() < 1e-7,
            "pts[0] should equal radius_pt: {}", (pts[0] - rp).length());
    }

    #[test]
    fn adr263_circle_on_torus_metric_radius_constant() {
        // Every sample is the same local-metric distance from the center
        // (the first-order geodesic-circle invariant, exact by construction).
        let (center, axis, refd) = (DVec3::ZERO, DVec3::Z, DVec3::X);
        let (u0, v0) = (0.4, 0.6);
        let cp = evaluate(center, axis, refd, TR_R, TR_r, u0, v0);
        let rp = evaluate(center, axis, refd, TR_R, TR_r, 0.55, 0.75);
        let pts = circle_on_torus(center, axis, refd, TR_R, TR_r, cp, rp, 0.05).unwrap();
        let m = TR_R + TR_r * v0.cos();
        let shortest = |mut d: f64| -> f64 {
            d %= TAU;
            if d > PI { d -= TAU; } else if d < -PI { d += TAU; }
            d
        };
        let metric_dist = |p: DVec3| -> f64 {
            let (_s, u, v) = project_to_torus(center, axis, refd, TR_R, TR_r, p).unwrap();
            let du = shortest(u - u0);
            let dv = shortest(v - v0);
            ((m * du).powi(2) + (TR_r * dv).powi(2)).sqrt()
        };
        let rho0 = metric_dist(pts[0]);
        for (i, &p) in pts.iter().enumerate() {
            assert!((metric_dist(p) - rho0).abs() < 1e-4,
                "pt {i} metric radius {} ≠ {rho0}", metric_dist(p));
        }
    }

    #[test]
    fn adr263_circle_on_torus_rejects_wrap_and_coincident() {
        let (center, axis, refd) = (DVec3::ZERO, DVec3::Z, DVec3::X);
        let cp = evaluate(center, axis, refd, TR_R, TR_r, 0.0, 0.0);
        // Coincident center/radius ⇒ zero radius.
        assert!(circle_on_torus(center, axis, refd, TR_R, TR_r, cp, cp, 0.05).is_none());
        // A large geodesic radius: du=0.8 ⇒ ρ = m·0.8 ≈ 48, ρ/r ≈ 4.8 ≥ π ⇒ the
        // param circle would wrap the tube (v-extent ≥ π) → self-overlap reject.
        let wrap = evaluate(center, axis, refd, TR_R, TR_r, 0.8, 0.0);
        assert!(circle_on_torus(center, axis, refd, TR_R, TR_r, cp, wrap, 0.05).is_none(),
            "tube-wrapping circle must be rejected (self-overlap guard)");
    }
}
