//! Surface point at a given geodesic distance — the inverse of what
//! `circle_on_*` measures.
//!
//! ADR-284 follow-up. `circle_on_{cylinder,sphere,cone,torus}` take a centre
//! point and a *radius point* and derive the circle's radius as the distance
//! between them ALONG THE SURFACE. That is right for a mouse, which has a
//! point. It is useless for a typed number: to honour "radius = 50" the caller
//! needs a radius point whose geodesic distance is exactly 50, and offsetting
//! 50 in the tangent plane is not that — it lands ~2% short at r=200/d=50 and
//! ~7% at d=100. Rather than draw a quietly wrong dimension, `applyVCBValue`
//! declined. This is what lets it stop declining.
//!
//! No inverse formula is derived per surface, because there is no need: each
//! surface has a direction along which the geodesic distance IS the offset, so
//! the point can be constructed rather than solved for.
//!
//! | surface  | direction        | why the distance is exactly `d`                |
//! |----------|------------------|------------------------------------------------|
//! | Cylinder | axial            | `rho = sqrt((r·Δu)² + Δv²)`, Δu=0 ⇒ rho = Δv    |
//! | Sphere   | any great circle | `rho = r·α`, so α = d/r                         |
//! | Cone     | slant (v)        | unrolled radius L = v/cos α; radial ⇒ ΔL = d    |
//! | Torus    | meridian         | minor-circle arc length = `minor·Δv`            |
//!
//! The circle is symmetric about its centre, so *which* direction does not
//! matter — only that the distance is honest along it.
//!
//! For the torus the guarantee is relative to the engine's own metric, which is
//! itself an approximation: a torus is not developable, so `circle_on_torus`
//! uses a param-space metric-scaled circle (ADR-263 L-263-2). Exactness here
//! means "matches what circle_on_torus will measure", not "geodesic in the
//! Riemannian sense". The tests assert against the forward metric for that
//! reason — the point of this helper is that the two agree.

use super::AnalyticSurface;
use glam::DVec3;

/// A point on `surface` whose geodesic distance from `center_pt` is `d`, as the
/// matching `circle_on_*` measures it.
///
/// `center_pt` need not be exactly on the surface — it is projected first, the
/// same way `circle_on_*` projects it. Returns `None` for a non-curved surface,
/// a non-positive `d`, non-finite input, or a degenerate projection (a point on
/// the axis, at a cone's apex…).
pub fn surface_point_at_geodesic_distance(
    surface: &AnalyticSurface,
    center_pt: DVec3,
    d: f64,
) -> Option<DVec3> {
    if !center_pt.is_finite() || !d.is_finite() || d <= 0.0 {
        return None;
    }
    match surface {
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } => {
            // Axial: Δu = 0, so the unrolled distance is |Δv| = d exactly.
            let (sp, _u, _v) = super::cylinder::project_to_cylinder(
                *axis_origin, *axis_dir, *radius, *ref_dir, center_pt,
            )?;
            Some(sp + axis_dir.normalize() * d)
        }

        AnalyticSurface::Sphere { center, radius, .. } => {
            // Great circle: rho = r·α, so sweep the centre direction by α = d/r.
            // Beyond a half turn the "circle" has wrapped past the far pole and
            // starts shrinking again — refuse rather than return an ambiguity.
            let alpha = d / *radius;
            if alpha >= std::f64::consts::PI {
                return None;
            }
            let sp = super::sphere::project_to_surface(*center, *radius, center_pt)?;
            let radial = (sp - *center).normalize();
            // Any axis perpendicular to `radial` gives the same geodesic
            // distance; pick a stable one.
            let seed = if radial.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
            let tangent = radial.cross(seed).normalize().cross(radial).normalize();
            Some(*center + (radial * alpha.cos() + tangent * alpha.sin()) * *radius)
        }

        AnalyticSurface::Cone { apex, axis_dir, half_angle, .. } => {
            // Unrolled, the cone is a sector whose radius is the slant length
            // L = v / cos(half_angle). Moving radially (along the slant) by d
            // changes L by exactly d.
            let sin_a = half_angle.sin();
            let cos_a = half_angle.cos();
            if !(sin_a > 1e-9) || !(cos_a > 1e-9) {
                return None; // degenerate cone — flat disc or a needle
            }
            let axis = axis_dir.normalize();
            let rel = center_pt - *apex;
            let v = rel.dot(axis); // axial distance from the apex
            if v <= 1e-9 {
                return None; // at/behind the apex — no surface direction
            }
            let radial = (rel - axis * v).normalize_or_zero();
            if radial.length_squared() < 0.5 {
                return None; // exactly on the axis — direction undefined
            }
            // Surface point at slant length L: apex + axis·(L·cosα) + radial·(L·sinα)
            let l0 = v / cos_a;
            let l1 = l0 + d;
            Some(*apex + axis * (l1 * cos_a) + radial * (l1 * sin_a))
        }

        AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } => {
            // Meridian: the minor circle has arc length minor·Δv, so Δv = d/minor.
            // Past a full turn it wraps onto itself.
            if !(*minor_radius > 1e-9) {
                return None;
            }
            let dv = d / *minor_radius;
            if dv >= std::f64::consts::TAU {
                return None;
            }
            let (_sp, u, v) = super::torus::project_to_torus(
                *center, *axis_dir, *ref_dir, *major_radius, *minor_radius, center_pt,
            )?;
            Some(super::torus::evaluate(
                *center, *axis_dir, *ref_dir, *major_radius, *minor_radius, u, v + dv,
            ))
        }

        // Plane and the NURBS-class surfaces have no geodesic-vs-chord gap worth
        // inverting here (a plane's geodesic IS the chord); the caller uses the
        // planar path for those.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surfaces::AnalyticSurface;
    use std::f64::consts::PI;

    fn cyl() -> AnalyticSurface {
        AnalyticSurface::Cylinder {
            axis_origin: DVec3::ZERO,
            axis_dir: DVec3::Z,
            radius: 200.0,
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, 400.0),
        }
    }

    #[test]
    fn cylinder_point_is_on_surface_at_axial_distance_d() {
        let s = cyl();
        let c = DVec3::new(200.0, 0.0, 200.0); // on the surface
        let p = surface_point_at_geodesic_distance(&s, c, 50.0).expect("cylinder point");
        // on the surface: distance to the axis is still the radius
        assert!((p.truncate().length() - 200.0).abs() < 1e-9, "off-surface: {p:?}");
        // purely axial ⇒ the engine's rho = sqrt((r·0)² + Δv²) = 50 exactly
        assert!((p.z - 250.0).abs() < 1e-9, "axial offset != d: {p:?}");
        assert!((p.x - 200.0).abs() < 1e-9 && p.y.abs() < 1e-9, "drifted around: {p:?}");
    }

    #[test]
    fn cylinder_engine_circle_through_it_has_radius_d() {
        // The decisive check: feed the point back to the ENGINE's own
        // circle_on_cylinder and confirm the geodesic radius it measures is d.
        // The samples' axial extent must span the centre ±d.
        let c = DVec3::new(200.0, 0.0, 200.0);
        let p = surface_point_at_geodesic_distance(&cyl(), c, 50.0).unwrap();
        let pts = super::super::cylinder::circle_on_cylinder(
            DVec3::ZERO, DVec3::Z, 200.0, DVec3::X, c, p, 0.05,
        )
        .expect("engine circle");
        let zmin = pts.iter().map(|q| q.z).fold(f64::MAX, f64::min);
        let zmax = pts.iter().map(|q| q.z).fold(f64::MIN, f64::max);
        assert!((zmax - 250.0).abs() < 0.5, "top of circle != centre+d: {zmax}");
        assert!((zmin - 150.0).abs() < 0.5, "bottom != centre-d: {zmin}");
    }

    #[test]
    fn sphere_engine_circle_radius_matches_r_sin_alpha() {
        // rho = r·α, so a geodesic radius of d must produce a circle whose
        // EUCLIDEAN radius is r·sin(d/r) — read off the engine's own output.
        let s = AnalyticSurface::Sphere {
            center: DVec3::ZERO, radius: 200.0, ref_dir: DVec3::X, axis_dir: DVec3::Z,
            u_range: (0.0, std::f64::consts::TAU), v_range: (-PI / 2.0, PI / 2.0),
        };
        let c = DVec3::new(200.0, 0.0, 0.0);
        let d = 50.0;
        let p = surface_point_at_geodesic_distance(&s, c, d).expect("sphere point");
        assert!((p.length() - 200.0).abs() < 1e-9, "off-surface: {p:?}");

        let curve = super::super::sphere::circle_on_sphere(DVec3::ZERO, 200.0, c, p)
            .expect("engine circle");
        match curve {
            crate::curves::AnalyticCurve::Circle { radius, .. } => {
                let expected = 200.0 * (d / 200.0).sin();
                assert!(
                    (radius - expected).abs() < 1e-6,
                    "engine radius {radius} != r·sin(d/r) {expected}",
                );
            }
            other => panic!("expected a Circle, got {other:?}"),
        }
    }

    #[test]
    fn sphere_beats_the_tangent_plane_shortcut() {
        // Why this helper exists. Offsetting d in the tangent plane — what the
        // VCB path would have done — lands SHORT, and the gap grows with d.
        let s = AnalyticSurface::Sphere {
            center: DVec3::ZERO, radius: 200.0, ref_dir: DVec3::X, axis_dir: DVec3::Z,
            u_range: (0.0, std::f64::consts::TAU), v_range: (-PI / 2.0, PI / 2.0),
        };
        let c = DVec3::new(200.0, 0.0, 0.0);
        for d in [50.0_f64, 100.0] {
            let p = surface_point_at_geodesic_distance(&s, c, d).unwrap();
            // ours: the swept angle is exactly d/r
            let ours = (p.normalize().dot(c.normalize())).acos() * 200.0;
            assert!((ours - d).abs() < 1e-6, "ours {ours} != {d}");
            // naive: a tangent-plane point at offset d projects to r·atan(d/r)
            let naive = 200.0 * (d / 200.0).atan();
            assert!(naive < d, "the shortcut must fall short");
            let err = (d - naive) / d;
            assert!(err > 0.01, "the error this fixes should be >1%, got {:.3}%", err * 100.0);
        }
    }

    #[test]
    fn cone_point_lies_on_the_cone_at_slant_offset_d() {
        let s = AnalyticSurface::Cone {
            apex: DVec3::new(0.0, 0.0, 400.0),
            axis_dir: -DVec3::Z, // apex → base
            half_angle: (200.0_f64 / 400.0).atan(),
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, 400.0),
        };
        let c = DVec3::new(100.0, 0.0, 200.0); // on the cone: r = 100 at v = 200
        let d = 40.0;
        let p = surface_point_at_geodesic_distance(&s, c, d).expect("cone point");
        // still on the cone: radial distance / axial distance == tan(half_angle)
        let axial = (DVec3::new(0.0, 0.0, 400.0) - p).dot(DVec3::Z);
        let radial = p.truncate().length();
        let ha = (200.0_f64 / 400.0).atan();
        assert!((radial / axial - ha.tan()).abs() < 1e-9, "off-cone: {p:?}");
        // slant length grew by exactly d
        let l0 = 200.0 / ha.cos();
        let l1 = axial / ha.cos();
        assert!((l1 - l0 - d).abs() < 1e-9, "slant delta {} != {d}", l1 - l0);
    }

    #[test]
    fn torus_point_advances_the_meridian_by_arc_length_d() {
        let s = AnalyticSurface::Torus {
            center: DVec3::ZERO, axis_dir: DVec3::Z, ref_dir: DVec3::X,
            major_radius: 300.0, minor_radius: 80.0,
            u_range: (0.0, std::f64::consts::TAU), v_range: (0.0, std::f64::consts::TAU),
        };
        let c = DVec3::new(380.0, 0.0, 0.0); // outer equator
        let d = 40.0;
        let p = surface_point_at_geodesic_distance(&s, c, d).expect("torus point");
        let (_sp, _u, v) = super::super::torus::project_to_torus(
            DVec3::ZERO, DVec3::Z, DVec3::X, 300.0, 80.0, p,
        )
        .unwrap();
        // arc length along the minor circle = minor·Δv
        assert!((v * 80.0 - d).abs() < 1e-6, "meridian arc {} != {d}", v * 80.0);
    }

    #[test]
    fn refuses_what_it_cannot_answer() {
        let s = cyl();
        let c = DVec3::new(200.0, 0.0, 200.0);
        assert!(surface_point_at_geodesic_distance(&s, c, 0.0).is_none(), "d=0");
        assert!(surface_point_at_geodesic_distance(&s, c, -5.0).is_none(), "d<0");
        assert!(surface_point_at_geodesic_distance(&s, c, f64::NAN).is_none(), "NaN d");
        assert!(
            surface_point_at_geodesic_distance(&s, DVec3::new(f64::NAN, 0.0, 0.0), 5.0).is_none(),
            "NaN centre",
        );
        // a plane has no geodesic-vs-chord gap — the caller uses the planar path
        let plane = AnalyticSurface::Plane {
            origin: DVec3::ZERO, normal: DVec3::Z, basis_u: DVec3::X,
            u_range: (-1.0, 1.0), v_range: (-1.0, 1.0),
        };
        assert!(surface_point_at_geodesic_distance(&plane, DVec3::ZERO, 5.0).is_none());
        // past half a turn the sphere's circle wraps the far pole
        let sph = AnalyticSurface::Sphere {
            center: DVec3::ZERO, radius: 10.0, ref_dir: DVec3::X, axis_dir: DVec3::Z,
            u_range: (0.0, std::f64::consts::TAU), v_range: (-PI / 2.0, PI / 2.0),
        };
        assert!(
            surface_point_at_geodesic_distance(&sph, DVec3::new(10.0, 0.0, 0.0), 100.0).is_none(),
            "d > πr must be refused, not wrapped",
        );
    }
}
