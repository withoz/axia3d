# ADR-273 — Self-Intersection Checker (최종 방어선)

**Status**: Accepted (구현 완료 — 5 commit, `6346e48` ~ `4d7a4d1`)
**Track**: Kernel Robustness (ADR-272 후속)
**Cross-link**: ADR-272(커널 스윕 + closure gate) · ADR-267(watertight gate) ·
ADR-268(topology≠orientation) · ADR-024(3-way chamfer) · ADR-068(InvariantVerifier
패널) · 메타-원칙 #4 #6 #9 #10 #11 #12

---

## 1. Context — 어떤 위상 검사도 못 잡는 class

ADR-272 D4 의 chamfer flap 은 **self-intersecting but manifold**: 두 면이 기하적
으로 겹치는데(poke-through/fold) DCEL 위상은 valid. 이 class 는:

- `verify_face_invariants`(I1–5) 통과
- `face_set_manifold_info`(is_closed_solid) 통과 (boundary 0)
- `collect_non_manifold_edges`(crack) 통과
- `verifyOutwardNormals`(winding) 통과
- **ADR-272 의 closure gate 도 통과** (열림 아니라 self-intersection)

즉 **self-intersection 을 검사하는 코드가 없어**서 오직 브라우저 시연 게이트
(사용자 시각)만 발견 가능했다. 본 ADR 은 그 검사기를 추가해 자동 검출 + 방어선
+ UI 노출을 구축한다.

## 2. Decision — 3 class 검출 검사기

### D1 — 코어 `Mesh::detect_self_intersections()` (`6346e48`, read-only)

- 모든 active face 를 **earcut** 로 3D 삼각형 분할 (구멍 포함, `boolean_geo::
  project_to_2d` 재사용).
- **Broad phase** — face AABB overlap (초기 O(F²), 후 공간 grid D3).
- **Narrow phase** — `boolean_geo::triangle_triangle_intersection` 재사용.
- WASM `detectSelfIntersections()` → JSON `{clean,count,pairs}`.

### D2 — closure gate 통합 (`ea8c2d5`, 엔진 결과 방어선)

`closure_preserving_gate_passed`(ADR-272 D2)에 self-intersection 추가. merge/
chamfer/fillet 전후 `detect_self_intersections().count()` 비교 → op 이 **새
self-intersection 유발 시** reject + 롤백. 이미 자기교차하던 입력은 count 증가
없으면 통과 → false rejection 0. **chamfer radius guard(입력, ADR-272 D4) +
SI gate(결과) = 2중 방어**.

### D3 — 공간 grid broad phase (`5d343fc`, 성능)

O(F²) → uniform 공간 grid. cell = 평균 face AABB extent (typical face ≈ 1 cell).
**겹치는 AABB 는 반드시 ≥1 cell 공유**(overlap 영역의 한 점의 cell 을 양쪽이
insert) → candidate 누락 0. `CELL_CAP`(512) 초과 span 의 큰 face 는 `big` 로
전수 비교(메모리 blow-up 회피, exhaustive 유지). candidate FxHashSet dedup,
최종 pair FaceId 정렬(결정적).

### D4 — "씬 무결성 검사" UI 노출 (`f162835`)

`InvariantVerifierPanel` → **"씬 무결성 검사"** 로 확장. Run Verify 가
`verifyInvariants`(ADR-007) + `detectSelfIntersections` 동시 실행. clean 시
"자기교차 0" green, dirty 시 "⚠ 자기교차 N pair" 헤더 + `Face A ⨯ Face B 교차`
행 + "→ Jump"(두 face 동시 선택). `WasmBridge.detectSelfIntersections()` typed
wrapper(graceful fallback).

### D5 — 심화: coplanar + vertex-공유 관통 (`4d7a4d1`)

MVP 는 (a) coplanar overlap 을 놓치고(`triangle_triangle_intersection` 은
coplanar None 반환), (b) vertex-공유 face 를 blanket skip 했다. 심화로 두
사각지대 해소 — blanket skip 제거 후 **인접은 배제, 실제 관통·겹침은 잡는
STRICT predicate**:

- **Coplanar overlap** — 같은 평면 + 2D area 겹침(한 face 정점이 상대 strict
  내부 OR edge proper-cross). 인접 coplanar 는 side-by-side 라 겹침 0 → 미flag.
  vertex 공유해도 fold-back 이면 검출.
- **Vertex-공유 non-coplanar 관통** — strict edge-pierces-interior(endpoint 가
  plane 을 margin 이상 straddle + hit barycentric strict 내부). 공유 edge/vertex
  는 plane 위(d≈0)라 straddle 안 함 → 배제, 공유 feature **너머** 관통만 검출.
- **Non-coplanar no-share** — 기존 plane-interval 경로 유지.

epsilon 은 삼각형 크기 상대(unitless barycentric / edge-length 비율)라 scale
무관.

## 3. Lock-ins

- **L1** — 검사기는 read-only. 판정은 face-pair 단위, adjacency(공유 feature 접촉)
  는 STRICT predicate 로 배제.
- **L2** — 게이트 통합은 before/after count 비교 (기존 자기교차 입력 false
  rejection 0).
- **L3** — grid 는 candidate 누락 0 보장 (겹치는 AABB = cell 공유), 큰 face 는
  big 전수.
- **L4** — 실제 solid(clean/fillet/chamfer/cylinder) false positive 0 이 회귀
  게이트.

## 4. MVP 한계 (별도 트랙)

- 관통점이 earcut 대각선 위에 정확히 landing (measure-zero — 실제 관통은 area
  스팬이라 무해).
- coplanar 판정의 near-parallel tolerance (현재 1e-7 cross).
- 대형 mesh 프로파일링 (grid 실측 벤치는 미착수).

## 5. 회귀 자산 (절대 #[ignore] 금지)

- `clean_box_has_no_self_intersection` · `valid_chamfer_stays_clean_no_false_positive`
- `overlapping_disjoint_faces_are_detected` · `separated_faces_not_flagged`
- `closed_solid_with_folded_flap_is_detected`
- `grid_scales_and_finds_planted_intersection`
- `coplanar_overlapping_faces_detected` · `coplanar_shared_vertex_fold_detected`
- `vertex_sharing_fold_penetration_detected` · `coplanar_adjacent_faces_not_flagged`
- vitest: `self_intersect_clean_shows_zero_note` · `self_intersect_dirty_shows_pairs_and_jump`

## D. Acceptance Log (2026-07-03)

| commit | 계층 |
|--------|------|
| `6346e48` | 코어 (earcut + AABB + tri-tri) + WASM export |
| `ea8c2d5` | closure gate 통합 (엔진 결과 방어선) |
| `5d343fc` | 공간 grid broad phase |
| `f162835` | "씬 무결성 검사" UI 노출 |
| `4d7a4d1` | 심화 (coplanar + vertex-공유 관통) |

**검증**: axia-geo 2134 · axia-wasm 82 · vitest(panel 8, WasmBridge 326) PASS.
node + 브라우저 end-to-end. 실제 solid(clean/fillet/chamfer/cylinder 24-seg)
전부 SI clean — 인접 면 많은 mesh 에서 false positive 0. **위상 valid + 기하
겹침 class(coplanar/vertex-공유 관통 포함)를 자동 검출 → 시연 게이트 의존도
대폭 감소.**
