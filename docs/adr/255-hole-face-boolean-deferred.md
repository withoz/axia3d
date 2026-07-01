# ADR-255 — P2 closure: Hole-face Boolean deferred (guardrails verified correct)

- **Status**: Accepted (defer decision — negative-decision lock-in)
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch)
- **Type**: De-risk closure (docs-only, 코드 변경 0)
- **Author**: WYKO + Claude (de-risk workflow + full-flow empirical probe)

## 1. Context

ADR-253 가 정정한 우선순위의 **P2 (C1 hole-face Boolean + ADR-192 §3.2b
annulus)** 진행. multi-week (constrained Delaunay) 가능성 때문에 구현 전
de-risk (4-agent workflow + 실제 엔진 full-flow probe).

## 2. 진짜 열린 결함 없음 — empirical 결론

de-risk 와 **full-flow empirical probe** (canonical: "empirical > audit,
특히 Boolean" — ADR-249 답습) 가 P2 에 *싼 진짜 결함이 없음* 을 확정. probe
가 audit 의 두 framing (silent no-op / inners() reject) 을 **모두 반증**:

| 시나리오 | 실측 (real engine round-trip) |
|---|---|
| **box − cylinder** (지원 config, 깨끗한 scene) | ✅ `ok:true, curved:true, resultFaces:8`, **manifold VALID (0 violations)**, NURBS surface 보존. Boolean 은 **결과로 holed 면(annular top/bottom)을 manifold-valid 하게 생성** |
| **cylinder − cylinder** (미지원 same-kind curved subtract) | ⚠️ **정직한 에러** ("this configuration does not support yet ... Use an axis-aligned box that only cuts the primitive in Z") + **mesh 무손상** (faceDelta 0, manifold valid). silent corruption 아님 |
| **C1 true planar hole-face Boolean** | reject = **올바른 guardrail** (explicit bail, no corruption) |

**핵심**: Boolean 서브시스템은 건강 — 지원 config 동작 + manifold valid,
미지원 config 정직한 에러 (silent corruption 0). C1 reject 는 올바른 동작.

## 3. audit framing 정정 (full-flow probe)

- **§3.2b 의 audit framing ("annulus inners() reject", boolean.rs:1586)
  = WRONG mechanism.** 실제 user-facing 흐름:
  1. `booleanDispatchDcelMulti` (Path 1, NURBS-DCEL) → Cylinder side 의
     `surface_to_bspline` Y-E 변환 실패 → `pathUsed='Mesh'` →
     `handleMultiDcelResult` false → fall-through (BooleanHandler.ts:252-253).
  2. `booleanOp` (Path 2, ADR-197/198 curved Boolean aware, `result.curved`)
     → 지원 config 동작 / 미지원 config 정직한 에러.
  - 즉 §3.2b 는 "inners() reject" 가 아니라 "curved-config 지원 경계" —
    그리고 그건 **올바른 guardrail** (정직한 에러). audit 가 제안한 "싼 gate
    relax" 는 틀린 메커니즘 기반 → 적용 불가/무효.
- **C1** = 진짜 triangulation gap (fan-tri `boolean.rs:1817` 가 hole 못
  다룸). 단 user-facing path 는 이 legacy reject 에 거의 도달 안 함 (Path 1
  dispatch 또는 curved Boolean 이 먼저). 도달해도 explicit bail (corruption 0).

## 4. Decision — DEFER P2

- **C1 (true planar hole-face Boolean)**: niche, **zero documented user
  demand**, 4-6주 constrained Delaunay (CDT robustness vs LOCKED #5 1.5μm,
  skinny triangles), 올바른 reject guardrail 이미 존재 → **defer**.
- **§3.2b**: non-defect (curved Boolean 지원 경계 + 정직한 에러). 확장
  (cylinder−cylinder / concave / non-axis-aligned / same-kind union) =
  deep SSI work (ADR-197/198 follow-up), demand 미검증 → **defer**.
- de-risk 의 가치: 4-6주 speculative CDT 회피 + Boolean 서브시스템이
  정직/정확 (silent corruption 0) 임을 확정 + scope tier anchor 봉인.

## 5. Scope tiers (future trigger anchor)

| Tier | 시나리오 | 메커니즘 | 비용 | LOCKED #1 / ADR-016 Q2 |
|---|---|---|---|---|
| MVP — C1 pre-split | ring/holed 면을 Boolean 전 N hole-free sub-face 로 split (legacy 경로) | 기존 split 재사용, CDT 없음 | ~3-4d | amendment 불필요 (operand hole-free) |
| Medium — curved config 확장 | cylinder−cylinder / concave / non-axis-aligned 등 | deep SSI (ADR-197/198) | multi-week | — (curved 경로, Q2 무관) |
| Full — constrained Delaunay | 임의 planar holed-face Boolean (true C1), N-hole nesting | 신규 CDT triangulator (`boolean.rs:1817` fan-tri 대체) | 4-6주, high risk | amendment 필요 (Q2 reversal, Push/Pull+Offset on ring 다운스트림) |

**재-open trigger**: 실제 P1/P3 페르소나 사용자가 holed-Boolean 또는
cylinder−cylinder Boolean 시나리오를 요구할 때. 그 전엔 budget 을 더 높은
가치 (P3 곡면 그리기 등) 에 투입.

## 6. Lock-ins

- **L-255-1** P2 defer — C1 niche/no-demand/4-6주 CDT, §3.2b non-defect.
  Boolean guardrails (reject/error) = 올바른 동작 (corruption 0).
- **L-255-2** full-flow empirical probe canonical — direct dispatch probe
  만으로는 불충분 (BooleanHandler fall-through `pathUsed==='Mesh'` →
  booleanOp). user-facing 결론은 **전체 제어 흐름** 캡처 필수.
- **L-255-3** box−cylinder (지원) manifold valid + curved 보존 = ADR-197/198
  curved Boolean 동작 증거. cylinder−cylinder 정직한 에러 = honest guardrail.
- **L-255-4** scope tier (MVP pre-split / Medium curved expand / Full CDT)
  봉인 — future trigger anchor.
- **L-255-5** 코드 변경 0 (docs-only defer closure, LOCKED #44).
- **L-255-6** audit synthesis ≠ ground truth (메타-원칙 #6) — §3.2b
  framing 2건 모두 probe 로 반증, truth over completion.

## 7. Lessons

- **L1 full-flow empirical canonical** — Boolean 같은 다단계 dispatch 는
  *전체 흐름* (dispatch → fall-through → fallback) 을 probe 해야 user-facing
  진실. 첫 probe (dispatch only) 가 "silent no-op" 으로 오도했으나, fall-
  through 의 `booleanOp` 가 실제 결과 (정직한 에러). ADR-249 / ADR-243 의
  "empirical > LLM/audit" 의 가장 강한 형태 (audit framing 2건 반증).
- **L2 honest guardrail ≠ defect** — 미지원 config 의 정직한 에러 + mesh
  무손상은 결함이 아니라 *올바른 경계*. "에러 난다 = 고쳐야 한다" 아님.
- **L3 defer 의 가치 (truth over completion)** — P2 의 명목 작업 (C1 +
  §3.2b)을 강행하지 않고 de-risk 진실 (no demand + 올바른 guardrail)로
  defer. 4-6주 speculative 작업 회피가 progress. ADR-076 (negative
  decision) / ADR-251 (honest closure) 답습.
- **L4 Boolean output holes ≠ input holes** — Boolean 은 결과로 holed 면을
  manifold-valid 하게 *생성* (box−cylinder annular). 입력 holed 면 (C1)
  만 미지원. uv_holes (출력) vs face.inners() (입력) 분리 일관.

## 8. Cross-link

- ADR-253 (P2 anchor — 우선순위) + ADR-254 (P1 closure, §3.2(c) 직전)
- ADR-192 §3.2b (annulus parity — audit framing source, probe 로 정정)
- ADR-197/198 (curved primitive Boolean — box−cylinder 동작 source)
- ADR-064/066 (NURBS Boolean DCEL — Path 1 dispatch, surface_to_bspline Y-E)
- ADR-016 Q2 (multi-loop face policy — Full CDT tier amendment 대상)
- ADR-249 / ADR-243 (empirical > audit canonical — full-flow 답습)
- ADR-076 (negative-decision lock-in) / ADR-251 (honest closure) 패턴
- LOCKED #1 (P7 manifold) / #5 (1.5μm) / #41 (ADR-101) / #44 (Complete
  Meaning per Merge) / 메타-원칙 #6 (Preventive) / #16 (자동화 antipattern)
- P3 (곡면 그리기 — 다음 진행) — Cyl/Cone/Torus sketching, ADR-202 mirror
