# ADR-257 — α spec: Cylinder Wall Circle Sketching (P3-B, face-split MVP)

- **Status**: Accepted (α~γ closure 2026-06-25 — 6-layer 스택 완성 + real Chromium E2E PASS; §9 Acceptance Log)
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch) — 곡면 sketching frontier (ADR-173 12-gate, S9 cylinder)
- **Author**: WYKO + Claude (de-risk workflow + empirical geometry/code probe)

## 1. Context

ADR-256 (LOCKED, P3 de-risk closure) 가 곡면 sketching 을 dedicated sprint
으로 defer. 사용자 결재 **"P3-B 전용 sprint 진행 — 상세 시뮬 먼저"** → 4-agent
구현 시뮬 + empirical probe 완료. 사용자 결재 **옵션 (A): Polyline +
circle-snap render**.

**P3-B MVP**: 실린더 측면(곡면) 벽에 닫힌 "porthole" 원을 그리면 그 측면 face
를 **cap + remainder 로 분할** (ADR-202 Sphere S9 의 1:1 mirror — extrude 없는
순수 곡면 sketch). ADR-173 곡면 column (S9 cylinder) closure.

## 2. 시뮬레이션 결과 (empirical-validated)

### 2.1 기하 검증 (JS probe, R=10/ρ=4)
- geodesic 샘플 48개 **전부 표면-on** (radial err = 0).
- best-fit-plane residual **0.279mm > 0 = 비평면** (u 0.8 rad straddle) →
  **`AnalyticCurve::Circle` 불가** → polyline (또는 NURBS) 필수.

### 2.2 crux 코드 확증
- **Split**: sphere 경로(`split_sphere_face_by_circle` mesh.rs:3334)는 내부
  tessellate 안 하고 **analytic self-loop Circle 유지** (`add_face_closed_curve`
  mesh.rs:5088 = 1 self-loop edge only). cylinder geodesic polyline 은
  **새 polyline-loop split 필요** (`split_face_by_chain` face_split.rs:568
  building block 재활용).
- **Render**: `tessellate_cylinder_clipped` (mesh.rs:2094)는 **plane-only**
  (oblique half-space gate, mesh.rs:2155-2157) → **circle-snap 신규 필요**.

## 3. Decision (lock-in)

- **D1 MVP = face-split-only** (cap + remainder, extrude 없음). ADR-202
  Sphere 1:1 mirror. Pocket/through-hole 은 orthogonal (ADR-190 family, defer).
- **D2 curve repr = polyline** (Vec\<DVec3\> geodesic 샘플). split 이 어차피
  polyline-bound (D1 §2.2) + NURBS 는 render reuse 0 + machinery → NURBS defer.
- **D3 render = circle-snap (목표) + HARD-flag fallback (in-sprint)**.
  목표는 sphere 완전 visual parity (geodesic crossing-snap). β-4 5-probe
  de-risk 후 over-budget 시 **polyline + HARD-flag** (메타-원칙 #15 arc 선례,
  facets 보이나 topology 정확, gate 닫힘)으로 mid-sprint 강등. (A)/(B)는
  β-1~3,5~7 공유 → **rework 0**.
- **D4 MVP cylinder only** (Cone/Torus = 후속). ρ < πR clamp (self-overlap
  guard) → 초과 시 graceful None + Toast.

## 4. 6-Layer reuse map (ADR-202 mirror)

| Layer | 처리 | 변경 |
|---|---|---|
| L1 `project_to_cylinder` | 신규(작음) | closed-form (h=(p-o)·axis; surface=foot+R·radial̂), sphere.rs:78 mirror |
| L1 `circle_on_cylinder` | template (sphere.rs:165) | **Vec\<DVec3\> polyline 반환**; unroll(ρ=R·angle, N 샘플 24-64) + map-back `cylinder::evaluate` + ρ<πR clamp |
| L2 `split_cylinder_face_by_circle` | template + 신규 boundary | twin-HE reparent (annulus.rs:319) **REUSE** (surface-agnostic); 1-edge self-loop → N-edge polyline loop 삽입 신규 (split_face_by_chain 재활용); cap+remainder 둘 다 Cylinder surface 상속 (ADR-089 A-χ) |
| L3 render circle-snap | **REWRITE (critical)** | co-cylindrical twin-gate (cheap, twin_role mesh.rs:1919 analog) + geodesic crossing-snap Sutherland-Hodgman (expensive ~250-400 LOC, tessellate_sphere_clipped mesh.rs:1871 mirror) |
| L4 `Scene::draw_circle_on_cylinder` | template-copy | scene.rs:2632 boilerplate, 2줄 swap |
| L5 WASM `drawCircleOnCylinder` + TS | template-copy | lib.rs:4554 mirror + WasmBridge wrapper |
| L6 `DrawCircleTool` surfaceKind===2 | template-copy | DrawCircleTool.ts:62 parallel branch |

## 5. Lock-ins

- **L-257-1** MVP face-split-only (no extrude), ADR-202 1:1 mirror.
- **L-257-2** curve repr = polyline (geodesic 샘플); NURBS defer.
- **L-257-3** render = circle-snap 목표 + HARD-flag in-sprint fallback
  (β-4 de-risk gated); (A)/(B) β-1~3,5~7 공유 rework 0.
- **L-257-4** geometry empirical-validated (표면-on + 비평면) — project_to_
  cylinder + unroll 공식 §2.1.
- **L-257-5** twin-HE reparent REUSE (annulus.rs:319 surface-agnostic);
  cap+remainder Cylinder surface 상속 (ADR-089 A-χ).
- **L-257-6** co-cylindrical twin-gate 필수 (render) — Boolean cap /
  무관 face mis-clip 차단 (sphere co-spherical gate mirror, L-83-5).
- **L-257-7** ρ < πR clamp (self-overlap guard) graceful None.
- **L-257-8** MVP cylinder only (Cone/Torus 후속 ADR).
- **L-257-9** Path Z atomic 8 sub-step; β-4 5-probe de-risk = α exit
  criterion; 사용자 시연 게이트 (ADR-087 K-ζ) γ 필수.
- **L-257-10** 절대 #[ignore] 금지.

## 6. Sub-step plan (Path Z atomic, ~13-17일)

| sub-step | 내용 | 비용(일) | risk |
|---|---|---|---|
| **α (본 spec)** | ADR + lock-in + render 전략 + β-4 5-probe de-risk 계획 | 1 | LOW |
| β-1 L1 project | `project_to_cylinder` + 회귀 | 1 | LOW |
| β-2 L1 curve-gen | `circle_on_cylinder` polyline (unroll + clamp) + 회귀 | 2 | MEDIUM |
| β-3 L2 split | `split_cylinder_face_by_circle` (polyline loop + twin reparent + surface 상속) + 회귀 | 2-3 | MEDIUM |
| **β-4 L3 render** | co-cylindrical twin-gate + geodesic crossing-snap (또는 fallback HARD-flag) | **3-5** | **CRITICAL** |
| β-5 L4 scene | `Scene::draw_circle_on_cylinder` | 1 | LOW |
| β-6 L5 bridge | WASM + TS wrapper | 1 | LOW |
| β-7 L6 dispatch | DrawCircleTool surfaceKind===2 branch | 1 | LOW |
| **γ E2E** | real Chromium 시연 (벽 원 → split → manifold valid → smooth) + 회귀 봉인 + 사용자 시연 게이트 | 1 | MEDIUM |

**β-4 5-probe de-risk (α exit criterion)**: axis-aligned belt circle /
oblique-straddling / near-clamp ρ≈πR / small porthole / off-axis center.
probe 후 circle-snap over-budget 판정 시 (B) HARD-flag 강등 결재.

## 7. Q1~Q5 (β 진입 전 결재 — α 단계 잠정)

- Q1 MVP scope: face-split-only (D1) ✅ 결재됨
- Q2 curve repr: polyline (D2) ✅ 결재됨
- Q3 render: circle-snap + HARD-flag fallback (D3) ✅ 결재됨
- Q4 surface: cylinder only MVP (D4) ✅ 결재됨
- Q5 render 강등 결정: β-4 de-risk probe 결과 후 (사용자 결재)

## 8. Cross-link

- ADR-256 (P3 defer — dedicated sprint source) + LOCKED #83 ADR-202 (Sphere
  S9 6-layer template) + ADR-173 12-gate (S9 곡면 column)
- ADR-089 A-χ (split surface 상속, LOCKED #35) / annulus.rs:319 (twin-HE reparent)
- ADR-189 (polygon-facet → Arc lesson — polyline facet risk 선례) / 메타-원칙
  #15 (HARD flag, render fallback 선례)
- ADR-205 (tessellate_cylinder_clipped — plane-only, render REWRITE 대상)
- ADR-087 K-ζ (사용자 시연 게이트) / ADR-046 P31 #4 (additive) / LOCKED #44
- 메타-원칙 #5 #6 #14 #15

## 9. Acceptance Log (α~γ closure, 2026-06-25)

6-layer 스택 (ADR-202 Sphere S9 mirror) Path Z atomic 8 sub-step:

| sub-step | layer | commit | 회귀 |
|---|---|---|---|
| α | spec | `d871c02` | — |
| β-1+β-2 | L1 geometry (`project_to_cylinder` + `circle_on_cylinder`) | `e8618d0` | axia-geo +11 |
| β-3 | L2 split (`split_cylinder_face_by_circle`) | `7a86645` | axia-geo +4 |
| β-4 | L3 render (`tessellate_cylinder_circle_clipped`) | `9c054d9` | axia-geo +3 |
| β-5 | L4 scene (`Scene::draw_circle_on_cylinder`) | `ee8a644` | axia-core +2 |
| β-6 | L5 bridge (WASM `drawCircleOnCylinder` + TS wrapper) | `75a988e` | vitest +3 |
| β-7 | L6 dispatch (DrawCircleTool surfaceKind===2) | `e18d96a` | vitest +4 |
| γ | E2E (real Chromium + prod build + fresh WASM) | `0788d80` | Playwright +2 |

**누적**: axia-geo +18 (2035), axia-core +2 (405), vitest +7 (bridge 3 + tool 4),
Playwright +2. 모두 PASS, 절대 #[ignore] 금지 준수.

**검증**: E2E 2/2 (single porthole split + two-porthole multi-hole, 둘 다 cap/
remainder Cylinder + manifold 0 violations + full 3D solid) + dev 서버 probe
(drawCircleOnCylinder export 존재 + flow + manifold) — 사용자 시연 readiness 확인.

## 10. Lessons (구현이 α plan 과 갈라진 지점 — canonical)

- **L-257-1 (β-4 render = UV-earcut, NOT Sutherland-Hodgman)**: α §6 은 sphere
  의 crossing-snap (`tessellate_sphere_clipped` mirror) 을 가정했으나, 원통은
  **developable** 이므로 unroll → `earcutr::earcut(uv-polygon, holes, 2)` 가
  자연스럽고 정확. cap = earcut(boundary uv) + seam-unwrap; remainder =
  earcut(band uv-rect + cap hole, seam-rotated). `tessellate_arc_bounded_face`
  의 UV-earcut 패턴을 Cylinder 로 mirror. **Q5 결과: circle-snap 채택 확정 —
  5-probe de-risk(belt+seam/non-seam/tiny/near-clamp/off-axis) PASS → B
  HARD-flag fallback 불필요.**
- **L-257-2 (β-5 ownership = dual-path, NOT sphere XIA-only)**: sphere 템플릿
  (`draw_circle_on_sphere`) 은 `face_to_xia` 만 reconcile (Shape-owned host →
  cap orphan 잠복 gap). cylinder 는 primitive(create_cylinder→XIA) + form-layer
  draw(exec_create_solid→Shape) 둘 다 흔해 **`exec_create_solid` dual-path
  (owning_shape 먼저, 없으면 owning_xia) mirror**. 4-agent 병렬 audit 가 sphere
  의 이 gap 을 노출.
- **L-257-3 (β-3 input = `Vec<DVec3>` geodesic polyline, NOT AnalyticCurve)**:
  원통 위 geodesic 원은 unroll 시 평면 원이지만 3D 로는 **비평면 N-edge
  polyline** → `AnalyticCurve::Circle` self-loop 으로 표현 불가 (sphere 와의
  핵심 차이). split 도 `add_face_with_holes` (N-gon) + N-edge twin reparent
  (annulus.rs 1-edge → N-edge 일반화).
- **L-257-4 (audit-discovered gaps, 둘 다 defer — sphere parity)**: ① split 이
  cap 에 `face_surface_owner_id` 미전파 (click group-select 안 됨 — sphere 동일,
  sketched region 개별 선택이 의미상 OK) ② Group membership 는 split 시 stale
  (어떤 split op 도 reconcile 안 함 — 기존 한계). 둘 다 P3-B 범위 외 future.

## 11. 후속 트랙 (별도 작업)

- **곡면 column 잔여**: Cone / Torus 벽 sketching (β-4 UV-earcut 은 Cone/Torus
  도 developable/quad-param 이라 mirror 가능 — 별도 ADR), Sphere 의 line(S3) /
  rect(S6) 등 비-circle 곡면 sketch.
- **비-manifold extrude 진단 (사용자 시연 2026-06-25)**: 큰 박스 면에 작은 rect
  → extrude 시 footprint 가 ≥3-face 비-manifold 로 남아 ADR-047 R1 주황 overlay
  점등. 실증: 깨끗한 box+rect 는 manifold(nm=0)이나 extrude 단계에서 발생;
  auto-intersect sub-face 가 Plane surface 없이(kind 0) 나오는 의심 지점 확인.
  ADR-102 cleave 가 이 시나리오를 커버 못하는 것으로 보임 — 별도 ADR (extrude
  footprint manifold fix). 본 P3-B (곡면 sketch) 와 직교.
