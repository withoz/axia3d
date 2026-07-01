//! 2D 기하 primitives (Vec2, segment intersection, point-in-polygon).
//!
//! **ADR-186 Phase 3 β-1** — AixiAcad `xia-form/src/boundary_kernel/geom2.rs`
//! (ADR-057 유도면 모델) 의 1:1 faithful port. kernel 내부 전용 `Vec2` (외부
//! 인터페이스는 glam `DVec3` 사용 — DCEL 통합 시 boundary 에서 변환).
//! deterministic (ADR-035 정합). **ADR-187 β-1**: topology-critical 부호 판정용
//! `orient2d_sign` 은 `robust` crate (ADR-058 Shewchuk 1.1, deterministic) 사용 —
//! "가벼움=속도" 정정 (빠름 AND 정확, 메타-원칙 #15).

#![allow(dead_code)]

/// 2D 벡터 (kernel 내부 전용).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    /// x 좌표.
    pub x: f64,
    /// y 좌표.
    pub y: f64,
}

impl Vec2 {
    /// 새 Vec2.
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// 덧셈.
    pub fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
    /// 뺄셈.
    pub fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
    /// 스칼라 곱.
    pub fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
    /// 내적.
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }
    /// 2D 외적 (z 성분).
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }
    /// 길이.
    pub fn len(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    /// 다른 점까지 거리.
    pub fn dist(self, o: Vec2) -> f64 {
        self.sub(o).len()
    }
}

/// 두 점이 eps 이내인지.
pub fn point_almost_eq(a: Vec2, b: Vec2, eps: f64) -> bool {
    a.dist(b) <= eps
}

/// **ADR-187 β-1** — robust 2D orientation 부호 (Shewchuk adaptive, *exact sign*).
///
/// `(b-a) × (c-a)` (z-성분) 의 부호를 정확히 반환:
///   - `+1` = c 가 directed line a→b 의 **좌측** (CCW)
///   - `-1` = **우측** (CW)
///   - `0`  = collinear (exact — robust 보장)
///
/// f64 cross product 의 `eps` 비교와 달리 **near-collinear 에서도 정확**.
/// `robust` crate (ADR-058 Shewchuk 1.1) 재사용 — adaptive 라 non-degenerate
/// 는 f64 속도, degenerate 만 exact escalate (빠름 AND 정확, 메타-원칙 #15).
/// boundary_kernel 의 topology-critical 부호 판정 (seg_intersect / angle sort /
/// point-in-polygon) 보강용.
#[inline]
pub fn orient2d_sign(a: Vec2, b: Vec2, c: Vec2) -> i32 {
    let s = robust::orient2d(
        robust::Coord { x: a.x, y: a.y },
        robust::Coord { x: b.x, y: b.y },
        robust::Coord { x: c.x, y: c.y },
    );
    if s > 0.0 {
        1
    } else if s < 0.0 {
        -1
    } else {
        0
    }
}

/// 점이 segment 위에 있는지 (perpendicular distance + t bounds 모두 검사).
///
/// 반환: `(on, t)` where `p ≈ a + t*(b-a)`.
pub fn point_on_segment(p: Vec2, a: Vec2, b: Vec2, eps: f64) -> (bool, f64) {
    let ab = b.sub(a);
    let ap = p.sub(a);
    let ab_len2 = ab.dot(ab);
    if ab_len2 <= eps * eps {
        // zero-length segment.
        return (point_almost_eq(p, a, eps), 0.0);
    }
    let t = ap.dot(ab) / ab_len2;
    let proj = a.add(ab.mul(t));
    let d = p.dist(proj);
    let on = d <= eps && t >= -eps && t <= 1.0 + eps;
    (on, t)
}

/// Segment-segment intersection 결과.
#[derive(Clone, Copy, Debug)]
pub enum SegIsect {
    /// 교차 없음.
    None,
    /// 한 점에서 교차.
    Point {
        /// 교차점.
        p: Vec2,
        /// segment a의 매개변수 t.
        t1: f64,
        /// segment b의 매개변수 u.
        t2: f64,
    },
    /// Colinear overlap — 두 segment가 같은 line 위에 일부 겹침.
    Overlap {
        /// overlap 구간의 한 끝.
        p1: Vec2,
        /// overlap 구간의 다른 끝.
        p2: Vec2,
    },
}

/// 두 segment의 robust intersection.
///
/// - Proper intersection + endpoint touch → `Point`.
/// - Colinear overlap (lo/hi clamping) → `Overlap`.
///
/// ## Bug fix #4 (vs 원본): cross eps 단위 처리
///
/// `rxs = r × s`의 단위는 distance²이지만 입력 `eps`는 distance 단위.
/// 따라서 `rxs.abs() <= eps * len_scale` 형식으로 scale-aware 비교 사용.
pub fn seg_intersect(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2, eps: f64) -> SegIsect {
    let r = a2.sub(a1);
    let s = b2.sub(b1);
    let rxs = r.cross(s);
    let q_p = b1.sub(a1);
    let qpxr = q_p.cross(r);
    // **Bug fix #4** — scale-aware threshold for cross product.
    let len_scale = (r.len() * s.len()).max(eps);
    let cross_eps = eps * len_scale;
    // colinear
    if rxs.abs() <= cross_eps && qpxr.abs() <= cross_eps {
        let rr = r.dot(r);
        if rr <= eps * eps {
            if point_almost_eq(a1, b1, eps) {
                return SegIsect::Point {
                    p: a1,
                    t1: 0.0,
                    t2: 0.0,
                };
            }
            return SegIsect::None;
        }
        let t0 = b1.sub(a1).dot(r) / rr;
        let t1 = b2.sub(a1).dot(r) / rr;
        let lo = t0.min(t1).max(0.0);
        let hi = t0.max(t1).min(1.0);
        if hi < lo - eps {
            return SegIsect::None;
        }
        let p_lo = a1.add(r.mul(lo));
        let p_hi = a1.add(r.mul(hi));
        if p_lo.dist(p_hi) <= eps {
            // degenerate to point.
            let bs = s.dot(s);
            let mut u = 0.0;
            if bs > eps * eps {
                u = p_lo.sub(b1).dot(s) / bs;
            }
            return SegIsect::Point {
                p: p_lo,
                t1: lo,
                t2: u,
            };
        }
        return SegIsect::Overlap { p1: p_lo, p2: p_hi };
    }
    // parallel non-colinear
    if rxs.abs() <= cross_eps && qpxr.abs() > cross_eps {
        return SegIsect::None;
    }
    let t = q_p.cross(s) / rxs;
    let u = q_p.cross(r) / rxs;
    if t >= -eps && t <= 1.0 + eps && u >= -eps && u <= 1.0 + eps {
        let p = a1.add(r.mul(t));
        return SegIsect::Point { p, t1: t, t2: u };
    }
    SegIsect::None
}

/// Signed area (CCW = positive).
pub fn polygon_signed_area(poly: &[Vec2]) -> f64 {
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        s += a.x * b.y - b.x * a.y;
    }
    0.5 * s
}

/// Centroid of simple polygon — abs(area) 사용으로 CW 안전.
pub fn polygon_centroid(poly: &[Vec2]) -> Vec2 {
    let mut a2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        let cross = p.x * q.y - q.x * p.y;
        a2 += cross;
        cx += (p.x + q.x) * cross;
        cy += (p.y + q.y) * cross;
    }
    let area = 0.5 * a2;
    if area.abs() < 1e-18 {
        let (sx, sy) = poly
            .iter()
            .fold((0.0, 0.0), |acc, p| (acc.0 + p.x, acc.1 + p.y));
        return Vec2::new(sx / poly.len() as f64, sy / poly.len() as f64);
    }
    let inv = 1.0 / (6.0 * area.abs());
    Vec2::new(cx * inv, cy * inv)
}

/// Point-in-polygon 결과 (boundary 명시).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pip {
    /// 내부.
    Inside,
    /// 외부.
    Outside,
    /// boundary edge 위.
    Boundary,
}

/// Even-odd rule + boundary detection.
///
/// **ADR-187 β-4** — ray-crossing 의 x 비교를 `orient2d_sign` (exact 부호) 으로
/// 교체. 기존 `p.x < x_int` (division + f64 비교) 는 near-boundary/near-vertex 에서
/// 흔들림 → containment 오분류 (hole 부착 오류) 가능. lower→upper 유향 edge 기준
/// p 의 좌/우를 robust 로 판정 (division 제거). boundary 검출은 eps 유지 (허용
/// 오차 정책).
pub fn point_in_polygon_even_odd(p: Vec2, poly: &[Vec2], eps: f64) -> Pip {
    let mut inside = false;
    let n = poly.len();
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + n - 1) % n];
        let (on, t) = point_on_segment(p, a, b, eps);
        if on && t >= -eps && t <= 1.0 + eps {
            return Pip::Boundary;
        }
        // half-open straddle 규약 (vertex 중복 계수 차단) 보존.
        if (a.y > p.y) != (b.y > p.y) {
            // edge 가 p.y 를 straddle — lower→upper 유향에서 p 가 좌(left)면
            // 교차가 p 우측 → ray(+x) 교차 → toggle. orient2d 로 exact.
            let (lower, upper) = if a.y < b.y { (a, b) } else { (b, a) };
            if orient2d_sign(lower, upper, p) > 0 {
                inside = !inside;
            }
        }
    }
    if inside {
        Pip::Inside
    } else {
        Pip::Outside
    }
}

/// 모든 점이 polygon 내부 또는 boundary인지.
pub fn all_inside(pts: &[Vec2], poly: &[Vec2], eps: f64) -> bool {
    pts.iter()
        .all(|&v| point_in_polygon_even_odd(v, poly, eps) != Pip::Outside)
}

/// Scale-aware eps suggestion — input bounding box diagonal × 1e-9.
pub fn eps_from_scale(points: &[Vec2]) -> f64 {
    if points.is_empty() {
        return 1e-9;
    }
    let mut minx = points[0].x;
    let mut maxx = points[0].x;
    let mut miny = points[0].y;
    let mut maxy = points[0].y;
    for p in points {
        minx = minx.min(p.x);
        maxx = maxx.max(p.x);
        miny = miny.min(p.y);
        maxy = maxy.max(p.y);
    }
    let diag = ((maxx - minx).powi(2) + (maxy - miny).powi(2)).sqrt();
    (diag * 1e-9).max(1e-9)
}

/// 무한 2D 직선 교차 — `p0 + t·d0` 와 `p1 + u·d1` 의 교점.
///
/// **ADR-259 β-1** — taper offset 의 corner 정점 계산용. `seg_intersect` 는
/// t∈[0,1] clamp 라 corner(연장선) 교차에 부적합 → 무한 직선 교차 전용.
/// near-parallel (`|d0 × d1| ≤ eps`) → `None` (호출자가 parallel fallback).
pub fn line_line_intersect_2d(p0: Vec2, d0: Vec2, p1: Vec2, d1: Vec2, eps: f64) -> Option<Vec2> {
    let denom = d0.cross(d1);
    if denom.abs() <= eps {
        return None;
    }
    let dp = p1.sub(p0);
    let t = dp.cross(d1) / denom;
    Some(p0.add(d0.mul(t)))
}

/// 단순 다각형 자기교차 검사 — 비인접 edge 쌍의 proper crossing/overlap.
///
/// **ADR-259 β-1** — fail-closed taper 의 핵심 가드. reflex 정점에서 inward
/// offset 이 과해 다각형이 자기교차(또는 토폴로지 분할)하면 `true` → 호출자가
/// 거부 (깨진 solid 생성 차단). 정점을 공유하는 인접 edge 는 skip.
pub fn polygon_self_intersects(poly: &[Vec2], eps: f64) -> bool {
    let n = poly.len();
    if n < 4 {
        return false;
    }
    for i in 0..n {
        let a1 = poly[i];
        let a2 = poly[(i + 1) % n];
        for j in (i + 1)..n {
            // 정점을 공유하는 인접 edge 는 skip (false positive 차단).
            if (j + 1) % n == i || (i + 1) % n == j {
                continue;
            }
            let b1 = poly[j];
            let b2 = poly[(j + 1) % n];
            match seg_intersect(a1, a2, b1, b2, eps) {
                SegIsect::None => {}
                _ => return true,
            }
        }
    }
    false
}

/// per-edge 수직 offset 결과 (fail-closed 분류).
///
/// **ADR-259 β-1** — taper top profile. `Ok` 외 모든 variant 는 호출자(taper
/// 커널)가 거부 → transaction rollback (깨진 solid 0 = "면깨짐 최대 방지").
#[derive(Clone, Debug, PartialEq)]
pub enum PolyOffset {
    /// 유효한 단순 offset 다각형 (입력과 동일 winding).
    Ok(Vec<Vec2>),
    /// 면적 ≈ 0 (point/line 으로 붕괴) 또는 부호 flip (fold-back).
    Degenerate,
    /// offset 이 자기교차 (reflex over-offset / 토폴로지 분할).
    SelfIntersect,
    /// sharp 정점에서 miter 길이가 한계 초과 (스파이크).
    Spike,
    /// 입력 부적합 (n<3, zero-area, degenerate edge).
    BadInput,
}

/// 단순 다각형의 per-edge 수직 offset (convex **및 concave**).
///
/// **ADR-259 β-1** — `distance > 0` = inward (수축, top profile), `< 0` = outward
/// (flare). CCW 정규화 후 각 변을 내부 수직으로 `distance` 이동, 인접 offset-line
/// 의 무한 교차로 새 정점 계산 (reflex 정점도 동일 — 교차가 반대쪽에 생길 뿐).
/// **측벽 평면성 핵심**: offset-line 은 원래 변과 평행 → top edge ∥ bottom edge →
/// 측벽 사다리꼴은 항상 평면 (convex/concave 무관, ADR-259 §2).
///
/// fail-closed: 자기교차/collapse/inversion/spike → 비-`Ok` (호출자 거부).
/// `miter_limit` = `|distance|` 배수 상한 (예: 16.0 — 극단 스파이크만 거부).
pub fn offset_polygon_2d(verts: &[Vec2], distance: f64, miter_limit: f64) -> PolyOffset {
    let n = verts.len();
    if n < 3 {
        return PolyOffset::BadInput;
    }
    // scale-relative degenerate-input 검사 (bbox diagonal²).
    let mut minx = verts[0].x;
    let mut maxx = verts[0].x;
    let mut miny = verts[0].y;
    let mut maxy = verts[0].y;
    for v in verts {
        minx = minx.min(v.x);
        maxx = maxx.max(v.x);
        miny = miny.min(v.y);
        maxy = maxy.max(v.y);
    }
    let diag2 = (maxx - minx).powi(2) + (maxy - miny).powi(2);
    let eps = (diag2.sqrt() * 1e-9).max(1e-9);
    let orig_area = polygon_signed_area(verts);
    if orig_area.abs() < 1e-10 * diag2.max(1e-18) {
        return PolyOffset::BadInput;
    }
    // CCW 작업 복사본 (inward = +90° 좌측 수직).
    let ccw = orig_area > 0.0;
    let mut poly: Vec<Vec2> = verts.to_vec();
    if !ccw {
        poly.reverse();
    }
    // 각 변의 offset-line (점 + 방향).
    let mut lines: Vec<(Vec2, Vec2)> = Vec::with_capacity(n);
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        let edge = b.sub(a);
        let len = edge.len();
        if len <= eps {
            return PolyOffset::BadInput; // degenerate edge.
        }
        let dir = edge.mul(1.0 / len);
        let inward = Vec2::new(-dir.y, dir.x); // CCW 내부 수직.
        lines.push((a.add(inward.mul(distance)), dir));
    }
    // 새 정점 = 인접 offset-line 무한 교차.
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let (p_prev, d_prev) = lines[(i + n - 1) % n];
        let (p_cur, d_cur) = lines[i];
        let w = match line_line_intersect_2d(p_prev, d_prev, p_cur, d_cur, 1e-9) {
            Some(w) => {
                // spike 가드 (sharp 정점에서 miter 폭주).
                if distance.abs() > eps && w.dist(poly[i]) > miter_limit * distance.abs() {
                    return PolyOffset::Spike;
                }
                w
            }
            None => {
                // 평행 인접 offset-line (≈180° turn) → 공유 정점 수직 이동.
                let inward = Vec2::new(-d_cur.y, d_cur.x);
                poly[i].add(inward.mul(distance))
            }
        };
        result.push(w);
    }
    // collapse / inversion 가드 (working 은 CCW → 양수 기대).
    let off_area = polygon_signed_area(&result);
    if off_area.abs() < 1e-9 * orig_area.abs() {
        return PolyOffset::Degenerate; // collapse to point/line.
    }
    if off_area <= 0.0 {
        return PolyOffset::Degenerate; // fold-back / inversion.
    }
    // inward containment 가드 — convex "tunnel-through" 차단.
    // 과한 inward offset 은 모든 변이 동시에 centroid 를 지나쳐 *반대편* 에서
    // 같은 winding 의 더 큰 다각형으로 재형성됨 → area/sign 가드를 통과해버림
    // (예: unit square d=2.0). inward 면 모든 offset 정점이 원본 내부여야 함.
    // (apothem-from-centroid 가드보다 robust — concave 의 valid offset 을 오거부
    // 하지 않음, ADR-259 §4.)
    if distance > 0.0 {
        for w in &result {
            if point_in_polygon_even_odd(*w, &poly, eps) == Pip::Outside {
                return PolyOffset::Degenerate;
            }
        }
    }
    // 자기교차 가드 (reflex over-offset / 토폴로지 분할).
    if polygon_self_intersects(&result, eps) {
        return PolyOffset::SelfIntersect;
    }
    // 입력 winding 복원.
    if !ccw {
        result.reverse();
    }
    PolyOffset::Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-186 β-1 — AixiAcad geom2 회귀 4건 1:1 재현 (port 검증).

    #[test]
    fn seg_intersect_x_crossing() {
        let res = seg_intersect(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
            1e-9,
        );
        match res {
            SegIsect::Point { p, .. } => {
                assert!((p.x - 0.5).abs() < 1e-9);
                assert!((p.y - 0.5).abs() < 1e-9);
            }
            _ => panic!("expected Point"),
        }
    }

    #[test]
    fn seg_intersect_colinear_overlap() {
        // a=(0,0)→(10,0), b=(2,0)→(5,0). overlap = (2,0)~(5,0).
        let res = seg_intersect(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(5.0, 0.0),
            1e-9,
        );
        match res {
            SegIsect::Overlap { p1, p2 } => {
                let xs = [p1.x, p2.x];
                assert!(xs.contains(&2.0) && xs.contains(&5.0));
            }
            _ => panic!("expected Overlap"),
        }
    }

    #[test]
    fn pip_inside_quad() {
        let poly = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        assert_eq!(
            point_in_polygon_even_odd(Vec2::new(0.5, 0.5), &poly, 1e-9),
            Pip::Inside
        );
        assert_eq!(
            point_in_polygon_even_odd(Vec2::new(2.0, 2.0), &poly, 1e-9),
            Pip::Outside
        );
    }

    #[test]
    fn signed_area_ccw_positive() {
        let poly = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ];
        let a = polygon_signed_area(&poly);
        assert!((a - 1.0).abs() < 1e-9, "area={}", a);
    }

    // ADR-187 β-1 — robust orient2d_sign (Shewchuk adaptive, exact sign).

    #[test]
    fn orient2d_sign_ccw_cw_collinear() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(1.0, 0.0);
        assert_eq!(orient2d_sign(a, b, Vec2::new(0.5, 1.0)), 1, "좌(CCW) = +1");
        assert_eq!(orient2d_sign(a, b, Vec2::new(0.5, -1.0)), -1, "우(CW) = -1");
        assert_eq!(orient2d_sign(a, b, Vec2::new(2.0, 0.0)), 0, "collinear = 0");
    }

    /// near-collinear / 큰 좌표 — robust 는 exact 부호 (f64 cross 가 흔들리는 영역).
    #[test]
    fn orient2d_sign_robust_near_collinear() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(1e8, 1e8);
        assert_eq!(orient2d_sign(a, b, Vec2::new(5e7, 5e7)), 0, "정확히 collinear → 0");
        assert_eq!(orient2d_sign(a, b, Vec2::new(5e7, 5e7 + 1.0)), 1, "약간 좌 → +1");
        assert_eq!(orient2d_sign(a, b, Vec2::new(5e7, 5e7 - 1.0)), -1, "약간 우 → -1");
    }

    // ADR-259 β-1 — taper supporting features (offset_polygon_2d + guards).

    fn unit_square() -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ]
    }

    /// CCW L-shape (concave at (1,1)); arms width 1.
    fn l_shape() -> Vec<Vec2> {
        vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(3.0, 0.0),
            Vec2::new(3.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 3.0),
            Vec2::new(0.0, 3.0),
        ]
    }

    #[test]
    fn adr259_offset_square_inward_shrinks() {
        match offset_polygon_2d(&unit_square(), 0.2, 16.0) {
            PolyOffset::Ok(p) => {
                assert_eq!(p.len(), 4);
                // [0.2,0.8]² → 0.6×0.6 = 0.36
                assert!((polygon_signed_area(&p).abs() - 0.36).abs() < 1e-6);
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn adr259_offset_square_outward_grows() {
        match offset_polygon_2d(&unit_square(), -0.2, 16.0) {
            PolyOffset::Ok(p) => {
                // [-0.2,1.2]² → 1.4×1.4 = 1.96
                assert!((polygon_signed_area(&p).abs() - 1.96).abs() < 1e-6);
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn adr259_offset_triangle_inward_ok() {
        let tri = vec![Vec2::new(0.0, 0.0), Vec2::new(4.0, 0.0), Vec2::new(2.0, 3.0)];
        let orig = polygon_signed_area(&tri).abs();
        match offset_polygon_2d(&tri, 0.3, 16.0) {
            PolyOffset::Ok(p) => {
                assert_eq!(p.len(), 3);
                assert!(polygon_signed_area(&p).abs() < orig, "inward → smaller");
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn adr259_offset_concave_l_inward_valid() {
        // d=0.2 → arm width 0.6 (1 - 2·0.2), simple polygon expected.
        match offset_polygon_2d(&l_shape(), 0.2, 16.0) {
            PolyOffset::Ok(p) => assert_eq!(p.len(), 6),
            other => panic!("expected Ok concave offset, got {:?}", other),
        }
    }

    #[test]
    fn adr259_offset_concave_l_over_inward_rejected() {
        // d=0.7 → arm width 1 - 1.4 < 0 → collapse/self-intersect (fail-closed).
        let res = offset_polygon_2d(&l_shape(), 0.7, 16.0);
        assert!(
            !matches!(res, PolyOffset::Ok(_)),
            "over-inward concave must be rejected, got {:?}",
            res
        );
    }

    #[test]
    fn adr259_offset_square_collapse_degenerate() {
        // d = inradius 0.5 → all verts collapse to center → Degenerate.
        assert_eq!(
            offset_polygon_2d(&unit_square(), 0.5, 16.0),
            PolyOffset::Degenerate
        );
    }

    #[test]
    fn adr259_offset_square_over_inward_tunnel_degenerate() {
        // d ≫ inradius → offset tunnels through center, re-forms as a LARGER
        // same-winding square (positive area, no self-intersect). Must be
        // rejected by the inward-containment guard (Fix A) — else a steep
        // taper would silently produce a giant inverted solid.
        assert_eq!(
            offset_polygon_2d(&unit_square(), 2.0, 16.0),
            PolyOffset::Degenerate
        );
    }

    #[test]
    fn adr259_offset_degenerate_input_badinput() {
        assert_eq!(
            offset_polygon_2d(&[Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0)], 0.1, 16.0),
            PolyOffset::BadInput,
            "n<3"
        );
        // collinear (zero-area) → BadInput.
        let line = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(2.0, 0.0),
        ];
        assert_eq!(offset_polygon_2d(&line, 0.1, 16.0), PolyOffset::BadInput);
    }

    #[test]
    fn adr259_line_line_intersect_basic_and_parallel() {
        let hit = line_line_intersect_2d(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(0.0, 1.0),
            1e-9,
        );
        match hit {
            Some(p) => {
                assert!((p.x - 1.0).abs() < 1e-9 && p.y.abs() < 1e-9);
            }
            None => panic!("expected intersection"),
        }
        // parallel → None.
        assert!(line_line_intersect_2d(
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 0.0),
            1e-9,
        )
        .is_none());
    }

    #[test]
    fn adr259_polygon_self_intersects_bowtie() {
        // bowtie (crossing diagonals) → self-intersects.
        let bowtie = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, 2.0),
            Vec2::new(2.0, 2.0),
        ];
        assert!(polygon_self_intersects(&bowtie, 1e-9));
        // simple square → no self-intersection.
        assert!(!polygon_self_intersects(&unit_square(), 1e-9));
    }
}
