//! Analytic SSI shortcuts — closed-form solutions for common primitive pairs
//! (Phase F Stage 1, ADR-034 §P19.6).
//!
//! These bypass the general subdivision algorithm when both surfaces are
//! analytic primitives with a well-known intersection form.

use glam::DVec3;

use super::SurfaceIntersection;

/// Plane-Plane intersection.
///
/// Returns:
/// - **Disjoint** (parallel, different offset): empty intersection
/// - **Coincident** (same plane): tangent_warning=true, empty points
/// - **Intersecting**: line of intersection (sampled as N points along the line)
///
/// `n_samples` controls how many points to sample along the intersection line.
/// `extent` is the half-length of the sampled segment around the closest point
/// to origin (mm).
pub fn plane_plane(
    origin_a: DVec3, normal_a: DVec3,
    origin_b: DVec3, normal_b: DVec3,
    n_samples: usize,
    extent: f64,
) -> SurfaceIntersection {
    let na = normal_a.normalize_or_zero();
    let nb = normal_b.normalize_or_zero();
    if na.length_squared() < 0.5 || nb.length_squared() < 0.5 {
        return SurfaceIntersection::default();
    }
    // Direction of intersection line = na × nb
    let dir = na.cross(nb);
    let dir_len = dir.length();

    if dir_len < 1e-9 {
        // Parallel planes
        let offset = (origin_b - origin_a).dot(na).abs();
        let mut result = SurfaceIntersection::default();
        if offset < 1e-9 {
            // Coincident — infinite intersection (tangent contact)
            result.tangent_warning = true;
        }
        // else: parallel disjoint, empty
        return result;
    }

    let dir_unit = dir / dir_len;

    // Solve for a point on both planes — Lagrange / pseudo-inverse style.
    // Plane A: na · X = na · origin_a
    // Plane B: nb · X = nb · origin_b
    // Pick X = α na + β nb (any 3rd direction would also work)
    let d_a = na.dot(origin_a);
    let d_b = nb.dot(origin_b);
    let denom = 1.0 - na.dot(nb).powi(2);
    if denom.abs() < 1e-12 {
        // Should be caught by parallel check, but defensive.
        return SurfaceIntersection::default();
    }
    let alpha = (d_a - d_b * na.dot(nb)) / denom;
    let beta = (d_b - d_a * na.dot(nb)) / denom;
    let p_on_line = na * alpha + nb * beta;

    // Sample N points along the line: p_on_line ± extent
    let n = n_samples.max(2);
    let mut points = Vec::with_capacity(n);
    let mut uv_a = Vec::with_capacity(n);
    let mut uv_b = Vec::with_capacity(n);
    for i in 0..n {
        let t = -extent + 2.0 * extent * (i as f64) / ((n - 1) as f64);
        let p = p_on_line + dir_unit * t;
        points.push(p);
        // For Plane parameterization: project p onto plane's basis_u/v.
        // Without basis info here, just use 0.5/0.5 placeholder; caller can
        // refine via plane.evaluate inverse.
        uv_a.push((0.5, 0.5));
        uv_b.push((0.5, 0.5));
    }
    SurfaceIntersection {
        points, uv_a, uv_b,
        closed: false,
        tangent_warning: false,
    }
}

/// Plane-Cylinder intersection.
///
/// Cylinder defined by `axis_origin + s · axis_dir` for `s ∈ ℝ` with radius `r`.
///
/// Result depends on plane-axis angle θ (between plane normal and axis):
/// - **θ = 0** (plane perpendicular to axis): circle of radius `r`
/// - **0 < θ < π/2**: ellipse (semi-major = r/cos(θ), semi-minor = r)
/// - **θ = π/2** (plane parallel to axis): two parallel lines (or none if
///   plane misses cylinder, or one tangent line)
///
/// MVP: returns sampled points along the intersection curve. `n_samples` for
/// circle/ellipse, fewer for tangent line.
#[allow(clippy::too_many_arguments)]
pub fn plane_cylinder(
    plane_origin: DVec3, plane_normal: DVec3,
    cyl_axis_origin: DVec3, cyl_axis_dir: DVec3, cyl_radius: f64,
    n_samples: usize,
) -> SurfaceIntersection {
    let n = plane_normal.normalize_or_zero();
    let a = cyl_axis_dir.normalize_or_zero();
    if n.length_squared() < 0.5 || a.length_squared() < 0.5 || cyl_radius <= 0.0 {
        return SurfaceIntersection::default();
    }

    let cos_theta = n.dot(a).abs();
    let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();

    if cos_theta > 1.0 - 1e-9 {
        // Plane perpendicular to axis → circle.
        // Find center: project cyl_axis_origin onto plane.
        let d = (cyl_axis_origin - plane_origin).dot(n);
        let center = cyl_axis_origin - n * d;
        // Build basis in plane perpendicular to axis.
        let arb = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u_basis = a.cross(arb).normalize_or_zero();
        let v_basis = a.cross(u_basis).normalize_or_zero();

        let n_pts = n_samples.max(8);
        let mut points = Vec::with_capacity(n_pts + 1);
        let mut uv_a = Vec::with_capacity(n_pts + 1);
        let mut uv_b = Vec::with_capacity(n_pts + 1);
        for i in 0..=n_pts {
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_pts as f64);
            let p = center + u_basis * (cyl_radius * theta.cos())
                           + v_basis * (cyl_radius * theta.sin());
            points.push(p);
            uv_a.push((0.5, 0.5));
            uv_b.push((theta, 0.0));
        }
        return SurfaceIntersection {
            points, uv_a, uv_b,
            closed: true,
            tangent_warning: false,
        };
    }

    if sin_theta < 1e-9 {
        // Should already be caught by cos_theta check.
        return SurfaceIntersection::default();
    }

    if cos_theta < 1e-9 {
        // Plane parallel to axis — possibly two parallel lines / none / tangent.
        // Distance from axis to plane:
        let d = (cyl_axis_origin - plane_origin).dot(n).abs();
        if d > cyl_radius + 1e-9 {
            return SurfaceIntersection::default();  // disjoint
        }
        // Compute foot of axis on plane
        let foot = cyl_axis_origin - n * (cyl_axis_origin - plane_origin).dot(n);
        let half_chord = ((cyl_radius * cyl_radius - d * d).max(0.0)).sqrt();
        // Two lines parallel to axis, offset by ±half_chord along (n × a)
        let perp = n.cross(a).normalize_or_zero();
        let line_extent = cyl_radius * 4.0;  // arbitrary sample range
        let n_pts = n_samples.max(4);
        let mut points = Vec::new();
        let mut uv_a = Vec::new();
        let mut uv_b = Vec::new();
        for sign in [1.0_f64, -1.0_f64] {
            let line_origin = foot + perp * (half_chord * sign);
            for i in 0..n_pts {
                let t = -line_extent
                    + 2.0 * line_extent * (i as f64) / ((n_pts - 1) as f64);
                let p = line_origin + a * t;
                points.push(p);
                uv_a.push((0.5, 0.5));
                uv_b.push((0.0, t));
            }
        }
        return SurfaceIntersection {
            points, uv_a, uv_b,
            closed: false,
            tangent_warning: half_chord < 1e-6,
        };
    }

    // General case: plane angle 0 < θ < π/2 → ellipse.
    // Center: intersection of axis with plane.
    let denom_axis = a.dot(n);
    if denom_axis.abs() < 1e-12 {
        return SurfaceIntersection::default();
    }
    let s_center = (plane_origin - cyl_axis_origin).dot(n) / denom_axis;
    let center = cyl_axis_origin + a * s_center;

    // Ellipse axes (θ = angle between the plane NORMAL and the axis):
    // - minor axis = perpendicular to (axis projected onto plane), length = r
    // - major axis = (axis projected onto plane).normalize() · r/cos(θ)
    //
    // ★ r/COS, not r/sin. At θ→0 the plane is perpendicular to the axis and the
    // cut is a circle (major → r); at θ→π/2 it is parallel and the cut opens
    // into two lines (major → ∞). `r/sin` inverts both limits. It read as
    // correct because sin = cos at 45°, and 45° was the only oblique angle any
    // test covered — measured 2026-08-10 on r=40 (max deviation OFF the
    // cylinder): 15° → 109.28, 30° → 29.28, 45° → 0, 60° → 16.91, 75° → 29.28.
    let axis_in_plane = (a - n * a.dot(n)).normalize_or_zero();
    let minor_axis = n.cross(axis_in_plane).normalize_or_zero();
    let major_len = cyl_radius / cos_theta;

    let n_pts = n_samples.max(8);
    let mut points = Vec::with_capacity(n_pts + 1);
    let mut uv_a = Vec::with_capacity(n_pts + 1);
    let mut uv_b = Vec::with_capacity(n_pts + 1);
    for i in 0..=n_pts {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_pts as f64);
        let p = center
            + axis_in_plane * (major_len * theta.cos())
            + minor_axis * (cyl_radius * theta.sin());
        points.push(p);
        uv_a.push((0.5, 0.5));
        uv_b.push((theta, 0.0));
    }
    SurfaceIntersection {
        points, uv_a, uv_b,
        closed: true,
        tangent_warning: false,
    }
}

/// Plane-Sphere intersection.
///
/// Given a plane (origin + normal) and a sphere (center + radius), the
/// intersection is determined by the signed distance `d` from the sphere
/// center to the plane:
/// - `|d| > r`        → empty (disjoint)
/// - `|d| == r` (±ε) → tangent point (1 sample, `tangent_warning=true`)
/// - `|d| < r`        → circle of radius `√(r² - d²)` centered at the
///                      projection of the sphere center onto the plane,
///                      lying in the plane.
///
/// `n_samples` is the number of points sampled around the circle (caller
/// decides density). For the tangent case, exactly one point is returned.
pub fn plane_sphere(
    plane_origin: DVec3, plane_normal: DVec3,
    sphere_center: DVec3, sphere_radius: f64,
    n_samples: usize,
) -> SurfaceIntersection {
    let n = plane_normal.normalize_or_zero();
    if n.length_squared() < 0.5 || sphere_radius <= 0.0 {
        return SurfaceIntersection::default();
    }

    // Signed distance from sphere center to plane.
    let d = (sphere_center - plane_origin).dot(n);
    let abs_d = d.abs();

    // Disjoint
    if abs_d > sphere_radius + 1e-9 {
        return SurfaceIntersection::default();
    }

    // Tangent (single point)
    let foot = sphere_center - n * d;
    if (abs_d - sphere_radius).abs() < 1e-9 {
        return SurfaceIntersection {
            points: vec![foot],
            uv_a: vec![(0.5, 0.5)],
            uv_b: vec![(0.0, 0.0)],
            closed: false,
            tangent_warning: true,
        };
    }

    // Proper circle
    let r_circ = (sphere_radius * sphere_radius - d * d).max(0.0).sqrt();

    // Build orthonormal basis in plane.
    let arb = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u_basis = n.cross(arb).normalize_or_zero();
    let v_basis = n.cross(u_basis).normalize_or_zero();

    let n_pts = n_samples.max(8);
    let mut points = Vec::with_capacity(n_pts + 1);
    let mut uv_a = Vec::with_capacity(n_pts + 1);
    let mut uv_b = Vec::with_capacity(n_pts + 1);
    for i in 0..=n_pts {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_pts as f64);
        let p = foot + u_basis * (r_circ * theta.cos())
                     + v_basis * (r_circ * theta.sin());
        points.push(p);
        uv_a.push((0.5, 0.5));
        // Sphere uv: (longitude θ, latitude derived from foot's z relative
        // to sphere center). Caller can refine via Sphere::project.
        uv_b.push((theta, 0.0));
    }
    SurfaceIntersection {
        points, uv_a, uv_b,
        closed: true,
        tangent_warning: false,
    }
}

/// Plane-Cone intersection.
///
/// Cone defined by `apex`, `axis_dir` (unit, pointing into the nappe), and
/// `half_angle` α (radians, 0 < α < π/2). Surface points: for s ≥ 0,
///   `P(s, θ) = apex + s·(axis·cosα + r_dir(θ)·sinα)`
///
/// For each sweep angle θ, we intersect the generator ray with the plane.
/// The resulting point set traces the conic section uniformly:
/// - Plane perpendicular to axis (|n·axis|=1) and not through apex → **circle**
/// - Plane through apex (n·(plane_origin-apex)=0) → degenerate (apex only,
///   `tangent_warning=true`)
/// - Plane oblique, all generator rays hit positive s → **ellipse** (closed)
/// - Plane parallel to one generator → **parabola** (open, missing one
///   sample where denominator ≈ 0)
/// - Plane tilted past slant angle → **hyperbola** branch (open)
///
/// `n_samples` controls angular sweep density.
pub fn plane_cone(
    plane_origin: DVec3, plane_normal: DVec3,
    apex: DVec3, axis_dir: DVec3, half_angle: f64,
    n_samples: usize,
) -> SurfaceIntersection {
    let n = plane_normal.normalize_or_zero();
    let a = axis_dir.normalize_or_zero();
    if n.length_squared() < 0.5 || a.length_squared() < 0.5 {
        return SurfaceIntersection::default();
    }
    if !(half_angle > 1e-9 && half_angle < std::f64::consts::FRAC_PI_2 - 1e-9) {
        return SurfaceIntersection::default();
    }

    let cos_a = half_angle.cos();
    let sin_a = half_angle.sin();

    // Plane-apex distance (signed)
    let d_apex = (apex - plane_origin).dot(n);

    // If plane passes through apex → degenerate (line(s) through apex). MVP
    // returns apex with tangent_warning.
    if d_apex.abs() < 1e-9 {
        return SurfaceIntersection {
            points: vec![apex],
            uv_a: vec![(0.5, 0.5)],
            uv_b: vec![(0.0, 0.0)],
            closed: false,
            tangent_warning: true,
        };
    }

    // Build orthonormal basis perpendicular to axis for r_dir(θ).
    let arb = if a.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u_basis = a.cross(arb).normalize_or_zero();
    let v_basis = a.cross(u_basis).normalize_or_zero();

    let n_pts = n_samples.max(16);
    let mut points = Vec::with_capacity(n_pts + 1);
    let mut uv_a = Vec::with_capacity(n_pts + 1);
    let mut uv_b = Vec::with_capacity(n_pts + 1);
    let mut all_valid = true;
    let plane_d_const = (plane_origin - apex).dot(n);  // numerator constant

    for i in 0..=n_pts {
        let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n_pts as f64);
        let r_dir = u_basis * theta.cos() + v_basis * theta.sin();
        let gen = a * cos_a + r_dir * sin_a;
        let denom = gen.dot(n);
        if denom.abs() < 1e-12 {
            // Generator parallel to plane — parabola asymptote miss.
            all_valid = false;
            continue;
        }
        let s = plane_d_const / denom;
        if s < 0.0 {
            // Hits opposite nappe (negative s) — skip.
            all_valid = false;
            continue;
        }
        let p = apex + gen * s;
        points.push(p);
        uv_a.push((0.5, 0.5));
        uv_b.push((theta, s));
    }

    if points.is_empty() {
        return SurfaceIntersection::default();
    }

    SurfaceIntersection {
        points, uv_a, uv_b,
        closed: all_valid,
        tangent_warning: false,
    }
}

/// Two radii this close count as EQUAL for the two-ellipse decomposition
/// (`plane.rs` SSOT — 1.5 μm, the same "these are the same surface" scale used
/// by every other coplanarity gate). Beyond it the curve is a genuine quartic.
const CYL_EQ_RADIUS_TOL: f64 = crate::plane::EPS_PLANE_OFFSET;
/// Two axes whose common perpendicular is shorter than this MEET (same SSOT).
const CYL_AXIS_MEET_TOL: f64 = crate::plane::EPS_PLANE_OFFSET;

/// A result that carries no curve, only "this pair needs Stage 2 subdivision /
/// is a tangent contact".
fn ssi_deferred() -> SurfaceIntersection {
    SurfaceIntersection { tangent_warning: true, ..SurfaceIntersection::default() }
}

/// The point where two non-parallel axes meet, or `None` if they are skew by
/// more than `tol`.
pub fn axes_meeting_point(
    origin_a: DVec3, dir_a: DVec3,
    origin_b: DVec3, dir_b: DVec3,
    tol: f64,
) -> Option<DVec3> {
    let cross = dir_a.cross(dir_b);
    let cross_len_sq = cross.length_squared();
    if cross_len_sq < 1e-18 {
        return None; // parallel — the caller handles that case
    }
    let delta = origin_b - origin_a;
    if delta.dot(cross).abs() / cross_len_sq.sqrt() > tol {
        return None; // skew
    }
    let s = delta.cross(dir_b).dot(cross) / cross_len_sq;
    Some(origin_a + dir_a * s)
}

/// `(angle, axial)` of `p` on the cylinder `(origin, axis)`. The angle is
/// measured in a basis derived from the axis alone — this module is given no
/// `ref_dir`, so it is self-consistent but NOT the surface's own `u`.
fn cylinder_uv(p: DVec3, origin: DVec3, axis: DVec3) -> (f64, f64) {
    let arb = if axis.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    let u_ref = axis.cross(arb).normalize_or_zero();
    let v_ref = axis.cross(u_ref).normalize_or_zero();
    let d = p - origin;
    let axial = d.dot(axis);
    let radial = d - axis * axial;
    let theta = radial.dot(v_ref).atan2(radial.dot(u_ref));
    let tau = 2.0 * std::f64::consts::PI;
    (((theta % tau) + tau) % tau, axial)
}

/// Cylinder-Cylinder intersection, as one entry per connected curve.
///
/// **Parallel axes** reduce to the 2D circle-circle problem in the plane
/// perpendicular to the axes:
/// - `d > r1 + r2`        → empty
/// - `d == r1 + r2` (±ε) → external tangent line (one branch, `tangent_warning`)
/// - `|r1-r2| < d < r1+r2` → two intersection lines parallel to axis
/// - `d == |r1-r2|`       → internal tangent line
/// - `d < |r1-r2|`        → one cylinder inside the other, empty
/// - `d ≈ 0 && r1 ≈ r2`  → coincident (one empty branch, `tangent_warning`)
///
/// **Crossing axes with EQUAL radii** → exactly TWO ELLIPSES, one on each
/// bisector plane. This is the crossing-bore case, and it is exact: with the
/// axes meeting at the origin, `a` along Z and `b` at angle θ, cylinder A is
/// `x²+y²=r²` and cylinder B is `|p|²−(p·b)²=r²`; substituting the first into
/// the second gives `z² = (p·b)²`, i.e. `z = ±(p·b)` — two PLANES, spanned by
/// `a±b`. Each plane cuts cylinder A in an ellipse ([`plane_cylinder`]).
/// Measured 2026-08-10 at 90° and 60°, r=40: every sample lies on BOTH
/// cylinders to 1.4e-14.
///
/// **Crossing axes with unequal radii, or skew axes** → one empty branch with
/// `tangent_warning=true`, meaning "use Stage 2 subdivision". (Measured for
/// r=40 vs r=25: the curve leaves either bisector plane by up to 29.194 mm — a
/// genuine quartic with no plane decomposition.)
#[allow(clippy::too_many_arguments)]
pub fn cylinder_cylinder_branches(
    axis_origin_a: DVec3, axis_dir_a: DVec3, radius_a: f64,
    axis_origin_b: DVec3, axis_dir_b: DVec3, radius_b: f64,
    n_samples: usize,
    extent: f64,
) -> Vec<SurfaceIntersection> {
    let aa = axis_dir_a.normalize_or_zero();
    let ab = axis_dir_b.normalize_or_zero();
    if aa.length_squared() < 0.5 || ab.length_squared() < 0.5
        || radius_a <= 0.0 || radius_b <= 0.0
    {
        return Vec::new();
    }

    // Check parallel: |aa · ab| ≈ 1
    let parallel = aa.dot(ab).abs() > 1.0 - 1e-9;
    if !parallel {
        // Equal radii + meeting axes → two ellipses on the bisector planes.
        if (radius_a - radius_b).abs() <= CYL_EQ_RADIUS_TOL {
            if let Some(meet) =
                axes_meeting_point(axis_origin_a, aa, axis_origin_b, ab, CYL_AXIS_MEET_TOL)
            {
                let mut out = Vec::with_capacity(2);
                for normal in [(aa + ab).normalize_or_zero(), (aa - ab).normalize_or_zero()] {
                    if normal.length_squared() < 0.5 {
                        continue; // anti-parallel halves — caught by `parallel` above
                    }
                    let mut si = plane_cylinder(meet, normal, meet, aa, radius_a, n_samples);
                    if si.points.is_empty() {
                        continue;
                    }
                    si.uv_a = si.points.iter().map(|&p| cylinder_uv(p, meet, aa)).collect();
                    si.uv_b = si.points.iter().map(|&p| cylinder_uv(p, meet, ab)).collect();
                    out.push(si);
                }
                if !out.is_empty() {
                    return out;
                }
            }
        }
        // Skew, or unequal radii → genuine quartic; defer to subdivision.
        return vec![ssi_deferred()];
    }

    // Project axis_origin_b onto plane through axis_origin_a perpendicular to aa.
    let delta = axis_origin_b - axis_origin_a;
    let perp_offset = delta - aa * delta.dot(aa);
    let d = perp_offset.length();

    // Coincident
    if d < 1e-9 && (radius_a - radius_b).abs() < 1e-9 {
        return vec![ssi_deferred()];
    }

    // Disjoint (external)
    if d > radius_a + radius_b + 1e-9 {
        return Vec::new();
    }
    // Disjoint (internal nesting, no intersection)
    if d + 1e-9 < (radius_a - radius_b).abs() {
        return Vec::new();
    }

    // Tangent (external or internal)
    let is_ext_tangent = (d - (radius_a + radius_b)).abs() < 1e-9;
    let is_int_tangent = (d - (radius_a - radius_b).abs()).abs() < 1e-9;

    // 2D circle-circle: solve in (u, v) basis where u = perp_offset/d
    // Center A at origin, center B at (d, 0).
    // Circle A: x² + y² = r1² ; Circle B: (x-d)² + y² = r2²
    // Subtract: -2dx + d² = r2² - r1² → x = (d² + r1² - r2²) / (2d)
    let u_basis = if d > 1e-12 {
        perp_offset / d
    } else {
        // d ≈ 0 fallback (shouldn't reach here unless tangent edge case)
        let arb = if aa.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        aa.cross(arb).normalize_or_zero()
    };
    let v_basis = aa.cross(u_basis).normalize_or_zero();

    let x = if d > 1e-12 {
        (d * d + radius_a * radius_a - radius_b * radius_b) / (2.0 * d)
    } else {
        0.0
    };
    let y_sq = (radius_a * radius_a - x * x).max(0.0);
    let y = y_sq.sqrt();

    let n_pts = n_samples.max(4);

    let signs: &[f64] = if is_ext_tangent || is_int_tangent {
        &[1.0]  // single tangent line
    } else {
        &[1.0, -1.0]  // two intersection lines
    };

    let mut out = Vec::with_capacity(signs.len());
    for &sign in signs {
        let line_origin_2d = u_basis * x + v_basis * (y * sign);
        let line_origin = axis_origin_a + line_origin_2d;
        let mut points = Vec::with_capacity(n_pts);
        let mut uv_a = Vec::with_capacity(n_pts);
        let mut uv_b = Vec::with_capacity(n_pts);
        for i in 0..n_pts {
            let t = -extent + 2.0 * extent * (i as f64) / ((n_pts - 1) as f64);
            let p = line_origin + aa * t;
            points.push(p);
            uv_a.push((0.0, t));
            uv_b.push((0.0, t));
        }
        out.push(SurfaceIntersection {
            points, uv_a, uv_b,
            closed: false,
            tangent_warning: is_ext_tangent || is_int_tangent,
        });
    }
    out
}

/// Cylinder-Cylinder intersection, flattened into a single result.
///
/// [`cylinder_cylinder_branches`] is the SSOT; this concatenates its branches
/// for callers that take one `SurfaceIntersection`. `closed` is only true when
/// there is exactly ONE branch and it closes — two ellipses concatenated are
/// two loops, not one, so a caller that needs the loop structure must use the
/// branch form.
#[allow(clippy::too_many_arguments)]
pub fn cylinder_cylinder(
    axis_origin_a: DVec3, axis_dir_a: DVec3, radius_a: f64,
    axis_origin_b: DVec3, axis_dir_b: DVec3, radius_b: f64,
    n_samples: usize,
    extent: f64,
) -> SurfaceIntersection {
    let branches = cylinder_cylinder_branches(
        axis_origin_a, axis_dir_a, radius_a,
        axis_origin_b, axis_dir_b, radius_b,
        n_samples, extent,
    );
    let mut out = SurfaceIntersection {
        closed: branches.len() == 1 && branches[0].closed,
        ..SurfaceIntersection::default()
    };
    for b in branches {
        out.tangent_warning |= b.tangent_warning;
        out.points.extend(b.points);
        out.uv_a.extend(b.uv_a);
        out.uv_b.extend(b.uv_b);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: DVec3, b: DVec3, eps: f64) -> bool {
        (a - b).length() < eps
    }

    // ─── Plane-Plane ─────────────────────────────────────────────────────

    #[test]
    fn plane_plane_perpendicular_yields_xy_axis_line() {
        // Z plane (z=0) ∩ X plane (x=0) → Y axis line.
        let result = plane_plane(
            DVec3::ZERO, DVec3::Z,
            DVec3::ZERO, DVec3::X,
            16, 10.0,
        );
        assert!(!result.is_empty());
        assert!(!result.closed);
        // All points should be on Y axis (x=0, z=0).
        for p in &result.points {
            assert!(p.x.abs() < 1e-9 && p.z.abs() < 1e-9,
                "point not on Y axis: {:?}", p);
        }
    }

    #[test]
    fn plane_plane_parallel_disjoint_empty() {
        let result = plane_plane(
            DVec3::ZERO, DVec3::Z,
            DVec3::new(0.0, 0.0, 5.0), DVec3::Z,
            16, 10.0,
        );
        assert!(result.is_empty());
        assert!(!result.tangent_warning);
    }

    #[test]
    fn plane_plane_coincident_warns() {
        let result = plane_plane(
            DVec3::ZERO, DVec3::Z,
            DVec3::ZERO, DVec3::Z,
            16, 10.0,
        );
        assert!(result.tangent_warning);
    }

    #[test]
    fn plane_plane_45deg_yields_diagonal_line() {
        // Z plane and a 45° tilted plane (normal = (1, 0, 1) / sqrt(2)).
        let n2 = DVec3::new(1.0, 0.0, 1.0).normalize();
        let result = plane_plane(
            DVec3::ZERO, DVec3::Z,
            DVec3::ZERO, n2,
            16, 10.0,
        );
        assert!(!result.is_empty());
        // Intersection direction = Z × (1,0,1)/√2 = (0, -√(1/2)... actually compute
        // properly: na = (0,0,1), nb = (1,0,1)/√2 → na × nb = (0·1 - 1·0, 1·1 - 0·1, 0·0 - 0·1)/√2
        //   = (0, 1, 0)/√2 → +Y axis.
        for p in &result.points {
            // All points lie on z=0 plane (Z plane): p.z = 0
            assert!(p.z.abs() < 1e-9);
            // And on tilted plane: x + z = 0 → x = 0
            assert!(p.x.abs() < 1e-9);
        }
    }

    // ─── Plane-Cylinder ────────────────────────────────────────────────────

    #[test]
    fn plane_cylinder_perpendicular_yields_circle() {
        // Cylinder axis = Y, radius 5. Plane = z=0, normal Z.
        // Wait: plane normal should be parallel to axis for "perpendicular" cut.
        // Cylinder along Y axis, plane normal = Y → plane is XZ horizontal.
        let result = plane_cylinder(
            DVec3::ZERO, DVec3::Y,                            // plane
            DVec3::ZERO, DVec3::Y, 5.0,                       // cylinder
            16,
        );
        assert!(!result.is_empty());
        assert!(result.closed, "perpendicular cut should be closed circle");
        // All points should be at distance 5 from axis (in XZ plane since axis=Y).
        for p in &result.points {
            let radial = DVec3::new(p.x, 0.0, p.z).length();
            assert!((radial - 5.0).abs() < 1e-6,
                "radial = {} ≠ 5", radial);
        }
    }

    #[test]
    fn plane_cylinder_perpendicular_offset_center() {
        // Cylinder axis = Y at (10, 0, 5), plane = y=3, normal Y.
        let result = plane_cylinder(
            DVec3::new(0.0, 3.0, 0.0), DVec3::Y,
            DVec3::new(10.0, 0.0, 5.0), DVec3::Y, 4.0,
            16,
        );
        assert!(!result.is_empty() && result.closed);
        // Circle center at (10, 3, 5), radius 4.
        for p in &result.points {
            let center = DVec3::new(10.0, 3.0, 5.0);
            let dist = (*p - center).length();
            assert!((dist - 4.0).abs() < 1e-6);
        }
    }

    #[test]
    fn plane_cylinder_45deg_yields_ellipse() {
        // Cylinder along Y axis, plane tilted 45° (normal = (0, 1, 1)/√2).
        // Should produce ellipse: minor = r, major = r/sin(45°) = r·√2.
        let normal = DVec3::new(0.0, 1.0, 1.0).normalize();
        let r = 5.0;
        let result = plane_cylinder(
            DVec3::ZERO, normal,
            DVec3::ZERO, DVec3::Y, r,
            32,
        );
        assert!(!result.is_empty() && result.closed);
        // For each point: project onto cylinder axis to get s, then
        // (point - axis·s) should have length = r (cylinder radial).
        for p in &result.points {
            let s = p.dot(DVec3::Y);
            let radial = *p - DVec3::Y * s;
            let radial_len = radial.length();
            assert!((radial_len - r).abs() < 1e-6,
                "radial = {} ≠ {} (cylinder radius)", radial_len, r);
        }
    }

    #[test]
    fn plane_cylinder_oblique_stays_on_the_cylinder_at_every_angle() {
        // 45° is the ONE oblique angle that cannot see a swapped sin/cos, and it
        // was the only one covered. Measured max deviation OFF the cylinder with
        // the old `r/sin`, r = 40: 15° → 109.28, 30° → 29.28, 45° → 0,
        // 60° → 16.91, 75° → 29.28.
        let r = 40.0;
        let axis = DVec3::Z;
        for deg in [15.0_f64, 30.0, 45.0, 60.0, 75.0] {
            let phi = deg.to_radians();
            let normal = DVec3::new(phi.sin(), 0.0, phi.cos()).normalize();
            let result = plane_cylinder(DVec3::ZERO, normal, DVec3::ZERO, axis, r, 64);
            assert!(!result.is_empty() && result.closed, "{deg}°");
            for &p in &result.points {
                let radial = (p - axis * p.dot(axis)).length();
                assert!((radial - r).abs() < 1e-9, "{deg}°: radial {radial} ≠ {r}");
                assert!(p.dot(normal).abs() < 1e-9, "{deg}°: not on the plane");
            }
            // Semi-major = r / cos(θ), semi-minor = r.
            let hi = result.points.iter().map(|p| p.length()).fold(0.0, f64::max);
            assert!((hi - r / phi.cos()).abs() < 1e-6, "{deg}°: major {hi} ≠ {}", r / phi.cos());
        }
    }

    #[test]
    fn plane_cylinder_distant_no_intersection() {
        // Plane parallel to axis but far from cylinder.
        let result = plane_cylinder(
            DVec3::new(20.0, 0.0, 0.0), DVec3::X,            // plane x=20
            DVec3::ZERO, DVec3::Y, 5.0,                       // cylinder around Y axis, r=5
            16,
        );
        assert!(result.is_empty(), "distant plane should not intersect");
    }

    #[test]
    fn plane_cylinder_parallel_to_axis_yields_two_lines() {
        // Plane parallel to axis (normal perpendicular to axis), cuts cylinder.
        // Axis = Y, plane normal = X (perpendicular to Y), passes through origin.
        // Should yield two parallel lines (at x=0, z=±5).
        let result = plane_cylinder(
            DVec3::ZERO, DVec3::X,                            // plane x=0
            DVec3::ZERO, DVec3::Y, 5.0,                       // cylinder
            8,
        );
        assert!(!result.is_empty());
        assert!(!result.closed);
        // All points should be on x=0.
        for p in &result.points {
            assert!(p.x.abs() < 1e-9);
        }
    }

    #[test]
    fn plane_cylinder_zero_radius_returns_empty() {
        let result = plane_cylinder(
            DVec3::ZERO, DVec3::Y,
            DVec3::ZERO, DVec3::Y, 0.0,
            8,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn plane_cylinder_degenerate_axis_returns_empty() {
        let result = plane_cylinder(
            DVec3::ZERO, DVec3::Y,
            DVec3::ZERO, DVec3::ZERO, 5.0,                    // zero axis_dir
            8,
        );
        assert!(result.is_empty());
    }

    // ─── Plane-Sphere ──────────────────────────────────────────────────────

    #[test]
    fn plane_sphere_through_center_yields_great_circle() {
        // Plane z=0, sphere at origin radius 5 → great circle r=5 in xy.
        let r = 5.0;
        let result = plane_sphere(
            DVec3::ZERO, DVec3::Z,
            DVec3::ZERO, r,
            32,
        );
        assert!(!result.is_empty() && result.closed);
        for p in &result.points {
            assert!(p.z.abs() < 1e-9, "point not on plane: {:?}", p);
            let radial = (p.x * p.x + p.y * p.y).sqrt();
            assert!((radial - r).abs() < 1e-6,
                "radial {} ≠ {}", radial, r);
        }
    }

    #[test]
    fn plane_sphere_offset_yields_smaller_circle() {
        // Plane z=3, sphere at origin radius 5 → circle radius √(25-9)=4 at z=3.
        let result = plane_sphere(
            DVec3::new(0.0, 0.0, 3.0), DVec3::Z,
            DVec3::ZERO, 5.0,
            32,
        );
        assert!(!result.is_empty() && result.closed);
        for p in &result.points {
            assert!((p.z - 3.0).abs() < 1e-9);
            let radial = (p.x * p.x + p.y * p.y).sqrt();
            assert!((radial - 4.0).abs() < 1e-6,
                "radial {} ≠ 4", radial);
        }
    }

    #[test]
    fn plane_sphere_tangent_yields_single_point() {
        // Plane z=5 tangent to sphere radius 5 at origin → single point (0,0,5).
        let result = plane_sphere(
            DVec3::new(0.0, 0.0, 5.0), DVec3::Z,
            DVec3::ZERO, 5.0,
            16,
        );
        assert_eq!(result.len(), 1);
        assert!(result.tangent_warning);
        assert!(approx_eq(result.points[0], DVec3::new(0.0, 0.0, 5.0), 1e-9));
    }

    #[test]
    fn plane_sphere_distant_no_intersection() {
        let result = plane_sphere(
            DVec3::new(0.0, 0.0, 10.0), DVec3::Z,
            DVec3::ZERO, 5.0,
            16,
        );
        assert!(result.is_empty());
        assert!(!result.tangent_warning);
    }

    #[test]
    fn plane_sphere_zero_radius_returns_empty() {
        let result = plane_sphere(
            DVec3::ZERO, DVec3::Z,
            DVec3::ZERO, 0.0,
            16,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn plane_sphere_oblique_plane_circle_in_plane() {
        // Tilted plane (normal = (1,1,1)/√3) through origin, sphere at origin r=4
        // → great circle r=4 lying in that tilted plane.
        let n = DVec3::new(1.0, 1.0, 1.0).normalize();
        let result = plane_sphere(
            DVec3::ZERO, n,
            DVec3::ZERO, 4.0,
            32,
        );
        assert!(!result.is_empty() && result.closed);
        for p in &result.points {
            // Distance from origin = 4
            assert!((p.length() - 4.0).abs() < 1e-6);
            // Lies on plane: p · n ≈ 0
            assert!(p.dot(n).abs() < 1e-9);
        }
    }

    // ─── Plane-Cone ────────────────────────────────────────────────────────

    #[test]
    fn plane_cone_perpendicular_yields_circle() {
        // Cone apex at origin, axis +Y, half-angle 30°.
        // Plane y=4 perpendicular to axis → circle at radius 4·tan(30°).
        let alpha = 30f64.to_radians();
        let result = plane_cone(
            DVec3::new(0.0, 4.0, 0.0), DVec3::Y,
            DVec3::ZERO, DVec3::Y, alpha,
            32,
        );
        assert!(!result.is_empty() && result.closed);
        let expected_r = 4.0 * alpha.tan();
        for p in &result.points {
            assert!((p.y - 4.0).abs() < 1e-6);
            let radial = (p.x * p.x + p.z * p.z).sqrt();
            assert!((radial - expected_r).abs() < 1e-6,
                "radial {} ≠ {}", radial, expected_r);
        }
    }

    #[test]
    fn plane_cone_through_apex_yields_apex_point() {
        let alpha = 30f64.to_radians();
        let result = plane_cone(
            DVec3::ZERO, DVec3::Z,        // plane through apex
            DVec3::ZERO, DVec3::Y, alpha,
            16,
        );
        assert_eq!(result.len(), 1);
        assert!(result.tangent_warning);
        assert!(approx_eq(result.points[0], DVec3::ZERO, 1e-9));
    }

    #[test]
    fn plane_cone_oblique_below_slant_yields_ellipse() {
        // Slight tilt — should produce ellipse (closed loop, all samples valid).
        let alpha = 20f64.to_radians();
        let normal = DVec3::new(0.0, 1.0, 0.2).normalize();  // 11° tilt
        let result = plane_cone(
            DVec3::new(0.0, 5.0, 0.0), normal,
            DVec3::ZERO, DVec3::Y, alpha,
            32,
        );
        assert!(!result.is_empty(), "should intersect");
        assert!(result.closed, "ellipse must be closed");
        // All points lie on plane: (p - plane_origin) · normal ≈ 0
        for p in &result.points {
            let pp = *p - DVec3::new(0.0, 5.0, 0.0);
            assert!(pp.dot(normal).abs() < 1e-6);
        }
    }

    #[test]
    fn plane_cone_steep_plane_yields_open_curve() {
        // Plane tilted past slant angle → hyperbola branch (open curve).
        let alpha = 20f64.to_radians();
        // Normal pointing mostly along +Z (perpendicular to axis +Y) → very steep.
        let normal = DVec3::new(0.0, 0.1, 1.0).normalize();
        let result = plane_cone(
            DVec3::new(0.0, 0.0, 3.0), normal,
            DVec3::ZERO, DVec3::Y, alpha,
            64,
        );
        // Some samples should be valid (hits cone), others skipped.
        assert!(!result.is_empty());
        assert!(!result.closed, "hyperbola is open");
    }

    #[test]
    fn plane_cone_invalid_half_angle_returns_empty() {
        // half-angle = 0 (degenerate to line)
        let result = plane_cone(
            DVec3::new(0.0, 4.0, 0.0), DVec3::Y,
            DVec3::ZERO, DVec3::Y, 0.0,
            16,
        );
        assert!(result.is_empty());
        // half-angle ≥ π/2 (degenerate to plane)
        let result = plane_cone(
            DVec3::new(0.0, 4.0, 0.0), DVec3::Y,
            DVec3::ZERO, DVec3::Y, std::f64::consts::FRAC_PI_2,
            16,
        );
        assert!(result.is_empty());
    }

    // ─── Cylinder-Cylinder ─────────────────────────────────────────────────

    #[test]
    fn cyl_cyl_parallel_offset_yields_two_lines() {
        // Both axes along Y, offset 4 in X, both radius 3 → two intersection
        // lines (since d=4, r1+r2=6, so 4 < 6).
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Y, 3.0,
            DVec3::new(4.0, 0.0, 0.0), DVec3::Y, 3.0,
            8, 10.0,
        );
        assert!(!result.is_empty());
        // x = (16 + 9 - 9) / 8 = 2; y = √(9-4) = √5
        // So lines at (2, t, ±√5).
        let sqrt5 = 5f64.sqrt();
        let mut found_pos = false;
        let mut found_neg = false;
        for p in &result.points {
            assert!((p.x - 2.0).abs() < 1e-6);
            if (p.z - sqrt5).abs() < 1e-6 { found_pos = true; }
            if (p.z + sqrt5).abs() < 1e-6 { found_neg = true; }
        }
        assert!(found_pos && found_neg, "should have both ±√5 lines");
    }

    #[test]
    fn cyl_cyl_parallel_external_tangent() {
        // d = r1 + r2 exactly → single tangent line.
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Y, 3.0,
            DVec3::new(7.0, 0.0, 0.0), DVec3::Y, 4.0,
            8, 10.0,
        );
        assert!(!result.is_empty());
        assert!(result.tangent_warning);
    }

    #[test]
    fn cyl_cyl_parallel_too_far_empty() {
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Y, 3.0,
            DVec3::new(20.0, 0.0, 0.0), DVec3::Y, 4.0,
            8, 10.0,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn cyl_cyl_parallel_nested_empty() {
        // Smaller cylinder fully inside larger, no surface intersection.
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Y, 10.0,
            DVec3::new(2.0, 0.0, 0.0), DVec3::Y, 3.0,  // d=2, |r1-r2|=7 → nested
            8, 10.0,
        );
        assert!(result.is_empty());
    }

    #[test]
    fn cyl_cyl_parallel_coincident_warns() {
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Y, 5.0,
            DVec3::ZERO, DVec3::Y, 5.0,
            8, 10.0,
        );
        assert!(result.is_empty());
        assert!(result.tangent_warning);
    }

    /// Distance from `p` to the cylinder `(origin, axis, radius)` surface.
    fn off_cylinder(p: DVec3, origin: DVec3, axis: DVec3, radius: f64) -> f64 {
        let d = p - origin;
        ((d - axis * d.dot(axis)).length() - radius).abs()
    }

    #[test]
    fn cyl_cyl_equal_radius_crossing_yields_two_ellipses() {
        // The crossing-bore case: equal radii, axes meeting at right angles.
        let r = 40.0;
        let branches = cylinder_cylinder_branches(
            DVec3::ZERO, DVec3::Z, r,
            DVec3::ZERO, DVec3::X, r,
            64, 10.0,
        );
        assert_eq!(branches.len(), 2, "two bisector planes → two curves");
        for b in &branches {
            assert!(b.closed, "each branch is a closed ellipse");
            assert!(!b.tangent_warning);
            assert_eq!(b.points.len(), 65);
            assert_eq!(b.uv_a.len(), 65);
            assert_eq!(b.uv_b.len(), 65);
            for &p in &b.points {
                assert!(off_cylinder(p, DVec3::ZERO, DVec3::Z, r) < 1e-9, "off cyl A: {p:?}");
                assert!(off_cylinder(p, DVec3::ZERO, DVec3::X, r) < 1e-9, "off cyl B: {p:?}");
            }
            // |p| runs r .. r√2 (the ellipse's minor and major ends).
            let lo = b.points.iter().map(|p| p.length()).fold(f64::MAX, f64::min);
            let hi = b.points.iter().map(|p| p.length()).fold(0.0, f64::max);
            assert!((lo - r).abs() < 1e-6, "minor end {lo} ≠ {r}");
            assert!((hi - r * 2f64.sqrt()).abs() < 1e-6, "major end {hi} ≠ r√2");
        }
        // The two branches lie on z = +x and z = −x respectively.
        let on_plane = |b: &SurfaceIntersection, n: DVec3| {
            b.points.iter().all(|p| p.dot(n).abs() < 1e-9)
        };
        let np = DVec3::new(1.0, 0.0, 1.0).normalize();
        let nm = DVec3::new(1.0, 0.0, -1.0).normalize();
        assert!(
            (on_plane(&branches[0], np) && on_plane(&branches[1], nm))
                || (on_plane(&branches[0], nm) && on_plane(&branches[1], np)),
            "branches should be the two bisector planes"
        );
    }

    /// Distance from `p` to the polyline `pts`.
    fn dist_to_polyline(p: DVec3, pts: &[DVec3]) -> f64 {
        pts.windows(2).fold(f64::MAX, |best, w| {
            let (a, b) = (w[0], w[1]);
            let ab = b - a;
            let t = if ab.length_squared() > 0.0 {
                ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            best.min((p - (a + ab * t)).length())
        })
    }

    #[test]
    fn cyl_cyl_equal_radius_crossing_covers_the_whole_intersection() {
        // Completeness, not just self-consistency. Sweeping a GRID and filtering
        // by "also on cylinder B" does not work here: at the two pinch points
        // (0, ±r, 0) the surfaces meet at a shallow angle, so a 1e-3 surface
        // tolerance admits points 0.15 mm away. So walk the shared curve
        // exactly — for every angle around A, `z = ±r·cos θ` — verify each such
        // point really is on BOTH cylinders (independent of the closed form),
        // then require the returned branches to carry it.
        let r = 40.0;
        let branches = cylinder_cylinder_branches(
            DVec3::ZERO, DVec3::Z, r,
            DVec3::ZERO, DVec3::X, r,
            64, 10.0,
        );
        assert_eq!(branches.len(), 2);
        for i in 0..720 {
            let theta = std::f64::consts::TAU * (i as f64) / 720.0;
            for sign in [1.0_f64, -1.0] {
                let p = DVec3::new(r * theta.cos(), r * theta.sin(), sign * r * theta.cos());
                assert!(off_cylinder(p, DVec3::ZERO, DVec3::Z, r) < 1e-12);
                assert!(off_cylinder(p, DVec3::ZERO, DVec3::X, r) < 1e-12);
                // Within the 64-sample polyline's sag (≈0.07 mm on an r√2 ellipse).
                let covered = branches
                    .iter()
                    .any(|b| dist_to_polyline(p, &b.points) < 0.1);
                assert!(covered, "shared point not carried by either branch: {p:?}");
            }
        }
    }

    #[test]
    fn cyl_cyl_equal_radius_crossing_at_60_degrees() {
        // Not 90° — this is the case that stayed broken while `plane_cylinder`
        // used r/sin instead of r/cos (sin = cos only at 45°, and the bisector
        // planes of a 90° crossing are exactly the 45° ones).
        let r = 40.0;
        let theta = 60f64.to_radians();
        let axis_b = DVec3::new(theta.sin(), 0.0, theta.cos()).normalize();
        let branches = cylinder_cylinder_branches(
            DVec3::ZERO, DVec3::Z, r,
            DVec3::ZERO, axis_b, r,
            64, 10.0,
        );
        assert_eq!(branches.len(), 2);
        for b in &branches {
            assert!(b.closed);
            for &p in &b.points {
                assert!(off_cylinder(p, DVec3::ZERO, DVec3::Z, r) < 1e-9, "off cyl A: {p:?}");
                assert!(off_cylinder(p, DVec3::ZERO, axis_b, r) < 1e-9, "off cyl B: {p:?}");
            }
        }
    }

    #[test]
    fn cyl_cyl_crossing_axes_that_meet_off_the_origin() {
        // The meeting point is found, not assumed to be the origin.
        let r = 12.0;
        let meet = DVec3::new(7.0, -3.0, 5.0);
        let branches = cylinder_cylinder_branches(
            meet - DVec3::Z * 100.0, DVec3::Z, r,
            meet - DVec3::X * 40.0, DVec3::X, r,
            32, 10.0,
        );
        assert_eq!(branches.len(), 2);
        for b in &branches {
            for &p in &b.points {
                assert!(off_cylinder(p, meet, DVec3::Z, r) < 1e-9);
                assert!(off_cylinder(p, meet, DVec3::X, r) < 1e-9);
            }
        }
    }

    #[test]
    fn cyl_cyl_unequal_radius_crossing_defers_to_subdivision() {
        // Unequal radii → a genuine quartic, no plane decomposition.
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Z, 40.0,
            DVec3::ZERO, DVec3::X, 25.0,
            64, 10.0,
        );
        assert!(result.is_empty());
        assert!(result.tangent_warning);
    }

    #[test]
    fn cyl_cyl_skew_perpendicular_axes_defer_to_subdivision() {
        // Perpendicular but NOT meeting (offset in Y) → not two ellipses.
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Z, 40.0,
            DVec3::new(0.0, 70.0, 0.0), DVec3::X, 40.0,
            64, 10.0,
        );
        assert!(result.is_empty());
        assert!(result.tangent_warning);
    }

    #[test]
    fn cyl_cyl_flattened_crossing_reports_two_loops_not_one() {
        // The single-result form concatenates both ellipses; it must NOT claim
        // to be one closed loop.
        let result = cylinder_cylinder(
            DVec3::ZERO, DVec3::Z, 40.0,
            DVec3::ZERO, DVec3::X, 40.0,
            64, 10.0,
        );
        assert_eq!(result.points.len(), 130);
        assert!(!result.closed);
        assert!(!result.tangent_warning);
    }

    // ─── SurfaceIntersection helpers ─────────────────────────────────────

    #[test]
    fn intersection_default_is_empty() {
        let r = SurfaceIntersection::default();
        assert!(r.is_empty());
        assert_eq!(r.len(), 0);
    }
}
