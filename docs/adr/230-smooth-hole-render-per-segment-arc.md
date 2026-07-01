# ADR-230 — Smooth Hole Render (Phase 0.5 — per-segment Arc)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Hole 커널 심화 (ADR-222 Phase 0.5)
- **Depends on**: ADR-222 (hole circle metadata Phase 0) / ADR-092 (per-segment Arc render
  pattern) / ADR-088 (curve_owner_id) / ADR-194 (punch_circular_hole) / ADR-135 (render chord_tol)

## 1. Context

ADR-222 Phase 0 가 punched circular hole 의 inner-loop edge N개에 단일
`AnalyticCurve::Circle` (+ 공유 curve_owner_id) 를 부착했다. 그러나 render edge sampler
(`mesh_export.rs::he_arc_fill_points`) 는 **Arc / Bezier / BSpline / NURBS arm 만 smooth
tessellate, `Circle` 은 `_ => Vec::new()` → 직선 chord** (ADR-222 §교훈 / ADR-092 finding).
즉 non-self-loop edge 위의 Circle 은 한 변이 직선으로 그려져 **구멍이 N각형 polygon 으로**
보였다 (self-loop Circle 은 `loop_verts.len()==1` fast-path 로 smooth — 별개).

## 2. Decision — per-segment Arc (ADR-092 패턴)

punch_circular_hole 의 inner-loop edge 부착을 **단일 Circle → per-segment Arc** 로 전환.
각 edge 는 그 두 endpoint 사이의 **minor subarc** Arc 를 carry (공유 owner_id 유지):
- render face fill (`he_arc_fill_points` Arc arm) + edge wireframe (`tessellate_edge` →
  `Arc.tessellate`) **둘 다 subarc 를 chord_tol 로 smooth tessellate** → 매끈한 ring.
- downstream 은 임의 Arc 에서 center/radius 읽기 가능 (Arc 가 Circle 의 모든 필드 + angular
  span 보유). curve-aware selection 은 공유 owner_id 로 grouping (ADR-088).

**구현** (`mesh.rs` punch_circular_hole, ADR-222 Circle 블록 교체):
- `basis_v = normal × basis_u` (arc::tessellate 와 일치) + `angle_of(p) = atan2((p-c)·bv, (p-c)·e1)`.
- loop 규약 (`he_arc_fill_points`): `dst(hes[i]) == verts[i]`, `origin == verts[i-1]` (wrap).
- 각 edge: `a0 = angle_of(origin)`, `d = angle_of(dst) - a0` 를 `(-π, π]` 로 정규화 (minor arc,
  segment 방향) → `Arc { center, radius, normal, basis_u, start_angle: a0, end_angle: a0 + d }`.
- render `he_arc_fill_points` 가 origin→dst 로 orient 하므로 Arc 방향은 자동 정합.

## 3. Lock-ins

- **L-230-1** per-segment Arc (minor subarc, 공유 owner_id) — ADR-222 단일 Circle 부착 supersede.
  ADR-092 (Push/Pull top rim) 패턴 답습.
- **L-230-2** topology UNCHANGED — N polygonal edge 그대로, Arc 는 metadata 만 (8 punch_* 테스트
  + manifold invariant 보존).
- **L-230-3** circular hole 한정 — `punch_rect_hole` (Window) 는 진짜 직사각 (직선 edge =
  chord 정확), 미접촉.
- **L-230-4** 두 render path 모두 smooth — face fill (he_arc_fill_points Arc arm) + edge
  wireframe (tessellate_edge → Arc.tessellate subarc). Circle render arm 추가 불요 (Arc 가
  기존 path 자연 활용).
- **L-230-5** downstream/selection 보존 — Arc center/radius (Circle 동등) + 공유 owner_id.
- **L-230-6** render-only 효과 — DCEL/manifold 변경 0 (ADR-007 invariant 보존).
- **L-230-7** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo lib **1990 PASS** — `adr230_punched_hole_inner_edges_carry_arc_curves` (Circle→Arc
  + **span-sum 가드**: per-segment subarc 합 ≈ 2π — proper minor subarc 증명, full circle 이나
  잘못된 span 이 아님) + `adr222_second_punch_tags_only_new_hole` (Arc, distinct owner per hole)
  + 8 punch_* (manifold/topology 불변). 테스트 net +0 (1 rename + 2 갱신, span-sum 가드 추가).
- TS/vitest 변경 0 (engine-only). WASM 재빌드.

## 5. 브라우저 검증 (real WASM)

- rect 2000×2000 → 4 edge segments (직선).
- `punchHole([0,0,0],[0,0,1], 300, 48)` → hole 경계 **384 edge segments** (48-segment Arc 가
  ≈8 sub-segment/edge 로 tessellate) → **SMOOTH** (vs 48 = chord/polygon).
- 384 >> 48 → per-segment Arc smooth 렌더 확정.

## 6. Lessons

- **L1** non-self-loop edge 의 Circle = chord (render `_ => Vec::new()`); smooth 하려면
  per-segment Arc (명시 start/end angle). self-loop Circle (loop_verts==1) 만 smooth fast-path.
- **L2** ADR-092 Push/Pull top rim 과 동일 패턴 — ring boundary 의 N edge 는 per-segment Arc.
  향후 ring/hole boundary smooth render 는 본 패턴 답습.
- **L3** minor-arc 정규화 (`d ∈ (-π, π]`) — 인접 verts 의 minor arc 가 inscribed polygon 의
  circle 경계. render 의 origin→dst orient 가 방향 자동 처리 → 부착 시 방향 무관.
- **L4** span-sum 가드 (Σ|arc span| ≈ 2π) = per-segment subarc 의 강한 regression 증명 (full
  circle/잘못된 span 차단).

## 7. 후속 (별도 트랙)

- smooth hole 의 **사용자 facing 시연** (DrawHoleTool 로 실제 그려 매끈 확인) — slow channel / manual.
- ADR-222 Phase 1 (Revolve multi-loop) / Phase 2 (Boolean) — Arc center/radius downstream 활용.
- 다른 ring boundary (annulus / Path B cylinder rim) 의 per-segment Arc 일관성 audit.

## 8. Cross-link

- ADR-222 (hole circle metadata Phase 0 — 본 ADR 이 Circle→Arc 정련) / ADR-092 (per-segment Arc
  render 패턴 source — Push/Pull top rim) / ADR-088 (curve_owner_id grouping) / ADR-194
  (punch_circular_hole) / ADR-135 (render chord_tol) / ADR-007 (manifold invariant 보존).
- 메타-원칙 #14 (면은 닫힌 경계로부터 — hole boundary = analytic arc) / LOCKED #44 (Complete
  Meaning per Merge) / ADR-046 P31 #4 (additive — 사용자 API/topology 무변경).
