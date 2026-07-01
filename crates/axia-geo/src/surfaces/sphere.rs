//! Sphere primitive (Phase D, ADR-031).
//!
//! Standard latitude / longitude parameterization:
//!
//! ```text
//! P(u, v) = center + R · (cos(v)·cos(u), cos(v)·sin(u), sin(v))
//! ```
//!
//! - `u`: longitude in radians, [0, 2π]
//! - `v`: latitude in radians, [-π/2, π/2]
//!
//! Outward normal: `(P - center) / R` (radial, axis-agnostic).
//!
//! ADR-204 — the sphere is an **oriented quadric**: `axis_dir` is the pole
//! direction (`v = +π/2 → center + axis_dir·R`) and `ref_dir` is the
//! longitude-zero seam (`u = 0, v = 0 → center + ref_dir·R`). With
//! `axis_dir = +Z, ref_dir = +X` this reduces exactly to the legacy
//! world-Z/X parameterization.

use glam::DVec3;

/// Orthonormal sphere basis `(r, b, a)`: `a` = pole (axis_dir normalized),
/// `r` = ref_dir orthogonalized to `a` (Gram-Schmidt), `b = a × r` (binormal).
/// Degenerate inputs fall back to +X / +Y / +Z, so `(Z, X)` → `(X, Y, Z)`.
#[inline]
fn sphere_basis(axis_dir: DVec3, ref_dir: DVec3) -> (DVec3, DVec3, DVec3) {
    let a = axis_dir.normalize_or_zero();
    let a = if a.length_squared() < 0.5 { DVec3::Z } else { a };
    let mut r = ref_dir - a * ref_dir.dot(a);
    if r.length_squared() < 1e-18 {
        // ref_dir ∥ axis → pick any perpendicular seed.
        let seed = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        r = seed - a * seed.dot(a);
    }
    let r = r.normalize_or_zero();
    let r = if r.length_squared() < 0.5 { DVec3::X } else { r };
    let b = a.cross(r);
    (r, b, a)
}

#[inline]
pub fn evaluate(center: DVec3, radius: f64, axis_dir: DVec3, ref_dir: DVec3, u: f64, v: f64) -> DVec3 {
    let (r, b, a) = sphere_basis(axis_dir, ref_dir);
    let cv = v.cos();
    let sv = v.sin();
    let cu = u.cos();
    let su = u.sin();
    center + radius * (cv * cu * r + cv * su * b + sv * a)
}

#[inline]
pub fn normal(center: DVec3, radius: f64, axis_dir: DVec3, ref_dir: DVec3, u: f64, v: f64) -> DVec3 {
    if radius.abs() < 1e-12 {
        return axis_dir.normalize_or_zero(); // degenerate → pole
    }
    (evaluate(center, radius, axis_dir, ref_dir, u, v) - center) / radius
}

/// ∂P/∂u — tangent in longitude direction (no pole component).
#[inline]
pub fn derivative_u(radius: f64, axis_dir: DVec3, ref_dir: DVec3, u: f64, v: f64) -> DVec3 {
    let (r, b, _a) = sphere_basis(axis_dir, ref_dir);
    let cv = v.cos();
    radius * cv * (-u.sin() * r + u.cos() * b)
}

/// ∂P/∂v — tangent in latitude direction.
#[inline]
pub fn derivative_v(radius: f64, axis_dir: DVec3, ref_dir: DVec3, u: f64, v: f64) -> DVec3 {
    let (r, b, a) = sphere_basis(axis_dir, ref_dir);
    let sv = v.sin();
    radius * (-sv * u.cos() * r - sv * u.sin() * b + v.cos() * a)
}

/// Project a world point onto the sphere surface (closest point along the
/// radial direction). Returns `center + R·(p-center).normalize()`.
#[inline]
pub fn project_to_surface(center: DVec3, radius: f64, p: DVec3) -> Option<DVec3> {
    let d = p - center;
    let len = d.length();
    if len < 1e-12 || !(radius > 0.0) {
        return None;
    }
    Some(center + d * (radius / len))
}

/// **ADR-202 β-1 (2026-06-17)** — great-circle Arc between two points on a
/// sphere ("draw line A→B on a sphere", Option C planar-section).
///
/// The plane through `a`, `b`, and the sphere center cuts the sphere in a
/// **great circle** — which is exactly the geodesic (shortest path) on a
/// sphere (Q2 lock-in). `a`/`b` are first projected onto the sphere. Returns
/// the SHORTER arc from A to B (start_angle 0 at A, end_angle ≤ π at B); the
/// resulting `AnalyticCurve::Arc` lies exactly on the sphere (every point at
/// distance `radius` from `center`).
///
/// `None` if `a`/`b` coincide or are antipodal (the great-circle plane is then
/// undefined — the radial directions are (anti)parallel so their cross product
/// vanishes), or for a degenerate sphere.
pub fn sphere_great_circle_arc(
    center: DVec3,
    radius: f64,
    a: DVec3,
    b: DVec3,
) -> Option<crate::curves::AnalyticCurve> {
    use std::f64::consts::{PI, TAU};
    if !(radius > 0.0) || !center.is_finite() {
        return None;
    }
    let da = a - center;
    let db = b - center;
    if da.length_squared() < 1e-18 || db.length_squared() < 1e-18 {
        return None;
    }
    let ua = da.normalize();
    let ub = db.normalize();
    // Great-circle plane normal = ûa × ûb (vanishes when (anti)parallel:
    // coincident or antipodal projected points → no unique great circle).
    let cross = ua.cross(ub);
    let cross_len = cross.length();
    if cross_len < 1e-9 {
        return None;
    }
    let mut normal = cross / cross_len;
    // basis_u points at A (so A is at angle 0); basis_v = normal × basis_u
    // (matches AnalyticCurve::Arc evaluate convention, curves/arc.rs).
    let basis_u = ua;
    let basis_v = normal.cross(basis_u); // unit (normal ⟂ basis_u, both unit)
    // Angle of B in [0, 2π).
    let mut theta_b = db.dot(basis_v).atan2(db.dot(basis_u));
    if theta_b < 0.0 {
        theta_b += TAU;
    }
    // Take the SHORTER arc: if the CCW span exceeds π, flip the plane
    // orientation so the (still start=0) CCW span becomes ≤ π. Flipping the
    // normal flips basis_v, mapping θ → 2π−θ; the endpoint B is preserved.
    if theta_b > PI {
        normal = -normal;
        theta_b = TAU - theta_b;
    }
    Some(crate::curves::AnalyticCurve::Arc {
        center,
        radius,
        normal,
        basis_u,
        start_angle: 0.0,
        end_angle: theta_b,
    })
}

/// **ADR-202 β-2a (2026-06-17)** — closed circle drawn ON a sphere ("draw circle
/// on a sphere", Q3 면 분할 — closed curves partition a curved face cleanly).
///
/// Given a center point `center_pt` and a radius-defining point `radius_pt`
/// (both projected onto the sphere), returns the **small circle** on the sphere:
/// the set of surface points at geodesic distance `α = ∠(Cp, Rp)` from Cp. The
/// circle's supporting plane is perpendicular to the axis `(Cp − sphere_center)`
/// at height `R·cos α`; circle radius = `R·sin α`. Every point of the returned
/// `AnalyticCurve::Circle` lies exactly on the sphere
/// (`(R·cos α)² + (R·sin α)² = R²`), so β-2b can split the sphere face along it
/// (cap inside + annulus outside, both inheriting the Sphere surface).
///
/// `None` if `center_pt`/`radius_pt` coincide (α≈0, zero radius), are antipodal
/// (α≈π, the circle collapses to a point), or for a degenerate sphere.
pub fn circle_on_sphere(
    center: DVec3,
    radius: f64,
    center_pt: DVec3,
    radius_pt: DVec3,
) -> Option<crate::curves::AnalyticCurve> {
    use std::f64::consts::PI;
    let cp = project_to_surface(center, radius, center_pt)?;
    let rp = project_to_surface(center, radius, radius_pt)?;
    let axis = (cp - center).normalize_or_zero();
    let urp = (rp - center).normalize_or_zero();
    if axis.length_squared() < 0.5 || urp.length_squared() < 0.5 {
        return None;
    }
    // geodesic radius angle α between the two surface directions.
    let cos_alpha = axis.dot(urp).clamp(-1.0, 1.0);
    let alpha = cos_alpha.acos();
    if alpha < 1e-6 || alpha > PI - 1e-6 {
        return None; // zero-radius or antipodal (point-like) circle
    }
    let circle_center = center + axis * (radius * cos_alpha);
    let circle_radius = radius * alpha.sin();
    // basis_u perpendicular to the axis (robust pick).
    let arb = if axis.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let basis_u = (arb - axis * arb.dot(axis)).normalize_or_zero();
    if basis_u.length_squared() < 0.5 {
        return None;
    }
    Some(crate::curves::AnalyticCurve::Circle {
        center: circle_center,
        radius: circle_radius,
        normal: axis,
        basis_u,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    #[test]
    fn evaluate_north_pole() {
        // u=0, v=π/2 → top of sphere
        let p = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, 0.0, FRAC_PI_2);
        assert!((p - DVec3::new(0.0, 0.0, 5.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_south_pole() {
        let p = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, 0.0, -FRAC_PI_2);
        assert!((p - DVec3::new(0.0, 0.0, -5.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_equator_u_zero_is_x_axis() {
        let p = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, 0.0, 0.0);
        assert!((p - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-12);
    }

    // ── ADR-202 β-1 — great-circle Arc (곡면 위 직접 그리기) ──

    use crate::curves::{AnalyticCurve, CurveOps};
    use crate::mesh::Mesh;

    /// Quarter great-circle arc on the unit sphere: A=(1,0,0), B=(0,1,0).
    /// Endpoints recovered, span = π/2, every sample on the sphere.
    #[test]
    fn adr202_great_circle_quarter_arc() {
        let mesh = Mesh::new();
        let c = DVec3::ZERO;
        let r = 1.0;
        let a = DVec3::new(1.0, 0.0, 0.0);
        let b = DVec3::new(0.0, 1.0, 0.0);
        let arc = sphere_great_circle_arc(c, r, a, b).expect("arc");
        let (sa, ea) = match arc {
            AnalyticCurve::Arc { start_angle, end_angle, .. } => (start_angle, end_angle),
            _ => panic!("expected Arc"),
        };
        assert!((sa - 0.0).abs() < 1e-12, "start at A (angle 0)");
        assert!((ea - FRAC_PI_2).abs() < 1e-9, "quarter arc span π/2, got {}", ea);
        // endpoints
        assert!((arc.evaluate(sa, &mesh).unwrap() - a).length() < 1e-9, "evaluate(start)=A");
        assert!((arc.evaluate(ea, &mesh).unwrap() - b).length() < 1e-9, "evaluate(end)=B");
        // every sample lies on the sphere (great circle).
        for k in 0..=20 {
            let t = sa + (ea - sa) * (k as f64 / 20.0);
            let p = arc.evaluate(t, &mesh).unwrap();
            assert!(((p - c).length() - r).abs() < 1e-9, "sample on sphere @t={}", t);
        }
    }

    /// Points OFF the sphere are projected first: A=(2,0,0), B=(0,3,0) on a
    /// unit sphere → the same quarter arc as the on-sphere case.
    #[test]
    fn adr202_great_circle_projects_offsurface_points() {
        let mesh = Mesh::new();
        let c = DVec3::ZERO;
        let r = 1.0;
        let arc = sphere_great_circle_arc(c, r, DVec3::new(2.0, 0.0, 0.0), DVec3::new(0.0, 3.0, 0.0)).unwrap();
        // endpoints are the projected points (1,0,0) and (0,1,0).
        let (sa, ea) = match arc {
            AnalyticCurve::Arc { start_angle, end_angle, .. } => (start_angle, end_angle),
            _ => panic!("Arc"),
        };
        assert!((arc.evaluate(sa, &mesh).unwrap() - DVec3::new(1.0, 0.0, 0.0)).length() < 1e-9);
        assert!((arc.evaluate(ea, &mesh).unwrap() - DVec3::new(0.0, 1.0, 0.0)).length() < 1e-9);
        assert!((ea - FRAC_PI_2).abs() < 1e-9);
    }

    /// SHORTER arc: B at 270° (−90°) must yield the 90° arc, not 270°.
    #[test]
    fn adr202_great_circle_takes_shorter_arc() {
        let mesh = Mesh::new();
        let c = DVec3::ZERO;
        let r = 1.0;
        let a = DVec3::new(1.0, 0.0, 0.0);
        let b = DVec3::new(0.0, -1.0, 0.0); // 270° CCW from A = 90° the short way
        let arc = sphere_great_circle_arc(c, r, a, b).unwrap();
        let (sa, ea) = match arc {
            AnalyticCurve::Arc { start_angle, end_angle, .. } => (start_angle, end_angle),
            _ => panic!("Arc"),
        };
        assert!((ea - FRAC_PI_2).abs() < 1e-9, "shorter arc = π/2, got {}", ea);
        assert!((arc.evaluate(ea, &mesh).unwrap() - b).length() < 1e-9, "endpoint still B");
    }

    /// Off-center sphere: arc on a sphere centered at (10,10,10).
    #[test]
    fn adr202_great_circle_offcenter_sphere() {
        let mesh = Mesh::new();
        let c = DVec3::new(10.0, 10.0, 10.0);
        let r = 5.0;
        let a = c + DVec3::new(5.0, 0.0, 0.0);
        let b = c + DVec3::new(0.0, 0.0, 5.0);
        let arc = sphere_great_circle_arc(c, r, a, b).unwrap();
        let (sa, ea) = match arc {
            AnalyticCurve::Arc { start_angle, end_angle, .. } => (start_angle, end_angle),
            _ => panic!("Arc"),
        };
        assert!((arc.evaluate(sa, &mesh).unwrap() - a).length() < 1e-9);
        assert!((arc.evaluate(ea, &mesh).unwrap() - b).length() < 1e-9);
        for k in 0..=20 {
            let t = sa + (ea - sa) * (k as f64 / 20.0);
            let p = arc.evaluate(t, &mesh).unwrap();
            assert!(((p - c).length() - r).abs() < 1e-8, "sample on sphere");
        }
    }

    /// Degenerate: coincident and antipodal points → None (great circle undefined).
    #[test]
    fn adr202_great_circle_degenerate_returns_none() {
        let c = DVec3::ZERO;
        let r = 1.0;
        // coincident
        assert!(sphere_great_circle_arc(c, r, DVec3::new(1.0, 0.0, 0.0), DVec3::new(2.0, 0.0, 0.0)).is_none());
        // antipodal
        assert!(sphere_great_circle_arc(c, r, DVec3::new(1.0, 0.0, 0.0), DVec3::new(-1.0, 0.0, 0.0)).is_none());
        // degenerate sphere
        assert!(sphere_great_circle_arc(c, 0.0, DVec3::new(1.0, 0.0, 0.0), DVec3::new(0.0, 1.0, 0.0)).is_none());
    }

    /// project_to_surface basics.
    #[test]
    fn adr202_project_to_surface() {
        let c = DVec3::ZERO;
        let p = project_to_surface(c, 5.0, DVec3::new(10.0, 0.0, 0.0)).unwrap();
        assert!((p - DVec3::new(5.0, 0.0, 0.0)).length() < 1e-12);
        // at center → None
        assert!(project_to_surface(c, 5.0, c).is_none());
    }

    // ── ADR-202 β-2a — circle on sphere (닫힌 원 → 곡면 분할) ──

    /// Circle around the north pole at geodesic radius π/4: lies on the unit
    /// sphere, supporting plane ⊥ axis at z=cos(π/4), radius sin(π/4); Rp is on it.
    #[test]
    fn adr202_circle_on_sphere_north_pole() {
        let mesh = Mesh::new();
        let c = DVec3::ZERO;
        let r = 1.0;
        let cp = DVec3::new(0.0, 0.0, 1.0); // north pole
        let aa = FRAC_PI_2 / 2.0; // π/4
        let rp = DVec3::new(aa.sin(), 0.0, aa.cos()); // on sphere, π/4 from pole
        let circ = circle_on_sphere(c, r, cp, rp).expect("circle");
        match circ {
            AnalyticCurve::Circle { center, radius, normal, .. } => {
                assert!((normal - DVec3::Z).length() < 1e-9, "axis = +Z (pole)");
                assert!((center - DVec3::new(0.0, 0.0, aa.cos())).length() < 1e-9, "plane at z=cos π/4");
                assert!((radius - aa.sin()).abs() < 1e-9, "circle radius = sin π/4");
            }
            _ => panic!("Circle"),
        }
        // Rp is exactly on the circle; every circle sample lies on the sphere.
        let on_circle = |p: DVec3| -> bool {
            // distance from sphere center is R AND from axis is circle_radius.
            ((p - c).length() - r).abs() < 1e-9
        };
        assert!(on_circle(rp));
        for k in 0..16 {
            let theta = std::f64::consts::TAU * (k as f64 / 16.0);
            let p = circ.evaluate(theta, &mesh).unwrap();
            assert!(on_circle(p), "circle sample on sphere @θ={}", theta);
        }
    }

    /// Off-center sphere + non-axis-aligned circle: all samples on the sphere,
    /// Rp on the circle.
    #[test]
    fn adr202_circle_on_sphere_offcenter_tilted() {
        let mesh = Mesh::new();
        let c = DVec3::new(3.0, -2.0, 7.0);
        let r = 4.0;
        let cp = c + DVec3::new(1.0, 1.0, 1.0).normalize() * r; // surface point
        let rp = c + DVec3::new(1.0, 0.0, 0.5).normalize() * r; // another surface point
        let circ = circle_on_sphere(c, r, cp, rp).expect("circle");
        // Rp lies on the resulting circle (closest-param check).
        let rp_dist = match &circ {
            AnalyticCurve::Circle { center, radius, normal, .. } => {
                let d = rp - *center;
                let along = d.dot(*normal);
                let radial = (d - *normal * along).length();
                (along.abs(), (radial - *radius).abs())
            }
            _ => panic!("Circle"),
        };
        assert!(rp_dist.0 < 1e-7, "Rp in circle plane");
        assert!(rp_dist.1 < 1e-7, "Rp at circle radius");
        for k in 0..16 {
            let theta = std::f64::consts::TAU * (k as f64 / 16.0);
            let p = circ.evaluate(theta, &mesh).unwrap();
            assert!(((p - c).length() - r).abs() < 1e-7, "sample on sphere");
        }
    }

    /// Degenerate: coincident (zero radius) / antipodal (point) / center pt → None.
    #[test]
    fn adr202_circle_on_sphere_degenerate_none() {
        let c = DVec3::ZERO;
        let r = 1.0;
        // coincident center/radius point → α≈0
        assert!(circle_on_sphere(c, r, DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, 2.0)).is_none());
        // antipodal → α≈π
        assert!(circle_on_sphere(c, r, DVec3::new(0.0, 0.0, 1.0), DVec3::new(0.0, 0.0, -1.0)).is_none());
        // center point at sphere center → no projection
        assert!(circle_on_sphere(c, r, c, DVec3::new(1.0, 0.0, 0.0)).is_none());
    }

    #[test]
    fn evaluate_equator_quarter_u_is_y_axis() {
        let p = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, FRAC_PI_2, 0.0);
        assert!((p - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_radius_invariant_everywhere() {
        let center = DVec3::ZERO;
        let r = 7.0;
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            for v_step in -4..=4 {
                let v = (v_step as f64) * std::f64::consts::FRAC_PI_8;
                let p = evaluate(center, r, DVec3::Z, DVec3::X, u, v);
                let dist = (p - center).length();
                assert!((dist - r).abs() < 1e-9,
                    "u={}, v={}: |p-c|={} ≠ r={}", u, v, dist, r);
            }
        }
    }

    #[test]
    fn evaluate_offset_center() {
        let c = DVec3::new(1.0, 2.0, 3.0);
        let p = evaluate(c, 5.0, DVec3::Z, DVec3::X, 0.0, 0.0);
        assert!((p - DVec3::new(6.0, 2.0, 3.0)).length() < 1e-9);
    }

    #[test]
    fn evaluate_full_longitude_period() {
        let p0 = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, 0.0, 0.0);
        let p1 = evaluate(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, 2.0 * PI, 0.0);
        assert!((p0 - p1).length() < 1e-9);
    }

    #[test]
    fn normal_unit_length_everywhere() {
        for u_step in 0..8 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_4;
            for v_step in -3..=3 {
                let v = (v_step as f64) * 0.4;  // avoid exact poles
                let n = normal(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, u, v);
                assert!((n.length() - 1.0).abs() < 1e-9,
                    "u={}, v={}: normal length={}", u, v, n.length());
            }
        }
    }

    #[test]
    fn normal_radial_outward_from_center() {
        let center = DVec3::new(10.0, 20.0, 30.0);
        let p = evaluate(center, 5.0, DVec3::Z, DVec3::X, 0.5, 0.3);
        let n = normal(center, 5.0, DVec3::Z, DVec3::X, 0.5, 0.3);
        let radial = (p - center).normalize();
        assert!((n - radial).length() < 1e-9);
    }

    #[test]
    fn derivative_u_perpendicular_to_normal() {
        for u_step in 0..6 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_3;
            for v_step in -2..=2 {
                let v = (v_step as f64) * 0.4;
                let n = normal(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, u, v);
                let d = derivative_u(5.0, DVec3::Z, DVec3::X, u, v);
                if d.length() > 1e-9 {
                    let dot = n.dot(d.normalize()).abs();
                    assert!(dot < 1e-9, "u={}, v={}: dot={}", u, v, dot);
                }
            }
        }
    }

    #[test]
    fn derivative_v_perpendicular_to_normal() {
        for u_step in 0..6 {
            let u = (u_step as f64) * std::f64::consts::FRAC_PI_3;
            for v_step in -2..=2 {
                let v = (v_step as f64) * 0.4;
                let n = normal(DVec3::ZERO, 5.0, DVec3::Z, DVec3::X, u, v);
                let d = derivative_v(5.0, DVec3::Z, DVec3::X, u, v);
                if d.length() > 1e-9 {
                    let dot = n.dot(d.normalize()).abs();
                    assert!(dot < 1e-9, "u={}, v={}: dot={}", u, v, dot);
                }
            }
        }
    }

    // ── ADR-204 — oriented sphere (axis_dir / ref_dir) ──

    /// axis_dir = +Y, ref_dir = +Z → pole on +Y, seam on +Z.
    #[test]
    fn adr204_oriented_axis_y_pole_on_axis() {
        let c = DVec3::new(1.0, 2.0, 3.0);
        let r = 4.0;
        // v = +π/2 (north pole) → center + axis_dir·r.
        let pole = evaluate(c, r, DVec3::Y, DVec3::Z, 0.0, FRAC_PI_2);
        assert!((pole - (c + DVec3::Y * r)).length() < 1e-9, "pole on axis_dir, got {:?}", pole);
        // u = 0, v = 0 (seam) → center + ref_dir·r.
        let seam = evaluate(c, r, DVec3::Y, DVec3::Z, 0.0, 0.0);
        assert!((seam - (c + DVec3::Z * r)).length() < 1e-9, "seam on ref_dir, got {:?}", seam);
    }

    /// axis_dir = +Z, ref_dir = +X reproduces the legacy world-Z/X formula exactly.
    #[test]
    fn adr204_zx_basis_byte_identical_to_legacy() {
        let c = DVec3::new(2.0, -1.0, 0.5);
        let r = 3.0;
        for &(u, v) in &[(0.0, 0.0), (0.7, 0.3), (FRAC_PI_2, -0.4), (PI, 0.9)] {
            let p = evaluate(c, r, DVec3::Z, DVec3::X, u, v);
            let legacy = c + DVec3::new(r * v.cos() * u.cos(), r * v.cos() * u.sin(), r * v.sin());
            assert!((p - legacy).length() < 1e-12, "Z/X == legacy at (u={}, v={})", u, v);
        }
    }

    /// Oriented sphere: normal stays radial, derivatives ⟂ normal.
    #[test]
    fn adr204_oriented_normal_radial_derivatives_perpendicular() {
        let c = DVec3::ZERO;
        let r = 5.0;
        let axis = DVec3::new(0.3, 0.4, 0.866).normalize();
        let refd = DVec3::Y;
        for &(u, v) in &[(0.5, 0.2), (2.0, -0.3), (4.0, 0.9)] {
            let p = evaluate(c, r, axis, refd, u, v);
            let n = normal(c, r, axis, refd, u, v);
            assert!((n - (p - c).normalize()).length() < 1e-9, "normal radial");
            assert!((n.length() - 1.0).abs() < 1e-9, "unit normal");
            let du = derivative_u(r, axis, refd, u, v);
            let dv = derivative_v(r, axis, refd, u, v);
            if du.length() > 1e-9 { assert!(n.dot(du.normalize()).abs() < 1e-9, "du ⊥ n"); }
            assert!(n.dot(dv.normalize()).abs() < 1e-9, "dv ⊥ n");
        }
    }

    /// `ref_dir` not exactly ⟂ `axis_dir` is Gram-Schmidt orthogonalized (robust).
    #[test]
    fn adr204_non_orthogonal_ref_is_orthogonalized() {
        let c = DVec3::ZERO;
        let r = 1.0;
        let axis = DVec3::Z;
        let skew_ref = DVec3::new(1.0, 0.0, 0.5); // has a +Z (axis) component
        // pole still on +Z, equator still on the sphere.
        let pole = evaluate(c, r, axis, skew_ref, 0.0, FRAC_PI_2);
        assert!((pole - DVec3::Z).length() < 1e-9, "pole on axis despite skew ref");
        let p = evaluate(c, r, axis, skew_ref, 1.0, 0.2);
        assert!((p.length() - 1.0).abs() < 1e-9, "on unit sphere");
    }
}
