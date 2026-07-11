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

- **β-2 (완료, §2.7)** — (a)/(b) 판별: **(a) stitch 버그** 확정 (비대칭 실제 관통
  crossing 64 에서도 open). 근본 = v2 classify/assemble 이 곡면 tube 의 through+blind
  혼합 관통 절단면 seam 을 못 닫음. tz=110 은 추가 tangent degenerate.
- **β-3 — 곡면 관통 seam weld fix** (근본): v2 Stage 5 assemble 이 곡면 operand tube
  하단 quad (keep_b flip) 와 box top annulus 경계의 seam vertex 를 공유하도록.
  ADR-277 박스-박스 seam weld 자산 (`weld_result_seam` 등) 을 곡면 관통 case 로 확장.
  boundary edge 위치 (β-2 후속 dump) 로 정확한 weld 지점 특정 → 수선.
- **β-4 — tangent degeneracy (tz=110) + SoS tie-break**: torus 중심이 정확히 면 위
  (crossing 0, 두 원 접) 인 최악 case. 접점을 공유 정점 or clean separation.
- **β-5 — 검증**: torus grazing sweep (tz 95~120) 전부 watertight cut; sphere/
  cone/cylinder + clean-overlap 회귀 보존. fail-closed 안전망 유지.
- **β-6 — E2E + 시연 + closure**.

**핵심 (판별 완료)**: (a) stitch 버그 — 고칠 수 있음 (fail-closed+UX 가 아닌 실제
수선). 단 v2 classify/assemble seam weld 를 곡면 through+blind 혼합 관통으로 확장하는
**규모 있는 CSG 수정** (박스-박스는 ADR-277 로 됨, 곡면 tube 미검증 경로). β-3 가
핵심 작업 — boundary edge 정밀 dump → seam weld 지점 특정 → 수선.

## 2.7. β-2 (a)/(b) 판별 완료 — (a) stitch 버그 (tangent 추가 degenerate)

tz sweep (`sim_beta2_tz_sweep_crossing_and_result`, 진단 후 제거):

| tz | box top 관통 torus quad | v2 subtract |
|---|---|---|
| 110 (대칭) | 0 (torus 가 평면을 두 원으로 **접**) | Err(open) |
| 100 / 105 / 108 (비대칭) | **64 (torus 가 평면을 실제 관통)** | **Err(open)** |

**판별 = (a) stitch 버그**. 비대칭(tz≠110, torus 가 box top 을 실제 CROSS, crossing
quad 64)에서도 결과가 **open** — 두 닫힌 solid 의 subtract 는 이론상 closed 여야
하므로 (b) 기하 본질이 아니라 **v2 classify/assemble 이 곡면 관통 절단면의 seam 을
못 닫는 (a) 버그**. tz=110(대칭)은 crossing 0 (tangent) 인 **추가 degenerate**.

**근본 (최종)**: torus tube 가 box 를 **한 면만 관통** (box top 뚫고 나가되 하단은
box 안에서 blind — through + blind 혼합 topology) 할 때, v2 Stage 4-5 (classify +
assemble, "shared vertex set" seam) 가 곡면 tube 하단 quad (keep_b flip) 와 box top
annulus 경계 사이 seam vertex 를 공유하지 못해 boundary edge 잔존. (박스-박스 CSG
는 ADR-277 로 seam weld 됨 — 곡면 tube 의 blind+through 혼합이 미검증 경로.)

## 2.8. β-3 weld 시도 → 실패 + 근본 심화 (tessellation 불일치)

boundary edge dump (`sim_beta3_boundary_dump`, tz=100 실제 관통): v2 Stage 1-5 수동
재현 → closed=false, **boundary 36 edges 전부 z=110** (box top plane). box top hole
경계 ↔ torus tube 절단 경계가 z=110 seam 에서 vertex 미공유.

**β-3 시도 = ADR-277 `weld_result_seam` (0.01mm position bucket → merge) 를 v2 Stage
5 후 적용.** 결과: 박스-박스 회귀 316 pass (weld 무해), **but grazing torus 여전히
open** (fail-closed test 통과 = 여전 Err). weld 로 안 닫힘 → **되돌림** (검증된 효과 0).

**근본 심화 (canonical)**: box top hole 경계 (arrange 다각형 근사) 와 torus tube cut
경계 (imprint 다각형 근사) 는 **같은 교차 원을 서로 다르게 tessellate** → seam verts
가 0.01mm 초과로 불일치 → position weld 로 merge 불가. ADR-277 박스-박스는 seg
endpoint 가 정확히 공유되어 "shared vertex set" 이 성립하지만, 곡면 관통은 두 operand
가 같은 교차 곡선을 독립 tessellate 하여 그 가정이 깨짐. **진짜 fix = imprint 의
shared vertex arrangement 를 곡면 관통으로 확장** (ADR-277 core 수정) — 매우 대규모.

**결정 포인트 (β 진행 전 결재 필요)**:
- **(A) imprint core 곡면 확장** — 두 operand 가 교차 곡선을 **공유 tessellation**
  으로 자르도록 (seg endpoint 를 공유 arrangement 로). ADR-277 core 의 곡면 확장,
  multi-week, LOCKED-boolean 회귀 위험 큼.
- **(B) fail-closed + UX** (실용 권장) — grazing 은 드문 케이스 (곡면을 면에 얕게
  스치게 subtract). 현재 fail-closed 안전 (손상 0). 사용자 facing 메시지로 "곡면을
  면에서 더 깊이/얕게 위치" 안내. 정밀도 무손상. sphere/cone/cylinder + clean-overlap
  torus 는 이미 완전 작동 — 남은 건 torus 얕은 관통 edge case 뿐.
- 완전 robust CSG (A) vs 실용 fail-closed (B) 는 투자 대비 가치의 trade-off.

## 2.9. β-3′ 최종 근본 — vertex 는 공유 O, edge connectivity 불일치 X

사용자 결재 (A) imprint core 확장 후, 정확한 진입점 특정 (`sim_beta3p_shared_
vertex_at_seam`, tz=100 실제 관통):

- z=110 seam verts: **A-origin 68 / B-origin 64, 공유 VertId = 64** — **vertex 는
  이미 dedup 공유됨!** (A 의 68 = 64 공유 + box top 사각형 코너 4). worst A→nearest-B
  33.8mm 는 box 코너(r>55, torus 무관).
- 그런데 boundary 36 (open) → **근본 = vertex 공유 O, EDGE CONNECTIVITY 불일치**.
  box top hole 경계 (arrange 원 다각형: 64 verts 를 두 원 r=25/55 로 인접 연결) 와
  torus tube cut (imprint 가 각 quad 를 z=110 seg 로 절단한 edge) 이 **같은 vertex 를
  서로 다른 이웃과 연결** → seam edge 가 짝이 안 맞아 boundary.

**최종 근본 (canonical)**: imprint 의 shared arrangement 는 **vertex 는 dedup** 하나
**edge connectivity 를 두 face 간 일치시키지 않음**. 박스-박스는 seg 가 직선이라 양
face 의 절단 edge 가 자동 일치하지만, 곡면 관통은 box face 쪽(arrange 원 다각형)과
곡면 quad 쪽(quad별 seg 절단)의 edge pairing 이 달라짐. **weld(vertex merge)로 불가
— 이미 공유된 vertex 의 edge 를 맞춰야.**

**imprint core 곡면 확장 roadmap (A, multi-week)**:
- **β-3a — 공유 edge arrangement**: find_intersections 의 seg 를 box face 와 곡면
  quad 양쪽에서 **같은 edge 목록**으로 imprint (ADR-277 `build_intersection_
  arrangement` 의 (idx0,idx1) pair 를 imprint_faces 에 통합 — vertex 뿐 아니라 edge
  도 공유). box face hole 경계 = 곡면 cut edge 가 정확히 같은 vertex pair.
- **β-3b — 검증**: torus grazing sweep watertight; 박스-박스 + sphere/cone/cyl +
  clean torus 회귀 보존.
- **β-4 — tangent degeneracy (tz=110, crossing 0)**.
- **규모/위험**: ADR-277 core (`imprint_faces` + `subdivide_face_2d` + `build_
  intersection_arrangement`) 의 곡면 확장 — LOCKED-boolean 회귀 위험 큼, multi-week
  atomic. fail-closed 안전망은 전 과정 유지 (β 미완 시 rollback).

## 2.10. β-3a de-risk — imprint core 는 이미 OK, 근본이 classify 로 재이동

β-3a 착수 de-risk (`derisk_beta3a_edge_mismatch`, tz=100) 가 §2.9 의 "edge
connectivity 불일치" 가설을 **뒤집음**:

- **imprint 직후** z=110 seam edge (VertId pair): A=68, B=64, **shared=64**,
  A-only=4 (box 외곽 코너 r=84.9, box side 와 공유 = 정상), **B-only=0**. → **imprint
  는 이미 vertex 뿐 아니라 EDGE 도 완전 공유** (seam 64 닫힘). **사용자 선택 A
  (imprint core 확장) 는 실제 근본이 아님 — imprint 는 이미 OK.**
- **classify 직후**: keep_a z110 edge=36, keep_b=64, **shared(seam)=32**, B-only=**32**.
  classify 가 box top 의 한 subface (inner disk, torus 구멍 안) 를 drop → torus 안쪽
  원 (32 edge) 이 짝 잃어 boundary.
- **point_in_solid 는 정확** — `point_in_solid(torus, hole_centre (0,0,110)) = false`
  (torus 구멍 = 밖, 정확), `(tube 40,0,100) = true` (정확). classify 오판의 원인은
  point_in_solid 가 아니라 **더 깊은 층** (box top inner disk 가 keep_a 에 없는 이유
  — arrange subface 생성 or classify 의 곡면 non-convex 처리, 미해결).

**메타-근본 (canonical)**: 이 트랙은 de-risk 마다 근본이 이동 (SI → open →
imprint edge → classify → point_in_solid → …). 각 measure 가 이전 가설을 뒤집음.
**imprint core (사용자 선택 A) 는 이미 OK** (seam 64 완전 공유). 진짜 근본은
classify/assemble 의 곡면 non-convex(genus-1 torus) 처리이며 **여러 미묘한 층이
얽혀** 완전 해결은 진짜 multi-week CSG 재작업 (imprint 확장이 아님). fail-closed
안전망은 전 과정 유지 — grazing torus 얕은 관통(드문 edge case)만 영향, sphere/cone/
cylinder + clean-overlap torus 는 완전 작동. **투자 대비 (B) fail-closed+UX 실용
재고 강력 권장** (완전 robust 는 여러 CSG 층의 multi-week).

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
