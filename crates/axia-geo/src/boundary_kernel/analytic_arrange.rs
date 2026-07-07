//! **ADR-186 (A)** — Analytic arrangement: 2D Line + Circle → **arc 경계 면**.
//!
//! 폴리곤 잔재 제거 = "한 방향(NURBS) 통합". 원을 polygon 으로 깎지 않고
//! analytic 교차(closed-form) + Arc 분할 + **tangent 기반 region 추출**
//! (Step 1+2 proof 일반화) 로 면을 직접 추출. polygon 은 렌더에만.
//!
//! ## 파이프라인
//! 1. 쌍별 analytic 교차 (line-line / line-circle / circle-circle, closed-form)
//! 2. 곡선 분할 — line→segment, circle→Arc (교차 param 에서)
//! 3. half-edge graph (vert = 교차점 ∪ endpoint, eps dedup)
//! 4. region 추출 — **departing tangent** angular sort + leftmost-turn
//! 5. signed area 로 bounded 면 (outer 제외)
//!
//! ## 한계 (본 모듈 v1 = Step 3)
//! - 교차하는 곡선만 (standalone/containment hole = 후속 step)
//! - collinear-overlap line 미처리 (별도)

#![allow(dead_code)]

use std::collections::HashSet;
use std::f64::consts::PI;

use glam::DVec3;

use super::geom2::Vec2;
use crate::curves::AnalyticCurve;
use crate::mesh::Mesh;

const TWO_PI: f64 = 2.0 * PI;

/// **ADR-186 A3 / Option B (B2)** — 2D freeform curve (Bezier / BSpline /
/// NURBS) inside the arrangement plane.
///
/// Stored as **2D control data** so `arrange` stays 2D-pure; all curve
/// math (eval / tangent / tessellate) lifts the 2D control points to z=0
/// and reuses the 3D `crate::curves` kernel (no 2D duplication). The DCEL
/// realization (B4) unprojects the 2D control points back to the draw plane.
///
/// Discriminator: `knots.is_empty()` → Bezier; else `weights.is_empty()`
/// → BSpline; else NURBS.
#[derive(Clone, Debug)]
pub struct Freeform2D {
    pub ctrl: Vec<Vec2>,
    pub knots: Vec<f64>,
    pub weights: Vec<f64>,
    pub degree: u32,
    /// **B4b-2b** — `curve_owner_id` (ADR-088 space) of the source freeform.
    /// Set by the feeding layer; carried through `arrange` (split_curve clone)
    /// so Phase 4 can tag each sub-bezier edge → B6 restores the original by
    /// owner-id (P5 idempotency). `None` for standalone/test freeforms.
    pub owner_id: Option<u32>,
}

impl Freeform2D {
    /// Bezier from 2D control points (degree = n - 1).
    pub fn bezier(ctrl: Vec<Vec2>) -> Self {
        Self { ctrl, knots: Vec::new(), weights: Vec::new(), degree: 0, owner_id: None }
    }
    /// BSpline from 2D control points + knots + degree.
    pub fn bspline(ctrl: Vec<Vec2>, knots: Vec<f64>, degree: u32) -> Self {
        Self { ctrl, knots, weights: Vec::new(), degree, owner_id: None }
    }
    /// NURBS from 2D control points + weights + knots + degree.
    pub fn nurbs(ctrl: Vec<Vec2>, weights: Vec<f64>, knots: Vec<f64>, degree: u32) -> Self {
        Self { ctrl, knots, weights, degree, owner_id: None }
    }
    /// **B4b-2b** — builder: tag with a source `curve_owner_id`.
    pub fn with_owner(mut self, owner: Option<u32>) -> Self {
        self.owner_id = owner;
        self
    }

    fn ctrl3d(&self) -> Vec<DVec3> {
        self.ctrl.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect()
    }

    /// Lift to a 3D `AnalyticCurve` in the z=0 plane (for CCI / split via the
    /// shared kernel). The caller projects results back to 2D.
    pub fn to_curve3d(&self) -> AnalyticCurve {
        let c = self.ctrl3d();
        if self.knots.is_empty() {
            AnalyticCurve::Bezier { control_pts: c }
        } else if self.weights.is_empty() {
            AnalyticCurve::BSpline { control_pts: c, knots: self.knots.clone(), degree: self.degree }
        } else {
            AnalyticCurve::NURBS {
                control_pts: c,
                weights: self.weights.clone(),
                knots: self.knots.clone(),
                degree: self.degree,
            }
        }
    }

    /// Parameter range. Bezier `[0,1]`; BSpline/NURBS `[knots[deg], knots[n]]`.
    pub fn param_range(&self) -> (f64, f64) {
        if self.knots.is_empty() {
            (0.0, 1.0)
        } else {
            let d = self.degree as usize;
            if self.knots.len() >= d + 1 + self.ctrl.len() {
                (self.knots[d], self.knots[self.ctrl.len()])
            } else {
                (0.0, 1.0)
            }
        }
    }

    /// Evaluate at parameter `t` (2D, projected). Reuses the 3D kernel (no mesh
    /// needed — only `Line` curves consult mesh, which Freeform never is).
    pub fn eval(&self, t: f64) -> Vec2 {
        let c = self.ctrl3d();
        let d = self.degree as usize;
        let p = if self.knots.is_empty() {
            crate::curves::bezier::evaluate(&c, t)
        } else if self.weights.is_empty() {
            crate::curves::bspline::evaluate(&c, &self.knots, d, t)
        } else {
            crate::curves::nurbs::evaluate(&c, &self.weights, &self.knots, d, t)
        };
        p.map(|q| Vec2::new(q.x, q.y)).unwrap_or(Vec2::new(0.0, 0.0))
    }

    /// First derivative (tangent, not unit length) at `t` (2D).
    pub fn tangent_raw(&self, t: f64) -> Vec2 {
        let c = self.ctrl3d();
        let d = self.degree as usize;
        let p = if self.knots.is_empty() {
            crate::curves::bezier::derivative(&c, t)
        } else if self.weights.is_empty() {
            crate::curves::bspline::derivative(&c, &self.knots, d, t)
        } else {
            crate::curves::nurbs::derivative(&c, &self.weights, &self.knots, d, t)
        };
        p.map(|q| Vec2::new(q.x, q.y)).unwrap_or(Vec2::new(1.0, 0.0))
    }
}

/// 입력 곡선 (2D, full).
#[derive(Clone, Debug)]
pub enum InputCurve {
    /// 직선 segment.
    Line { a: Vec2, b: Vec2 },
    /// full 원.
    Circle { center: Vec2, radius: f64 },
    /// **ADR-200 (A1)** — open arc `[a0, a1]` (CCW, a1 > a0, a1 ≤ a0 + 2π).
    /// 부분 원(사용자가 원의 일부 삭제 / DrawArc)을 1급 입력으로 처리. Circle 과
    /// 달리 **self-closing 아님** — 면을 단독으로 만들지 않고, 다른 곡선과 닫힌
    /// 영역을 이룰 때만 면화 (DCEL cycle walk 가 자연 처리, lone arc = spur →
    /// signed-area ≈ 0 → 필터). param 규약 = arc frame 각도 [a0, a1].
    Arc {
        center: Vec2,
        radius: f64,
        a0: f64,
        a1: f64,
    },
    /// **B2** — full freeform (Bezier / BSpline / NURBS) closed/open curve.
    Freeform(Freeform2D),
}

/// 면 경계 sub-curve (분할 결과).
#[derive(Clone, Debug)]
pub enum SubCurve {
    /// 직선 조각.
    Line { a: Vec2, b: Vec2 },
    /// arc (a0→a1, CCW: a1 > a0).
    Arc {
        center: Vec2,
        radius: f64,
        a0: f64,
        a1: f64,
    },
    /// **B2** — freeform sub-curve over parameter sub-range `[t0, t1]`
    /// (t1 > t0 = forward). Carries the full `Freeform2D`; the sub-range
    /// selects the portion. Standalone closed curve uses the full range.
    Freeform {
        f2d: Freeform2D,
        t0: f64,
        t1: f64,
    },
}

/// 추출된 면.
#[derive(Clone, Debug)]
pub struct ArrFace {
    /// outer boundary (CCW), sub-curve 순서.
    pub outer: Vec<SubCurve>,
    /// hole (inner) loops.
    pub holes: Vec<Vec<SubCurve>>,
}

// ─────────────────────────── helpers ───────────────────────────

fn norm_angle(a: f64) -> f64 {
    let mut x = a % TWO_PI;
    if x < 0.0 {
        x += TWO_PI;
    }
    x
}

fn vec_angle(v: Vec2) -> f64 {
    norm_angle(v.y.atan2(v.x))
}

fn lerp(a: Vec2, b: Vec2, t: f64) -> Vec2 {
    Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn circle_pt(center: Vec2, r: f64, ang: f64) -> Vec2 {
    Vec2::new(center.x + r * ang.cos(), center.y + r * ang.sin())
}

/// **ADR-200 (A1)** — `theta_raw`(임의 각도)를 arc frame `[a0, a0+2π)` 로 unwrap
/// 후, arc `[a0, a1]` 위(within `eps_ang`)면 param(=`a0 + d`) 반환. off-arc = None.
/// 끝점 근처(2π wrap)는 a0 으로, sweep 초과는 a1 으로 clamp.
fn arc_param_if_on(theta_raw: f64, a0: f64, a1: f64, eps_ang: f64) -> Option<f64> {
    let sweep = a1 - a0;
    let mut d = norm_angle(theta_raw - a0); // [0, 2π)
    if d > TWO_PI - eps_ang {
        d = 0.0; // theta ≈ a0 (slightly below) → arc start
    }
    if d <= sweep + eps_ang {
        Some(a0 + d.min(sweep))
    } else {
        None
    }
}

fn normalize(v: Vec2) -> Vec2 {
    let l = v.len();
    if l < 1e-15 {
        Vec2::new(0.0, 0.0)
    } else {
        Vec2::new(v.x / l, v.y / l)
    }
}

impl SubCurve {
    fn start_pt(&self) -> Vec2 {
        match self {
            SubCurve::Line { a, .. } => *a,
            SubCurve::Arc { center, radius, a0, .. } => circle_pt(*center, *radius, *a0),
            SubCurve::Freeform { f2d, t0, .. } => f2d.eval(*t0),
        }
    }
    fn end_pt(&self) -> Vec2 {
        match self {
            SubCurve::Line { b, .. } => *b,
            SubCurve::Arc { center, radius, a1, .. } => circle_pt(*center, *radius, *a1),
            SubCurve::Freeform { f2d, t1, .. } => f2d.eval(*t1),
        }
    }
    /// 시작점에서 떠나는 tangent (start→end 방향).
    fn tangent_start(&self) -> Vec2 {
        match self {
            SubCurve::Line { a, b } => normalize(b.sub(*a)),
            SubCurve::Arc { a0, a1, .. } => {
                let s = (a1 - a0).signum();
                normalize(Vec2::new(-a0.sin() * s, a0.cos() * s))
            }
            SubCurve::Freeform { f2d, t0, t1 } => {
                let s = (t1 - t0).signum();
                let d = f2d.tangent_raw(*t0);
                normalize(Vec2::new(d.x * s, d.y * s))
            }
        }
    }
    /// 끝점에서 떠나는 tangent (end→start 방향, backward).
    fn tangent_end(&self) -> Vec2 {
        match self {
            SubCurve::Line { a, b } => normalize(a.sub(*b)),
            SubCurve::Arc { a0, a1, .. } => {
                let s = (a1 - a0).signum();
                // end 에서 a1→a0 (backward) = -(derivative at a1, forward 방향)
                normalize(Vec2::new(a1.sin() * s, -a1.cos() * s))
            }
            SubCurve::Freeform { f2d, t0, t1 } => {
                let s = (t1 - t0).signum();
                let d = f2d.tangent_raw(*t1);
                normalize(Vec2::new(-d.x * s, -d.y * s))
            }
        }
    }
    /// **DIAG → fix (2026-06-18)** — Signed curvature (heading-angle rate) in the
    /// traversal direction. `forward` = start→end. **+** = turning left (CCW),
    /// `0` = straight (line), **−** = turning right (CW). Used ONLY as a tie-break
    /// in the departing-tangent angular sort at **tangency** points (a curve
    /// touching a line / another curve so two half-edges share a departing
    /// tangent — e.g. a circle tangent to a rect edge at a corner). Without it the
    /// cycle walk cannot order the coincident half-edges → 1 degenerate face
    /// instead of the correct sub-faces. Equivalent to ordering by the angle of a
    /// point a hair along each curve: larger curvature (bending more CCW) sits at a
    /// slightly larger angle → sorts later. Scale-invariant (κ = 1/r).
    fn signed_curvature(&self, forward: bool) -> f64 {
        match self {
            SubCurve::Line { .. } => 0.0,
            SubCurve::Arc { radius, a0, a1, .. } => {
                let s = (*a1 - *a0).signum(); // +1 CCW, −1 CW (forward traversal)
                let k = s / radius.max(1e-12);
                if forward {
                    k
                } else {
                    -k
                }
            }
            // Freeform tangency tie-break = future (user bug is circle/Arc). 0 keeps
            // freeform unchanged (no regression to the standalone-bezier path).
            SubCurve::Freeform { .. } => 0.0,
        }
    }
    /// from-vert 에서 to-vert 까지의 sample 점 (to-vert 제외), traversal 방향.
    /// `forward` = start→end.
    fn samples(&self, forward: bool, n: usize) -> Vec<Vec2> {
        match self {
            SubCurve::Line { a, b } => {
                vec![if forward { *a } else { *b }]
            }
            SubCurve::Arc { center, radius, a0, a1 } => {
                let (f0, f1) = if forward { (*a0, *a1) } else { (*a1, *a0) };
                (0..n)
                    .map(|k| {
                        let t = f0 + (f1 - f0) * (k as f64) / (n as f64);
                        circle_pt(*center, *radius, t)
                    })
                    .collect()
            }
            SubCurve::Freeform { f2d, t0, t1 } => {
                let (f0, f1) = if forward { (*t0, *t1) } else { (*t1, *t0) };
                (0..n)
                    .map(|k| {
                        let t = f0 + (f1 - f0) * (k as f64) / (n as f64);
                        f2d.eval(t)
                    })
                    .collect()
            }
        }
    }
}

// ─────────────────────────── intersections (closed-form) ───────────────────────────

/// line(segment) ∩ line(segment) → (t_on_1, t_on_2, point).
fn isect_line_line(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2, eps: f64) -> Vec<(f64, f64, Vec2)> {
    let d1 = a1.sub(a0);
    let d2 = b1.sub(b0);
    let denom = d1.cross(d2);
    if denom.abs() < eps * eps {
        return Vec::new(); // parallel / collinear (별도 처리)
    }
    let diff = b0.sub(a0);
    let t = diff.cross(d2) / denom;
    let u = diff.cross(d1) / denom;
    if t >= -1e-9 && t <= 1.0 + 1e-9 && u >= -1e-9 && u <= 1.0 + 1e-9 {
        vec![(t.clamp(0.0, 1.0), u.clamp(0.0, 1.0), lerp(a0, a1, t))]
    } else {
        Vec::new()
    }
}

/// line(segment) ∩ circle → (t_on_line, angle_on_circle, point).
fn isect_line_circle(a0: Vec2, a1: Vec2, c: Vec2, r: f64, _eps: f64) -> Vec<(f64, f64, Vec2)> {
    let d = a1.sub(a0);
    let f = a0.sub(c);
    let aa = d.dot(d);
    if aa < 1e-18 {
        return Vec::new();
    }
    let bb = 2.0 * f.dot(d);
    let cc = f.dot(f) - r * r;
    let disc = bb * bb - 4.0 * aa * cc;
    if disc < 0.0 {
        return Vec::new();
    }
    let sq = disc.sqrt();
    let mut out = Vec::new();
    for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
        if t >= -1e-9 && t <= 1.0 + 1e-9 {
            let p = lerp(a0, a1, t.clamp(0.0, 1.0));
            let ang = norm_angle((p.y - c.y).atan2(p.x - c.x));
            out.push((t.clamp(0.0, 1.0), ang, p));
        }
    }
    out
}

/// circle ∩ circle → (angle_on_0, angle_on_1, point).
fn isect_circle_circle(c0: Vec2, r0: f64, c1: Vec2, r1: f64, eps: f64) -> Vec<(f64, f64, Vec2)> {
    let d = c1.sub(c0);
    let dist = d.len();
    if dist < eps || dist > r0 + r1 + eps || dist < (r0 - r1).abs() - eps {
        return Vec::new(); // 동심 / 분리 / 포함
    }
    let a = (r0 * r0 - r1 * r1 + dist * dist) / (2.0 * dist);
    let h2 = r0 * r0 - a * a;
    let h = h2.max(0.0).sqrt();
    let mid = c0.add(d.mul(a / dist));
    let perp = Vec2::new(-d.y / dist, d.x / dist);
    let cand = if h < eps {
        vec![mid] // tangent (1점)
    } else {
        vec![mid.add(perp.mul(h)), mid.sub(perp.mul(h))]
    };
    cand.into_iter()
        .map(|p| {
            let a0 = norm_angle((p.y - c0.y).atan2(p.x - c0.x));
            let a1 = norm_angle((p.y - c1.y).atan2(p.x - c1.x));
            (a0, a1, p)
        })
        .collect()
}

/// dispatch — (param_on_c1, param_on_c2, point).
/// **B5** — lift a 2D `InputCurve` to a 3D `AnalyticCurve` (z=0 plane) for the
/// shared type-agnostic CCI kernel. Line → degree-1 Bezier (2 control points,
/// param `[0,1]`, no mesh-vertex lookup); Circle → `AnalyticCurve::Circle`
/// (param = angle, +Z normal); Freeform → `to_curve3d`. Parameter conventions
/// are chosen to align with `split_curve` (line → `[0,1]`, circle → angle,
/// freeform → param) so intersect results feed it directly without recompute.
fn lift_to_curve3d(c: &InputCurve) -> AnalyticCurve {
    match c {
        InputCurve::Line { a, b } => AnalyticCurve::Bezier {
            control_pts: vec![DVec3::new(a.x, a.y, 0.0), DVec3::new(b.x, b.y, 0.0)],
        },
        InputCurve::Circle { center, radius } => AnalyticCurve::Circle {
            center: DVec3::new(center.x, center.y, 0.0),
            radius: *radius,
            normal: DVec3::new(0.0, 0.0, 1.0),
            basis_u: DVec3::new(1.0, 0.0, 0.0),
        },
        InputCurve::Arc { center, radius, a0, a1 } => AnalyticCurve::Arc {
            center: DVec3::new(center.x, center.y, 0.0),
            radius: *radius,
            normal: DVec3::new(0.0, 0.0, 1.0),
            basis_u: DVec3::new(1.0, 0.0, 0.0),
            start_angle: *a0,
            end_angle: *a1,
        },
        InputCurve::Freeform(f) => f.to_curve3d(),
    }
}

fn intersect(c1: &InputCurve, c2: &InputCurve, eps: f64) -> Vec<(f64, f64, Vec2)> {
    match (c1, c2) {
        (InputCurve::Line { a, b }, InputCurve::Line { a: a2, b: b2 }) => {
            isect_line_line(*a, *b, *a2, *b2, eps)
        }
        (InputCurve::Line { a, b }, InputCurve::Circle { center, radius }) => {
            isect_line_circle(*a, *b, *center, *radius, eps)
        }
        (InputCurve::Circle { center, radius }, InputCurve::Line { a, b }) => {
            isect_line_circle(*a, *b, *center, *radius, eps)
                .into_iter()
                .map(|(t, ang, p)| (ang, t, p)) // swap: param_on_c1=circle angle
                .collect()
        }
        (
            InputCurve::Circle { center: c0, radius: r0 },
            InputCurve::Circle { center: c1c, radius: r1 },
        ) => isect_circle_circle(*c0, *r0, *c1c, *r1, eps),
        // ── ADR-200 (A1) Arc arms — 원 교차를 arc 각도범위 [a0,a1] 로 클립.
        (InputCurve::Arc { center, radius, a0, a1 }, InputCurve::Line { a, b }) => {
            let ea = (eps / radius.max(1e-9)).max(1e-9);
            isect_line_circle(*a, *b, *center, *radius, eps)
                .into_iter()
                .filter_map(|(t, ang, p)| arc_param_if_on(ang, *a0, *a1, ea).map(|pa| (pa, t, p)))
                .collect()
        }
        (InputCurve::Line { a, b }, InputCurve::Arc { center, radius, a0, a1 }) => {
            let ea = (eps / radius.max(1e-9)).max(1e-9);
            isect_line_circle(*a, *b, *center, *radius, eps)
                .into_iter()
                .filter_map(|(t, ang, p)| arc_param_if_on(ang, *a0, *a1, ea).map(|pa| (t, pa, p)))
                .collect()
        }
        (
            InputCurve::Arc { center: ca, radius: ra, a0, a1 },
            InputCurve::Circle { center: cc, radius: rc },
        ) => {
            let ea = (eps / ra.max(1e-9)).max(1e-9);
            isect_circle_circle(*ca, *ra, *cc, *rc, eps)
                .into_iter()
                .filter_map(|(aa, ac, p)| arc_param_if_on(aa, *a0, *a1, ea).map(|pa| (pa, ac, p)))
                .collect()
        }
        (
            InputCurve::Circle { center: cc, radius: rc },
            InputCurve::Arc { center: ca, radius: ra, a0, a1 },
        ) => {
            let ea = (eps / ra.max(1e-9)).max(1e-9);
            isect_circle_circle(*cc, *rc, *ca, *ra, eps)
                .into_iter()
                .filter_map(|(ac, aa, p)| arc_param_if_on(aa, *a0, *a1, ea).map(|pa| (ac, pa, p)))
                .collect()
        }
        (
            InputCurve::Arc { center: c0, radius: r0, a0: s0, a1: e0 },
            InputCurve::Arc { center: c1c, radius: r1, a0: s1, a1: e1 },
        ) => {
            let ea0 = (eps / r0.max(1e-9)).max(1e-9);
            let ea1 = (eps / r1.max(1e-9)).max(1e-9);
            isect_circle_circle(*c0, *r0, *c1c, *r1, eps)
                .into_iter()
                .filter_map(|(aa, ab, p)| {
                    let pa = arc_param_if_on(aa, *s0, *e0, ea0)?;
                    let pb = arc_param_if_on(ab, *s1, *e1, ea1)?;
                    Some((pa, pb, p))
                })
                .collect()
        }
        // B3 + B5 — any pair involving a freeform: lift both inputs to 3D
        // AnalyticCurve and intersect via the shared type-agnostic CCI kernel
        // (subdivide + Newton, ADR-030). Line → degree-1 Bezier, Circle →
        // Circle, Freeform → to_curve3d (via `lift_to_curve3d`). Param
        // conventions align directly with split_curve (line → [0,1], circle →
        // angle, freeform → param), so (t1, t2) feed it without recompute.
        // All line/circle-only pairs use the closed-form arms above; only
        // freeform-involving pairs reach here (freeform×freeform B3,
        // freeform×line/circle B5). CCI tol 10× tighter than the dedup eps so
        // endpoints at a shared intersection merge robustly in VertSet.
        (c1, c2) => {
            let mesh = Mesh::new();
            let tol = (eps * 0.1).max(1e-9);
            match crate::curves::intersect::intersect_curves(
                &lift_to_curve3d(c1),
                &lift_to_curve3d(c2),
                &mesh,
                tol,
            ) {
                Ok(hits) => hits
                    .into_iter()
                    .map(|h| (h.t1, h.t2, Vec2::new(h.point.x, h.point.y)))
                    .collect(),
                Err(_) => Vec::new(),
            }
        }
    }
}

/// **4-α** — 공선(collinear) + 중첩(overlap) 직선 쌍 → 각자 분할 param
/// (서로의 interior endpoint). 완전 동일(분할점 0)은 None (dedup 이 처리).
fn collinear_overlap(
    a0: Vec2,
    a1: Vec2,
    b0: Vec2,
    b1: Vec2,
    eps: f64,
) -> Option<(Vec<f64>, Vec<f64>)> {
    let d = a1.sub(a0);
    let dlen = d.len();
    let db = b1.sub(b0);
    let dblen = db.len();
    if dlen < eps || dblen < eps {
        return None;
    }
    // b0, b1 이 line A 위인가 (수직거리 < eps) → 공선.
    if d.cross(b0.sub(a0)).abs() / dlen > eps || d.cross(b1.sub(a0)).abs() / dlen > eps {
        return None;
    }
    let pe = 1e-9; // param eps
    // B endpoints → A param.
    let tb0 = b0.sub(a0).dot(d) / (dlen * dlen);
    let tb1 = b1.sub(a0).dot(d) / (dlen * dlen);
    let (tlo, thi) = (tb0.min(tb1), tb0.max(tb1));
    // [0,1] (A) ∩ [tlo,thi] (B on A).
    if thi.min(1.0) - tlo.max(0.0) < pe {
        return None; // 중첩 없음 (점접촉 포함)
    }
    let mut pa = Vec::new();
    for &t in &[tb0, tb1] {
        if t > pe && t < 1.0 - pe {
            pa.push(t);
        }
    }
    // A endpoints → B param.
    let ta0 = a0.sub(b0).dot(db) / (dblen * dblen);
    let ta1 = a1.sub(b0).dot(db) / (dblen * dblen);
    let mut pb = Vec::new();
    for &t in &[ta0, ta1] {
        if t > pe && t < 1.0 - pe {
            pb.push(t);
        }
    }
    if pa.is_empty() && pb.is_empty() {
        return None; // 완전 동일 변 → dedup 이 처리
    }
    Some((pa, pb))
}

// ─────────────────────────── split ───────────────────────────

fn split_curve(c: &InputCurve, params: &[f64]) -> Vec<SubCurve> {
    match c {
        InputCurve::Line { a, b } => {
            let mut ts: Vec<f64> = params.to_vec();
            ts.push(0.0);
            ts.push(1.0);
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let mut subs = Vec::new();
            for w in ts.windows(2) {
                let pa = lerp(*a, *b, w[0]);
                let pb = lerp(*a, *b, w[1]);
                if pa.dist(pb) > 1e-9 {
                    subs.push(SubCurve::Line { a: pa, b: pb });
                }
            }
            subs
        }
        InputCurve::Circle { center, radius } => {
            let mut angs: Vec<f64> = params.iter().map(|a| norm_angle(*a)).collect();
            angs.sort_by(|x, y| x.partial_cmp(y).unwrap());
            angs.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            if angs.len() < 2 {
                return Vec::new(); // standalone circle = 후속 step
            }
            let mut subs = Vec::new();
            for i in 0..angs.len() {
                let a0 = angs[i];
                let a1 = if i + 1 < angs.len() {
                    angs[i + 1]
                } else {
                    angs[0] + TWO_PI
                };
                subs.push(SubCurve::Arc {
                    center: *center,
                    radius: *radius,
                    a0,
                    a1,
                });
            }
            subs
        }
        // ── ADR-200 (A1) — open arc 분할 (wrap 없음). params 는 arc frame 각도.
        // 양 끝(a0,a1) 추가 후 인접쌍 → SubCurve::Arc. 교차 0 (lone arc) → 전체
        // arc 1조각 (DCEL spur, 면 미형성 — standalone 케이스 없음).
        InputCurve::Arc { center, radius, a0, a1 } => {
            let mut ts: Vec<f64> = params
                .iter()
                .cloned()
                .filter(|t| *t > *a0 - 1e-9 && *t < *a1 + 1e-9)
                .collect();
            ts.push(*a0);
            ts.push(*a1);
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let mut subs = Vec::new();
            for w in ts.windows(2) {
                subs.push(SubCurve::Arc {
                    center: *center,
                    radius: *radius,
                    a0: w[0],
                    a1: w[1],
                });
            }
            subs
        }
        InputCurve::Freeform(f2d) => {
            // B2: standalone (no intersection params) → empty; the standalone
            // handler in `arrange` adds the full closed loop.
            if params.is_empty() {
                return Vec::new();
            }
            // B3: closed-curve split with NO wrap — include the closure anchor
            // (t_min = t_max point) as a graph vertex. Split params + both range
            // ends → contiguous sub-ranges [t_min, p0], [p0, p1], …, [pk, t_max].
            // The first sub-curve starts and the last ends at the closure point
            // (same position, deduped in VertSet → valence-2 pass-through vertex).
            let (r0, r1) = f2d.param_range();
            let mut ts: Vec<f64> = params.to_vec();
            ts.push(r0);
            ts.push(r1);
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let mut subs = Vec::new();
            for w in ts.windows(2) {
                if (w[1] - w[0]).abs() > 1e-9 {
                    subs.push(SubCurve::Freeform { f2d: f2d.clone(), t0: w[0], t1: w[1] });
                }
            }
            subs
        }
    }
}

// ─────────────────────────── vertex set (eps dedup) ───────────────────────────

struct VertSet {
    pts: Vec<Vec2>,
    eps: f64,
}
impl VertSet {
    fn new(eps: f64) -> Self {
        Self {
            pts: Vec::new(),
            eps,
        }
    }
    fn add(&mut self, p: Vec2) -> usize {
        for (i, q) in self.pts.iter().enumerate() {
            if p.dist(*q) < self.eps {
                return i;
            }
        }
        self.pts.push(p);
        self.pts.len() - 1
    }
}

// ─────────────────────────── region extraction ───────────────────────────

struct HalfEdge {
    from: usize,
    to: usize,
    twin: usize,
    tang: Vec2,
    /// Signed curvature in the departing direction — tangency tie-break (2026-06-18).
    bend: f64,
    edge: usize,    // EdgeRec index
    forward: bool,  // start→end
}

struct EdgeRec {
    va: usize,
    vb: usize,
    sub: SubCurve,
}

/// 메인 진입점 — 입력 곡선 → bounded arc 경계 면.
pub fn arrange(curves: &[InputCurve], eps: f64) -> Vec<ArrFace> {
    let n = curves.len();
    // 1. 쌍별 교차 → per-curve params.
    let mut params: Vec<Vec<f64>> = vec![Vec::new(); n];
    for i in 0..n {
        for j in (i + 1)..n {
            for (ti, tj, _p) in intersect(&curves[i], &curves[j], eps) {
                params[i].push(ti);
                params[j].push(tj);
            }
            // 4-α: 공선-overlap (직선-직선 만). 교차로 안 잡히는 포개진 변 분할.
            if let (
                InputCurve::Line { a: a0, b: a1 },
                InputCurve::Line { a: b0, b: b1 },
            ) = (&curves[i], &curves[j])
            {
                if let Some((pa, pb)) = collinear_overlap(*a0, *a1, *b0, *b1, eps) {
                    params[i].extend(pa);
                    params[j].extend(pb);
                }
            }
        }
    }
    // 2. 분할 + vert dedup → EdgeRec.
    let mut vs = VertSet::new(eps);
    let mut edges: Vec<EdgeRec> = Vec::new();
    // 4-α: 직선 edge dedup (공선-overlap 으로 양쪽에서 만들어진 동일 조각 병합).
    // **직선만** — arc 는 절대 dedup 안 함 (원-원 lens 는 같은 2 vert 사이 arc 4개!).
    let mut line_pairs: HashSet<(usize, usize)> = HashSet::new();
    for (i, c) in curves.iter().enumerate() {
        for sub in split_curve(c, &params[i]) {
            let va = vs.add(sub.start_pt());
            let vb = vs.add(sub.end_pt());
            if va == vb {
                continue;
            }
            if matches!(sub, SubCurve::Line { .. }) {
                let key = (va.min(vb), va.max(vb));
                if !line_pairs.insert(key) {
                    continue; // 동일 직선 변 중복 → skip
                }
            }
            edges.push(EdgeRec { va, vb, sub });
        }
    }
    // 주의: edges 가 비어도 (standalone 원만) early-return 금지 — 아래 standalone
    // 처리가 disk 면을 만든다.
    let nv = vs.pts.len();
    // 3. half-edge.
    let mut hes: Vec<HalfEdge> = Vec::new();
    for (ei, e) in edges.iter().enumerate() {
        let fwd = hes.len();
        hes.push(HalfEdge {
            from: e.va,
            to: e.vb,
            twin: fwd + 1,
            tang: e.sub.tangent_start(),
            bend: e.sub.signed_curvature(true),
            edge: ei,
            forward: true,
        });
        hes.push(HalfEdge {
            from: e.vb,
            to: e.va,
            twin: fwd,
            tang: e.sub.tangent_end(),
            bend: e.sub.signed_curvature(false),
            edge: ei,
            forward: false,
        });
    }
    // per-vertex departing he, CCW sort.
    let mut depart: Vec<Vec<usize>> = vec![Vec::new(); nv];
    for (i, h) in hes.iter().enumerate() {
        depart[h.from].push(i);
    }
    // Departing CCW order, **seam-robust tangency tie-break (2026-06-18)**: when two
    // half-edges leave a vertex in the same direction (a circle tangent to a rect
    // edge / another curve), `vec_angle(tang)` is equal and they must be sub-ordered
    // by `bend` (signed curvature) — straight (0) between bending-right (−) and
    // bending-left (+), matching the angle a hair along each curve. CRITICAL: the raw
    // atan2 angle has a 0/2π seam — a tangent at +x reads 0° on one half-edge but
    // ~360° on another (float sign of tangent.y). A pairwise wrapped-diff comparator
    // is then NON-transitive and `sort_by` scatters the siblings to opposite ends of
    // the list → `next_of` picks the wrong continuation → the disk loop merges into
    // the outer loop → multi-tangent face loss (inscribed circle, corner pocket).
    // Fix: a VALID total-order key — quantize the angle into integer buckets
    // (seam-wrapped via rem_euclid so 2π ≡ 0) as the primary key, then `bend`. Exact
    // tangent siblings (Δangle ~ float noise ≪ bucket) always share a bucket; the
    // circular order is correct and `next_of`'s modular index handles the seam wrap.
    const TANG_TIE_EPS: f64 = 1e-6;
    let n_buckets = (TWO_PI / TANG_TIE_EPS).round() as i64;
    let angle_bucket = |v: Vec2| -> i64 {
        ((vec_angle(v) / TANG_TIE_EPS).round() as i64).rem_euclid(n_buckets)
    };
    for d in depart.iter_mut() {
        d.sort_by(|&a, &b| {
            angle_bucket(hes[a].tang)
                .cmp(&angle_bucket(hes[b].tang))
                .then_with(|| {
                    hes[a]
                        .bend
                        .partial_cmp(&hes[b].bend)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    }
    // next(h) = h.to 에서 twin 의 CW-rotate (prev in CCW order).
    let next_of = |h: usize, hes: &[HalfEdge], depart: &[Vec<usize>]| -> usize {
        let tw = hes[h].twin;
        let list = &depart[hes[h].to];
        let idx = list.iter().position(|&x| x == tw).unwrap();
        list[(idx + list.len() - 1) % list.len()]
    };
    // cycles.
    let mut visited = vec![false; hes.len()];
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    for start in 0..hes.len() {
        if visited[start] {
            continue;
        }
        let mut cyc = Vec::new();
        let mut h = start;
        loop {
            visited[h] = true;
            cyc.push(h);
            h = next_of(h, &hes, &depart);
            if h == start || visited[h] {
                break;
            }
        }
        cycles.push(cyc);
    }
    // 4. signed area → bounded (positive) outer loops.
    let cycle_area = |cyc: &[usize]| -> f64 {
        let mut poly: Vec<Vec2> = Vec::new();
        for &h in cyc {
            poly.extend(hes[h].edge_samples(&edges));
        }
        signed_area(&poly)
    };
    let mut loops: Vec<Vec<SubCurve>> = Vec::new();
    for cyc in &cycles {
        if cycle_area(cyc) > eps {
            loops.push(cyc.iter().map(|&h| hes[h].oriented_sub(&edges)).collect());
        }
    }
    // standalone 원 (교차 < 2 distinct) → full disk loop (1면). disjoint nested 시
    // nesting 이 부모 면의 hole 로도 배정 (annulus).
    for (i, c) in curves.iter().enumerate() {
        if let InputCurve::Circle { center, radius } = c {
            let mut a: Vec<f64> = params[i].iter().map(|x| norm_angle(*x)).collect();
            a.sort_by(|x, y| x.partial_cmp(y).unwrap());
            a.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            if a.len() < 2 {
                loops.push(vec![SubCurve::Arc {
                    center: *center,
                    radius: *radius,
                    a0: 0.0,
                    a1: TWO_PI,
                }]);
            }
        }
    }
    // standalone freeform (B2) — 교차 param < 2 distinct → full closed loop (1면).
    // Circle 과 동일 의미론 — disjoint nested 시 nest_loops 가 hole 로 배정.
    for (i, c) in curves.iter().enumerate() {
        if let InputCurve::Freeform(f2d) = c {
            let mut p: Vec<f64> = params[i].clone();
            p.sort_by(|x, y| x.partial_cmp(y).unwrap());
            p.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            if p.len() < 2 {
                let (t0, t1) = f2d.param_range();
                loops.push(vec![SubCurve::Freeform { f2d: f2d.clone(), t0, t1 }]);
            }
        }
    }
    // 5. nesting — disjoint 포함 loop 을 부모 면의 hole 로 배정.
    nest_loops(loops, eps)
}

// ─────────────────────────── nesting ───────────────────────────

fn loop_polygon(lp: &[SubCurve]) -> Vec<Vec2> {
    let mut poly = Vec::new();
    for s in lp {
        poly.extend(s.samples(true, 32));
    }
    poly
}

fn poly_centroid(poly: &[Vec2]) -> Vec2 {
    if poly.is_empty() {
        return Vec2::new(0.0, 0.0);
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for p in poly {
        sx += p.x;
        sy += p.y;
    }
    Vec2::new(sx / poly.len() as f64, sy / poly.len() as f64)
}

/// 두 sub-curve 가 같은 edge 의 반대 방향인가 (인접 면 사이 공유 내부 edge).
/// arc 는 endpoints(reversed) + center + radius + **midpoint** 일치 — midpoint 가
/// short/long arc 를 구별해 "같은 endpoint 다른 호" (lens 내부 호 vs 초승달 외곽
/// 호) false-match 를 차단.
fn is_reverse_twin(a: &SubCurve, b: &SubCurve, tol: f64) -> bool {
    if a.start_pt().dist(b.end_pt()) >= tol || a.end_pt().dist(b.start_pt()) >= tol {
        return false;
    }
    match (a, b) {
        (SubCurve::Line { .. }, SubCurve::Line { .. }) => true,
        (
            SubCurve::Arc { center: ca, radius: ra, a0: a0a, a1: a1a },
            SubCurve::Arc { center: cb, radius: rb, a0: a0b, a1: a1b },
        ) => {
            if ca.dist(*cb) >= tol || (ra - rb).abs() >= tol {
                return false;
            }
            let ma = circle_pt(*ca, *ra, (a0a + a1a) * 0.5);
            let mb = circle_pt(*cb, *rb, (a0b + a1b) * 0.5);
            ma.dist(mb) < tol
        }
        _ => false,
    }
}

/// 인접 cluster 의 union 외곽선 = 멤버 loop 들에서 공유 내부 edge (reverse-twin
/// 쌍) 를 소거하고 남은 boundary sub-curve 를 endpoint 로 체인. 정확히 1개 닫힌
/// loop 으로 닫히고 잔여 sub-curve 가 없을 때만 Some (아니면 caller fallback).
fn union_outline(members: &[&Vec<SubCurve>], tol: f64) -> Option<Vec<SubCurve>> {
    let all: Vec<SubCurve> = members.iter().flat_map(|l| l.iter().cloned()).collect();
    let n = all.len();
    let mut canceled = vec![false; n];
    for i in 0..n {
        if canceled[i] {
            continue;
        }
        for j in (i + 1)..n {
            if canceled[j] {
                continue;
            }
            if is_reverse_twin(&all[i], &all[j], tol) {
                canceled[i] = true;
                canceled[j] = true;
                break;
            }
        }
    }
    let kept: Vec<SubCurve> = (0..n).filter(|&i| !canceled[i]).map(|i| all[i].clone()).collect();
    if kept.len() < 2 {
        return None;
    }
    // 체인 (end→start). 정확히 1 loop 으로 닫히고 잔여 없어야 union 외곽선.
    let mut used = vec![false; kept.len()];
    used[0] = true;
    let mut chain = vec![kept[0].clone()];
    let mut cur = kept[0].end_pt();
    let start_pt = kept[0].start_pt();
    loop {
        if cur.dist(start_pt) < tol && chain.len() >= 2 {
            if used.iter().any(|&u| !u) {
                return None; // 잔여 sub-curve → 연결 cluster 아님, fallback
            }
            return Some(chain);
        }
        let mut found = None;
        for k in 0..kept.len() {
            if !used[k] && kept[k].start_pt().dist(cur) < tol {
                found = Some(k);
                break;
            }
        }
        match found {
            Some(k) => {
                used[k] = true;
                cur = kept[k].end_pt();
                chain.push(kept[k].clone());
            }
            None => return None, // 닫히지 않음 → fallback
        }
    }
}

/// disjoint 포함 (loop A 가 더 큰 loop B 안) → A 는 자기 면 + B 의 hole.
/// 교차 arrangement 의 인접 tile 면은 centroid 가 서로 안 들어가 nesting 안 됨.
fn nest_loops(loops: Vec<Vec<SubCurve>>, eps: f64) -> Vec<ArrFace> {
    use super::geom2::{point_in_polygon_even_odd, Pip};
    let polys: Vec<Vec<Vec2>> = loops.iter().map(|l| loop_polygon(l)).collect();
    let areas: Vec<f64> = polys.iter().map(|p| signed_area(p).abs()).collect();
    // 각 loop 의 꼭짓점 (SubCurve 시작점). 인접(정점 공유) 판정용.
    let ends: Vec<Vec<Vec2>> = loops
        .iter()
        .map(|l| l.iter().map(|s| s.start_pt()).collect())
        .collect();
    // 정점 공유 = tiled 인접 (disjoint nesting 아님). 공유 tol = eps×1000 (교차점은 exact
    // 라 ~0, disjoint nested 원/rect 는 수 mm 떨어져 false match 없음).
    let share_tol = eps.max(1e-9) * 1000.0;
    let shares = |i: usize, j: usize| -> bool {
        ends[i]
            .iter()
            .any(|pa| ends[j].iter().any(|pb| pa.dist(*pb) < share_tol))
    };
    let n = loops.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let c = poly_centroid(&polys[i]);
        let mut best: Option<usize> = None;
        for j in 0..n {
            if i == j || areas[j] <= areas[i] {
                continue;
            }
            // 인접 tiled 면 (정점 공유) → hole 아님. centroid 휴리스틱의 concave 오작동 차단.
            if shares(i, j) {
                continue;
            }
            if point_in_polygon_even_odd(c, &polys[j], eps) == Pip::Inside {
                best = match best {
                    Some(b) if areas[b] <= areas[j] => Some(b),
                    _ => Some(j),
                };
            }
        }
        parent[i] = best;
    }
    let mut faces: Vec<ArrFace> = loops
        .iter()
        .map(|l| ArrFace {
            outer: l.clone(),
            holes: Vec::new(),
        })
        .collect();
    // 부모별 children 그룹 (BTreeMap = 결정적 순서, snapshot determinism).
    use std::collections::BTreeMap;
    let mut children_of: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        if let Some(p) = parent[i] {
            children_of.entry(p).or_default().push(i);
        }
    }
    for (p, children) in children_of {
        // children 을 상호 정점공유(=인접 tile)로 cluster (union-find).
        // 인접 cluster → 내부 공유 edge 소거한 union 외곽선 1 hole (rect 안 겹치는
        // 원 = lens + 초승달 → 1 peanut hole). disjoint children → 별개 hole (multi-hole).
        let m = children.len();
        let mut comp: Vec<usize> = (0..m).collect();
        let root = |comp: &Vec<usize>, mut x: usize| {
            while comp[x] != x {
                x = comp[x];
            }
            x
        };
        for a in 0..m {
            for b in (a + 1)..m {
                if shares(children[a], children[b]) {
                    let ra = root(&comp, a);
                    let rb = root(&comp, b);
                    if ra != rb {
                        comp[ra] = rb;
                    }
                }
            }
        }
        let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for a in 0..m {
            clusters.entry(root(&comp, a)).or_default().push(children[a]);
        }
        for (_r, cluster) in clusters {
            if cluster.len() == 1 {
                faces[p].holes.push(loops[cluster[0]].clone());
                continue;
            }
            let members: Vec<&Vec<SubCurve>> = cluster.iter().map(|&c| &loops[c]).collect();
            let member_area: f64 = cluster.iter().map(|&c| areas[c]).sum();
            let merged = union_outline(&members, share_tol).filter(|o| {
                // sanity: union 면적 ≈ 멤버 합 (peanut = lens + 초승달).
                let ua = signed_area(&loop_polygon(o)).abs();
                (ua - member_area).abs() < member_area.max(1.0) * 0.05
            });
            match merged {
                Some(outline) => faces[p].holes.push(outline),
                None => {
                    // fallback: 개별 hole (회귀 없음).
                    for &c in &cluster {
                        faces[p].holes.push(loops[c].clone());
                    }
                }
            }
        }
    }
    faces
}

impl HalfEdge {
    fn edge_samples(&self, edges: &[EdgeRec]) -> Vec<Vec2> {
        edges[self.edge].sub.samples(self.forward, 32)
    }
    /// traversal 방향에 맞춘 SubCurve (forward=그대로, backward=뒤집기).
    fn oriented_sub(&self, edges: &[EdgeRec]) -> SubCurve {
        let s = &edges[self.edge].sub;
        if self.forward {
            s.clone()
        } else {
            match s {
                SubCurve::Line { a, b } => SubCurve::Line { a: *b, b: *a },
                SubCurve::Arc {
                    center,
                    radius,
                    a0,
                    a1,
                } => SubCurve::Arc {
                    center: *center,
                    radius: *radius,
                    a0: *a1,
                    a1: *a0,
                },
                SubCurve::Freeform { f2d, t0, t1 } => SubCurve::Freeform {
                    f2d: f2d.clone(),
                    t0: *t1,
                    t1: *t0,
                },
            }
        }
    }
}

fn signed_area(poly: &[Vec2]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..poly.len() {
        let j = (i + 1) % poly.len();
        s += poly[i].x * poly[j].y - poly[j].x * poly[i].y;
    }
    s * 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn areas(faces: &[ArrFace]) -> Vec<f64> {
        faces
            .iter()
            .map(|f| {
                let mut poly = Vec::new();
                for s in &f.outer {
                    poly.extend(s.samples(true, 48));
                }
                signed_area(&poly)
            })
            .collect()
    }

    /// ADR-280 α — DE-RISK SIM. Does the arrange TILE THE FULL SQUARE when the
    /// solid-top's outer boundary is fed alongside a crossing rect + circle?
    ///
    /// Production bug: `reconstruct_input_curves` excludes the box-top square edges
    /// (they are `volume_edges`, shared with the walls), so the arrange gets only
    /// {rect(4) + circle(1)} = 5 curves → it tiles only rect∪circle, the
    /// square-minus-shapes region is lost → 10 open boundary edges (top opens).
    ///
    /// This sim validates the FIX DIRECTION: feed the square boundary too → the
    /// arrange must tile the WHOLE square (Σ face areas ≈ square area). If it does,
    /// the β fix (feed solid-top outer boundary + materialize with dedup) is sound.
    #[test]
    fn adr280_sim_arrange_tiles_full_square_with_boundary() {
        let sq = 60.0_f64;   // square half-extent → 120×120 = 14400
        let square = vec![
            InputCurve::Line { a: Vec2::new(-sq, -sq), b: Vec2::new(sq, -sq) },
            InputCurve::Line { a: Vec2::new(sq, -sq), b: Vec2::new(sq, sq) },
            InputCurve::Line { a: Vec2::new(sq, sq), b: Vec2::new(-sq, sq) },
            InputCurve::Line { a: Vec2::new(-sq, sq), b: Vec2::new(-sq, -sq) },
        ];
        let circle = InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 40.0 };
        // rect (crossing the circle): corners (-15,-15)..(55,55)
        let rect = vec![
            InputCurve::Line { a: Vec2::new(-15.0, -15.0), b: Vec2::new(55.0, -15.0) },
            InputCurve::Line { a: Vec2::new(55.0, -15.0), b: Vec2::new(55.0, 55.0) },
            InputCurve::Line { a: Vec2::new(55.0, 55.0), b: Vec2::new(-15.0, 55.0) },
            InputCurve::Line { a: Vec2::new(-15.0, 55.0), b: Vec2::new(-15.0, -15.0) },
        ];

        // (A) CURRENT (broken) input — circle + rect only (no square boundary).
        let mut without = vec![circle.clone()];
        without.extend(rect.clone());
        let fa = arrange(&without, 1e-4);
        let ta: f64 = areas(&fa).iter().map(|x| x.abs()).sum();
        println!("[A no-square] faces={} Σarea={:.0} (square=14400)", fa.len(), ta);

        // (B) FIXED input — square boundary + circle + rect.
        // Net tiled area = Σ (outer − holes) — a proper partition sums to 14400.
        let mut with = square.clone();
        with.push(circle.clone());
        with.extend(rect.clone());
        let fb = arrange(&with, 1e-4);
        let net = |f: &ArrFace| -> f64 {
            let mut poly = Vec::new();
            for s in &f.outer { poly.extend(s.samples(true, 48)); }
            let outer = signed_area(&poly).abs();
            let holes: f64 = f.holes.iter().map(|h| {
                let mut hp = Vec::new();
                for s in h { hp.extend(s.samples(true, 48)); }
                signed_area(&hp).abs()
            }).sum();
            outer - holes
        };
        let tb: f64 = fb.iter().map(net).sum();
        let with_holes = fb.iter().filter(|f| !f.holes.is_empty()).count();
        println!("[B with-square] faces={} (with-holes={}) net-tiled-area={:.0} (square=14400)", fb.len(), with_holes, tb);

        // (A) must NOT cover the square (only rect∪circle region).
        assert!(ta < 12000.0, "no-square input tiles only rect∪circle (Σ={ta:.0} < 12000)");
        // (B) with the square boundary, the arrange NET-tiles the FULL SQUARE.
        assert!(
            (tb - 14400.0).abs() < 300.0,
            "ADR-280 fix direction: feeding the square boundary net-tiles the full \
             square (net={tb:.0} ≈ 14400) → arrange handles it, fix is sound"
        );
        assert!(fb.len() >= 4, "square partitioned into ≥4 sub-faces, got {}", fb.len());
    }

    /// Step 1 일반화 — 직선(secant) + 원 → 면 2 (각 chord+arc).
    #[test]
    fn arrange_line_circle_two_faces() {
        let r = 2.0_f64;
        let cy = 0.5_f64;
        let cx = (r * r - cy * cy).sqrt();
        let curves = vec![
            InputCurve::Line {
                a: Vec2::new(-cx - 1.0, cy),
                b: Vec2::new(cx + 1.0, cy),
            },
            InputCurve::Circle {
                center: Vec2::new(0.0, 0.0),
                radius: r,
            },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("line-circle faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "2 bounded faces");
        let total: f64 = a.iter().sum();
        assert!((total - PI * r * r).abs() < 0.05 * PI * r * r, "합=원판: {}", total);
    }

    // ── ADR-200 (A1) InputCurve::Arc feasibility ─────────────────────────

    /// A1 — 부분 arc(반원) + secant chord → **1 면** (half-disk). 부분 arc 가
    /// 다른 곡선과 닫힌 영역을 이루면 정상 면화. (A2 가 보존만 하던 케이스를
    /// arrange 가 직접 면화.)
    #[test]
    fn arrange_arc_plus_chord_one_face() {
        let r = 2.0_f64;
        // right half-arc [-π/2, π/2] (x ≥ 0), endpoints (0,-2),(0,2).
        let curves = vec![
            InputCurve::Arc { center: Vec2::new(0.0, 0.0), radius: r, a0: -PI / 2.0, a1: PI / 2.0 },
            InputCurve::Line { a: Vec2::new(0.0, -3.0), b: Vec2::new(0.0, 3.0) },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("arc+chord faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 1, "half-arc + chord → 1 half-disk face");
        assert!((a[0].abs() - PI * r * r / 2.0).abs() < 0.05 * PI * r * r, "half-disk area: {}", a[0]);
    }

    /// A1 — lone arc (open, 다른 곡선 없음) → **면 0** (self-closing 아님).
    /// Circle 과 결정적 차이: standalone 케이스 없음, DCEL spur 는 면 미형성.
    #[test]
    fn arrange_lone_arc_no_face() {
        let curves = vec![InputCurve::Arc {
            center: Vec2::new(0.0, 0.0),
            radius: 2.0,
            a0: 0.0,
            a1: PI, // upper half, open
        }];
        let faces = arrange(&curves, 1e-7);
        println!("lone arc faces={}", faces.len());
        assert_eq!(faces.len(), 0, "open arc alone forms no face");
    }

    /// A1 — Arc×Line 교차는 arc 각도범위 [a0,a1] 로 클립. 직선이 원을 2점에서
    /// 만나도 arc 위 점만 반환.
    #[test]
    fn intersect_arc_line_clips_to_range() {
        // right half-arc [-π/2, π/2]; horizontal line y=0 crosses circle at
        // (2,0) angle 0 (ON arc) and (-2,0) angle π (OFF arc).
        let arc = InputCurve::Arc { center: Vec2::new(0.0, 0.0), radius: 2.0, a0: -PI / 2.0, a1: PI / 2.0 };
        let line = InputCurve::Line { a: Vec2::new(-3.0, 0.0), b: Vec2::new(3.0, 0.0) };
        let hits = intersect(&arc, &line, 1e-7);
        println!("arc×line hits={:?}", hits);
        assert_eq!(hits.len(), 1, "only the (2,0) crossing is on the right arc");
        assert!((hits[0].2.x - 2.0).abs() < 1e-6 && hits[0].2.y.abs() < 1e-6, "hit at (2,0)");
    }

    /// A1 — Arc×Circle 교차로 arc 가 분할되어 면 형성 (A2 가 못 하던 overlap 재교차).
    /// 반원 arc + 그 위를 가로지르는 작은 원 → 분할된 sub-arc + 원이 면 형성.
    #[test]
    fn arrange_arc_circle_overlap_splits() {
        let r = 2.0_f64;
        // right half-arc + secant chord (closes half-disk) + a small circle
        // overlapping the right region → splits the half-disk into sub-faces.
        let curves = vec![
            InputCurve::Arc { center: Vec2::new(0.0, 0.0), radius: r, a0: -PI / 2.0, a1: PI / 2.0 },
            InputCurve::Line { a: Vec2::new(0.0, -3.0), b: Vec2::new(0.0, 3.0) },
            InputCurve::Circle { center: Vec2::new(1.0, 0.0), radius: 0.8 },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("arc+chord+circle faces={} areas={:?}", faces.len(), a);
        assert!(faces.len() >= 2, "overlapping circle splits the half-disk (≥2 faces), got {}", faces.len());
        // total area conserved ≈ half-disk (the small circle is fully inside).
        let total: f64 = a.iter().map(|x| x.abs()).sum::<f64>();
        assert!(total > PI * r * r / 2.0 * 0.8, "area roughly half-disk, got {}", total);
    }

    /// **Pin-down (2026-06-15)** — stage-3 재현 (사용자 "두 원 반쯤 겹침 + 두 원
    /// 관통 직사각형 → 원이 각진 다이아몬드로 깨짐"). 2 overlapping circles +
    /// 가로 bar(rect 4 lines)를 `arrange` 에 **직접** 먹이면 bar 가 양 원의 rim 을
    /// 잘라 면이 늘어나고 rim 은 arc 로 유지된다. 통과 = arrange 의 Circle×Line /
    /// Arc×Line 분할 (split_curve) 은 **정상** → "사각형이 원 rim 을 안 자른다"
    /// 버그는 arrange (B) 가 아니라 **상류 (reconstruct feed (A) / scope (C))** 에
    /// 있음을 확정. (browser 측정: 2원 lens 3면 → +rect 11면, rim arc 8개 미절단
    /// 520/300mm 동일, fill 1504→100 tris 붕괴.)
    #[test]
    fn arrange_two_circles_plus_bar_splits_rims_pindown() {
        let r = 2.0_f64;
        let ca = Vec2::new(0.0, 0.0);
        let cb = Vec2::new(2.0, 0.0); // centers 1 radius apart → lens overlap
        // stage 2 — 2 circles only (lens).
        let lens = arrange(
            &[
                InputCurve::Circle { center: ca, radius: r },
                InputCurve::Circle { center: cb, radius: r },
            ],
            1e-7,
        );
        println!("2 circles (lens) faces={}", lens.len());

        // stage 3 — + horizontal bar (rect, 4 lines) through both, extending beyond.
        let mut all = vec![
            InputCurve::Circle { center: ca, radius: r },
            InputCurve::Circle { center: cb, radius: r },
        ];
        all.push(InputCurve::Line { a: Vec2::new(-3.0, 0.5), b: Vec2::new(5.0, 0.5) });
        all.push(InputCurve::Line { a: Vec2::new(5.0, 0.5), b: Vec2::new(5.0, -0.5) });
        all.push(InputCurve::Line { a: Vec2::new(5.0, -0.5), b: Vec2::new(-3.0, -0.5) });
        all.push(InputCurve::Line { a: Vec2::new(-3.0, -0.5), b: Vec2::new(-3.0, 0.5) });
        let withbar = arrange(&all, 1e-7);
        println!("2 circles + bar faces={}", withbar.len());

        // arrange CORRECTLY splits: the bar cuts both circle rims → MORE faces.
        assert!(
            withbar.len() > lens.len(),
            "bar must split the circle rims → more faces than lens-only ({} vs {})",
            withbar.len(),
            lens.len()
        );
        // rims stay arc-bounded after the split (sub-arcs present, not polygonized).
        let has_arc = withbar
            .iter()
            .any(|f| f.outer.iter().any(|sc| matches!(sc, SubCurve::Arc { .. })));
        assert!(has_arc, "rim must stay arc-bounded after bar split");
    }

    /// A1 β-4 — Arc×Freeform 교차는 CCI 커널(`lift_to_curve3d(Arc)`=AnalyticCurve
    /// ::Arc, evaluate(t)=각도, parameter_range=[a0,a1])을 거치며 **arc 각도범위로
    /// 클립** (split_curve 각도 규약과 정합). 우측 arc + x=1 freeform(직선 Bezier)
    /// → arc 위 2점, x=-1(좌측) → 0점.
    #[test]
    fn intersect_arc_freeform_clips_to_arc_range() {
        let arc = InputCurve::Arc { center: Vec2::new(0.0, 0.0), radius: 2.0, a0: -PI / 2.0, a1: PI / 2.0 };
        // vertical line x=1 as a degree-1 Bezier → crosses circle at (1,±√3),
        // both on the RIGHT arc (angles ±π/3).
        let right = InputCurve::Freeform(Freeform2D::bezier(vec![Vec2::new(1.0, -3.0), Vec2::new(1.0, 3.0)]));
        let hr = intersect(&arc, &right, 1e-7);
        println!("arc×freeform(right) hits={}", hr.len());
        assert_eq!(hr.len(), 2, "x=1 freeform crosses the right arc at 2 points");
        for (pa, _, p) in &hr {
            assert!(*pa >= -PI / 2.0 - 1e-6 && *pa <= PI / 2.0 + 1e-6, "arc param in [a0,a1]: {}", pa);
            assert!((p.x - 1.0).abs() < 1e-4, "hit on x=1");
        }
        // vertical line x=-1 → crosses circle at (-1,±√3), angles ±2π/3 OFF the
        // right arc → 0 hits (CCI restricts to parameter_range [a0,a1]).
        let left = InputCurve::Freeform(Freeform2D::bezier(vec![Vec2::new(-1.0, -3.0), Vec2::new(-1.0, 3.0)]));
        let hl = intersect(&arc, &left, 1e-7);
        println!("arc×freeform(left) hits={}", hl.len());
        assert_eq!(hl.len(), 0, "x=-1 freeform misses the right arc (off-range)");
    }

    /// A1 β-4 — arc 가 각도 0 을 통과(wrap, a1 > 2π 표현)해도 교차·면화 정확.
    /// arc [7π/4, 9π/4] (315°→45°, x>0 우측) + secant chord → half-ish disk 면.
    #[test]
    fn arrange_arc_wraps_past_zero() {
        let r = 2.0_f64;
        // arc from 315° to 45° (CCW through 0°), endpoints (√2,-√2),(√2,√2).
        let s = std::f64::consts::FRAC_1_SQRT_2 * 2.0; // r/√2
        let curves = vec![
            InputCurve::Arc { center: Vec2::new(0.0, 0.0), radius: r, a0: 7.0 * PI / 4.0, a1: 9.0 * PI / 4.0 },
            // chord closing the right cap: vertical line through both endpoints x=√2.
            InputCurve::Line { a: Vec2::new(s, -3.0), b: Vec2::new(s, 3.0) },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("arc-wrap faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 1, "wrapping arc + chord → 1 cap face");
        // cap area (circular segment beyond x=√2) > 0, < quarter disk.
        assert!(a[0].abs() > 0.0 && a[0].abs() < PI * r * r, "cap area sane: {}", a[0]);
    }

    /// Step 2 일반화 — 원-원 → 면 3 (lens + crescent 2).
    #[test]
    fn arrange_circle_circle_three_faces() {
        let r = 2.0_f64;
        let curves = vec![
            InputCurve::Circle {
                center: Vec2::new(0.0, 0.0),
                radius: r,
            },
            InputCurve::Circle {
                center: Vec2::new(2.0, 0.0),
                radius: r,
            },
        ];
        let faces = arrange(&curves, 1e-7);
        let mut a = areas(&faces);
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        println!("circle-circle faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 3, "lens + 2 crescent");
        let lens = 8.0 * 0.5_f64.acos() - 12.0_f64.sqrt();
        assert!((a[0] - lens).abs() < 0.1 * lens, "lens≈{}: {}", lens, a[0]);
    }

    /// 단일 사각형 (4 line) → 면 1.
    #[test]
    fn arrange_single_rect_one_face() {
        let curves = vec![
            InputCurve::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
            InputCurve::Line { a: Vec2::new(4.0, 0.0), b: Vec2::new(4.0, 4.0) },
            InputCurve::Line { a: Vec2::new(4.0, 4.0), b: Vec2::new(0.0, 4.0) },
            InputCurve::Line { a: Vec2::new(0.0, 4.0), b: Vec2::new(0.0, 0.0) },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("rect faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 1, "1 face");
        assert!((a[0] - 16.0).abs() < 1e-6, "area 16: {}", a[0]);
    }

    /// 사각형 + 원 (겹침) → 면 여러 (직선+arc 혼합 arrangement).
    #[test]
    fn arrange_rect_plus_circle_mixed() {
        let curves = vec![
            InputCurve::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 0.0) },
            InputCurve::Line { a: Vec2::new(4.0, 0.0), b: Vec2::new(4.0, 4.0) },
            InputCurve::Line { a: Vec2::new(4.0, 4.0), b: Vec2::new(0.0, 4.0) },
            InputCurve::Line { a: Vec2::new(0.0, 4.0), b: Vec2::new(0.0, 0.0) },
            // circle centered at a corner area, overlapping the rect boundary.
            InputCurve::Circle { center: Vec2::new(4.0, 2.0), radius: 1.5 },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("rect+circle faces={} areas={:?}", faces.len(), a);
        // 원이 오른쪽 변(x=4)을 가로지름 → rect 가 분할 + 원의 바깥 부분.
        assert!(faces.len() >= 2, "겹침 → 면 분할: {}", faces.len());
        // 모든 면 positive area.
        for &ar in &a {
            assert!(ar > 1e-6, "면 positive area");
        }
        // 교차(tile) → hole 없음.
        for f in &faces {
            assert!(f.holes.is_empty(), "교차 tile 면 hole 없음");
        }
    }

    /// **Secant baseline (2026-06-18)** — 큰 원이 rect 의 **top + bottom 두 변**을
    /// interior 점에서 가로지름 (오른쪽으로 삐져나옴). 부분겹침 2점 교차 →
    /// A∖B / A∩B / B∖A = **3 면**. clean(non-tangent) secant 는 항상 정상이었음 —
    /// tangent-corner 회귀(`arrange_rect_circle_tangent_corners_three_faces`)의 대조군.
    #[test]
    fn arrange_rect_circle_secant_three_faces() {
        let mut curves = rect_curves((-4.0, -3.0), (4.0, 3.0));
        curves.push(InputCurve::Circle { center: Vec2::new(3.0, 0.0), radius: 4.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("DIAG faces={} areas={:?}", faces.len(), a);
        for (i, f) in faces.iter().enumerate() {
            let kinds: Vec<&str> = f
                .outer
                .iter()
                .map(|s| match s {
                    SubCurve::Line { .. } => "L",
                    SubCurve::Arc { .. } => "A",
                    SubCurve::Freeform { .. } => "F",
                })
                .collect();
            println!("  face {}: outer={:?} holes={}", i, kinds, f.holes.len());
        }
        assert_eq!(faces.len(), 3, "rect + 원(top·bottom 교차) → 3 sub-face");
        for &ar in &a {
            assert!(ar > 1e-6, "면 positive area: {}", ar);
        }
    }

    /// **Tangency degeneracy fix (2026-06-18)** — 원이 rect 의 **두 모서리(corner)**
    /// 를 정확히 통과 + 인접 변에 **접(tangent)**. center(4,0) r3 → 모서리 (4,±3)
    /// 통과, top/bottom 변(y=±3)에 접. 그 꼭짓점에서 top 변과 좌측 호의 departing
    /// tangent 가 같아 cycle walk 가 엉켜 **1 degenerate 면**이던 것을 `bend`(signed
    /// curvature) tie-break 로 수정. 정답 = 3면 (rect∖circle / rect∩circle / circle∖rect).
    #[test]
    fn arrange_rect_circle_tangent_corners_three_faces() {
        let mut curves = rect_curves((-4.0, -3.0), (4.0, 3.0));
        curves.push(InputCurve::Circle { center: Vec2::new(4.0, 0.0), radius: 3.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("DIAG-CORNER faces={} areas={:?}", faces.len(), a);
        for (i, f) in faces.iter().enumerate() {
            let kinds: Vec<&str> = f
                .outer
                .iter()
                .map(|s| match s {
                    SubCurve::Line { .. } => "L",
                    SubCurve::Arc { .. } => "A",
                    SubCurve::Freeform { .. } => "F",
                })
                .collect();
            println!("  face {}: outer={:?} holes={}", i, kinds, f.holes.len());
        }
        assert_eq!(faces.len(), 3, "원이 모서리 통과해도 3 sub-face (got {})", faces.len());
    }

    /// **Transversal-corner robustness (2026-06-18)** — 원이 rect 모서리 (4,3) 를
    /// **transversal**(접 아님) 통과 + top/right 변 interior 도 교차. center(2,1)
    /// r=2√2 → (0,3),(4,-1) interior + (4,3) 모서리. tangent 가 모두 distinct 이므로
    /// 기존 sort 로 처리 (bend tie-break 무관) → collapse 없음. 면 ≥ 3, 모두 positive.
    #[test]
    fn arrange_rect_circle_through_corner_transversal() {
        let mut curves = rect_curves((-4.0, -3.0), (4.0, 3.0));
        curves.push(InputCurve::Circle {
            center: Vec2::new(2.0, 1.0),
            radius: (8.0_f64).sqrt(),
        });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("DIAG-TRANSVERSAL faces={} areas={:?}", faces.len(), a);
        assert!(faces.len() >= 3, "모서리 transversal 통과 → collapse 없음 (got {})", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "면 positive area: {}", ar);
        }
    }

    // ── Pin-down (2026-06-18): cardinal/tangent 면-소실 재현 (deepen-ours step 0) ──
    // 사용자 보고 "원과 사각형이 만날 때 면이 없어지는 문제". derived cycle-walk
    // (next_of, leftmost-turn)가 접점/cardinal 정렬에서 취약. 아래 4 케이스를
    // arrange(B, 분석면 경로)에 직접 먹여 현 실패를 못박는다. 통과 = robust,
    // 실패(면 collapse) = exhaustive-walk 심화(step 1) 필요 지점.

    /// (C0-a) 원이 rect **한 변에 안쪽에서 접** (axis-aligned). center(0,-2) r3 →
    /// bottom 변 y=-5 에 (0,-5)에서 접, 나머지는 rect 내부. 정답 = 2면
    /// (rect∖disk + disk, 접점 1 vertex pinch). collapse(1면) = 버그.
    #[test]
    fn pindown_rect_circle_tangent_inside_edge() {
        let mut curves = rect_curves((-5.0, -5.0), (5.0, 5.0));
        curves.push(InputCurve::Circle { center: Vec2::new(0.0, -2.0), radius: 3.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        let holed: usize = faces.iter().filter(|f| !f.holes.is_empty()).count();
        println!("PIN tangent-inside faces={} areas={:?} holed={}", faces.len(), a, holed);
        // areas()=outer only (hole 미차감) → 면적 합 검증 대신 nesting 검증.
        assert_eq!(faces.len(), 2, "안쪽 접 → 2면 (rect-with-hole + disk), got {}", faces.len());
        assert_eq!(holed, 1, "rect 가 disk 를 hole 로 가져야 (got {} holed)", holed);
    }

    /// (C0-b) 원 **중심이 rect 변 위** (cardinal 정렬). center(0,-5) r3 → bottom 변
    /// y=-5 를 (±3,-5)에서 가로지름, 상반원 내부 / 하반원 외부. 정답 = 3면
    /// (rect∖상반원 / 상반원 / 하반원). 진단 failure mode (b).
    #[test]
    fn pindown_rect_circle_center_on_edge() {
        let mut curves = rect_curves((-5.0, -5.0), (5.0, 5.0));
        curves.push(InputCurve::Circle { center: Vec2::new(0.0, -5.0), radius: 3.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PIN center-on-edge faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 3, "중심이 변 위 → 3면 (rect∖상반원/상반원/하반원), got {}", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "면 positive area: {}", ar);
        }
    }

    /// (C0-c) 원이 rect **한 변에 바깥에서 접** (external pinch). center(0,-9) r4 →
    /// bottom 변 y=-5 에 (0,-5)에서 접, 원은 rect 밖. 정답 = 2면 (rect + disk,
    /// 접점 1 vertex 공유). collapse / 면 흡수 = 버그.
    #[test]
    fn pindown_rect_circle_tangent_outside_edge() {
        let mut curves = rect_curves((-5.0, -5.0), (5.0, 5.0));
        curves.push(InputCurve::Circle { center: Vec2::new(0.0, -9.0), radius: 4.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PIN tangent-outside faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "바깥 접 → 2면 (rect + disk), got {}", faces.len());
        let total: f64 = a.iter().map(|x| x.abs()).sum();
        assert!((total - (100.0 + PI * 16.0)).abs() < 0.5, "총=rect+disk, got {}", total);
    }

    /// (C0-d) **내접원** — center(0,0) r5, rect ±5 → 4 변 모두에 접 (±5,0),(0,±5).
    /// 가장 degenerate cardinal (4 동시 접). 정답 = 5면 (disk + 4 corner).
    /// 4 동시 tangency → cycle-walk 가장 취약. collapse = step 1 필요 강한 증거.
    #[test]
    fn pindown_rect_circle_inscribed_four_tangent() {
        let mut curves = rect_curves((-5.0, -5.0), (5.0, 5.0));
        curves.push(InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 5.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PIN inscribed-4tangent faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 5, "내접원 → 5면 (disk + 4 corner), got {}", faces.len());
        let total: f64 = a.iter().map(|x| x.abs()).sum();
        assert!((total - 100.0).abs() < 0.5, "총=rect(100), got {}", total);
    }

    /// (C0-e) **코너 포켓** — 원이 인접 두 변에 접 (corner 안쪽). rect(0,0)-(10,10),
    /// center(3,3) r3 → left x=0 에 (0,3), bottom y=0 에 (3,0) 접. 2 접점이 코너를
    /// **핀치로 분리** → 정답 = 3 tile: corner-pocket(1.93) + main(69.83) + disk(28.24),
    /// 합 = rect(100). (seam-robust tie-break 전: 1.93 미세 코너만 남고 붕괴.)
    #[test]
    fn pindown_rect_circle_corner_pocket_two_tangent() {
        let mut curves = rect_curves((0.0, 0.0), (10.0, 10.0));
        curves.push(InputCurve::Circle { center: Vec2::new(3.0, 3.0), radius: 3.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PIN corner-pocket faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 3, "코너 포켓 2접 → 3 tile (corner+main+disk), got {}", faces.len());
        let total: f64 = a.iter().map(|x| x.abs()).sum();
        assert!((total - 100.0).abs() < 0.5, "합=rect(100), got {}", total);
        for &ar in &a {
            assert!(ar > 1e-6, "면 positive area: {}", ar);
        }
    }

    /// (C0-f) **rect 가 원 안에** — center(0,0) r6, rect ±2 (완전 내부). 정답 = 2면
    /// (rect interior + ring=원∖rect, ring 은 rect 를 hole 로). disjoint nesting.
    #[test]
    fn pindown_rect_inside_circle() {
        let mut curves = rect_curves((-2.0, -2.0), (2.0, 2.0));
        curves.push(InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 6.0 });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        let holed: usize = faces.iter().filter(|f| !f.holes.is_empty()).count();
        println!("PIN rect-inside-circle faces={} areas={:?} holed={}", faces.len(), a, holed);
        assert_eq!(faces.len(), 2, "rect 내부 → 2면 (rect + ring), got {}", faces.len());
        assert_eq!(holed, 1, "원판이 rect 를 hole 로 (got {} holed)", holed);
    }

    // ── Structural stress probe (2026-06-18): deepen-ours step 2 — surgical fix 이후
    // 남은 robustness 경계 측정. 통과 = engine 이미 robust (구조적 포팅 불필요,
    // truth-over-estimate), 실패 = 구조적 심화 (T-junction 수렴 / robust predicate)
    // 의 concrete 타겟. ──

    /// (S1) 원-원 **외접** (external tangent). c(0,0)r2 + c(4,0)r2, (2,0)에서 접.
    /// 정답 = 2 disk (접점 1 vertex 공유). 곡선-곡선 접점 robustness.
    #[test]
    fn probe_two_circles_external_tangent() {
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 2.0 },
            InputCurve::Circle { center: Vec2::new(4.0, 0.0), radius: 2.0 },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE 2circ-ext-tangent faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "외접 2원 → 2 disk, got {}", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "positive: {}", ar);
        }
    }

    /// (S2) 원-원 **내접** (internal tangent). c(0,0)r3 + c(1,0)r2, (3,0)에서 접.
    /// 정답 = 2면 (outer crescent pinched + inner disk).
    #[test]
    fn probe_two_circles_internal_tangent() {
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 3.0 },
            InputCurve::Circle { center: Vec2::new(1.0, 0.0), radius: 2.0 },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE 2circ-int-tangent faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "내접 2원 → 2면, got {}", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "positive: {}", ar);
        }
    }

    /// (S3) **3원 Venn** (mutual overlap). 모두 쌍별 겹침 (일반 위치). 다수 region.
    #[test]
    fn probe_three_circles_venn() {
        let r = 2.0_f64;
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: r },
            InputCurve::Circle { center: Vec2::new(2.0, 0.0), radius: r },
            InputCurve::Circle { center: Vec2::new(1.0, 1.732), radius: r },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE 3circ-venn faces={} areas={:?}", faces.len(), a);
        assert!(faces.len() >= 6, "Venn 3원 → 다수 region (≥6), got {}", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "positive: {}", ar);
        }
    }

    /// (S4) **T-junction**: line 끝점이 원 rim 위. c(0,0)r2 + line (2,0)[rim]→(4,0)[밖].
    /// line = spur(dangling) → 면 미형성. 정답 = 1 disk (endpoint-on-curve dedup).
    #[test]
    fn probe_line_endpoint_on_circle_rim() {
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 2.0 },
            InputCurve::Line { a: Vec2::new(2.0, 0.0), b: Vec2::new(4.0, 0.0) },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE line-on-rim faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 1, "rim spur line → 1 disk, got {}", faces.len());
        assert!((a[0].abs() - PI * 4.0).abs() < 0.2, "disk area 4π: {}", a[0]);
    }

    /// (S5) **near-tangent secant**: line y=1.999 가 원 top(y=2)에 거의 접. 두 crossing
    /// 매우 가까움(±0.063). near-degenerate predicate. 정답 = 2면 (tiny cap + 나머지).
    #[test]
    fn probe_near_tangent_secant() {
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 2.0 },
            InputCurve::Line { a: Vec2::new(-3.0, 1.999), b: Vec2::new(3.0, 1.999) },
        ];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE near-tangent faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "near-tangent secant → 2면, got {}", faces.len());
        for &ar in &a {
            assert!(ar > 1e-6, "positive: {}", ar);
        }
    }

    /// (S6) **3-way concurrent**: square + diagonal (corner 에서 2변+대각선 = 3-way).
    /// rect(0,0)-(4,4) + diag (0,0)→(4,4). 정답 = 2 triangle (각 area 8).
    #[test]
    fn probe_square_diagonal_concurrent() {
        let mut curves = rect_curves((0.0, 0.0), (4.0, 4.0));
        curves.push(InputCurve::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(4.0, 4.0) });
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("PROBE square-diag faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 2, "square+diag → 2 triangle, got {}", faces.len());
        for &ar in &a {
            assert!((ar.abs() - 8.0).abs() < 0.1, "각 삼각형 area 8: {}", ar);
        }
    }

    /// **Simulation sweep (2026-06-18)** — circle ⊆ rect 연속체 전수 area-conservation.
    /// rect(0,0)-(10,10) area=100. center grid × radius(변까지=접 포함)를 쓸어 각
    /// arrangement 의 tiled area(Σ outer − Σ hole) = 100 검증. 접/cardinal/corner-
    /// pocket/내접/containment 경계와 그 주변 전부 샘플 → 면 소실이 어디서든 area
    /// 결손/중복으로 검출. (12 핀포인트 테스트를 넘어 연속체 robustness 증명.)
    #[test]
    fn sim_circle_in_rect_area_conservation_sweep() {
        fn poly_area(loop_: &[SubCurve]) -> f64 {
            let mut p = Vec::new();
            for s in loop_ {
                p.extend(s.samples(true, 64));
            }
            signed_area(&p).abs()
        }
        fn tiled(faces: &[ArrFace]) -> f64 {
            faces
                .iter()
                .map(|f| poly_area(&f.outer) - f.holes.iter().map(|h| poly_area(h)).sum::<f64>())
                .sum()
        }
        let coords: [f64; 7] = [2.0, 2.5, 3.0, 3.5, 4.0, 4.5, 5.0];
        let mut checked = 0usize;
        let mut worst = 0.0_f64;
        let mut worst_at = (0.0, 0.0, 0.0);
        for &cx in &coords {
            for &cy in &coords {
                let max_r = cx.min(cy).min(10.0 - cx).min(10.0 - cy);
                let mut r = 0.5;
                while r <= max_r + 1e-9 {
                    let mut curves = rect_curves((0.0, 0.0), (10.0, 10.0));
                    curves.push(InputCurve::Circle { center: Vec2::new(cx, cy), radius: r });
                    let faces = arrange(&curves, 1e-7);
                    let t = tiled(&faces);
                    let err = (t - 100.0).abs();
                    if err > worst {
                        worst = err;
                        worst_at = (cx, cy, r);
                    }
                    assert!(
                        err < 0.6,
                        "면 소실/중복: center=({},{}) r={} tiled={:.3} (≠100) faces={}",
                        cx, cy, r, t, faces.len()
                    );
                    for f in &faces {
                        assert!(
                            poly_area(&f.outer) > 1e-7,
                            "degenerate face at ({},{}) r={}",
                            cx, cy, r
                        );
                    }
                    checked += 1;
                    r += 0.5;
                }
            }
        }
        println!(
            "SIM swept {} (circle⊆rect) area-conservation, worst err={:.4} at {:?}",
            checked, worst, worst_at
        );
        assert!(checked > 100, "충분히 쓸었나: {}", checked);
    }

    /// **Simulation sweep — partial overlap (2026-06-18)** — 원이 rect 변을 가로질러
    /// 삐져나오는 부분 겹침 연속체. 불변식: rect 를 가르는 arrangement 의 면 중
    /// **centroid 가 rect 내부인 면들의 tiled 합 = 100** (rect 는 항상 정확히 타일
    /// 됨; 원이 변을 안 자르는 버그 / 내부 면 소실 시 합 < 100). 원이 rect 를
    /// 포함하지 않는 partial 만 (contain 시 ring centroid 오분류 회피).
    #[test]
    fn sim_circle_partial_overlap_rect_tiling_sweep() {
        fn poly_area(loop_: &[SubCurve]) -> f64 {
            let mut p = Vec::new();
            for s in loop_ {
                p.extend(s.samples(true, 64));
            }
            signed_area(&p).abs()
        }
        fn outer_centroid(f: &ArrFace) -> Vec2 {
            let mut p = Vec::new();
            for s in &f.outer {
                p.extend(s.samples(true, 32));
            }
            poly_centroid(&p)
        }
        // 원이 rect 경계를 가로지르되 rect 를 포함하지 않는 center×r 그리드.
        let cfgs: &[(f64, f64, f64)] = &[
            (10.0, 5.0, 2.0), (10.0, 5.0, 3.0), (9.0, 5.0, 2.0), (11.0, 5.0, 2.0),
            (5.0, 10.0, 2.0), (5.0, 10.0, 3.0), (5.0, 9.0, 2.0), (5.0, 11.0, 2.0),
            (10.0, 10.0, 2.0), (10.0, 10.0, 3.0), (10.0, 10.0, 4.0), (0.0, 0.0, 3.0),
            (0.0, 5.0, 2.5), (10.0, 2.0, 2.5), (2.0, 10.0, 2.5), (8.0, 10.0, 3.0),
        ];
        let mut worst = 0.0_f64;
        let mut worst_at = (0.0, 0.0, 0.0);
        for &(cx, cy, r) in cfgs {
            let mut curves = rect_curves((0.0, 0.0), (10.0, 10.0));
            curves.push(InputCurve::Circle { center: Vec2::new(cx, cy), radius: r });
            let faces = arrange(&curves, 1e-7);
            // rect 내부 면 (centroid ∈ (0,10)²) tiled 합.
            let inside: f64 = faces
                .iter()
                .filter(|f| {
                    let c = outer_centroid(f);
                    c.x > 0.0 && c.x < 10.0 && c.y > 0.0 && c.y < 10.0
                })
                .map(|f| poly_area(&f.outer) - f.holes.iter().map(|h| poly_area(h)).sum::<f64>())
                .sum();
            let err = (inside - 100.0).abs();
            if err > worst {
                worst = err;
                worst_at = (cx, cy, r);
            }
            assert!(
                err < 0.6,
                "rect 내부 면 합 ≠ 100 (면 소실/미절단): center=({},{}) r={} inside={:.3} faces={}",
                cx, cy, r, inside, faces.len()
            );
            for f in &faces {
                assert!(poly_area(&f.outer) > 1e-7, "degenerate at ({},{}) r={}", cx, cy, r);
            }
        }
        println!(
            "SIM partial-overlap {} cfgs, rect-tiling worst err={:.4} at {:?}",
            cfgs.len(), worst, worst_at
        );
    }

    /// **Axis-A probe (2026-06-18)** — 좌표 정밀도 robust-predicate 트랙의 concrete
    /// 타겟 탐색. arrange 는 절대 eps (denom<eps² / dist<eps) + naive signed_area
    /// 를 쓰므로 city-scale 좌표에서 degrade 가능. corner-pocket / inscribed 를
    /// ×1e0..1e6 으로 키워 면적 보존(rel_err) 측정. 통과 = scale-robust (Axis-A
    /// 불요), 실패 = scale-aware tolerance 필요 (Axis-A 타겟). 탐색용: 너그러운
    /// bound (rel_err < 1%) — 깨지는 scale 을 println 으로 노출.
    #[test]
    fn probe_axis_a_large_scale_area_conservation() {
        fn poly_area(loop_: &[SubCurve]) -> f64 {
            let mut p = Vec::new();
            for s in loop_ {
                p.extend(s.samples(true, 64));
            }
            signed_area(&p).abs()
        }
        fn tiled(faces: &[ArrFace]) -> f64 {
            faces
                .iter()
                .map(|f| poly_area(&f.outer) - f.holes.iter().map(|h| poly_area(h)).sum::<f64>())
                .sum()
        }
        // (cx, cy, r, expected_faces) at unit scale.
        let cfgs: &[(f64, f64, f64, usize)] = &[(3.5, 3.5, 3.5, 3), (5.0, 5.0, 5.0, 5)];
        for &scale in &[1.0_f64, 1e2, 1e3, 1e4, 1e5, 1e6] {
            for &(cx, cy, r, exp) in cfgs {
                let mut curves = rect_curves((0.0, 0.0), (10.0 * scale, 10.0 * scale));
                curves.push(InputCurve::Circle {
                    center: Vec2::new(cx * scale, cy * scale),
                    radius: r * scale,
                });
                let faces = arrange(&curves, 1e-7);
                let t = tiled(&faces);
                let expected = 100.0 * scale * scale;
                let rel = (t - expected).abs() / expected;
                println!(
                    "AXIS-A scale={:>7.0e} cfg=({},{},{}) faces={}/{} rel_err={:.2e}",
                    scale, cx, cy, r, faces.len(), exp, rel
                );
                assert!(
                    rel < 1e-2 && faces.len() == exp,
                    "scale={:e} cfg=({},{},{}) faces={}/{} rel_err={:.2e} → Axis-A 타겟",
                    scale, cx, cy, r, faces.len(), exp, rel
                );
            }
        }
    }

    /// **Axis-A offset probe (2026-06-18)** — naive signed_area 의 catastrophic
    /// cancellation frontier 특성화. near-tangent secant (cap area ~8.4e-5 mm²) 를
    /// 원점에서 멀리 옮겨 (offset 0..1e6), 좌표 ~offset 의 shoelace 가 작은 cap 을
    /// sign-flip 으로 잃는지 측정. cap 유지 = 2 면, 소실 = 1 면. 탐색용 (assert 없음,
    /// frontier println). 8.4e-5 mm² @ 1km offset 은 비현실적 corner — robust
    /// signed_area 가치가 arrange 에 있는지 판단용.
    #[test]
    fn probe_axis_a_near_tangent_at_offset() {
        for &off in &[0.0_f64, 1e2, 1e3, 1e4, 1e5, 1e6] {
            let curves = vec![
                InputCurve::Circle { center: Vec2::new(off, off), radius: 2.0 },
                InputCurve::Line {
                    a: Vec2::new(off - 3.0, off + 1.999),
                    b: Vec2::new(off + 3.0, off + 1.999),
                },
            ];
            let faces = arrange(&curves, 1e-7);
            let min_area = faces
                .iter()
                .map(|f| {
                    let mut p = Vec::new();
                    for s in &f.outer {
                        p.extend(s.samples(true, 64));
                    }
                    signed_area(&p).abs()
                })
                .fold(f64::INFINITY, f64::min);
            println!(
                "AXIS-A-OFFSET off={:>7.0e} faces={} min_face_area={:.3e}",
                off, faces.len(), min_area
            );
            // topology 보존 = 면 소실 0 (cap 항상 유지). area VALUE 는 1e6 offset 에서
            // shoelace cancellation 으로 열화(8.4e-5→3e-4)하나 eps floor(1e-7) 위라
            // 면 합성 무영향 → robust signed_area 가 arrange 엔 불요 (truth over estimate).
            assert_eq!(faces.len(), 2, "off={:e}: near-tangent cap 면 소실 (faces≠2)", off);
        }
    }

    /// **2026-06-03** — rect 안 **겹치는** 원 2개 → rect 에 **1 union hole**
    /// (peanut 외곽선) + lens + 초승달 2. 인접 면이 각각 hole(3개) 되는 버그 차단.
    #[test]
    fn arrange_rect_two_overlapping_circles_one_union_hole() {
        let mut curves = rect_curves((0.0, 0.0), (12.0, 8.0));
        curves.push(InputCurve::Circle { center: Vec2::new(5.0, 4.0), radius: 2.2 });
        curves.push(InputCurve::Circle { center: Vec2::new(7.0, 4.0), radius: 2.2 });
        let faces = arrange(&curves, 1e-7);
        assert_eq!(faces.len(), 4, "rect + lens + 초승달2 = 4면: {}", faces.len());
        // hole 정확히 1개 (rect 의 peanut union hole) — 개별 3개 아님.
        let total_holes: usize = faces.iter().map(|f| f.holes.len()).sum();
        assert_eq!(total_holes, 1, "rect union hole 1개 (개별 3 버그 차단): {}", total_holes);
        // hole 보유 면 = rect (outer 4 line), hole = 2 arc (peanut 외곽선).
        let holed = faces.iter().find(|f| !f.holes.is_empty()).unwrap();
        assert_eq!(holed.outer.len(), 4, "hole 면은 rect (4 line)");
        let hole = &holed.holes[0];
        assert_eq!(hole.len(), 2, "peanut hole = 2 sub-curve: {}", hole.len());
        assert_eq!(
            hole.iter().filter(|s| matches!(s, SubCurve::Arc { .. })).count(),
            2,
            "peanut hole 둘 다 arc"
        );
    }

    /// **2026-06-03** — rect 안 **disjoint** 원 2개 → rect 에 **별개 hole 2개**
    /// (multi-hole 보존, union 병합 안 함). 겹침 케이스와 구별.
    #[test]
    fn arrange_rect_two_disjoint_circles_two_holes() {
        let mut curves = rect_curves((0.0, 0.0), (20.0, 8.0));
        curves.push(InputCurve::Circle { center: Vec2::new(5.0, 4.0), radius: 2.0 });
        curves.push(InputCurve::Circle { center: Vec2::new(15.0, 4.0), radius: 2.0 });
        let faces = arrange(&curves, 1e-7);
        assert_eq!(faces.len(), 3, "rect + 2 disk = 3면: {}", faces.len());
        let total_holes: usize = faces.iter().map(|f| f.holes.len()).sum();
        assert_eq!(total_holes, 2, "disjoint → 별개 hole 2개: {}", total_holes);
        let max_holes = faces.iter().map(|f| f.holes.len()).max().unwrap();
        assert_eq!(max_holes, 2, "rect 한 면이 hole 2개 (multi-hole): {}", max_holes);
    }

    /// standalone 원 (교차 없음) → 면 1 (disk).
    #[test]
    fn arrange_standalone_circle_one_disk() {
        let r = 2.0_f64;
        let curves = vec![InputCurve::Circle {
            center: Vec2::new(0.0, 0.0),
            radius: r,
        }];
        let faces = arrange(&curves, 1e-7);
        let a = areas(&faces);
        println!("standalone circle faces={} areas={:?}", faces.len(), a);
        assert_eq!(faces.len(), 1, "disk 1면");
        assert!((a[0].abs() - PI * r * r).abs() < 0.05 * PI * r * r, "원판 면적");
        assert!(faces[0].holes.is_empty(), "hole 없음");
    }

    /// **B2 PROOF (ADR-186 A3 / Option B)** — standalone closed Bezier →
    /// 1 disk face with a single Freeform boundary sub-curve (no holes).
    /// arrange 가 freeform variant 를 standalone 으로 처리함을 증명.
    #[test]
    fn arrange_standalone_closed_bezier_one_disk() {
        let f = Freeform2D::bezier(vec![
            Vec2::new(5.0, 0.0),
            Vec2::new(12.0, 5.0),
            Vec2::new(5.0, 10.0),
            Vec2::new(-2.0, 5.0),
            Vec2::new(5.0, 0.0),
        ]);
        let curves = vec![InputCurve::Freeform(f)];
        let faces = arrange(&curves, 1e-7);
        assert_eq!(faces.len(), 1, "standalone closed bezier → 1 disk: {}", faces.len());
        assert!(faces[0].holes.is_empty(), "hole 없음");
        assert_eq!(faces[0].outer.len(), 1, "outer = 1 freeform sub-curve");
        assert!(
            matches!(faces[0].outer[0], SubCurve::Freeform { .. }),
            "outer 는 Freeform sub-curve"
        );
        let a = areas(&faces)[0].abs();
        assert!(a > 5.0 && a < 140.0, "enclosed area 합리적 (bbox 14×10 내): {}", a);
    }

    /// **B2 PROOF** — standalone closed BSpline → 1 disk face (Freeform boundary).
    #[test]
    fn arrange_standalone_closed_bspline_one_disk() {
        // Clamped cubic, closed (cp[0] ≈ cp[last]).
        let f = Freeform2D::bspline(
            vec![
                Vec2::new(5.0, 0.0),
                Vec2::new(11.0, 3.0),
                Vec2::new(9.0, 9.0),
                Vec2::new(1.0, 9.0),
                Vec2::new(-1.0, 3.0),
                Vec2::new(5.0, 0.0),
            ],
            vec![0.0, 0.0, 0.0, 0.0, 0.33, 0.66, 1.0, 1.0, 1.0, 1.0],
            3,
        );
        let curves = vec![InputCurve::Freeform(f)];
        let faces = arrange(&curves, 1e-7);
        assert_eq!(faces.len(), 1, "standalone closed bspline → 1 disk: {}", faces.len());
        assert!(faces[0].holes.is_empty(), "hole 없음");
        assert!(matches!(faces[0].outer[0], SubCurve::Freeform { .. }), "outer Freeform");
        let a = areas(&faces)[0].abs();
        assert!(a > 5.0, "enclosed area positive: {}", a);
    }

    /// **B3 PROOF (ADR-186 A3 / Option B core)** — 2 overlapping closed Beziers
    /// → 3 faces (lens + 2 crescents) via CCI freeform-freeform intersection.
    /// B1 split + B2 variant + B3 CCI 결합 = freeform lens 의 핵심 증명.
    #[test]
    fn arrange_two_overlapping_beziers_three_faces() {
        let blob = |cx: f64| {
            Freeform2D::bezier(vec![
                Vec2::new(cx, 0.0),
                Vec2::new(cx + 7.0, 5.0),
                Vec2::new(cx, 10.0),
                Vec2::new(cx - 7.0, 5.0),
                Vec2::new(cx, 0.0),
            ])
        };
        let curves = vec![
            InputCurve::Freeform(blob(5.0)),
            InputCurve::Freeform(blob(8.0)),
        ];
        let faces = arrange(&curves, 1e-4);
        let mut a: Vec<f64> = areas(&faces).iter().map(|x| x.abs()).collect();
        a.sort_by(|x, y| x.partial_cmp(y).unwrap());
        println!(
            "2-bezier-overlap faces={} areas(sorted)={:?}",
            faces.len(),
            a.iter().map(|x| (x * 10.0).round() / 10.0).collect::<Vec<_>>()
        );
        assert_eq!(faces.len(), 3, "lens + crescent 2 = 3면: {}", faces.len());
        for f in &faces {
            assert!(f.holes.is_empty(), "교차 tile → hole 없음");
            assert!(
                f.outer.iter().all(|s| matches!(s, SubCurve::Freeform { .. })),
                "모든 경계가 freeform sub-curve"
            );
        }
        assert!(a[0] > 1.0, "lens positive area: {}", a[0]);
        // lens(최소) < crescent(나머지 둘).
        assert!(a[0] <= a[1] && a[0] <= a[2], "lens 가 최소 면적");
    }

    fn rect_curves(lo: (f64, f64), hi: (f64, f64)) -> Vec<InputCurve> {
        let (x0, y0) = lo;
        let (x1, y1) = hi;
        vec![
            InputCurve::Line { a: Vec2::new(x0, y0), b: Vec2::new(x1, y0) },
            InputCurve::Line { a: Vec2::new(x1, y0), b: Vec2::new(x1, y1) },
            InputCurve::Line { a: Vec2::new(x1, y1), b: Vec2::new(x0, y1) },
            InputCurve::Line { a: Vec2::new(x0, y1), b: Vec2::new(x0, y0) },
        ]
    }

    /// **4-α 회귀 (2026-06-02)** — collinear-overlap 직선 처리. (probe 로 범위 확정 후 정답 lock.)
    #[test]
    fn arrange_collinear_overlap_lines() {
        let eps = 1e-7;
        let sum = |f: &[ArrFace]| -> f64 { areas(f).iter().sum() };

        // 1 — 인접 rect (변 x=4 완전 공유) → 2면 (각 16).
        let mut c1 = rect_curves((0.0, 0.0), (4.0, 4.0));
        c1.extend(rect_curves((4.0, 0.0), (8.0, 4.0)));
        let f1 = arrange(&c1, eps);
        assert_eq!(f1.len(), 2, "인접 rect → 2면 (변 공유): {:?}", areas(&f1));
        assert!((sum(&f1) - 32.0).abs() < 1e-6, "합 32: {}", sum(&f1));

        // 2 — 부분겹침 + 변 공선중첩 (ADR-101) → 3면 (left/overlap/right, 각 8, 합 24=union).
        let mut c2 = rect_curves((0.0, 0.0), (4.0, 4.0));
        c2.extend(rect_curves((2.0, 0.0), (6.0, 4.0)));
        let f2 = arrange(&c2, eps);
        assert_eq!(f2.len(), 3, "공선 부분겹침 → 3면: {:?}", areas(&f2));
        assert!((sum(&f2) - 24.0).abs() < 1e-6, "합 24(union): {}", sum(&f2));

        // 3 — 비공선 부분겹침 (대조군) → 3면 (합 28).
        let mut c3 = rect_curves((0.0, 0.0), (4.0, 4.0));
        c3.extend(rect_curves((2.0, 2.0), (6.0, 6.0)));
        let f3 = arrange(&c3, eps);
        assert_eq!(f3.len(), 3, "비공선 겹침 → 3면: {:?}", areas(&f3));
        assert!((sum(&f3) - 28.0).abs() < 1e-6, "합 28(union): {}", sum(&f3));

        // 4 — containment rect → annulus (2면, hole 1).
        let mut c4 = rect_curves((0.0, 0.0), (10.0, 10.0));
        c4.extend(rect_curves((3.0, 3.0), (7.0, 7.0)));
        let f4 = arrange(&c4, eps);
        assert_eq!(f4.len(), 2, "containment → 2면");
        assert_eq!(
            f4.iter().filter(|f| !f.holes.is_empty()).count(),
            1,
            "annulus hole 1"
        );
    }

    /// **잔존 진단** — arrange(2rect+circle) 출력이 manifold 인가 (각 edge ≤2 face)?
    /// DCEL viol 의 근본이 arrange 모듈인지 DCEL 매핑인지 가르는 핵심 테스트.
    #[test]
    fn arrange_2rect_circle_module_manifold() {
        use std::collections::HashMap;
        let mut curves = rect_curves((0.0, 0.0), (4.0, 4.0));
        curves.extend(rect_curves((1.5, 0.0), (5.5, 4.0)));
        curves.push(InputCurve::Circle { center: Vec2::new(2.0, 1.0), radius: 1.8 });
        let faces = arrange(&curves, 1e-7);
        let q = |x: f64| (x / 1e-5).round() as i64;
        let key = |a: Vec2, b: Vec2| -> (i64, i64, i64, i64) {
            let (k1, k2) = ((q(a.x), q(a.y)), (q(b.x), q(b.y)));
            if k1 <= k2 { (k1.0, k1.1, k2.0, k2.1) } else { (k2.0, k2.1, k1.0, k1.1) }
        };
        let mut usage: HashMap<(i64, i64, i64, i64), usize> = HashMap::new();
        for f in &faces {
            for sc in &f.outer {
                *usage.entry(key(sc.start_pt(), sc.end_pt())).or_default() += 1;
            }
        }
        let over: Vec<_> = usage.iter().filter(|(_, &c)| c > 2).collect();
        println!("arrange 2rect+circle: {} faces, {} edges over-used", faces.len(), over.len());
        for (k, c) in &over {
            println!("  edge ({},{})-({},{}) used {}x", k.0, k.1, k.2, k.3, c);
        }
        assert!(over.is_empty(), "arrange 모듈 manifold: {} edge >2 face", over.len());
    }

    /// **Step 6 사전검토** — arrange(동심원 2개) nesting 확인 (annulus 되는지).
    #[test]
    fn sim_concentric_circles_arrange() {
        let curves = vec![
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 10.0 },
            InputCurve::Circle { center: Vec2::new(0.0, 0.0), radius: 4.0 },
        ];
        let faces = arrange(&curves, 1e-7);
        for (i, f) in faces.iter().enumerate() {
            println!("  concentric face {}: {} outer, {} holes", i, f.outer.len(), f.holes.len());
        }
    }

    /// **잔존 심층 진단** — arrange(2rect+circle) 출력 SubCurve 종류 + holes 덤프.
    /// 원 오른쪽이 Arc 인지 Line 인지 + hole 유무 (D1 polygonize 원인 확인).
    #[test]
    fn sim_2rect_circle_arrange_dump() {
        let mut curves = rect_curves((0.0, 0.0), (4.0, 4.0));
        curves.extend(rect_curves((1.5, 0.0), (5.5, 4.0)));
        curves.push(InputCurve::Circle { center: Vec2::new(2.0, 1.0), radius: 1.8 });
        let faces = arrange(&curves, 1e-7);
        let kind = |sc: &SubCurve| match sc {
            SubCurve::Line { .. } => "Line",
            SubCurve::Arc { .. } => "Arc",
            SubCurve::Freeform { .. } => "Freeform",
        };
        for (i, f) in faces.iter().enumerate() {
            println!("face {}: {} outer, {} holes", i, f.outer.len(), f.holes.len());
            for sc in &f.outer {
                let a = sc.start_pt();
                let b = sc.end_pt();
                println!("    {} ({:.2},{:.2})-({:.2},{:.2})", kind(sc), a.x, a.y, b.x, b.y);
            }
            for (hi, h) in f.holes.iter().enumerate() {
                println!("    hole {}: {} subcurves", hi, h.len());
            }
        }
    }

    /// 큰 사각형 + 안에 disjoint 작은 원 → annulus(rect, hole 원) + disk(원). ADR-021 P7.
    #[test]
    fn arrange_rect_with_disjoint_circle_annulus() {
        let curves = vec![
            InputCurve::Line { a: Vec2::new(0.0, 0.0), b: Vec2::new(10.0, 0.0) },
            InputCurve::Line { a: Vec2::new(10.0, 0.0), b: Vec2::new(10.0, 10.0) },
            InputCurve::Line { a: Vec2::new(10.0, 10.0), b: Vec2::new(0.0, 10.0) },
            InputCurve::Line { a: Vec2::new(0.0, 10.0), b: Vec2::new(0.0, 0.0) },
            // 안에 떨어진 작은 원 (rect 경계와 안 닿음).
            InputCurve::Circle { center: Vec2::new(5.0, 5.0), radius: 2.0 },
        ];
        let faces = arrange(&curves, 1e-7);
        println!(
            "rect+disjoint circle faces={} holes={:?}",
            faces.len(),
            faces.iter().map(|f| f.holes.len()).collect::<Vec<_>>()
        );
        assert_eq!(faces.len(), 2, "annulus + disk = 2면");
        let with_hole = faces.iter().filter(|f| !f.holes.is_empty()).count();
        assert_eq!(with_hole, 1, "rect 면이 원 hole 1개 (annulus)");
    }

    // ── B5 — freeform × line/circle mixed intersection (ADR-186 A3) ──
    // The type-agnostic CCI kernel (ADR-030) handles mixed pairs once both
    // inputs are lifted to 3D AnalyticCurve. Param conventions map directly to
    // split_curve (line → [0,1], circle → angle).

    fn b5_blob() -> InputCurve {
        // closed bezier blob, cx=50 (browser-demo shape).
        InputCurve::Freeform(Freeform2D::bezier(vec![
            Vec2::new(50.0, 0.0),
            Vec2::new(120.0, 50.0),
            Vec2::new(50.0, 100.0),
            Vec2::new(-20.0, 50.0),
            Vec2::new(50.0, 0.0),
        ]))
    }

    #[test]
    fn b5_intersect_freeform_x_line_two_crossings() {
        let blob = b5_blob();
        // horizontal line through the blob middle (y=50) → crosses left + right.
        let line = InputCurve::Line { a: Vec2::new(-60.0, 50.0), b: Vec2::new(160.0, 50.0) };
        let hits = intersect(&blob, &line, 1e-4);
        assert_eq!(hits.len(), 2, "freeform×line should cross twice, got {:?}", hits);
        // param on the line (t2) must be in [0,1] (split_curve(Line) convention).
        for (_, t2, _) in &hits {
            assert!(*t2 >= -1e-9 && *t2 <= 1.0 + 1e-9, "line param {} not in [0,1]", t2);
        }
    }

    #[test]
    fn b5_intersect_freeform_x_circle_two_crossings() {
        let blob = b5_blob();
        // circle centered on the blob's right boundary (~(74,50)) → straddles → 2.
        let circ = InputCurve::Circle { center: Vec2::new(74.0, 50.0), radius: 18.0 };
        let hits = intersect(&blob, &circ, 1e-4);
        assert_eq!(hits.len(), 2, "freeform×circle should cross twice, got {:?}", hits);
        // param on the circle (t2) must be an angle in [0, 2π] (split_curve(Circle)).
        for (_, t2, _) in &hits {
            assert!(*t2 >= -1e-9 && *t2 <= TWO_PI + 1e-9, "circle angle {} not in [0,2π]", t2);
        }
    }

    #[test]
    fn b5_intersect_symmetric_swapped_args() {
        let blob = b5_blob();
        let line = InputCurve::Line { a: Vec2::new(-60.0, 50.0), b: Vec2::new(160.0, 50.0) };
        let h1 = intersect(&blob, &line, 1e-4); // (Freeform, Line)
        let h2 = intersect(&line, &blob, 1e-4); // (Line, Freeform) — swapped arm
        assert_eq!(h1.len(), 2);
        assert_eq!(h2.len(), 2, "swapped (Line, Freeform) arm must also find 2");
        // h2's t1 is now the line param ([0,1]); h2's t2 the freeform param.
        for (t1, _, _) in &h2 {
            assert!(*t1 >= -1e-9 && *t1 <= 1.0 + 1e-9, "swapped line param {} not in [0,1]", t1);
        }
    }

    #[test]
    fn b5_arrange_freeform_x_rect_partial_overlap_splits() {
        // blob + a rect overlapping its right half → 3 faces (blob-only,
        // rect-only, lens). Without the B5 arm this would be 2 separate
        // overlapping faces; the mixed CCI split yields the lens.
        let mut curves = vec![b5_blob()];
        curves.extend([
            InputCurve::Line { a: Vec2::new(50.0, 28.0), b: Vec2::new(110.0, 28.0) },
            InputCurve::Line { a: Vec2::new(110.0, 28.0), b: Vec2::new(110.0, 72.0) },
            InputCurve::Line { a: Vec2::new(110.0, 72.0), b: Vec2::new(50.0, 72.0) },
            InputCurve::Line { a: Vec2::new(50.0, 72.0), b: Vec2::new(50.0, 28.0) },
        ]);
        let faces = arrange(&curves, 1e-4);
        assert!(
            faces.len() >= 3,
            "blob + overlapping rect → >=3 faces (lens split), got {}",
            faces.len()
        );
    }
}
