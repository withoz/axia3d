# ADR-223 — Boolean Multi-loop (β-4) De-risk + Defer Decision

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Hole 커널 심화 (ADR-222 후속) / Boolean (ADR-197 β-4) — **Deferred**
- **Depends on**: ADR-197 (Path B Boolean) / ADR-064 (NURBS-DCEL depth≤1) / ADR-222 (hole
  circle metadata) / ADR-016 Q2 (multi-loop policy) / ADR-076 (legacy mesh Boolean sunset)

## 1. Context

ADR-222 Phase 0(hole circle metadata) closure 후, 사용자가 "구멍 뚫린 솔리드를 다시
Boolean(입체볼륨 수정·완성)" 가치를 추구 → Boolean multi-loop(= ADR-197 β-4) 경로를
검토. 정식 절차(de-risk → 시뮬레이션 → 결재)로 진행한 결과 **보류 결정**. 본 ADR은 그
de-risk + 시뮬레이션 findings를 canonical 기록으로 남긴다 (부정 결정 명시 lock-in,
ADR-076/125/127 패턴).

## 2. De-risk findings (코드 확증)

**아키텍처 분기 해소** — Boolean multi-loop의 두 경로:

| 경로 | 상태 | 판정 |
|---|---|---|
| **legacy mesh Boolean (Path A)** | sunset (ADR-076), fan triangulation | **REJECT** |
| **NURBS-DCEL (Path B, ADR-197)** | production 방향, 곡면 보존 | 올바른 경로 |

**mesh-CDT 기각 3 근거**:
1. **Sunset 경로** — ADR-197(2026-06-11 결재)이 Path B를 production으로 채택, mesh Boolean은
   dormant (ADR-076 sunset).
2. **Surface metadata 파괴** — fan/CDT polygonize → sphere가 plane이 됨 (L-197-6 anti-pattern).
3. **체인 편집 깨짐** — mesh path는 face genealogy 없음 → subtract-then-subtract / hole-then-
   boolean 체인에서 hole 추적 불가 (volume 편집에 치명적).

**정확한 흐름** (코드 reading):
- `Mesh::boolean`(boolean.rs:1445) → `prepare_solid`(:1808)가 **`face.outer()`만** fan-
  triangulate → inner loop(hole) 무시. 가드(:1586)가 hole 차단.
- `nurbs_boolean_to_dcel`(boolean_nurbs_dcel.rs:114) → 두 면의 `surface()` 요구 →
  `nurbs_boolean_v2` SSI(**표면 기반**) → `containment_to_faces_with_loops`(:241)는 **SSI trim
  결과(`phase_j.trim`)만** 사용, depth≤1 → **입력 면의 기존 hole(inner loops)은 결과에 미반영**.

## 3. Simulation (런타임 확증)

box top 면에 r2 circular hole punch → `prepare_solid` 호출 (scratch test, 비커밋):
- holed 면 → **2 triangles** (outer 4-vert fan, 48-vert hole **무시**).
- hole 중심이 fan 삼각형에 **덮임 = true** → **hole이 solid로 취급** (point_in_solid 오분류).

→ "가드 제거"만으로는 fan이 hole을 메워 **semantically 잘못된 Boolean** 결과.

## 4. 핵심 결론 — depth-1 MVP가 예상보다 큼

사용자 실수요("구멍 뚫린 솔리드 Boolean, 구멍 보존")는 **양쪽 경로 모두 실패**:
- Mesh path: hole 메움 (시뮬 확증) → 오분류.
- NURBS-DCEL: SSI가 표면만 봄, containment는 SSI trim만 사용 → **입력 hole 미보존**.

이전 "wired but untested depth-1"은 **SSI-결과 hole**(예: torus∩box→ring)이지 **입력-보존
hole**이 아니었음. 사용자 실수요 = 더 어려운 **입력-보존 hole** 케이스.

**재추정**: depth-1 MVP는 M(3-4주, "가드 제거 + 테스트")이 아니라 **L/XL multi-month** —
입력-hole-aware NURBS-DCEL(SSI/trim이 입력 boundary hole 결합, 올바른 경로) 또는
CDT-on-legacy(아키텍처 부적합). 시뮬레이션이 **3-4주 commit 전에 벽을 발견**.

## 5. Decision — β-4 Defer

**Boolean multi-loop(β-4)을 보류한다.**
- 입력-hole-preserving Boolean은 L/XL multi-month로 확정 → 현 시점 commit 부적합.
- ADR-197 곡면 Boolean(sphere/cylinder/cone/torus ∩ box, ADR-197/198/204/205)은 **이미 강함**
  → 입체볼륨 편집 대부분 커버. hole-through-Boolean은 future milestone.
- **ADR-016 Q2 Boolean 거부 유지** — 본 ADR은 정책 amendment 아님 (Push/Pull만 ADR-191로
  완화된 상태 불변). LOCKED #1 / ADR-016 Q2 변경 0.

## 6. Lock-ins

- **L-223-1** mesh-CDT retrofit **canonical 기각** — legacy/sunset 경로에 투자 금지 (surface
  파괴 + 체인 깨짐). 향후 Boolean 작업은 NURBS-DCEL(Path B) 경로.
- **L-223-2** β-4(입력-hole-aware Boolean) 경로 = NURBS-DCEL이 입력 boundary hole을 SSI/trim에
  결합 (future). depth-1 MVP도 이 작업 필요 (입력-보존 hole은 "wired" 아님).
- **L-223-3** ADR-016 Q2 Boolean 거부 **불변** (본 ADR은 amendment 아님, 메타-원칙 #10).
- **L-223-4** ADR-222 Phase 0 circle metadata는 향후 β-4의 SSI dispatch 입력으로 유효(보존) —
  단 현재 SSI는 아직 미소비 (TrimCurve2D는 Line polyline만 생성).
- **L-223-5** 시뮬레이션/de-risk findings(prepare_solid fan fills hole / SSI ignores input
  holes)는 β-4 재개 시 anchor. 코드 변경 0 (de-risk + 기록만).
- **L-223-6** 절대 #[ignore] 금지 (해당 없음 — 코드 변경 0).

## 7. 후속 (β-4 재개 시 / 다른 우선순위)

- β-4 재개 trigger: 입력-hole-aware NURBS-DCEL이 실 수요로 부상 시 (L/XL multi-month ADR).
- 즉시 가능한 대안: 24-도구 폭(3P-Plane / NURBS surface / Wall) / Phase 0.5(smooth hole
  render, per-segment Arc ADR-092 패턴) / ADR-197 곡면 Boolean edge case 마무리.

## 8. Cross-link

- ADR-197 (Path B Boolean — production 방향, 곡면 핸들러) / ADR-064 (NURBS-DCEL depth≤1)
- ADR-076 (legacy mesh Boolean sunset) / ADR-222 (hole circle metadata, Phase 0)
- ADR-016 Q2 (multi-loop 정책 — 불변) / ADR-191 (Push/Pull Q2 완화 선례)
- ADR-076/125/127 (부정/defer 결정 명시 lock-in 패턴) / ADR-092 (per-segment Arc — smooth render)
- 메타-원칙 #6 (Preventive — 시뮬레이션이 벽 사전 발견) / #10 (ADR 불변) / LOCKED #1 #44 #79
