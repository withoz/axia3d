# ADR-200 — InputCurve::Arc First-Class (A1 full alignment)

- **Status**: Accepted
- **Date**: 2026-06-15
- **Track**: 곡선 면분할 — ADR-199 (A2 부분 arc 가드) 후속, 완전 정합
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

ADR-199 (A2) 가 "부분 arc → full circle 부활" 회귀를 차단했으나, **부분 arc 를
보존(preserve)만** 하고 arrange 에 입력하지 않는다 — 새 도형이 부분 arc 와
겹칠 때 arc 를 *재교차/분할* 하지 못한다 (보존된 면 + 새 arrangement 가 별개).
또한 **DrawArc 로 그린 호** 가 auto-division("선만 그려, 케이크는 알아서
나뉜다") 에 참여하지 못한다 (arc = 보존 특례, 분할 불참).

A1 = `analytic_arrange::arrange` 가 **`InputCurve::Arc` 를 1급 입력**으로 받아,
호가 직선·원과 동등하게 교차·분할·면화에 참여. AixxiA `xia-form` 의 "곡선 1급"
원칙 정합 + ADR-199 의 보존 workaround 제거.

### 아키텍처 finding (audit)

`SubCurve`(분할 결과 타입)에 **`Arc` 가 이미 완비** (start_pt/end_pt/tangent/
samples). gap 은 **입력 측** (`InputCurve` 에 Arc 없음) + 교차 클리핑 + open-arc
면추출. → A1 은 "multi-week" 추정보다 작음 (수 시간~수일).

## 2. Decision

### Lock-ins

- **L-200-1 (InputCurve::Arc)** `Arc { center, radius, a0, a1 }` (CCW, a0 < a1
  ≤ a0+2π). param 규약 = arc frame 각도 `[a0, a1]`.
- **L-200-2 (self-closing 아님)** Circle 과 달리 standalone 면 **없음**. 호는
  다른 곡선과 닫힌 영역을 이룰 때만 면화. lone arc = DCEL spur → signed-area
  ≈ 0 → 자동 필터 (직선 dangling 과 동일, 기존 메커니즘 재사용).
- **L-200-3 (교차 각도 클립)** Arc 교차 = 원 교차(`isect_line_circle` /
  `isect_circle_circle`)를 arc 각도범위 `[a0,a1]` 로 클립 (`arc_param_if_on`
  unwrap + in-range). Arc×Line / Arc×Circle / Arc×Arc 3-arm. eps_ang =
  (eps/radius).max(1e-9).
- **L-200-4 (split wrap 없음)** `split_curve(Arc)` 는 params + 양 끝(a0,a1) →
  인접쌍 SubCurve::Arc. Circle 의 wrap(a0+2π) 없음. params 0 → 전체 arc 1조각.
- **L-200-5 (A2 coverage 검사 재사용)** `reconstruct_input_curves` 의 닫힌-고리
  판정 보존 — full(closed loop)→Circle / partial(dangling)→**InputCurve::Arc**
  (β-2; A2 의 preserve 대체).
- **L-200-6** Arc×Freeform → CCI 커널(`lift_to_curve3d(Arc)`=AnalyticCurve::Arc).
  param 규약 정합은 β-4 검증.
- **L-200-7** ADR-046 P31 #4 additive — 공개 API 무변경. 효과는 rederive 경로.
- **L-200-8** 절대 #[ignore] 금지.

### Sub-step (Path Z atomic, 각 별도 결재/커밋)

| step | 내용 | 상태 |
|---|---|---|
| **β-1** | `analytic_arrange`: InputCurve::Arc + intersect 3-arm + split + lift + no-standalone + feasibility 단위 시뮬 | ✅ `ed566ed` |
| **β-2** | `reconstruct_input_curves`: partial arc → `InputCurve::Arc` (개별 fragment, arc 미드포인트로 방향 결정, 평면 frame 무관). A2 preserve/보존 로직 제거 | ✅ 본 커밋 |
| **β-3** | end-to-end 회귀: 부분 arc 가 overlap 도형과 실제 교차/분할(A.right 2→8) + 삭제분 부활 0 + arc 보존 + manifold valid | ✅ 본 커밋 |
| **β-4** | Arc×Freeform CCI param 정합(코드분석+테스트) + wrap-around + 전체 sweep + WASM 브라우저 시연 | ✅ 본 커밋 |
| **γ** | closure (본 ADR Accepted, 전체 β 완료, 회귀 +7 누적) | ✅ 본 커밋 |

## 3. 시뮬레이션 / 검증 (β-1)

feasibility 단위 시뮬 (실측):

**β-1** (analytic_arrange 단위):

| 테스트 | 결과 | 의미 |
|---|---|---|
| `arrange_arc_plus_chord_one_face` | faces=1, area=6.279 (≈2π) | 부분 arc + chord → 면화 |
| `arrange_lone_arc_no_face` | faces=0 | open arc 단독 면 미형성 |
| `intersect_arc_line_clips_to_range` | hits=1 ((2,0)만) | 각도 클립 정확 ((-2,0) off-arc 제외) |
| `arrange_arc_circle_overlap_splits` | faces=2 (ring+disk) | arc-면이 containment 참여 |

**β-3** (Scene production 경로 end-to-end):

| 테스트 | 결과 | 의미 |
|---|---|---|
| `adr200_partial_arc_participates_in_overlap_split` | 부분 arc fragment 2→8 (원 B 와 교차 분할) + 삭제분 부활 0 + arc 보존 + valid | A2 가 못 하던 overlap 재교차 |

**β-4** (Arc×Freeform CCI + wrap + WASM 시연):

- **CCI param 정합 확정 (코드 분석)**: `AnalyticCurve::Arc::evaluate(t)` =
  `circle::evaluate(...t)` → **t = 각도** (Circle 동일). `parameter_range()` =
  `[a0, a1]` → CCI 커널이 arc 각도범위로 자동 클립. split_curve 각도 규약과 정합,
  변환 불필요.
- `intersect_arc_freeform_clips_to_arc_range`: 우측 arc + x=1 Bezier → 2 hits
  (arc 위), x=-1 → 0 hits (off-range). ✅
- `arrange_arc_wraps_past_zero`: arc [7π/4, 9π/4] (각도 0 통과) + chord → 1 cap 면. ✅
- **실제 WASM 시연** (rebuild + 브라우저): 원+secant→2면 / +겹침 원→6면 valid
  (0 violations), edgeKinds arc=8/circle=0/line=11 (**arc 보존**, polygonize 0,
  크래시 0).

회귀 누적: axia-geo 1812 → **1818** (+6: β-1 4 + β-4 2), axia-core 366 → **367**
(+1: β-3). 전체 워크스페이스 **2336 PASS**, 0 regression, 절대 #[ignore] 0.
ADR-199 회귀 2개(부활 차단 + idempotency) β-2 하에서 유지.

## 3.5 DrawArc 통합 (post-γ, A1 user-facing payoff)

엔진 측 호 1급 지원(β-1~γ) 위에, **그린 호가 auto-division 에 참여**하도록 통합.

- `Scene::rederive_after_curve_draw(plane_point, plane_normal)` 신규 — 곡선 draw
  후 호 평면의 coplanar 활성 면을 seed 로 `intersect_faces_inner` 발동. free 곡선
  edge(면 미소속)가 reconstruct 에서 `InputCurve::Arc` 로 투입(β-2)되어 arrange 가
  면 분할. flag OFF 면 no-op. 호출자 transaction 안에서 실행 (단일 Undo).
- `draw_arc_with_curve` (WASM) — arc edge + owner_id 생성 후 `rederive_after_curve_
  draw(center, normal)` 호출.
- 회귀 `adr200_drawn_arc_divides_face`: 원 면 위 가로지르는 호 → 면 1→2 + valid +
  arc 보존(axia-core 368). **실제 WASM 브라우저 시연**: drawArcWithCurve → 면 1→2,
  arc 18 보존, valid (0 violations).
- "선만 그려, 케이크는 알아서 나뉜다" 가 **호까지 확장**.

## 3.6 DrawBezier/BSpline 통합 (freeform 곡선, A1 완전 확장)

호(Arc) 통합(§3.5)을 freeform 곡선(Bezier/BSpline)으로 확장. Arc 보다 gap 하나
더 — reconstruct 의 freeform 분기는 `freeform_curve_source(owner)` 필요:

- `draw_bezier_with_curve` / `draw_bspline_with_curve` (WASM) — owner_id 부여 후
  ① `set_freeform_curve_source(owner, curve)` (reconstruct 가 `InputCurve::Freeform`
  으로 투입 → **smooth 보존**, polygonize 아님) ② 곡선 평면(tessellation best-fit
  normal + pts[0])으로 `rederive_after_curve_draw` 호출.
- reconstruct freeform 분기는 **gate-implicit** (owner_id + source 존재) —
  `freeform_overlap_on_draw` flag 무관 (그 flag 는 Phase 0.5 detection 만 gate).
- 회귀 `adr200_drawn_bezier_divides_face_smooth`: 원 면 위 Bezier → 면 1→2 + valid
  + Bezier edge 보존(axia-core 369). **실제 WASM 브라우저 시연** (기본 flag 상태):
  * Bezier → 면 1→2, **Arc 4 + Bezier 2** (smooth, polygonize 0), valid
  * BSpline → 면 1→2, **Arc 4 + BSpline 2** (smooth), valid
- 모든 곡선 스케칭(호/Bezier/BSpline)이 auto-division 참여 + smooth 보존.

## 3.7 AABB seed 최적화 (post-§3.6)

`rederive_after_curve_draw` 가 coplanar 전체 면을 seed 하던 것을 **곡선 AABB 와
겹치는 면만 seed** 로 좁힘. 시그니처 `(plane_point, plane_normal)` → `(curve_pts:
&[DVec3], plane_normal)` — plane_point=curve_pts[0], AABB=curve_pts min/max.

- 면 필터 = coplanar (첫 loop vert on plane) **AND** `face_world_aabb` 겹침.
- **scope 정확성**: 곡선과 무관한 disjoint coplanar 면은 re-derive 안 함 → edge
  ID 보존 (이전엔 전체 coplanar 면 re-derive → 무관 면 edge 재생성, owner_id 손실
  위험).
- **perf**: 대규모 sketch 에서 곡선 1개 draw 시 해당 영역만 re-derive.
- 회귀 `adr200_aabb_seed_excludes_disjoint_face`: 원 A + 멀리 원 B → A 근처 호 →
  A 분할 + **B Circle edge 무손상**(axia-core 370). **브라우저 시연**: A 분할
  (2→3면), B intact Circle (circleEdges=1), valid.
- callers (draw_arc/bezier/bspline) 가 곡선 점(arc segment / tessellation) 전달.

## 4. Out of scope (후속 / future)

- **DrawNURBS** — DrawNurbsTool 은 NURBS **surface patch** 도구 (2D 곡선 분할 아님).
  NURBS-curve WASM draw 함수 부재. 곡선 draw 추가 시 동일 패턴 적용.
- **DrawSpline smooth** — DrawSplineTool 은 bspline tessellation granularity
  (~4096 edge, syncMesh freeze) 때문에 polyline fallback 중. coarse tessellation
  개선 시 smooth bspline 사용 가능 (별도 perf 트랙).
- 비-Z 평면 arc 의 basis_u frame 정밀도 — β-2 의 미드포인트 방향 결정은 frame
  무관이나, lift_to_curve3d 는 canonical Z/X 사용 (Circle 과 동일 기존 한계).
  Z=0 sketching plane (LOCKED #63) 이 dominant 라 영향 미미.
- Arc×NURBS-surface SSI (3D) — 본 ADR 은 2D planar arrangement (호 ↔ 직선/원/
  freeform-curve). Surface 교차는 별도 트랙.

## 5. Cross-link

ADR-199 (A2 부분 arc 가드 — 직계 predecessor) · ADR-186/189 (유도면 +
analytic_arrange) · ADR-028 (Edge.curve Arc) · ADR-032 (DrawArc 마이그레이션) ·
ADR-030 (CCI 커널 — Arc×Freeform) · AixxiA `xia-form` (곡선 1급 원칙) · 메타-원칙
#5/#6/#14/#16.
