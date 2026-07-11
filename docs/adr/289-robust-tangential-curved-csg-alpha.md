# ADR-289 — α spec: Robust curved CSG for tangential / shallow-penetration operands

- **Status**: Proposed (α spec — measure-first 근본 분석 완료; β 구현 결재 대기)
- **Date**: 2026-07-11
- **Track**: Kernel Robustness / CSG (ADR-276/277/278 계보)
- **Author**: WYKO + Claude (measure-first characterization)
- **Meta-principles**: #4 (SSOT), #6 (measure-first), #9 (no regression), #10 (LOCKED change = new ADR), #14 (face from closed boundary)

---

## 1. Context

ADR-278 (+2026-07-11 follow-ups) 로 Path B 곡면 Boolean 이 **subtract + union +
intersect × 4곡면(cyl/sphere/cone/torus) × 임의 축** 완성 (clean overlap). 남은
유일 gap = **grazing/tangential** — 곡면 operand 가 box 면에 접하거나 얕게 관통할
때 결과가 self-intersect → ADR-276 validity gate 가 reject → fail-closed (안전,
cut 안 됨). 사용자 결재 (2026-07-11): "robust CSG (exact/SoS)" 로 근본 해결.

## 2. Measure-first 근본 분석 (핵심 — exact arithmetic 이 아니다)

**특성화 sweep (`measure_grazing_tangency_threshold`, `analyze_grazing_si_pairs`):**

- **grazing 은 torus 특유** — sphere(convex) / cone·cylinder(ruled) 는 접점 포함
  **모든 위치에서 clean** (sphere cz 70~150 전부 ok, si=0). 오직 **non-convex
  torus tube** 가 box 면을 얕게 관통할 때만 SI (tz=95~120, si=64~128).
- **SI 128 pairs 전부 = `box top face (Plane, z=110)` × `torus tube 의 polygonal
  quad (z≈107~113)`**. box top face 가 torus 의 얕은 관통 교차를 **imprint(split)
  하지 못하고 그대로 남아** → torus quad 들과 관통 = SI.
- **∴ 근본은 좌표 정밀도(exact arithmetic)가 아니라 imprint TOPOLOGY robustness**
  — 얕은/접하는 관통에서 (a) 교차 곡선 assembly, (b) face split 이 실패. standalone
  polygonal torus 는 SI=0 (`adr278_polygonal_torus_builder_is_watertight`) 이므로
  operand 자체는 정상 — boolean 결과의 imprint 실패가 원인.

**결론**: 사용자가 지목한 "exact/SoS" 의 exact arithmetic 부분은 이 gap 의 근본이
아니다 (SI 는 정밀도가 아닌 imprint topology). 진짜 필요 = **shallow/tangential
penetration 에서의 imprint robustness** (+ SoS-style degeneracy 처리는 보조).

## 3. 접근 옵션

| # | 접근 | 평가 |
|---|---|---|
| **A** | **Shallow-penetration imprint robustness** — 얕은 관통 교차 곡선 assembly + face split 강건화 (box face 가 torus quad 교차를 정확히 split) | ✅ **근본 fix** (measure 가 지목). torus-특유로 scope 좁음. |
| B | Exact arithmetic / SoS degeneracy 처리 | ⚠ 근본 아님 (SI 는 정밀도 무관 topology). degeneracy tie-break 보조로만 가치. |
| C | Perturbation fallback (torus 를 면에서 ε 밀기) | ❌ 정밀도 훼손 (LOCKED #5). 사용자 이전 결재에서 배제 방향. |
| D | fail-closed 유지 (현상) | 안전하지만 gap 미해결 (사용자가 해결 요청). |

**채택 방향 (β 결재 대상): A (imprint robustness) 우선 + B (degeneracy tie-break)
보조.** exact arithmetic 전면 재작성은 근본 분석상 불필요/과투자.

## 4. β roadmap (multi-week atomic, 각 sub-step 별도 결재 가능)

- **β-1 — imprint 실패 정밀 조사** (measure): `find_intersections_polygonal` 이
  box top × torus quad 교차를 *찾는가*? 찾는데 `split_faces_by_chains` /
  `assemble_closed_loops` 가 얕은 관통(lens/band)에서 왜 split 못하나? 정확한
  실패 지점 특정 (교차 곡선이 매우 짧음 / 다중 조각 / tube inner+outer wall 이
  면을 각각 관통 → 2 loop). 코드 0, 진단 test 만.
- **β-2 — shallow 교차 곡선 assembly 강건화**: 얕은 관통의 교차 곡선(들)을
  box face 위 closed loop / open chain 으로 정확히 조립 (기존 `assemble_chains`
  /`assemble_closed_loops` 확장).
- **β-3 — face split 강건화**: box face 를 그 교차 loop 로 split (얕은 lens/band
  hole punch — `apply_closed_loop_split` 확장). SI 제거.
- **β-4 — degeneracy tie-break (SoS 보조)**: 정확히 접하는(tangent, 관통 깊이 0)
  경우의 tie-break — 접점을 공유 정점으로 or clean separation 판정.
- **β-5 — 검증**: torus grazing sweep (tz 95~120) 전부 watertight cut; sphere/
  cone/cylinder 회귀 보존; ADR-276 gate 는 진짜 corrupt 만 reject (robust 결과는
  통과). fail-closed 안전망 유지.
- **β-6 — E2E + 시연 + closure**.

## 5. Lock-ins (β 강제)

- **L-289-1** 근본 = imprint topology robustness (exact arithmetic 아님) — measure 근거.
- **L-289-2** fail-closed 안전망 유지 — β 가 처리 못하는 잔여 degeneracy 는 여전히
  ADR-276 gate 가 reject → rollback (corrupt 결과 절대 commit 안 함).
- **L-289-3** 정밀도 무손상 — perturbation(operand 이동) 금지 (LOCKED #5). imprint
  는 원본 좌표 위에서.
- **L-289-4** sphere/cone/cylinder + clean-overlap torus 회귀 전부 보존 (ADR-278
  자산). 신규 SI 도입 0.
- **L-289-5** fix 는 `boolean_solid` (imprint/split) 내부 — WASM/bridge/tool 무변경
  (모든 caller 자동 전파).
- **L-289-6** 절대 #[ignore] 금지.

## 6. Test 자산 (α — 특성화 회귀, 이미 landed)

- `adr278_grazing_sphere_clean_at_any_z` — convex sphere subtract 는 접점 포함
  모든 z 에서 watertight (grazing 없음).
- `adr278_grazing_torus_shallow_penetration_fails_closed` — torus tz=110 grazing
  subtract 는 ADR-276 gate 가 reject (Err) = fail-closed (현 상태 문서화; β 후
  이 test 는 "cut watertight" 로 승격).

## 7. Cross-link

- ADR-278 (Path B 곡면 Boolean — subtract/union/intersect × 4곡면 × 임의 축;
  grazing 만 남음) / ADR-277 (general polyhedral CSG v2 — imprint 파이프라인) /
  ADR-276 (validity gate — fail-closed) / ADR-104 family (Path B primitives) /
  ADR-115 (torus kernel-native) / LOCKED #5 (정밀도 정책) / LOCKED #94 (Path B
  curved Boolean) / 메타-원칙 #4 #6 #9 #14 / `project-boolean-runtime-finding`.
