# ADR-201 — Bounded Sub-Range Freeform Draw (DrawSpline smooth)

- **Status**: Accepted
- **Date**: 2026-06-15
- **Track**: 곡선 면분할 — ADR-200 후속, freeform 곡선 스케칭 smooth + bounded
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

ADR-200 으로 모든 곡선(호/Bezier/BSpline)이 auto-division 에 참여한다. 그러나
**DrawSplineTool 은 여전히 polyline fallback** (`drawPolylineAsShape`, 96 samples)
을 쓴다 — `drawBSplineWithCurve` 가 `tessellate(0.001)` 로 **~4096 line 세그먼트**
를 만들어 syncMesh 를 freeze 시키기 때문 (사용자 보고 2026-06-05). 결과: DrawSpline
은 analytic B-spline 정체성을 잃고 polyline 이 된다.

### α audit finding (중요)

**sub-range 인프라가 이미 존재한다**:
- `AnalyticCurve::split_at(t, mid_vert)` (`curves/synthesize.rs:250`) — Bezier/
  BSpline/NURBS 모두 **shape-preserving reparametrized 분할** (회귀 테스트 존재).
- `face_rederive::extract_world_subcurve` (`:887`) — `split_at` 2회로 `[t0,t1]`
  sub-curve 추출. rederive 로 분할된 freeform 은 이미 올바른 sub-curve(매끈) 로
  materialize 됨.

→ 진짜 gap 은 **초기 draw 표현**: `draw_bspline_with_curve` 가 (a) chord_tol
0.001 로 과도 세그먼트 + (b) 각 세그먼트에 **full curve** 부여 (sub-range 아님).
draw_arc 는 이미 각 세그먼트에 **sub-angle** [a0,a1] 부여하는데, freeform 은
full curve 라 render 가 세그먼트마다 full curve tessellate → coarse 화하면 overlap.

## 2. Decision (proposed)

draw_bezier/bspline 을 **bounded 세그먼트 + 세그먼트별 sub-range 곡선**으로 전환
(draw_arc 의 sub-angle 패턴을 freeform 으로). 그 후 DrawSplineTool 이 analytic
B-spline 을 사용.

### Lock-ins (proposed)

- **L-201-1** 세그먼트별 sub-range — `draw_*_with_curve` 가 각 세그먼트 edge 에
  `split_at` 로 추출한 `[t_i, t_{i+1}]` sub-curve 부여 (full curve 아님). render
  (`he_arc_fill_points` + edge wireframe)가 sub-curve 만 tessellate → coarse
  세그먼트도 smooth, overlap 없음.
- **L-201-2** Bounded 세그먼트 수 — 적응적 chord_tol 이되 **cap (예: 32~64)**.
  syncMesh freeze 차단. render 가 sub-curve 로 smooth 채움.
- **L-201-3** owner_id + freeform_curve_source 유지 (ADR-200 §3.6) — division
  참여 + idempotency.
- **L-201-4** DrawSplineTool flip — `drawPolylineAsShape` → `drawBSplineWithCurve`.
  analytic B-spline 정체성 복원 (selection/IFC/downstream kernel-aware).
- **L-201-5** rederive 의 freeform materialize 경로 무변경 (이미 sub-range 처리).
- **L-201-6** ADR-200 AABB seed / division 회귀 유지. 절대 #[ignore] 금지.

### Sub-step (Path Z atomic)

| step | 내용 | 상태 |
|---|---|---|
| α | 본 spec (audit + 설계) | ✅ `8f9ed41` |
| **β-1** | `AnalyticCurve::subcurve(t0,t1)` 신규 + `draw_bspline/bezier_with_curve` bounded(SEGS=64) uniform 샘플 + 세그먼트별 sub-range 곡선 + 시뮬 | ✅ `26ba5c2` |
| **β-2** | DrawSplineTool flip (polyline → drawBSplineWithCurve, clamped knots + degree=min(3,N-1), -1 시 polyline graceful fallback) + 브라우저 검증 | ✅ 본 커밋 |
| **β-3** | **vanish 버그 fix** (open freeform near-but-not-crossing face → 미분할 시 snapshot 복원, 곡선 보존) + 적대적 검토 3-각도 + A1 방어 회귀 + degree-1 회귀 + Undo 무결성(브라우저) | ✅ 본 커밋 |
| γ | closure + LOCKED. follow-up: (a) A1 robust guard(전역 count → rebuild 구조 signal), (b) scoped snapshot 성능, (c) `face_surface_reversed` latent restore 패치 | 후속 |

### β-3 — non-crossing freeform vanish fix

**버그 (시뮬레이션 확정)**: open freeform(DrawSpline/Bezier)이 면 근처(AABB
overlap)지만 가로지르지 않으면, rederive 의 reconstruct→arrange 가 곡선 edge 를
제거하고 재생성 안 해(open curve 는 면 미형성) **곡선이 사라짐** (데이터 손실).

**fix**: `rederive_after_curve_draw` 가 rederive 전 snapshot → face count 미증가
(분할 미발생)면 `restore_scene_snapshot` 으로 복원 → 곡선을 그린 그대로 보존.
분할(face +1) 시만 rederive 결과 keep. point-in-face 기반 crossing 판정의
boundary-touch fragility(arc 끝점이 면 경계 위) 회피 — 결과(face count) 기반이라
robust.

- 회귀 `adr201_b3_lone_freeform_near_face_preserved`: 원 + 코너 밖 B-spline →
  rederive 후 bspline edge 보존(미소실) + faces 불변 + valid.
- 회귀 `adr201_b3_two_point_degree1_spline`: degree-1 (2 control pt) B-spline =
  직선도 crash 없이 처리(subcurve degree-1) → 원 가로지름 시 분할.
- 회귀 `adr201_b3_promoted_hole_not_rolled_back_by_nearby_spline`: **A1 방어** —
  rect 안 원(자동 promote 된 hole) + 근처 비교차 spline → rederive. hole 은
  snapshot 에 이미 존재하므로 restore 가 **보존**(롤백 아님) + spline 보존
  (axia-core 374).
- **Undo 무결성 (브라우저)**: snapshot/restore 가 transaction 에 투명 — 분할
  spline → undo → 원만(단일 step) / 보존 spline → undo → 원만(spline 제거, 단일
  step). 둘 다 valid + can_undo 정합.
- **브라우저**: 원 밖(코너) spline → faces 1 유지 bspEdges 64 보존 / 원 가로지름
  spline → faces 1→2 분할 Arc4+BSpline16 smooth (둘 다 valid). 전체 2344 PASS.

### β-3 적대적 검토 (커밋 전 3-각도 독립 audit)

snapshot/restore guard 는 core scene 메서드라 커밋 전 병렬 적대적 검토:

1. **Snapshot 완전성 — SOUND**: rederive 경로(`rederive_coplanar_on_draw` +
   legacy)가 변경하는 모든 Scene/Mesh 상태(mesh / xias / face_to_xia / owner-id
   maps / freeform_curve_source / boundary_loops / next_* counters)는 모두
   round-trip(restore 또는 source-of-truth 재구성). 유일한 serialized-but-not-
   restored 필드 `mesh.face_surface_reversed` 는 Boolean concave-subtract 전용
   이라 이 경로엔 미작성 → 이 롤백엔 무관(benign). *별도 latent 패치 권장*.
2. **face-count 휴리스틱 — SOUND-BUT-FRAGILE (A1 documented limitation)**:
   `faces_after <= faces_before` 는 **net-zero 토폴로지 편집**(containment
   ring+hole 승격, scene.rs:2153-2200 post-process)을 "분할 없음"으로 오판할
   여지. **단, 생산 플로우(원 그리기 → 즉시 promote → 스플라인)에서는 hole 이
   snapshot 에 이미 존재 → 안전** (회귀 `adr201_b3_promoted_hole_not_rolled_back_
   by_nearby_spline` 로 lock). A1 오발화는 "promote 가능하나 미-promote 된
   containment 가 open-curve rederive 중 promote 되는" *비정상 시퀀스* 한정이며,
   그조차 **비파괴적(놓친 부수 승격, 다음 op 가 재-promote)** — β-3 가 고치는
   *데이터 손실*보다 약함. **future robust guard**: 전역 count 대신 rebuild 의
   구조 변경 signal(`created_faces` + containment promote 여부) 사용(γ-follow-up).
3. **성능 — SHIP-WITH-NOTE**: `scene_snapshot()` 은 full-scene bincode (O(scene)).
   단 `seeds` 비어있으면 미실행(disjoint draw 무비용), 그리고 3 생산 caller 는
   이미 transaction 용 snapshot 2회를 찍으므로 본 guard 는 coplanar 곡선 커밋당
   3번째 — asymptotic 동일, 상수 1.5×. 대형 스케치(100k HE)에서 메타-원칙 #11
   Commit<100ms 근접 가능 → **follow-up**: seed-face/mesh-only scoped snapshot.

**커밋 결정**: SHIP. β-3 는 실제 데이터 손실(곡선 소실)을 고치고, A1 은 비파괴적
edge-case 로 회귀 테스트로 경계 확정 + future robust-guard 로 추적. error-path 는
기존(pre-change)과 동일(neutral). 모든 call-site 정합.

### β-2 검증

- DrawSplineTool.test (7 tests): drawBSplineWithCurve 호출(clamped knots/degree)
  + kernel reject(-1) → polyline fallback + 2pt degree-1. vitest 2185 passed.
- **브라우저** (5-control-point spline degree 3 가 원 가로지름): 면 1→2 +
  Arc4+BSpline16 (smooth) + 20 edges (bounded) + valid. DrawSpline 이 이제
  analytic smooth B-spline 을 그리고 auto-division 참여 (이전 polyline fallback).

### β-1 검증 (실측)

- `subcurve_shape_preserving` (synthesize) — Bezier/BSpline endpoint+midpoint
  preserve (axia-geo 1819).
- `adr201_bounded_subrange_bspline_divides_smooth` (scene) — 원 + B-spline →
  면 1→2 + BSpline edge 보존 + edge < 200 (bounded) + valid (axia-core 371).
- **실제 WASM 브라우저**: lone B-spline = **64 edges** (이전 ~4096, freeze 차단),
  원 분할 → 면 1→2 (Arc 4 + BSpline 10, smooth), valid. 전체 워크스페이스 2341.

## 3. 검증 계획

- β-1 시뮬: bounded sub-range bspline → DCEL edge ≤ cap + render smooth + 면 분할.
- β-2 브라우저: DrawSpline 으로 그린 bspline 이 smooth + 면 분할 + syncMesh 정상
  (freeze 없음).
- 회귀: draw_bspline edge count bounded + sub-range 부착 + division.

## 4. Out of scope

- DrawFreehandTool / DrawBezierTool 의 동일 전환 (Bezier 는 이미 작동, Freehand 는
  별도).
- DrawNURBS curve 도구 신설 (DrawNurbsTool 은 surface patch).
- 적응적 LOD (camera-distance) freeform tessellation (ADR-135 호환, 별도).

## 5. Cross-link

ADR-200 (곡선 1급 + DrawArc/Bezier/BSpline 통합) · ADR-089 (닫힌 곡선 kernel-
native — 본 ADR 은 열린 곡선 sub-range 버전) · ADR-032 (DrawBezier/BSpline) ·
ADR-135 (render chord_tol) · `AnalyticCurve::split_at` (synthesize.rs) · 메타-원칙
#5/#6/#11/#14.
