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

/// ADR-284 β-1 — project a drawn POLYLINE (rect / polygon / freehand / bezier
/// corners) onto this torus and sample it into on-surface points, ready for
/// [`crate::mesh::Mesh::split_torus_face_by_circle`] (shape-agnostic closed-
/// polyline split). Mirror of [`crate::surfaces::cylinder::polyline_on_cylinder`],
/// but the torus is **doubly-periodic** — BOTH `u` (major) and `v` (minor) wrap —
/// so both params are unrolled continuously and the encircle guard checks BOTH
/// windings. The torus is non-developable, so parameter-space edge sampling is a
/// first-order approximation (sufficient for a simple on-surface split loop).
///
/// `None` if < 2 points, any point is not on this torus (> 1e-3 mm), a CLOSED
/// loop encircles the major OR the minor circle (`|winding| > π`), an OPEN loop
/// spans ≥ a full turn in either param, or fewer than 3 samples.
#[allow(clippy::too_many_arguments)]
pub fn polyline_on_torus(
    center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    pts: &[DVec3],
    closed: bool,
    chord_tol: f64,
) -> Option<Vec<DVec3>> {
    use std::f64::consts::{PI, TAU};
    if pts.len() < 2 {
        return None;
    }
    let unroll = |cur: f64, prev: f64| -> f64 {
        let mut x = cur;
        while x - prev > PI {
            x -= TAU;
        }
        while x - prev < -PI {
            x += TAU;
        }
        x
    };
    // Project each point → (u, v), unrolling BOTH params continuously.
    let mut uv: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    let (mut up, mut vp) = (0.0, 0.0);
    for (i, &p) in pts.iter().enumerate() {
        // Project each drawn point onto the torus (tool points are on the tangent
        // plane — project them). None only for un-projectable input (axis / NaN).
        let (_sp, u, v) =
            project_to_torus(center, axis_dir, ref_dir, major_radius, minor_radius, p)?;
        let (uc, vc) = if i == 0 { (u, v) } else { (unroll(u, up), unroll(v, vp)) };
        up = uc;
        vp = vc;
        uv.push((uc, vc));
    }
    let n = uv.len();
    // Wrap guard on BOTH windings (major u + minor v).
    if closed {
        let (mut wu, mut wv) = (0.0, 0.0);
        for e in 0..n {
            let (u0, v0) = uv[e];
            let (u1, v1) = uv[(e + 1) % n];
            let mut du = (u1 - u0) % TAU;
            if du > PI {
                du -= TAU;
            } else if du < -PI {
                du += TAU;
            }
            let mut dv = (v1 - v0) % TAU;
            if dv > PI {
                dv -= TAU;
            } else if dv < -PI {
                dv += TAU;
            }
            wu += du;
            wv += dv;
        }
        if wu.abs() > PI || wv.abs() > PI {
            return None;
        }
    } else {
        let (umin, umax) = uv.iter().fold((f64::MAX, f64::MIN), |(a, b), &(u, _)| (a.min(u), b.max(u)));
        let (vmin, vmax) = uv.iter().fold((f64::MAX, f64::MIN), |(a, b), &(_, v)| (a.min(v), b.max(v)));
        if umax - umin >= TAU - 1e-6 || vmax - vmin >= TAU - 1e-6 {
            return None;
        }
    }
    // Chord-sample each edge parametrically; map back with `evaluate`. Each edge
    // emits its START + interior points (not its end) → no duplicate vertices.
    let edge_count = if closed { n } else { n - 1 };
    let mut out: Vec<DVec3> = Vec::new();
    for e in 0..edge_count {
        let (u0, v0) = uv[e];
        let (u1, v1) = uv[(e + 1) % n];
        // first-order metric length: ds² = (R + r·cos v0)²du² + r²dv².
        let scale = (major_radius + minor_radius * v0.cos()).abs();
        let flat = (((u1 - u0) * scale).powi(2) + ((v1 - v0) * minor_radius).powi(2)).sqrt();
        let k = ((flat / chord_tol.max(1e-6)).ceil() as usize).clamp(1, 64);
        for s in 0..k {
            let t = s as f64 / k as f64;
            let u = u0 + (u1 - u0) * t;
            let v = v0 + (v1 - v0) * t;
            out.push(evaluate(center, axis_dir, ref_dir, major_radius, minor_radius, u, v));
        }
    }
    if out.len() < 3 {
        return None;
    }
    Some(out)
}

/// ADR-288 β-1 — ray ∩ torus intersection parameters `t` (sorted ascending) for
/// the world ray `origin + t·dir` (dir need not be unit — it is normalized). The
/// torus implicit in its local frame (r→x, perp→y, axis→z) is
/// `F = (|p|² + R² − r²)² − 4R²(px² + py²) = 0`; substituting the ray gives a
/// quartic in `t`. Rather than a fragile closed-form (Ferrari), F is sampled
/// along the ray's segment inside the torus bounding sphere (radius R+r), sign
/// changes are bisected, and each root is Newton-free bisection-polished. Returns
/// up to 4 roots (empty if the ray misses the bounding sphere / degenerate).
pub fn ray_torus_intersections(
    center: DVec3,
    axis_dir: DVec3,
    ref_dir: DVec3,
    major_radius: f64,
    minor_radius: f64,
    origin: DVec3,
    dir: DVec3,
) -> Vec<f64> {
    let (rr, rm) = (major_radius, minor_radius);
    if !(rr > 0.0 && rm > 0.0) {
        return Vec::new();
    }
    let (axis, rbasis, pbasis) = basis(axis_dir, ref_dir);
    if axis.length_squared() < 0.5 {
        return Vec::new();
    }
    let d = dir.normalize_or_zero();
    if d.length_squared() < 0.5 {
        return Vec::new();
    }
    // Ray in the torus-local frame.
    let rel = origin - center;
    let ol = DVec3::new(rel.dot(rbasis), rel.dot(pbasis), rel.dot(axis));
    let dl = DVec3::new(d.dot(rbasis), d.dot(pbasis), d.dot(axis));
    let f = |t: f64| -> f64 {
        let q = ol + dl * t;
        let g = q.length_squared() + rr * rr - rm * rm;
        g * g - 4.0 * rr * rr * (q.x * q.x + q.y * q.y)
    };
    // Ray ∩ bounding sphere (radius R+r): dl is unit → t² + 2(ol·dl)t + (|ol|²−br²)=0.
    let br = rr + rm;
    let b = 2.0 * ol.dot(dl);
    let c = ol.length_squared() - br * br;
    let disc = b * b - 4.0 * c;
    if disc < 0.0 {
        return Vec::new(); // misses the bounding sphere
    }
    let sq = disc.sqrt();
    let (t0, t1) = (0.5 * (-b - sq), 0.5 * (-b + sq));
    // Sample F over [t0, t1]; bisect each sign change.
    const STEPS: usize = 512;
    let mut roots: Vec<f64> = Vec::new();
    let mut prev_t = t0;
    let mut prev_f = f(t0);
    for i in 1..=STEPS {
        let t = t0 + (t1 - t0) * (i as f64 / STEPS as f64);
        let ft = f(t);
        if prev_f == 0.0 {
            roots.push(prev_t);
        } else if (prev_f < 0.0) != (ft < 0.0) {
            let (mut lo, mut flo, mut hi) = (prev_t, prev_f, t);
            for _ in 0..64 {
                let m = 0.5 * (lo + hi);
                let fm = f(m);
                if (flo < 0.0) != (fm < 0.0) {
                    hi = m;
                } else {
                    lo = m;
                    flo = fm;
                }
            }
            roots.push(0.5 * (lo + hi));
        }
        prev_t = t;
        prev_f = ft;
    }
    roots.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI, TAU};

    /// ADR-288 β-1 — ray ∩ torus. A ray along −X through z=0 of a (R=10, r=3)
    /// Z-axis torus hits 4 walls at x = 13, 7, −7, −13 (outer/inner/inner/outer);
    /// a ray far above the torus misses (0 roots).
    #[test]
    fn adr288_ray_torus_diametral_four_roots_and_miss() {
        let c = DVec3::ZERO;
        let (r, rm) = (10.0, 3.0);
        // Ray from (20,0,0) along −X (through the whole donut at z=0).
        let ts = ray_torus_intersections(c, DVec3::Z, DVec3::X, r, rm,
            DVec3::new(20.0, 0.0, 0.0), DVec3::new(-1.0, 0.0, 0.0));
        assert_eq!(ts.len(), 4, "diametral ray hits 4 walls, got {ts:?}");
        // x = 20 − t → walls at x = 13, 7, −7, −13 → t = 7, 13, 27, 33.
        let want = [7.0, 13.0, 27.0, 33.0];
        for (got, w) in ts.iter().zip(want.iter()) {
            assert!((got - w).abs() < 1e-3, "root {got} ≈ {w} (all {ts:?})");
        }

        // A ray well above the torus (z = 100) misses entirely.
        let miss = ray_torus_intersections(c, DVec3::Z, DVec3::X, r, rm,
            DVec3::new(20.0, 0.0, 100.0), DVec3::new(-1.0, 0.0, 0.0));
        assert!(miss.is_empty(), "ray above the torus misses, got {miss:?}");

        // A tube bore from the outer wall (13,0,0) along −X exits the inner wall:
        // first far root (after t≈0) is the inner wall at x=7 (t≈6).
        let bore = ray_torus_intersections(c, DVec3::Z, DVec3::X, r, rm,
            DVec3::new(13.0, 0.0, 0.0), DVec3::new(-1.0, 0.0, 0.0));
        assert!(bore.iter().any(|&t| (t - 6.0).abs() < 1e-2),
            "bore from outer wall reaches the inner wall at t≈6 (x=7), got {bore:?}");
    }

    #[test]
    fn polyline_on_torus_rect_samples_on_surface() {
        // ADR-284 β-1 — a small rect (4 corners) on a torus wall projects to a
        // closed on-surface loop; every sample lies on the torus.
        let (c, ax, refd, rmaj, rmin) = (DVec3::ZERO, DVec3::Z, DVec3::X, 10.0, 3.0);
        let corners: Vec<DVec3> = [(-0.2, -0.3), (0.2, -0.3), (0.2, 0.3), (-0.2, 0.3)]
            .iter()
            .map(|&(u, v)| evaluate(c, ax, refd, rmaj, rmin, u, v))
            .collect();
        let samples = polyline_on_torus(c, ax, refd, rmaj, rmin, &corners, true, 0.3)
            .expect("rect projects onto torus");
        assert!(samples.len() >= 4);
        for &p in &samples {
            let (sp, _u, _v) = project_to_torus(c, ax, refd, rmaj, rmin, p).unwrap();
            assert!((sp - p).length() < 1e-9, "every sample lies on the torus");
        }
    }

    #[test]
    fn polyline_on_torus_rejects_major_wrap() {
        // A loop spanning the whole MAJOR circle (u = 0, π/2, π, 3π/2) → encircle
        // → None.
        let (c, ax, refd, rmaj, rmin) = (DVec3::ZERO, DVec3::Z, DVec3::X, 10.0, 3.0);
        let corners: Vec<DVec3> = [0.0, FRAC_PI_2, PI, 3.0 * FRAC_PI_2]
            .iter()
            .map(|&u| evaluate(c, ax, refd, rmaj, rmin, u, 0.0))
            .collect();
        assert!(
            polyline_on_torus(c, ax, refd, rmaj, rmin, &corners, true, 0.3).is_none(),
            "full major-circle loop → None (encircle guard)"
        );
    }

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
