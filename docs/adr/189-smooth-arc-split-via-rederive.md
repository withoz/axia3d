# ADR-189 — 곡선 경계 매끈 분할 (Arc-Preserving Split via Re-Derive Arrange)

> ADR-174 §L-174-12 가 "future ADR" 로 deferred 한 **Approach B (true 2-arc)** 의
> realization. ADR-174 의 Approach A (polygonize) 는 **legacy (faceRederive OFF)
> 경로로 first-class 보존** — 본 ADR 은 supersede 가 아닌 **gated 확장**이다.

- **Status**: Accepted
- **Date**: 2026-06-09
- **Realizes**: ADR-174 §L-174-12 (future ADR) + §6 Out of scope "Approach B"
- **Track**: 6 (boundary kernel / 유도면)

---

## Canonical anchor (사용자 보고 + 결재, 2026-06-09)

> 스크린샷: 겹친 원/사각형에 각진 "간섭 라인" 다수.
> "이렇게 간섭라인들이 생기는 이유는?"
> → 원인 audit: 원이 직선/사각형에 잘릴 때 **polygonize (28-gon)** 됨 (ADR-174
>   Approach A).
> 결재: **(b)** "원↔직선/사각형 교차 시 다각형화 제거 (매끈한 Arc 유지)" +
>   "**자동 분할은 반드시 되어야 돼**".

---

## 1. Problem — Approach A 의 polygonize 손실

ADR-174 Approach A (LOCKED #75 L-75-1): 직선이 Path B Circle self-loop face 를
가로지르면 `exec_draw_line` Step 0 pre-pass 가 `polygonize_closed_curve_face` 로
원을 ~28 직선으로 변환한 뒤 직선 파이프라인 (ADR-172) 으로 분할한다. 이유 —
`find_line_crossings` 가 self-loop 곡선 edge 를 못 잡음 (양 endpoint 동일 anchor
→ d2=0 "평행").

결과 (실측): kernel-native Circle (1 edge, smooth) → **28 직선 edge**. 겹침 多발
장면에서 각진 "간섭 라인" 난립 (사용자 보고).

---

## 2. 왜 지금 가능한가 — Approach B 의 재정의 (low-risk route)

ADR-174 §2.1 의 deferred Approach B 는 "**self-loop edge 를 2 arc edge 로 직접
split**" (Bug 1D self-loop split 선해소 필요 → multi-week + 높은 회귀 위험) 으로
정의됐다.

**본 ADR 의 통찰 (사전검토 발견)**: 그 위험한 self-loop split 없이도, **arc-aware
re-derive arrange (ADR-186, `analytic_arrange.rs`) 가 이미 "2D Line + Circle →
arc 경계 면" 을 한다**. 원×원 교차가 Arc 를 유지하는 게 그 증거. 즉 원이 깨지는
유일한 이유는 *draw 단계의 polygonize 한 줄* 이고, 그걸 arc-aware 경로로 대체하면
self-loop split 신규 primitive **0** 으로 목표 달성.

→ Approach B 의 *goal* (매끈 arc 분할) 을, *mechanism* (self-loop split) 대신
**arrange route** 로 realize. Bug 1D carve-out 위배 0, multi-week → single-commit.

---

## 3. Solution — B-route (gated faceRederive ON)

`exec_draw_line` (scene.rs):

1. **Step 0 게이팅** — `face_rederive_on_draw` ON 이면 polygonize 안 함; 교차된
   Circle face id 만 수집 (`rederive_circle_seeds`).
2. **standalone line 트리거** — no-epoch 분기에서 `intersect_faces_inner(seeds)`
   호출 → rederive arrange 가 Circle (`InputCurve::Circle`) + line
   (`InputCurve::Line`) 재구성 → arc-bounded half-disk.
3. **rect / circle (epoch)** — finalizer rederive 의 affected-region scope 가
   겹친 원을 이미 포함 → 자동 arc 분할 (`exec_draw_rect` = `exec_draw_line` ×4).
4. **legacy (faceRederive OFF)** — Approach A (polygonize) 보존.

---

## 4. Lock-ins

- **L-189-1** B-route = arrange route (self-loop split 신규 primitive 0, Bug 1D
  회피). ADR-174 Approach B 의 *goal* realize, *mechanism* 변경.
- **L-189-2** Gated faceRederive ON only — production default 는 arc, legacy OFF
  는 polygonize (Approach A) 보존 (ADR-174 §L-75-1 회귀 자산 불변).
- **L-189-3** 자동 분할 유지 강제 (사용자 요구 canonical) — face split 결과
  invariant 불변, edge geometry (arc vs line) 만 변경.
- **L-189-4** curves/ 미접촉 (NURBS kernel carve-out, L-174-5 / L-70-5 정합) —
  line-circle 교차는 arrange 의 closed-form (operations 레벨).
- **L-189-5** ADR-174 supersede 아님 — additive gated 확장. Approach A 는 legacy
  경로로 first-class 보존.
- **L-189-6** 메타-원칙 #14 (곡선 경계 → disk-topology) + #15 (split contract) +
  #16 (WHEN gate) 보존.
- **L-189-7** 절대 #[ignore] 금지.

---

## 5. Acceptance Log

### 5.1 사전검토 + 결재 (2026-06-09)
- 사용자 스크린샷 → 원인 audit (polygonize = ADR-174 Approach A, scene.rs:4402).
- 사전검토 발견: arrange 가 이미 Line+Circle→Arc → self-loop split 불필요 (risk
  재정의).
- 결재: B-route (arrange 재사용) + faceRederive ON gating.

### 5.2 구현 — commit `65b6484` (LOCAL, adr-186/boundary-kernel-port)
- `scene.rs` +135: Step 0 게이팅 + standalone-line 트리거 + 회귀 2.
- 회귀: `adr174_approach_b_line_thru_circle_keeps_arcs` (ON → arc, ≤a few lines,
  manifold) + `adr174_approach_b_legacy_off_still_polygonizes` (OFF → polygon).
- 워크스페이스: axia-core 336 / axia-geo 1694 / foreign 138 / transaction 4 /
  wasm 8 — **2180 PASS, 0 failed, 0 ignored**.

### 5.3 브라우저 검증 (production defaults, ADR-087 K-ζ)
| 시나리오 | 이전 | 이후 |
|---|---|---|
| 직선 → 원 | 28 직선 | 2면, **4 Arc + 1 chord**, manifold valid |
| 사각형 → 원 | 다각형 | 5면, **8 Arc + 8 Line**, valid |
| 원2 + 사각2 (스크린샷) | 다각형 난립 | 37면, **56 Arc**, valid 0 violations |
| legacy OFF | 다각형 | **28 직선 보존** (Approach A) |

---

## 6. Out of scope (ADR-174 §6 잔존)

- 곡면 *surface* 위 drawing (S3/S6/S9 — sphere/cylinder face 위 line) — 별개 트랙.
- Bezier/BSpline/NURBS self-loop 경계 polygonize (현재 `None`).
- self-loop split primitive (`split_edge` self-loop 분기, Bug 1D) — 본 ADR 이
  *우회* (불필요화). primitive 자체는 미해결로 잔존 (offset/fillet 가 정확 단일
  arc edge 를 요구하는 downstream trigger 시 별도 ADR).
- Arc-edge 다단 crossing 정밀도 (multi-edge face 의 arc segment crossing).

---

## 7. Cross-link

### ADR
- **ADR-174** §L-174-12 / §2.1 / §6 (deferred Approach B — 본 ADR 이 realize)
- **ADR-186** 유도면 re-derive (`analytic_arrange.rs` arrange route source)
- **ADR-089** Path B closed-curve Circle (1 anchor + 1 self-loop edge)
- **ADR-172** 직선 crossing pipeline (Approach A 가 재사용)
- **ADR-105** polygonize helper (Approach A, legacy 경로 보존)
- **ADR-049** P-5e-α (engine default OFF + production ON gating 패턴)
- **ADR-087** K-ζ (사용자 시연 게이트)

### LOCKED / 메타-원칙
- **LOCKED #75** (ADR-174) — amendment note (Approach B realized)
- **메타-원칙 #14** (곡선 경계 → disk-topology) / **#15** (split contract) /
  **#16** (WHEN gate) — 보존
- commit `65b6484`

---

## 8. Lessons

- **L1 — 사전검토가 risk 를 재정의** — deferred "multi-week self-loop split" 가
  실은 arrange route 로 *single-commit 저위험* 가능했다. 매트릭스 audit 이
  deferred 가정 (high risk) 을 무효화한 사례 (audit-first canonical).
- **L2 — Gated 확장 패턴** (ADR-049 P-5e-α 답습) — production 새 동작 (arc) +
  legacy 회귀 자산 보존 (polygon) 동시. supersede 회피.
- **L3 — 기존 자산 재활용** — arrange 가 이미 "Line + Circle → arc 경계 면" 을
  하므로 신규 geometry 코드 0 (메타-원칙 자산 재활용). 원이 깨지던 유일 원인은
  draw 단계 polygonize 한 줄.
