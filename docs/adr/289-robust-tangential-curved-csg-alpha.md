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

## 2.5. β-1 시뮬 — 실패 단계 정밀 특정 (imprint 파이프라인 단계별 관찰)

`boolean_solid_v2` imprint 파이프라인 = **① `find_intersections_polygonal` (교차
세그먼트) → ② `assemble_closed_loops`/`assemble_chains` (loop/chain 조립) → ③
`subdivide_face_2d` (face split, hole 포함)**. torus tz=110 (중심=box top plane,
대칭 최악) subtract 를 단계별 시뮬(`sim_grazing_imprint_investigation`, 진단용
후 제거):

- **① find_intersections_polygonal = 정확** — box top face 에 **128 세그먼트,
  z=110.00 정확, r∈[25.00, 55.00]** (= torus 의 두 동심원 major−minor=25 /
  major+minor=55). 교차 검출은 완벽.
- **② assemble = 완전 실패 (0 chain, 0 loop)** — **근본 여기**. box top 세그먼트의
  **node 64개 전부 degree 4** (zero-length 0). 원인: torus 중심이 평면에 대칭이라
  tube **상단·하단이 같은 두 동심원을 각각 새겨 세그먼트가 겹침** (각 교차점에
  상단 2 + 하단 2 = degree 4). `assemble_closed_loops` (boolean.rs:9157) 는
  **"모든 node degree == 2 아니면 전부 포기"** (all-or-nothing `return
  Vec::new()`) → 0 loop.
- **③ subdivide** — ② 가 0 loop 주니 constraint 없음 → box top split 안 됨 →
  원본 유지 → torus quad 와 관통 = 128 SI.

**정밀 근본 (canonical)**: (a) `assemble_closed_loops` 의 all-or-nothing degree-2
포기 + (b) 대칭 관통의 **중복 세그먼트 (degree 4)** + (c) 관통 교차가 **두 동심원
(annulus)** 인데 assemble→subdivide 가 다중 loop / hole 을 처리 안 함. exact
arithmetic 무관 (좌표 정확, z=110.00 / r=25·55 exact).

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

**β-1 (완료, §2.5)** — 실패 단계 정밀 특정: find_intersections 정확(128 seg, 두
동심원), assemble_closed_loops all-or-nothing degree-2 포기 + 대칭 중복 degree-4 +
annulus(다중 loop) 미처리.

- **β-2 — `assemble_closed_loops` 강건화** (근본 fix, 예상 최대 효과):
  - (a) **중복 세그먼트 dedup** — undirected edge 중복 제거 (대칭 관통의 겹친
    세그먼트 degree 4 → 2). endpoint 는 spatial-hash (LOCKED #5, 0.15μm) 로 병합.
  - (b) **all-or-nothing 제거** — degree≠2 node 있어도 degree-2 subgraph 만 loop
    추출 (전부 포기 대신). 남는 loose end 는 fail-closed 로.
  - (c) **다중 loop 반환** — 두 동심원(r=25, r=55) 각각 별도 loop.
  - 순수 유틸(free fn) — `boolean()` (v1) 무영향, 기존 corner/notch/slot 회귀 보존.
- **β-3 — annulus split (다중 동심 loop → outer + hole)**: box face 를 outer
  loop(r=55) + inner hole(r=25) 로 split. `subdivide_face_2d` 가 다중 loop 를
  outer/hole 로 분류 (nesting test) → `imprint_faces` 의 기존 `sf.holes` /
  `add_face_with_holes` 경로 (boolean.rs:1704) 활용. torus 관통 band 를 정확히
  잘라냄 → SI 제거.
- **β-4 — 비대칭 grazing + SoS tangent tie-break**: tz≠110 (상/하단 tube 가 서로
  다른 원) + 정확히 접하는(관통 깊이 0, degree 4 중복이 clean-touch) 경우. 접점을
  공유 정점 or clean separation 판정 (SoS-style symbolic tie-break).
- **β-5 — 검증**: torus grazing sweep (tz 95~120) 전부 watertight cut; sphere/
  cone/cylinder + clean-overlap 회귀 보존; ADR-276 gate 는 진짜 corrupt 만 reject.
  fail-closed 안전망 유지 (β 미처리 잔여는 여전히 rollback).
- **β-6 — E2E + 시연 + closure**.

**핵심**: β-2(assemble dedup + all-or-nothing 제거 + 다중 loop) + β-3(annulus
split) 가 근본 fix. β-4 는 잔여 degeneracy. 예상: β-2+β-3 로 grazing 대부분 해소.

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
