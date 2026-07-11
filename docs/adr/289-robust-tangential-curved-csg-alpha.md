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

## 2.6. β-1 재측정 정정 — 진짜 근본은 SI 가 아니라 OPEN BOUNDARY

§2.5 는 `assemble_closed_loops` 를 근본으로 지목했으나 **오판**이었다 (measure-first
로 교정). 실제 imprint 경로는 `imprint_faces → subdivide_face_2d → **arrange**`
(`assemble_closed_loops` 아님 — 그건 notch/slot 별도 경로). 실제 경로를 단계별로
재측정 (`sim_arrange_grazing_real_path`, 진단 후 제거):

1. **`subdivide_face_2d` (arrange) = 작동** — box top + 128 segs → **3 subfaces**
   (box 나머지 r=55 hole / annulus r=55 outer + r=25 hole / r=25 disk). box top 을
   두 동심원으로 정확히 split (annulus 처리 정상).
2. **`imprint_faces` = 작동** — box top 실제 split (6→8 faces, box_top 비활성화) +
   **imprint 후 `detect_self_intersections` = 0**. torus quad 는 box top annulus
   hole 을 통과 → SI 없음. **imprint 단계가 SI 를 완전히 해결.**
3. **`boolean_solid_v2` 전체 = 실패** — gate: `invariants_valid=true,
   **self_intersection_clean=TRUE**, **closed_solid=FALSE**`. 즉 **SI 는 clean
   (imprint 가 해결)**, 근본은 **결과가 OPEN (닫힌 solid 아님)** → closed-solid
   gate 가 정확히 reject → fail-closed.

**진짜 근본 (canonical, 정정)**: grazing subtract 는 **SI 문제가 아니라
open-boundary 문제**. torus 가 box 를 완전 관통하지 않고 **스치면(shallow)**, 절단면
(torus tube 의 얕은 관통 표면) 이 box 경계와 완전히 stitch 되지 않아 boundary
edges 가 남음 → 결과가 열림. exact arithmetic / assemble / arrange / imprint 모두
무관 — **classify/assemble (v2 Stage 4-5) 의 open-boundary stitch** 가 근본.
(fail-closed 는 올바르게 작동 중 — open 결과를 commit 안 함.)

## 3. 접근 옵션

| # | 접근 | 평가 |
|---|---|---|
| **A** | **Shallow-penetration imprint robustness** — 얕은 관통 교차 곡선 assembly + face split 강건화 (box face 가 torus quad 교차를 정확히 split) | ✅ **근본 fix** (measure 가 지목). torus-특유로 scope 좁음. |
| B | Exact arithmetic / SoS degeneracy 처리 | ⚠ 근본 아님 (SI 는 정밀도 무관 topology). degeneracy tie-break 보조로만 가치. |
| C | Perturbation fallback (torus 를 면에서 ε 밀기) | ❌ 정밀도 훼손 (LOCKED #5). 사용자 이전 결재에서 배제 방향. |
| D | fail-closed 유지 (현상) | 안전하지만 gap 미해결 (사용자가 해결 요청). |

**채택 방향 (β 결재 대상): A (imprint robustness) 우선 + B (degeneracy tie-break)
보조.** exact arithmetic 전면 재작성은 근본 분석상 불필요/과투자.

## 4. β roadmap (재조준 — 근본 = open boundary, §2.6)

**β-1 (완료, §2.5+§2.6)** — 실패 단계 정밀 특정 (2회 measure 교정): find_intersections
✅, subdivide/arrange ✅ (3 subfaces annulus), imprint ✅ (**SI 0**). 진짜 근본 =
v2 Stage 4-5 (classify/assemble) 의 **open boundary** (closed_solid=false). SI 는
imprint 가 이미 해결. §2.5 의 assemble_closed_loops 지목은 오판 (imprint 경로 아님).

- **β-2 — open boundary 원인 정밀 특정** (measure): imprint 후 SI=0 이지만 v2 전체
  결과가 closed_solid=false. boundary edges 가 **어디서** 남는지 (torus tube 얕은
  관통 절단면 ↔ box top annulus hole 경계 stitch 실패? classify 가 annulus subface
  를 잘못 keep/drop? torus tube 의 box 안/밖 경계 face 가 seam 미형성?). Stage 4
  (classify) + Stage 5 (assemble) 후 boundary edge 위치 dump. 코드 0.
- **β-3 — open boundary stitch fix**: β-2 가 지목한 지점 수선 (얕은 관통 절단면과
  box annulus 경계의 seam weld / classify tie-break). torus tube 얕은 관통이 box
  를 완전 관통 안 해도 절단면이 닫히도록.
- **β-4 — 비대칭 grazing (tz≠110) + SoS tangent tie-break**: 상/하단 tube 가 서로
  다른 원 + 정확히 접하는(관통 깊이 0) 경우.
- **β-5 — 검증**: torus grazing sweep (tz 95~120) 전부 watertight cut; sphere/
  cone/cylinder + clean-overlap 회귀 보존. fail-closed 안전망 유지.
- **β-6 — E2E + 시연 + closure**.

**미해결 질문 (β-2 measure 대상)**: grazing subtract 의 open boundary 가 (a) stitch
버그 (고칠 수 있음) 인지 (b) 기하 본질 (torus 가 box 를 스치기만 하면 절단이 진짜
불완전 → open 이 올바른 결과, cut 불가) 인지. (b) 라면 β 는 "open 을 고치는" 게
아니라 "fail-closed + 명확한 UX" 가 정답 — β-2 measure 가 이를 판별.

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
