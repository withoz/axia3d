//! ADR-059 Phase N Step 1 — Curve & Surface Synthesizer.
//!
//! Provides default `AnalyticCurve` / `AnalyticSurface` synthesis from
//! topology-only inputs (vertex IDs, vertex positions). Used by the
//! Mesh during Phase N migration as the fallback when no explicit
//! curve/surface is provided.
//!
//! ## Lock-in (ADR-059 §B — Synthesizer Default)
//!
//! - `synthesize_line_curve(v_small, v_large)` → `AnalyticCurve::Line`
//!   (mesh-relative — vertex moves auto-propagate)
//! - `synthesize_plane_surface(outer_verts)` → `AnalyticSurface::Plane`
//!   (Newell normal + centroid origin + orthogonal basis_u)
//!
//! ## Lock-in (ADR-059 §C — Size budget)
//!
//! - `mem::size_of::<AnalyticCurve>()` ≤ 96 bytes
//! - `mem::size_of::<AnalyticSurface>()` ≤ 100 bytes (Box NURBS variants)

use glam::DVec3;

use super::AnalyticCurve;
use crate::entities::id::VertId;
use crate::surfaces::AnalyticSurface;

// ────────────────────────────────────────────────────────────────────
// Curve synthesizer
// ────────────────────────────────────────────────────────────────────

/// Default `AnalyticCurve` for an edge with vertex pair `(v_small, v_large)`.
///
/// Returns `AnalyticCurve::Line { start: v_small, end: v_large }` —
/// mesh-relative variant where vertex moves automatically propagate
/// (Line.evaluate consults Mesh state).
///
/// Per ADR-059 §B lock-in: this is the canonical default. All
/// `Mesh::add_edge` paths with no explicit curve must call this
/// synthesizer (Phase N Step 1 → 3 incremental migration).
#[inline]
pub fn synthesize_line_curve(v_small: VertId, v_large: VertId) -> AnalyticCurve {
    AnalyticCurve::Line { start: v_small, end: v_large }
}

// ────────────────────────────────────────────────────────────────────
// Surface synthesizer
// ────────────────────────────────────────────────────────────────────

/// Default `AnalyticSurface` for a face with given outer-loop vertex
/// positions. Returns a best-fit `AnalyticSurface::Plane`:
///   - normal = Newell normal of the loop (handles non-convex)
///   - origin = centroid of vertices
///   - basis_u = arbitrary orthogonal in-plane axis
///
/// Per ADR-059 §B lock-in: canonical default for `add_face`.
///
/// **Non-planar case (deferred)**: if Newell normal is degenerate
/// (loop coplanar with axis), returns Plane with normal = Z and
/// warns. Phase K Loft fitting is the proper remedy (later phase).
pub fn synthesize_plane_surface(outer_verts: &[DVec3]) -> AnalyticSurface {
    if outer_verts.is_empty() {
        return default_plane_z();
    }

    let centroid = outer_verts.iter().copied().sum::<DVec3>()
        / (outer_verts.len() as f64);

    let normal = newell_normal(outer_verts);
    let normal = if normal.length_squared() < 1e-20 {
        // Degenerate (zero-area or collinear loop) — fall back to +Z
        DVec3::Z
    } else {
        normal.normalize()
    };

    let basis_u = orthogonal_basis(normal);

    AnalyticSurface::Plane {
        origin: centroid,
        normal,
        basis_u,
        u_range: (-1e6, 1e6),
        v_range: (-1e6, 1e6),
    }
}

#[inline]
fn default_plane_z() -> AnalyticSurface {
    AnalyticSurface::Plane {
        origin: DVec3::ZERO,
        normal: DVec3::Z,
        basis_u: DVec3::X,
        u_range: (-1e6, 1e6),
        v_range: (-1e6, 1e6),
    }
}

/// Newell's method — robust normal for arbitrary planar polygons
/// (works for non-convex loops). Returns un-normalized vector with
/// magnitude proportional to area.
fn newell_normal(verts: &[DVec3]) -> DVec3 {
    let n = verts.len();
    if n < 3 { return DVec3::ZERO; }
    let mut normal = DVec3::ZERO;
    for i in 0..n {
        let curr = verts[i];
        let next = verts[(i + 1) % n];
        normal.x += (curr.y - next.y) * (curr.z + next.z);
        normal.y += (curr.z - next.z) * (curr.x + next.x);
        normal.z += (curr.x - next.x) * (curr.y + next.y);
    }
    normal
}

/// Compute an arbitrary unit vector orthogonal to `normal`.
/// Picks the world axis least aligned with `normal` and projects out
/// the `normal` component.
fn orthogonal_basis(normal: DVec3) -> DVec3 {
    // Pick axis with smallest |normal.component|
    let abs = normal.abs();
    let alt = if abs.x <= abs.y && abs.x <= abs.z { DVec3::X }
              else if abs.y <= abs.z { DVec3::Y }
              else { DVec3::Z };
    let proj = alt - normal * alt.dot(normal);
    proj.normalize_or_zero()
}

// ────────────────────────────────────────────────────────────────────
// Step 2 — Parameter inversion + split_at
// ────────────────────────────────────────────────────────────────────

use anyhow::{bail, Result};

use crate::mesh::Mesh;

/// Per ADR-059 §A1.3 lock-in — explicit failure modes for parameter
/// inversion (Bezier/BSpline/NURBS Newton iteration deferred).
#[derive(Clone, Debug, PartialEq)]
pub enum SplitParameterError {
    /// Newton iteration failed to converge within max_iter.
    NewtonDiverged { iterations: usize },
    /// 3D point produces multiple valid parameter values (e.g., on a
    /// circle's diameter).
    MultipleRoots { count: usize },
    /// 3D point is too far from any curve point (> tol).
    PointOffCurve { distance: f64 },
    /// Curve variant deferred to Phase I knot insertion (Bezier /
    /// BSpline / NURBS).
    DeferredToPhaseI,
}

impl AnalyticCurve {
    /// Step 2 — Find parameter `t` such that `evaluate(t) ≈ p`.
    ///
    /// MVP scope (ADR-059 §A1.3 lock-in):
    ///   - Line: closed-form projection
    ///   - Arc:  closed-form angle (atan2 + range mapping)
    ///   - Circle: closed-form angle
    ///   - Bezier / BSpline / NURBS: Err(DeferredToPhaseI)
    ///
    /// `mesh` is required for Line variant (mesh-relative endpoints).
    pub fn parameter_at_3d_point(
        &self, p: DVec3, mesh: &Mesh,
    ) -> std::result::Result<f64, SplitParameterError> {
        const POINT_OFF_CURVE_TOL: f64 = 1.5e-4; // LOCKED #5 (ADR-147 Scenario B1: 1.5μm → 0.15μm)
        match self {
            AnalyticCurve::Line { start, end } => {
                let pa = mesh.vertex_pos(*start)
                    .map_err(|_| SplitParameterError::PointOffCurve { distance: f64::INFINITY })?;
                let pb = mesh.vertex_pos(*end)
                    .map_err(|_| SplitParameterError::PointOffCurve { distance: f64::INFINITY })?;
                let dir = pb - pa;
                let len_sq = dir.length_squared();
                if len_sq < 1e-30 {
                    return Err(SplitParameterError::PointOffCurve { distance: 0.0 });
                }
                let t = (p - pa).dot(dir) / len_sq;
                // Verify p is actually on the line
                let p_on = pa + dir * t;
                let drift = (p - p_on).length();
                if drift > POINT_OFF_CURVE_TOL {
                    return Err(SplitParameterError::PointOffCurve { distance: drift });
                }
                Ok(t.clamp(0.0, 1.0))
            }
            AnalyticCurve::Circle { center, radius, normal, basis_u } => {
                let basis_v = normal.cross(*basis_u).normalize_or_zero();
                let local = p - *center;
                let x = local.dot(*basis_u);
                let y = local.dot(basis_v);
                let r_actual = (x * x + y * y).sqrt();
                if (r_actual - radius).abs() > POINT_OFF_CURVE_TOL {
                    return Err(SplitParameterError::PointOffCurve {
                        distance: (r_actual - radius).abs(),
                    });
                }
                let mut angle = y.atan2(x);
                if angle < 0.0 { angle += std::f64::consts::TAU; }
                Ok(angle)
            }
            AnalyticCurve::Arc {
                center, radius, normal, basis_u, start_angle, end_angle,
            } => {
                let basis_v = normal.cross(*basis_u).normalize_or_zero();
                let local = p - *center;
                let x = local.dot(*basis_u);
                let y = local.dot(basis_v);
                let r_actual = (x * x + y * y).sqrt();
                if (r_actual - radius).abs() > POINT_OFF_CURVE_TOL {
                    return Err(SplitParameterError::PointOffCurve {
                        distance: (r_actual - radius).abs(),
                    });
                }
                // Map atan2 to [start_angle, end_angle] range
                let mut angle = y.atan2(x);
                let two_pi = std::f64::consts::TAU;
                while angle < *start_angle - 1e-9 { angle += two_pi; }
                while angle > *end_angle + 1e-9 { angle -= two_pi; }
                if angle < *start_angle - 1e-6 || angle > *end_angle + 1e-6 {
                    return Err(SplitParameterError::PointOffCurve {
                        distance: 0.0, // approximation
                    });
                }
                Ok(angle.clamp(*start_angle, *end_angle))
            }
            AnalyticCurve::Bezier { .. }
            | AnalyticCurve::BSpline { .. }
            | AnalyticCurve::NURBS { .. } => {
                self.freeform_param_at_point(p, mesh)
            }
        }
    }

    /// **ADR-186 step ① (2026-06-16)** — point→parameter inversion for free-form
    /// curves (Bezier / BSpline / NURBS) via coarse subdivide + Gauss-Newton on
    /// `g(t) = |C(t) - p|²`. Returns a parameter in `parameter_range()` (the same
    /// convention `evaluate`/`split_at` use), so `split_at(t)` is automatically
    /// consistent — a free-form edge split at a CCI crossing now yields sub-curves
    /// (`Bezier`/`BSpline`/`NURBS`) instead of plain `Line`s, preserving curve
    /// metadata across the split (was `DeferredToPhaseI`).
    fn freeform_param_at_point(
        &self, p: DVec3, mesh: &Mesh,
    ) -> std::result::Result<f64, SplitParameterError> {
        use crate::curves::CurveOps;
        const SUBDIVIDE_SAMPLES: usize = 64;
        const NEWTON_MAX_ITER: usize = 32;
        const NEWTON_PARAM_TOL: f64 = 1e-12;
        const POINT_OFF_CURVE_TOL: f64 = 1.5e-4; // LOCKED #5 (ADR-147 Scenario B1)

        let (t_min, t_max) = self.parameter_range();
        if !(t_min.is_finite() && t_max.is_finite()) || t_max <= t_min {
            return Err(SplitParameterError::PointOffCurve { distance: f64::INFINITY });
        }

        // Stage 1 — coarse subdivide: nearest sample to p.
        let mut best_t = t_min;
        let mut best_d2 = f64::INFINITY;
        for k in 0..=SUBDIVIDE_SAMPLES {
            let frac = k as f64 / SUBDIVIDE_SAMPLES as f64;
            let t = t_min + (t_max - t_min) * frac;
            let c = match self.evaluate(t, mesh) {
                Ok(c) if c.is_finite() => c,
                _ => return Err(SplitParameterError::PointOffCurve {
                    distance: f64::INFINITY,
                }),
            };
            let d2 = (c - p).length_squared();
            if d2 < best_d2 {
                best_d2 = d2;
                best_t = t;
            }
        }

        // Stage 2 — Gauss-Newton on g(t) = |C(t) - p|²:
        //   g'(t)  = 2 (C(t)-p)·C'(t)
        //   g''(t) ≈ 2 C'(t)·C'(t)   (Gauss-Newton; drops the curvature term —
        //   the subdivide step lands close to the local min so this suffices).
        let mut t = best_t;
        for _ in 0..NEWTON_MAX_ITER {
            let c = self.evaluate(t, mesh)
                .map_err(|_| SplitParameterError::NewtonDiverged { iterations: 0 })?;
            let dc = self.derivative(t, mesh)
                .map_err(|_| SplitParameterError::NewtonDiverged { iterations: 0 })?;
            if !c.is_finite() || !dc.is_finite() {
                return Err(SplitParameterError::NewtonDiverged { iterations: 0 });
            }
            let r = c - p;
            let grad = 2.0 * r.dot(dc);
            let hess = 2.0 * dc.dot(dc);
            if hess < 1e-24 {
                break; // tangent vanishes (cusp) — accept current t
            }
            let step = -grad / hess;
            let new_t = (t + step).clamp(t_min, t_max);
            let moved = (new_t - t).abs();
            t = new_t;
            if moved < NEWTON_PARAM_TOL {
                break;
            }
        }

        // Stage 3 — verify the converged point is actually on the curve.
        let c = self.evaluate(t, mesh)
            .map_err(|_| SplitParameterError::PointOffCurve { distance: f64::INFINITY })?;
        let dist = (c - p).length();
        if dist > POINT_OFF_CURVE_TOL {
            return Err(SplitParameterError::PointOffCurve { distance: dist });
        }
        Ok(t)
    }
}

impl AnalyticCurve {
    /// Split this curve at parameter `t ∈ parameter_range`, returning
    /// two curves `(left, right)` such that `left` covers `[t_min, t]`
    /// and `right` covers `[t, t_max]`.
    ///
    /// **Scope** (Phase N Step 2 + ADR-186 A3 / Option B freeform):
    ///   - Line:   trivial — endpoint refers to caller-provided new vert
    ///   - Arc:    parameter clamping (start_angle/end_angle adjusted)
    ///   - Circle: Err — caller should split into two Arcs explicitly
    ///   - Bezier: de Casteljau subdivision (each half re-parameterised [0,1])
    ///   - BSpline / NURBS: knot insertion to multiplicity = degree +
    ///     control-point partition (each half clamped, param **preserved**)
    ///
    /// The `mid_vert` parameter is the new VertId at the split point
    /// (created by `Mesh::split_edge`). Used only by `Line` variant
    /// to populate the new endpoint reference; ignored by other variants.
    pub fn split_at(&self, t: f64, mid_vert: VertId) -> Result<(Self, Self)> {
        match self {
            AnalyticCurve::Line { start, end } => {
                // Line is mesh-relative — split into two Lines sharing mid_vert
                Ok((
                    AnalyticCurve::Line { start: *start, end: mid_vert },
                    AnalyticCurve::Line { start: mid_vert, end: *end },
                ))
            }
            AnalyticCurve::Arc {
                center, radius, normal, basis_u, start_angle, end_angle,
            } => {
                // Arc parameter t is the angle in [start_angle, end_angle]
                if t < *start_angle - 1e-12 || t > *end_angle + 1e-12 {
                    bail!("split_at: t={} outside arc range [{}, {}]",
                        t, start_angle, end_angle);
                }
                Ok((
                    AnalyticCurve::Arc {
                        center: *center, radius: *radius,
                        normal: *normal, basis_u: *basis_u,
                        start_angle: *start_angle,
                        end_angle: t,
                    },
                    AnalyticCurve::Arc {
                        center: *center, radius: *radius,
                        normal: *normal, basis_u: *basis_u,
                        start_angle: t,
                        end_angle: *end_angle,
                    },
                ))
            }
            AnalyticCurve::Circle { .. } => {
                bail!("split_at: Circle must be promoted to Arc before splitting \
                       (Phase N Step 2: caller responsibility)");
            }
            AnalyticCurve::Bezier { control_pts } => {
                // de Casteljau subdivision (existing tested helper). Each half
                // is re-parameterised to [0, 1]: left covers [0, t], right [t, 1].
                if control_pts.len() < 2 {
                    bail!("split_at: Bezier needs ≥ 2 control points");
                }
                let (l, r) = crate::curves::bezier::subdivide(control_pts, t);
                Ok((
                    AnalyticCurve::Bezier { control_pts: l },
                    AnalyticCurve::Bezier { control_pts: r },
                ))
            }
            AnalyticCurve::BSpline { control_pts, knots, degree } => {
                let ((lc, lk), (rc, rk)) = crate::curves::knot::split_bspline(
                    control_pts, knots, *degree as usize, t,
                )?;
                Ok((
                    AnalyticCurve::BSpline { control_pts: lc, knots: lk, degree: *degree },
                    AnalyticCurve::BSpline { control_pts: rc, knots: rk, degree: *degree },
                ))
            }
            AnalyticCurve::NURBS { control_pts, weights, knots, degree } => {
                let ((lc, lw, lk), (rc, rw, rk)) = crate::curves::knot::split_nurbs(
                    control_pts, weights, knots, *degree as usize, t,
                )?;
                Ok((
                    AnalyticCurve::NURBS {
                        control_pts: lc, weights: lw, knots: lk, degree: *degree,
                    },
                    AnalyticCurve::NURBS {
                        control_pts: rc, weights: rw, knots: rk, degree: *degree,
                    },
                ))
            }
        }
    }

    /// **ADR-201 (β-1)** — extract the shape-preserving sub-curve over parameter
    /// `[t0, t1]` (t1 > t0) via two `split_at`. Bezier: `right` 가 [0,1] 로
    /// 재매개화되므로 t1 → `(t1 - t0) / (1 - t0)`. BSpline/NURBS/Arc: param 보존.
    /// Line 은 미지원 (mesh-relative vertex). 결과는 그 구간만의 reparametrized
    /// 곡선 — bounded sub-range freeform draw 의 세그먼트별 곡선에 사용.
    pub fn subcurve(&self, t0: f64, t1: f64) -> Result<AnalyticCurve> {
        let dummy = VertId::default();
        let (_, right) = self.split_at(t0, dummy)?; // [t0, end]
        let t1_in_right = match self {
            AnalyticCurve::Bezier { .. } => {
                if (1.0 - t0).abs() < 1e-12 {
                    bail!("subcurve: degenerate Bezier range (t0≈1)");
                }
                (t1 - t0) / (1.0 - t0)
            }
            AnalyticCurve::BSpline { .. }
            | AnalyticCurve::NURBS { .. }
            | AnalyticCurve::Arc { .. } => t1, // param/angle preserved by split_at
            _ => bail!("subcurve: unsupported curve type (Line is mesh-relative)"),
        };
        let (mid, _) = right.split_at(t1_in_right, dummy)?; // [t0, t1]
        Ok(mid)
    }

    /// **CAD trim (2026-06-15)** — promote a full `Circle` into N open `Arc`s at
    /// the given crossing angles (arc-frame radians). `split_at` rejects a
    /// `Circle` directly ("Circle must be promoted to Arc before splitting");
    /// this implements that promotion — the analytic core of a CAD-style trim
    /// (a line crossing a circle → 2 Arcs). Mirrors the
    /// `split_curve(InputCurve::Circle)` arrange logic (normalise + sort + dedup
    /// + consecutive pairs, last wraps `+2π`) but at the DCEL-agnostic
    /// `AnalyticCurve` level, so a kernel trim op can apply it to a single
    /// existing self-loop Circle edge without a full-region re-derive.
    ///
    /// Returns `Err` for a non-Circle, or for `< 2` distinct crossings (no
    /// closed sub-region to split — the caller keeps the whole Circle). The
    /// resulting Arcs partition the circle exactly: arc `i` spans
    /// `[θ_i, θ_{i+1}]`, the last `[θ_{n-1}, θ_0 + 2π]`, covering `[θ_0, θ_0+2π)`
    /// with no gap or overlap.
    pub fn trim_circle_to_arcs(&self, angles: &[f64]) -> Result<Vec<AnalyticCurve>> {
        use std::f64::consts::TAU;
        let (center, radius, normal, basis_u) = match self {
            AnalyticCurve::Circle { center, radius, normal, basis_u } => {
                (*center, *radius, *normal, *basis_u)
            }
            _ => bail!("trim_circle_to_arcs: receiver is not a Circle"),
        };
        let norm = |a: f64| -> f64 {
            let mut x = a % TAU;
            if x < 0.0 {
                x += TAU;
            }
            x
        };
        let mut angs: Vec<f64> = angles.iter().map(|a| norm(*a)).collect();
        angs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        angs.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        if angs.len() < 2 {
            bail!(
                "trim_circle_to_arcs: need ≥2 distinct crossings, got {}",
                angs.len()
            );
        }
        let n = angs.len();
        let mut arcs = Vec::with_capacity(n);
        for i in 0..n {
            let a0 = angs[i];
            let a1 = if i + 1 < n { angs[i + 1] } else { angs[0] + TAU };
            arcs.push(AnalyticCurve::Arc {
                center,
                radius,
                normal,
                basis_u,
                start_angle: a0,
                end_angle: a1,
            });
        }
        Ok(arcs)
    }
}

// ────────────────────────────────────────────────────────────────────
// Tests — ADR-059 §3 Step 1 (4 회귀)
// ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    /// ADR-059 §3 Step 1 #1 — synthesize_line_curve produces Line variant
    /// with the correct vertex IDs.
    #[test]
    fn edge_curve_mandatory_synthesizes_line_by_default() {
        let v0 = VertId::new(7);
        let v1 = VertId::new(13);
        let curve = synthesize_line_curve(v0, v1);
        match curve {
            AnalyticCurve::Line { start, end } => {
                assert_eq!(start, v0);
                assert_eq!(end, v1);
            }
            other => panic!("expected Line, got {:?}", other),
        }
    }

    /// ADR-059 §3 Step 1 #2 — synthesize_plane_surface produces a Plane
    /// passing through the centroid with Newell normal.
    #[test]
    fn face_surface_mandatory_synthesizes_plane_by_default() {
        // Unit square in z=5 plane (CCW)
        let verts = vec![
            DVec3::new(0.0, 0.0, 5.0),
            DVec3::new(1.0, 0.0, 5.0),
            DVec3::new(1.0, 1.0, 5.0),
            DVec3::new(0.0, 1.0, 5.0),
        ];
        let surface = synthesize_plane_surface(&verts);
        match surface {
            AnalyticSurface::Plane { origin, normal, .. } => {
                // Centroid = (0.5, 0.5, 5.0)
                assert!((origin - DVec3::new(0.5, 0.5, 5.0)).length() < 1e-9);
                // Newell normal of CCW XY square = +Z
                assert!((normal - DVec3::Z).length() < 1e-9,
                    "expected +Z normal, got {:?}", normal);
            }
            other => panic!("expected Plane, got {:?}", other),
        }
    }

    /// ADR-059 §C (Amendment 1 적용) — analytic enum size budget.
    ///
    /// Original §C target (100/96 bytes) was unachievable: Plane
    /// variant alone is 104 bytes (3 DVec3 + 2 (f64,f64)). Boxing Plane
    /// would force heap deref on every hot-path access — net negative.
    ///
    /// Amendment 1 §A1.1 revised to 132/112 bytes (Plane natural limit).
    /// Box wrapping for NURBSSurface/BSplineSurface/BezierPatch is a
    /// FUTURE optimization (§A1.2 lock-in) — currently inline at
    /// 128/104 bytes both within revised budget.
    ///
    /// **Budget enforcement**: ≤ 132 / ≤ 112 (production lock-in).
    /// Crossing these thresholds requires new amendment + memory
    /// budget re-measurement.
    #[test]
    fn analytic_surface_size_within_budget() {
        let cur = mem::size_of::<AnalyticSurface>();
        eprintln!("ADR-059 §C: AnalyticSurface size = {} bytes (Amendment 1 target ≤ 132)", cur);
        assert!(cur <= 132,
            "AnalyticSurface size {} bytes exceeds ADR-059 §C Amendment 1 target 132 bytes. \
             Either box heavy variants per §A1.2 lock-in, or amend §C with rationale.",
            cur);

        let cur_curve = mem::size_of::<AnalyticCurve>();
        eprintln!("ADR-059 §C: AnalyticCurve  size = {} bytes (Amendment 1 target ≤ 112)", cur_curve);
        assert!(cur_curve <= 112,
            "AnalyticCurve size {} bytes exceeds ADR-059 §C Amendment 1 target 112 bytes",
            cur_curve);
    }

    /// ADR-059 §3 Step 1 #4 — synthesize_plane uses Newell normal +
    /// centroid (not just first triangle).
    #[test]
    fn synthesize_plane_uses_newell_normal_and_centroid() {
        // L-shape in XY plane (non-convex but planar)
        let verts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(2.0, 1.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(1.0, 2.0, 0.0),
            DVec3::new(0.0, 2.0, 0.0),
        ];
        let surface = synthesize_plane_surface(&verts);
        match surface {
            AnalyticSurface::Plane { origin, normal, basis_u, .. } => {
                // Newell normal of a CCW XY loop = +Z
                assert!((normal - DVec3::Z).length() < 1e-9,
                    "Newell normal: expected +Z, got {:?}", normal);
                // Centroid of L-shape: average of 6 verts
                let expected_centroid: DVec3 = verts.iter().sum::<DVec3>() / 6.0;
                assert!((origin - expected_centroid).length() < 1e-9);
                // basis_u must be unit + perpendicular to normal
                assert!((basis_u.length() - 1.0).abs() < 1e-9, "basis_u must be unit");
                assert!(basis_u.dot(normal).abs() < 1e-9, "basis_u perpendicular to normal");
            }
            other => panic!("expected Plane, got {:?}", other),
        }
    }

    /// Step 2 prep #5 — split_at on Line produces two Lines sharing mid_vert.
    #[test]
    fn split_at_line_produces_two_lines() {
        let v0 = VertId::new(1);
        let v1 = VertId::new(2);
        let mid = VertId::new(99);
        let line = synthesize_line_curve(v0, v1);
        let (left, right) = line.split_at(0.5, mid).unwrap();
        match (left, right) {
            (AnalyticCurve::Line { start: s1, end: e1 },
             AnalyticCurve::Line { start: s2, end: e2 }) => {
                assert_eq!(s1, v0);
                assert_eq!(e1, mid);
                assert_eq!(s2, mid);
                assert_eq!(e2, v1);
            }
            other => panic!("expected (Line, Line), got {:?}", other),
        }
    }

    /// Step 2 prep #6 — split_at on Arc produces two Arcs with adjusted
    /// angle ranges.
    #[test]
    fn split_at_arc_produces_two_arcs() {
        let arc = AnalyticCurve::Arc {
            center: DVec3::ZERO, radius: 1.0,
            normal: DVec3::Z, basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::PI,  // half circle
        };
        let mid = VertId::new(99);
        let split_t = std::f64::consts::FRAC_PI_2;  // quarter point
        let (left, right) = arc.split_at(split_t, mid).unwrap();
        match (left, right) {
            (AnalyticCurve::Arc { start_angle: s1, end_angle: e1, .. },
             AnalyticCurve::Arc { start_angle: s2, end_angle: e2, .. }) => {
                assert!((s1 - 0.0).abs() < 1e-12);
                assert!((e1 - split_t).abs() < 1e-12);
                assert!((s2 - split_t).abs() < 1e-12);
                assert!((e2 - std::f64::consts::PI).abs() < 1e-12);
            }
            other => panic!("expected (Arc, Arc), got {:?}", other),
        }
    }

    /// Step 2 prep #7 — split_at on Circle returns Err (caller must
    /// promote to Arc first).
    #[test]
    fn split_at_circle_returns_err() {
        let circle = AnalyticCurve::Circle {
            center: DVec3::ZERO, radius: 1.0,
            normal: DVec3::Z, basis_u: DVec3::X,
        };
        assert!(circle.split_at(1.0, VertId::new(99)).is_err());
    }

    /// **CAD trim 시뮬레이션 (2026-06-15)** — 선이 원을 가로지를 때 원을 2 Arc 로
    /// 쪼개는 trim 의 해석적 핵심 (`trim_circle_to_arcs`). 수직선 x=0 이 r=100 원을
    /// (0,±r) 에서 가로지름 → angles π/2, 3π/2 → **2 Arc**. 검증: ① 각도범위
    /// [π/2,3π/2]+[3π/2,5π/2] ② 끝점 = 교차점 ((0,±r)) ③ 두 arc tessellation 이
    /// 원을 gap 없이 덮음 (4 cardinal 모두 + 공유 끝점 연속) ④ 모든 점이 정확히
    /// radius 위. split_at 이 거부하던 "Circle → Arc promote" 를 구현 → DCEL trim
    /// op 가 self-loop Circle edge 에 그대로 적용 가능 (full re-derive 불필요).
    #[test]
    fn sim_cad_trim_circle_to_two_arcs() {
        use std::f64::consts::{PI, TAU};
        let center = DVec3::ZERO;
        let radius = 100.0_f64;
        let circle = AnalyticCurve::Circle { center, radius, normal: DVec3::Z, basis_u: DVec3::X };

        // vertical secant x=0 crosses the circle at (0,±r) → angles π/2, 3π/2.
        let arcs = circle.trim_circle_to_arcs(&[PI / 2.0, 3.0 * PI / 2.0]).unwrap();
        assert_eq!(arcs.len(), 2, "Circle → 2 Arcs");

        // ① angle ranges: [π/2, 3π/2] (left half) + [3π/2, 5π/2] (right half).
        let ranges: Vec<(f64, f64)> = arcs
            .iter()
            .map(|a| match a {
                AnalyticCurve::Arc { start_angle, end_angle, .. } => (*start_angle, *end_angle),
                other => panic!("expected Arc, got {:?}", other),
            })
            .collect();
        assert!((ranges[0].0 - PI / 2.0).abs() < 1e-9 && (ranges[0].1 - 3.0 * PI / 2.0).abs() < 1e-9);
        assert!((ranges[1].0 - 3.0 * PI / 2.0).abs() < 1e-9 && (ranges[1].1 - (PI / 2.0 + TAU)).abs() < 1e-9);
        // partition is exact: total swept angle = 2π, no gap/overlap.
        let swept: f64 = ranges.iter().map(|(s, e)| e - s).sum();
        assert!((swept - TAU).abs() < 1e-9, "arcs partition the full circle, swept={}", swept);

        // ② + ③ + ④ — tessellate both arcs; union must cover the circle smoothly.
        let ct = 0.05;
        let mut pts: Vec<DVec3> = Vec::new();
        for a in &arcs {
            if let AnalyticCurve::Arc { center, radius, normal, basis_u, start_angle, end_angle } = a {
                let t = crate::curves::arc::tessellate(
                    *center, *radius, *normal, *basis_u, *start_angle, *end_angle, ct,
                );
                assert!(t.len() >= 3, "each arc tessellates smoothly (got {})", t.len());
                pts.extend(t);
            }
        }
        // ④ every sample lies on the circle (radius exact).
        for p in &pts {
            assert!(((*p - center).length() - radius).abs() < 1e-6, "point on circle: {:?}", p);
        }
        // ③ all 4 cardinal points present (continuity around the full rim).
        let near = |target: DVec3| pts.iter().any(|p| (*p - target).length() < 1.0);
        assert!(near(DVec3::new(radius, 0.0, 0.0)), "+x covered");
        assert!(near(DVec3::new(-radius, 0.0, 0.0)), "-x covered");
        assert!(near(DVec3::new(0.0, radius, 0.0)), "+y covered (crossing)");
        assert!(near(DVec3::new(0.0, -radius, 0.0)), "-y covered (crossing)");

        // error cases: non-Circle, and < 2 distinct crossings (keep whole circle).
        let line = AnalyticCurve::Line { start: VertId::new(0), end: VertId::new(1) };
        assert!(line.trim_circle_to_arcs(&[0.0, PI]).is_err(), "non-Circle rejected");
        assert!(circle.trim_circle_to_arcs(&[PI / 2.0]).is_err(), "<2 crossings rejected");
        // duplicate angles dedup to <2 → Err (tangent / coincident crossing).
        assert!(circle.trim_circle_to_arcs(&[PI / 2.0, PI / 2.0 + 1e-12]).is_err(), "coincident dedup");
    }

    /// ADR-186 A3 / Option B (B1) — split_at on Bezier (de Casteljau).
    /// Each half re-parameterised to [0,1]; shape-preserving.
    #[test]
    fn split_at_bezier_shape_preserving() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        let bezier = AnalyticCurve::Bezier {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(2.0, 5.0, 0.0),
                DVec3::new(8.0, 5.0, 0.0),
                DVec3::new(10.0, 0.0, 0.0),
            ],
        };
        let t = 0.4;
        let (left, right) = bezier.split_at(t, VertId::new(99)).unwrap();
        for i in 0..=12 {
            let u = i as f64 / 12.0;
            // left covers [0, t] reparam → orig(u·t)
            let lo = left.evaluate(u, &mesh).unwrap();
            let oo = bezier.evaluate(u * t, &mesh).unwrap();
            assert!((lo - oo).length() < 1e-9, "bezier left @u={}: {:?} vs {:?}", u, lo, oo);
            // right covers [t, 1] reparam → orig(t + u·(1-t))
            let ro = right.evaluate(u, &mesh).unwrap();
            let oor = bezier.evaluate(t + u * (1.0 - t), &mesh).unwrap();
            assert!((ro - oor).length() < 1e-9, "bezier right @u={}: {:?} vs {:?}", u, ro, oor);
        }
    }

    /// ADR-201 (β-1) — subcurve(t0,t1) shape-preserving for Bezier + BSpline.
    /// 결과 곡선을 자기 param range 전체로 평가하면 원곡선 [t0,t1] 를 추적.
    #[test]
    fn subcurve_shape_preserving() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        // ── Bezier (range [0,1], subcurve reparam to [0,1]) ──
        let bez = AnalyticCurve::Bezier {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(2.0, 6.0, 0.0),
                DVec3::new(8.0, 6.0, 0.0),
                DVec3::new(10.0, 0.0, 0.0),
            ],
        };
        let (t0, t1) = (0.25, 0.75);
        let sub = bez.subcurve(t0, t1).unwrap();
        let (sr0, sr1) = sub.parameter_range();
        for i in 0..=12 {
            let u = i as f64 / 12.0;
            let su = sr0 + (sr1 - sr0) * u; // sub param
            let ou = t0 + (t1 - t0) * u; // orig param
            let sv = sub.evaluate(su, &mesh).unwrap();
            let ov = bez.evaluate(ou, &mesh).unwrap();
            assert!((sv - ov).length() < 1e-9, "bezier subcurve @u={}: {:?} vs {:?}", u, sv, ov);
        }
        // ── BSpline (knots preserved; subcurve range = [t0,t1]) ──
        // 4 control points, degree 2 → 7 clamped knots (1 internal).
        let bsp = AnalyticCurve::BSpline {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 3.0, 0.0),
                DVec3::new(3.0, 3.0, 0.0),
                DVec3::new(5.0, 0.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
            degree: 2,
        };
        let (bt0, bt1) = (0.3, 0.8);
        let bsub = bsp.subcurve(bt0, bt1).unwrap();
        let (br0, br1) = bsub.parameter_range();
        for i in 0..=12 {
            let u = i as f64 / 12.0;
            let su = br0 + (br1 - br0) * u;
            let ou = bt0 + (bt1 - bt0) * u;
            let sv = bsub.evaluate(su, &mesh).unwrap();
            let ov = bsp.evaluate(ou, &mesh).unwrap();
            assert!((sv - ov).length() < 1e-7, "bspline subcurve @u={}: {:?} vs {:?}", u, sv, ov);
        }
    }

    /// B1 — split_at on BSpline (knot insertion); param preserved.
    #[test]
    fn split_at_bspline_shape_preserving() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        let bspline = AnalyticCurve::BSpline {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
                DVec3::new(5.0, 1.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
            degree: 3,
        };
        let t = 0.5;
        let (left, right) = bspline.split_at(t, VertId::new(99)).unwrap();
        for i in 0..=12 {
            let u = t * (i as f64 / 12.0);
            let l = left.evaluate(u, &mesh).unwrap();
            let o = bspline.evaluate(u, &mesh).unwrap();
            assert!((l - o).length() < 1e-7, "bspline left @u={}: {:?} vs {:?}", u, l, o);
        }
        for i in 0..=12 {
            let u = t + (1.0 - t) * (i as f64 / 12.0);
            let r = right.evaluate(u, &mesh).unwrap();
            let o = bspline.evaluate(u, &mesh).unwrap();
            assert!((r - o).length() < 1e-7, "bspline right @u={}: {:?} vs {:?}", u, r, o);
        }
    }

    /// B1 — split_at on NURBS (rational knot insertion); param preserved.
    #[test]
    fn split_at_nurbs_shape_preserving() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        let nurbs = AnalyticCurve::NURBS {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
            ],
            weights: vec![1.0, 0.5, 0.8, 1.0],
            knots: vec![0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0],
            degree: 2,
        };
        let t = 0.5;
        let (left, right) = nurbs.split_at(t, VertId::new(99)).unwrap();
        for i in 0..=12 {
            let u = t * (i as f64 / 12.0);
            let l = left.evaluate(u, &mesh).unwrap();
            let o = nurbs.evaluate(u, &mesh).unwrap();
            assert!((l - o).length() < 1e-7, "nurbs left @u={}: {:?} vs {:?}", u, l, o);
        }
        for i in 0..=12 {
            let u = t + (1.0 - t) * (i as f64 / 12.0);
            let r = right.evaluate(u, &mesh).unwrap();
            let o = nurbs.evaluate(u, &mesh).unwrap();
            assert!((r - o).length() < 1e-7, "nurbs right @u={}: {:?} vs {:?}", u, r, o);
        }
    }

    // ── Step 2 full — parameter_at_3d_point (Line/Circle/Arc) ──

    /// Step 2 #1 — parameter_at_3d_point on Line returns correct t.
    #[test]
    fn parameter_at_3d_point_on_line() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let line = synthesize_line_curve(v0, v1);

        // Midpoint: t = 0.5
        let t = line.parameter_at_3d_point(DVec3::new(5.0, 0.0, 0.0), &mesh).unwrap();
        assert!((t - 0.5).abs() < 1e-9);

        // Quarter point: t = 0.25
        let t2 = line.parameter_at_3d_point(DVec3::new(2.5, 0.0, 0.0), &mesh).unwrap();
        assert!((t2 - 0.25).abs() < 1e-9);

        // Endpoint: t = 0
        let t3 = line.parameter_at_3d_point(DVec3::new(0.0, 0.0, 0.0), &mesh).unwrap();
        assert!(t3.abs() < 1e-9);
    }

    /// Step 2 #2 — Line rejects off-curve points (drift > LOCKED #5).
    #[test]
    fn parameter_at_3d_point_line_rejects_off_curve() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::ZERO);
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let line = synthesize_line_curve(v0, v1);
        // Point 5 mm off line (way > 1.5μm tol)
        let r = line.parameter_at_3d_point(DVec3::new(5.0, 5.0, 0.0), &mesh);
        assert!(matches!(r, Err(SplitParameterError::PointOffCurve { .. })));
    }

    /// Step 2 #3 — parameter_at_3d_point on Circle returns angle.
    #[test]
    fn parameter_at_3d_point_on_circle() {
        let mesh = Mesh::new();
        let circle = AnalyticCurve::Circle {
            center: DVec3::ZERO, radius: 1.0,
            normal: DVec3::Z, basis_u: DVec3::X,
        };
        // Point at angle 0: (1, 0, 0)
        let t0 = circle.parameter_at_3d_point(DVec3::new(1.0, 0.0, 0.0), &mesh).unwrap();
        assert!(t0.abs() < 1e-9);
        // Point at angle π/2: (0, 1, 0)
        let t90 = circle.parameter_at_3d_point(DVec3::new(0.0, 1.0, 0.0), &mesh).unwrap();
        assert!((t90 - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    /// Step 2 #4 — parameter_at_3d_point on Arc respects angle range.
    #[test]
    fn parameter_at_3d_point_on_arc_respects_range() {
        let mesh = Mesh::new();
        // Half arc 0 → π
        let arc = AnalyticCurve::Arc {
            center: DVec3::ZERO, radius: 1.0,
            normal: DVec3::Z, basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: std::f64::consts::PI,
        };
        // Point at midpoint angle (π/2): (0, 1, 0)
        let t = arc.parameter_at_3d_point(DVec3::new(0.0, 1.0, 0.0), &mesh).unwrap();
        assert!((t - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
    }

    /// Step 2 #5 — **ADR-186 step ① (2026-06-16)** — Bezier point→param
    /// inversion now succeeds (was `DeferredToPhaseI`). A point sampled at
    /// t=0.4 inverts back to t≈0.4 within tolerance.
    #[test]
    fn parameter_at_3d_point_bezier_inverts() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        // Non-degenerate cubic Bezier (curved, not collinear).
        let bezier = AnalyticCurve::Bezier {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
            ],
        };
        for t_expected in [0.0_f64, 0.2, 0.4, 0.6, 0.8, 1.0] {
            let p = bezier.evaluate(t_expected, &mesh).unwrap();
            let t = bezier.parameter_at_3d_point(p, &mesh).unwrap();
            assert!((t - t_expected).abs() < 1e-6,
                "bezier invert: t={} → t'={}", t_expected, t);
        }
    }

    /// Step ① #5b — split a Bezier at an inverted CCI-style point yields two
    /// `Bezier` sub-curves (NOT `Line`s) that reconstruct the parent.
    #[test]
    fn bezier_split_at_inverted_param_preserves_curve() {
        use crate::curves::CurveOps;
        let mut mesh = Mesh::new();
        let bezier = AnalyticCurve::Bezier {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 3.0, 0.0),
                DVec3::new(4.0, 3.0, 0.0),
                DVec3::new(5.0, 0.0, 0.0),
            ],
        };
        let cut_point = bezier.evaluate(0.4, &mesh).unwrap();
        let t = bezier.parameter_at_3d_point(cut_point, &mesh).unwrap();
        let mid = mesh.add_vertex(cut_point);
        let (left, right) = bezier.split_at(t, mid).unwrap();
        // Both halves stay Bezier (curve metadata preserved, not Lines).
        assert!(matches!(left, AnalyticCurve::Bezier { .. }), "left not Bezier");
        assert!(matches!(right, AnalyticCurve::Bezier { .. }), "right not Bezier");
        // Sample reconstruction: left[0,1] ≈ parent[0,t], right[0,1] ≈ parent[t,1].
        for u in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
            let l = left.evaluate(u, &mesh).unwrap();
            let o = bezier.evaluate(0.4 * u, &mesh).unwrap();
            assert!((l - o).length() < 1e-7, "left @u={}: {:?} vs {:?}", u, l, o);
            let r = right.evaluate(u, &mesh).unwrap();
            let o2 = bezier.evaluate(0.4 + 0.6 * u, &mesh).unwrap();
            assert!((r - o2).length() < 1e-7, "right @u={}: {:?} vs {:?}", u, r, o2);
        }
    }

    /// Step ① #5c — BSpline point→param inversion round-trips.
    #[test]
    fn parameter_at_3d_point_bspline_inverts() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        // Cubic clamped BSpline, 5 control points.
        let bspline = AnalyticCurve::BSpline {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(2.0, -1.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
            ],
            knots: vec![0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0],
            degree: 3,
        };
        let (t0, t1) = bspline.parameter_range();
        for frac in [0.1_f64, 0.3, 0.5, 0.7, 0.9] {
            let t_expected = t0 + (t1 - t0) * frac;
            let p = bspline.evaluate(t_expected, &mesh).unwrap();
            let t = bspline.parameter_at_3d_point(p, &mesh).unwrap();
            let p_back = bspline.evaluate(t, &mesh).unwrap();
            assert!((p_back - p).length() < 1e-5,
                "bspline invert: frac={} drift={}", frac, (p_back - p).length());
        }
    }

    /// Step ① #5d — NURBS point→param inversion round-trips.
    #[test]
    fn parameter_at_3d_point_nurbs_inverts() {
        use crate::curves::CurveOps;
        let mesh = Mesh::new();
        let nurbs = AnalyticCurve::NURBS {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
            ],
            weights: vec![1.0, 2.0, 0.5, 1.0],
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
            degree: 3,
        };
        let (t0, t1) = nurbs.parameter_range();
        for frac in [0.15_f64, 0.4, 0.65, 0.85] {
            let t_expected = t0 + (t1 - t0) * frac;
            let p = nurbs.evaluate(t_expected, &mesh).unwrap();
            let t = nurbs.parameter_at_3d_point(p, &mesh).unwrap();
            let p_back = nurbs.evaluate(t, &mesh).unwrap();
            assert!((p_back - p).length() < 1e-5,
                "nurbs invert: frac={} drift={}", frac, (p_back - p).length());
        }
    }

    /// Step ① #5e — free-form inversion rejects an off-curve point.
    #[test]
    fn parameter_at_3d_point_bezier_rejects_off_curve() {
        let mesh = Mesh::new();
        let bezier = AnalyticCurve::Bezier {
            control_pts: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 2.0, 0.0),
                DVec3::new(3.0, 2.0, 0.0),
                DVec3::new(4.0, 0.0, 0.0),
            ],
        };
        // Point 10 mm off the curve (way beyond LOCKED #5 tol).
        let r = bezier.parameter_at_3d_point(DVec3::new(2.0, 50.0, 0.0), &mesh);
        assert!(matches!(r, Err(SplitParameterError::PointOffCurve { .. })),
            "expected PointOffCurve, got {:?}", r);
    }

    /// Step 2 #6 — Round-trip: parameter_at_3d_point ∘ evaluate ≈ identity
    /// (for Line + Arc which support both).
    #[test]
    fn parameter_at_3d_point_roundtrip_line_arc() {
        use crate::curves::CurveOps;
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::ZERO);
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));

        // Line round-trip
        let line = synthesize_line_curve(v0, v1);
        for t_expected in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
            let p = line.evaluate(t_expected, &mesh).unwrap();
            let t_recovered = line.parameter_at_3d_point(p, &mesh).unwrap();
            assert!((t_recovered - t_expected).abs() < 1e-9,
                "line roundtrip: t={} → t'={}", t_expected, t_recovered);
        }

        // Arc round-trip
        let arc = AnalyticCurve::Arc {
            center: DVec3::ZERO, radius: 1.0,
            normal: DVec3::Z, basis_u: DVec3::X,
            start_angle: 0.0, end_angle: std::f64::consts::PI,
        };
        for t_expected in [0.0_f64, 0.5, 1.0, 1.5, std::f64::consts::PI] {
            let p = arc.evaluate(t_expected, &mesh).unwrap();
            let t_recovered = arc.parameter_at_3d_point(p, &mesh).unwrap();
            assert!((t_recovered - t_expected).abs() < 1e-6,
                "arc roundtrip: t={} → t'={}", t_expected, t_recovered);
        }
    }

    /// Bonus: degenerate (collinear) loop falls back to default +Z plane
    /// without panic.
    #[test]
    fn synthesize_plane_degenerate_loop_falls_back() {
        // Collinear points
        let verts = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
        ];
        let surface = synthesize_plane_surface(&verts);
        match surface {
            AnalyticSurface::Plane { normal, .. } => {
                // Newell normal degenerate → fallback to +Z
                assert!((normal - DVec3::Z).length() < 1e-9);
            }
            other => panic!("expected Plane fallback, got {:?}", other),
        }
    }
}
