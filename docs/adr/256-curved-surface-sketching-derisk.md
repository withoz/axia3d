# ADR-256 — P3 de-risk closure: Curved-surface sketching (Cylinder/Cone/Torus) deferred to dedicated sprint

- **Status**: Accepted (de-risk closure + defer decision)
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch) — 곡면 sketching frontier (ADR-173 12-gate)
- **Type**: De-risk closure (docs-only, 코드 변경 0)
- **Author**: WYKO + Claude (de-risk workflow + full-flow empirical probe)

## 1. Context

ADR-253 우선순위 **P3 (곡면 위 그리기/선분할 — Cylinder/Cone/Torus,
ADR-202 Sphere S9 MVP 확장)**. ADR-173 12-gate 매트릭스의 곡면 column
(S3/S6/S9/S12) frontier — ADR-202 가 Sphere circle (S9) 를 closure.
P3 = Cyl/Cone/Torus 로 확장. 구현 전 de-risk (4-agent workflow + 실제
엔진 full-flow probe).

## 2. de-risk findings (empirical)

### 2.1 ADR-202 Sphere = 재사용 가능한 6-layer template
P3 가 1:1 mirror 할 수 있는 vertical slice (각 layer 독립 testable):

| Layer | 함수 | file:line | P3 재사용 |
|---|---|---|---|
| L1 project | `project_to_surface` | `surfaces/sphere.rs:78` | ADAPT (surface별) |
| L1 curve gen | `circle_on_sphere` → `AnalyticCurve::Circle` | `surfaces/sphere.rs:165` | ADAPT |
| L2 split | `split_sphere_face_by_circle` (cap+annulus, twin-HE reparent) | `mesh.rs:3334` | **REUSE** (surface-agnostic core) |
| L3 render | `tessellate_sphere_clipped` (Sutherland-Hodgman + co-spherical twin gate) | `mesh.rs:1871` | ADAPT |
| L4 scene | `Scene::draw_circle_on_sphere` (single Undo + XIA inherit) | `scene.rs:2622` | REUSE (template) |
| L5 bridge | `draw_circle_on_sphere` + TS wrapper | `lib.rs:4552` | REUSE (template) |
| L6 dispatch | `DrawCircleTool` `surfaceKind===3` gate | `DrawCircleTool.ts:62` | REUSE (template) |

**Pattern-12**: render scaffold 이미 존재 — `tessellate_cylinder_clipped`
(`mesh.rs:2094`), `tessellate_cone_clipped` (`mesh.rs:2212`),
`tessellate_torus_clipped` (`mesh.rs:2512+`) 모두 ADR-205 Boolean corner
clipping 용으로 이미 구현됨. P3 는 oblique-plane clip → circle clip 으로
ADAPT 만.

### 2.2 핵심 기하 finding — "circle on cylinder" 의 두 의미 (canonical)
**구는 작은 원이 표면에 놓이지만 실린더는 안 놓인다.** "circle on
cylinder" 는 두 별개 해석:

- **A. 위도 링 (latitude ring)**: ⊥축 평면 원, 반지름 = **정확히 실린더
  r**, 실린더를 **빙 두르는 전체 링**. 표면에 놓이는 **유일한** 평면 원
  (x²+y²=r² 위 평면 원은 z=const 링뿐; 반지름<r 작은 원은 표면 밖;
  oblique = ellipse). exact Circle. 측면 → 2 띠 분할. **구멍 아님.**
- **B. 작은 벽 원 (small wall circle)**: 벽의 작은 닫힌 loop (porthole).
  geodesic 원 = developable 실린더를 펼친 평평한 원을 3D 로 mapping =
  **닫힌 3D 곡선** (평면 Circle/Arc/ellipse 아님). **깨끗한 AnalyticCurve
  표현 없음** → polyline (다각형 — ADR-189 이전 "간섭 라인" 류) 또는
  NURBS fit. 사용자 직관 ("벽에 원/구멍") 부합.

synthesis 가 A (exact Circle MVP) 를 추천하면서 시나리오는 B (벽 구멍)
를 들어 **두 의미를 혼동**. 사용자 결재 = **B (의도 부합)**.

### 2.3 B redundancy 확인 — 진짜 gap (empirical)
"실린더 곡면 벽에 원/구멍" 을 만드는 현재 경로 = **0**:
- `punchHole` (face-level) on cylinder wall → **FAILED** (ret -1,
  faceDelta 0; planar host 필요, 곡면 미지원).
- `drillThroughHole` (관통) radial on cylinder wall → **FAILED**
  (ret -1, faceDelta 0; planar entry/exit 필요).
- `cylinder − cylinder` Boolean (P2, ADR-255) → 미지원 (정직한 에러).

→ P3-B 는 punch/drill/Boolean 어느 것과도 **중복 아님**. 실재 gap.

## 3. Decision — DEFER P3-B to dedicated sprint

**B (작은 벽 원) 는 실재 gap 이나 5-6주+ dedicated feature** (ADR-202
Sphere 보다 어려움):
- geodesic 3D 곡선 = 깨끗한 AnalyticCurve 없음 → polyline (시각 다각형)
  또는 NURBS fit (machinery).
- "구멍" 버전은 곡면-aware drill 또는 curved Boolean 필요 — 둘 다 미지원.
- feature 작업 (defect 아님), 완전한 설계 (curve 표현 / 곡면 split /
  render) 필요.

사용자 결재 **"현 세션 consolidate — P3-B 는 전용 sprint"**. de-risk 가
설계 anchor + gap 확인 + scope tier 를 봉인 → 별도 집중 sprint 에서 구현.

## 4. Scope tiers (future dedicated sprint anchor)

| Tier | 내용 | curve repr | 비용 | risk |
|---|---|---|---|---|
| **A** 위도 링 | ⊥축 exact Circle, 측면 2 띠 분할 | `AnalyticCurve::Circle` (clean) | 3-4주 | LOW (ADR-202 1:1 mirror) |
| **B-polyline** 작은 벽 원 | geodesic circle → polyline 경계, cap+나머지 | polyline (다각형 render) | 4-5주 | MEDIUM |
| **B-NURBS** 작은 벽 원 | geodesic circle → NURBS fit (smooth) | NURBS | 6-7주 | MEDIUM-HIGH |
| Cone ⊥ circle | exact Circle (apex 특이점) | Circle | +2-3주 | LOW-MEDIUM |
| Torus / oblique / line-on-curved (S3) | spiric / ellipse / geodesic line | NURBS / plane_torus SSI (미존재) | multi-week each | HIGH |

**dedicated sprint 진입 trigger**: 사용자가 곡면 sketching 을 명시 요구
하고 B 표현 (polyline vs NURBS) 을 결재할 때. 그 전엔 budget 을 더 높은
가치에 투입.

## 5. Lock-ins

- **L-256-1** P3-B (작은 벽 원) = 실재 gap (punch/drill/Boolean 모두
  곡면 벽 미지원, empirical 확인) — but 5-6주+ dedicated feature, defer.
- **L-256-2** ADR-202 Sphere 6-layer template = P3 mirror anchor (L2/L4/
  L5/L6 REUSE, L1/L3 ADAPT). render scaffold (tessellate_{cyl,cone,torus}_
  clipped) Pattern-12 이미 존재 (ADR-205).
- **L-256-3** 기하 canonical — A 위도 링 (exact Circle, 표면-on 유일
  평면 원, 측면 2 띠) ≠ B 작은 벽 원 (geodesic 3D 곡선, polyline/NURBS,
  사용자 직관). 둘 혼동 금지.
- **L-256-4** B curve 표현 미결 — polyline (다각형, ADR-189 회귀 위험)
  vs NURBS (machinery). dedicated sprint 에서 결재.
- **L-256-5** 곡면 hole = 곡면-aware drill / curved Boolean 필요 (둘 다
  미지원) — B 의 "구멍" 버전은 별도 큰 작업.
- **L-256-6** 코드 변경 0 (docs-only de-risk closure, LOCKED #44).
- **L-256-7** ADR-202 (S9 sphere) closure 보존 — P3 는 그 확장, supersede
  아님.

## 6. Lessons

- **L1 de-risk 가 geometry/intent mismatch 차단** — synthesis 의 "exact
  Circle MVP" 가 실제로는 위도 링 (≠ 사용자 의도 "벽 구멍"). ADR-202
  template 정독 + 실린더 기하 분석으로 commit 전 발견 → 잘못된 3-4주 회피.
  empirical/template-read > synthesis 추론.
- **L2 redundancy 확인 = 진짜 gap 검증** — punch/drill/Boolean 곡면 벽
  실패를 empirical probe 로 확인 → P3-B 가 중복 아닌 실재 gap. 가치 검증.
- **L3 honest scale 인식** — P3 가 "MVP" 라기엔 5-6주+ dedicated feature
  (geodesic curve, 곡면-aware ops). defer-to-sprint = truth over completion
  (ADR-251/255 답습). de-risk 가 설계 anchor 봉인.
- **L4 "표면에 놓이는 곡선" 의 surface 별 차이** — 구(작은 원 OK) vs
  developable 실린더 (위도 링만 평면, 작은 원은 geodesic 3D) vs 원뿔/
  토러스. 곡면 sketching 의 일반 원칙: surface 곡률 종류가 curve 표현
  난이도 지배.

## 7. Cross-link

- ADR-253 (P3 anchor — 우선순위) + ADR-254 (P1) + ADR-255 (P2 defer)
- ADR-202 (Sphere circle S9 — 6-layer template, P3 mirror source) + LOCKED #83
- ADR-173 12-gate (곡면 column S3/S6/S9/S12 frontier) + LOCKED #74
- ADR-205 (tessellate_{cyl,cone,torus}_clipped render scaffold — Pattern-12)
- ADR-189 (closed-curve polygon 회귀 — B-polyline 위험 source)
- ADR-158 (Ellipse = NURBS — oblique cut 표현)
- ADR-194/221 (punch/drill Hole 도구 — 곡면 벽 미지원 확인) + ADR-255 (Boolean)
- ADR-251/255 (de-risk defer closure 패턴) / 메타-원칙 #5 #6 #16 / LOCKED #44
