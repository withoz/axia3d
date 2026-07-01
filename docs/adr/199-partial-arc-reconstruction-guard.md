# ADR-199 — Partial-Arc Reconstruction Guard (A2 coverage check)

- **Status**: Accepted
- **Date**: 2026-06-15
- **Track**: 곡선 면분할 — `face_rederive` 유도면 모델 (ADR-186/189) 후속 회귀 fix
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

사용자 보고: "원과 사각이 복잡하게 그려질 때 원과 아크의 선이 연결되어
**의도하지 않은 선**이 생성된다." 3개 병렬 코드조사 + 정밀 시뮬레이션으로
근본 원인을 확정했다.

### 1.1 확정된 근본 원인 (시뮬레이션 evidence)

`face_rederive::reconstruct_input_curves` (production rederive 경로,
`face_rederive_on_draw` default ON) 가 DCEL arc edge 를 `InputCurve` 로
재구성할 때, **모든 arc 를 "잘린 원의 조각"으로 가정**하고 같은
`(center, radius)` 의 arc 들을 하나의 **full `InputCurve::Circle`** 로
병합했다 (idempotency 목적 — 원본 주석 "arc 가 split-circle 조각이라는 가정,
genuine partial arc(DrawArc)는 future").

정밀 시뮬레이션 (Scene 레벨, production 경로):

| 단계 | faces | circle A leftArcs | LEFT-region-faces |
|---|---|---|---|
| 원 A (full) | 1 | 0 | 0 |
| + secant (반원 2개) | 2 | 2 | 1 |
| **왼쪽 반원 삭제** | 1 | **0** | **0** |
| **오른쪽에 원 B overlap → rederive** | 6 | **2 ⚠** | **1 ⚠** |

→ 사용자가 원의 **왼쪽 반을 지운** 뒤 **오른쪽에만** 무관한 원 B 를 그렸는데,
rederive 가 남은 오른쪽 arc 를 보고 full circle 로 **완성** → **삭제된 왼쪽 반이
통째로 부활**. 이것이 "의도하지 않은 선"의 정체.

### 1.2 참조 엔진 (AixxiA) 비교

AixxiA `xia-form` boundary kernel 은 polygon-only Bentley-Ottmann + half-edge
region 추출이며, 각 arc edge 에 **독립적 per-edge `CurveProvenance
{line_id, t0, t1}`** 를 부여한다 — full circle 로 **절대 재구성하지 않음**.
부분 arc 는 자기 t-span 만 유지하므로 삭제분이 되살아나는 일이 구조적으로 없다.

## 2. Decision (A2 — coverage check)

3개 옵션을 시뮬레이션으로 비교했다:

| 옵션 | 버그 차단 | idempotency | 우리 경로 적용성 |
|---|---|---|---|
| **(A2) coverage 검사** | ✅ | ✅ | ✅ analytic arc 에 맞음 |
| (C) AixxiA chord-rejection 중점검사 | ❌ | (깨짐) | ❌ faceted 표현 전제 |
| (A1) arrange Arc 1급 지원 | ✅ | ✅ | ✅ 완전 정합 (multi-week) |

**(C) 기각 근거 (실측):** per-edge 중점 sagitta 가 부분 arc 와 full-circle 조각
모두 **59mm 로 동일** (둘 다 진짜 90° arc). (C) 는 *개별 엣지* 만 보므로 둘을
구분 못 한다. (A2) 는 arc 들의 *집합*(closed-loop) 을 보므로 정확히 구분한다.

### Lock-ins

- **L-199-1 (coverage 판정, canonical)** `reconstruct_input_curves` 는 arc 를
  즉시 full Circle 로 병합하지 않는다. `(center, radius)` 별로 arc edge 를 모아
  **정점 incidence** 로 판정: 모든 끝점 degree 2 (닫힌 고리) = 전체 원 →
  `InputCurve::Circle` 재구성. dangling 끝점(degree 1) 존재 = 부분 arc →
  재구성 안 함. 방향-무관 (start/end angle 모호성 회피).
- **L-199-2 (full-circle self-loop)** `AnalyticCurve::Circle` self-loop edge
  (Path B 원) 는 항상 full → 무조건 Circle 재구성.
- **L-199-3 (보존)** 부분 arc 를 가진 면/엣지는 removal 에서 제외 (A1 freeform
  self-loop 보존 가드 답습). arrange 가 부분 arc 를 재구성하지 않으므로,
  제거하면 그대로 소멸(삭제분 부활은 막되 **남은 부분 arc 면까지 잃는** 회귀).
  보존 면의 **모든 outer edge** 를 함께 보존해 dangling 차단.
- **L-199-4 (idempotency 불변)** 전체 원이 secant 로 split 된 경우(닫힌 고리)는
  기존대로 1 Circle 재구성 — 정상 분할 케이스 무손상. 기존 회귀
  `beta_reconstruct_input_curves_arc_merge` (4 quarter-arc → 1 circle) 보존.
- **L-199-5 (deterministic)** 키 정렬(`sort_unstable`) 후 emit — HashMap
  iteration 비결정성 제거 (arrange 입력 순서 안정).
- **L-199-6 (overlap 거동 검증)** 새 도형이 보존된 부분 arc 와 겹쳐도
  manifold-valid (시뮬레이션: faces=3, valid=true). 면 손실/중복 0.
- **L-199-7** ADR-046 P31 #4 additive — 공개 API/명령/단축키 무변경. 효과는
  production rederive 경로 내부.
- **L-199-8** 절대 #[ignore] 금지.

## 3. 시뮬레이션 / 검증

- 버그 재현 sim: 부분 arc + overlap → leftArcs 0→2 (부활) 확정.
- (C) vs (A2) 비교 sim: per-edge sagitta 59mm 동일 → (C) 구분 불가, (A2) 정확.
- A2 판정 로직 sim: C(self-loop)=FULL / B(closed-loop)=FULL / A(dangling)=PARTIAL.
- A2 end-to-end sim (production 경로): step-4 leftArcs **0** (부활 차단),
  LEFT-region-faces **0**, valid=true, overlap 거동 manifold-valid.
- 회귀 누적: axia-core +2 (`adr199_erased_partial_arc_not_resurrected_by_overlap`
  + `adr199_full_circle_split_stays_idempotent`). axia-core 364 → **366**,
  axia-geo **1812** 유지 (0 regression, 절대 #[ignore] 0).

## 4. Out of scope (follow-up)

- **(A1) 완전 정합** — `analytic_arrange::arrange` 가 `InputCurve::Arc` 를 1급
  입력으로 지원 + AixxiA 식 per-edge provenance. 부분 arc 가 새 도형과 겹칠 때
  analytic 재교차까지 정확 처리 (현 A2 는 보존 + 별도 arrange). multi-week atomic.
- **(C) 보조 가드** — A1 전환 시 faceted 경로(legacy polygonize)에 chord-rejection
  중점검사 함께 검토. 현재 우리 rederive 경로엔 적용 대상 없음 (interior line=0).
- **DrawArc genuine partial arc** — 사용자가 명시적으로 부분 호를 그리는 도구.
  A2 가 이미 부분 arc 보존하므로 자연 호환되나 별도 검증 필요.
- **legacy polygonize 경로** (`face_rederive_on_draw=false`) — 본 fix 무관
  (rederive 경로 전용). legacy 는 ADR-174 Approach A 로 별도.

## 5. Cross-link

ADR-186/189 (유도면 모델 + arc-aware re-derive) · ADR-174 (curve-edge crossing-
split, Approach A/B) · ADR-101 (auto-intersect coplanar, legacy polygonize) ·
ADR-028 (Edge.curve analytic) · ADR-089 (Path B closed-curve face) · AixxiA
`xia-form/boundary_kernel` (per-edge CurveProvenance 비교 reference) · 메타-원칙
#5 (명확하면 자동) / #6 (Preventive) / #14 (면은 닫힌 경계로부터) / #16 (휴리스틱
자동화 antipattern — 부분 arc 의 full-circle 추측이 바로 그 사례).
