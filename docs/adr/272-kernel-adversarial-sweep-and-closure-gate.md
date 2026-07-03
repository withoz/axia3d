# ADR-272 — Kernel Adversarial Sweep + Closure-Preserving Gate

**Status**: Accepted (구현 완료 — 8 commit, `10f428f` ~ `d3d1bf8`)
**Track**: Kernel Robustness (ADR-267/268 계열 — "topology ≠ orientation")
**Cross-link**: ADR-267(watertight production gate) · ADR-268(curved-profile cut
winding, topology≠orientation) · ADR-007(face orientation policy) · ADR-019(Line
is Truth) · ADR-024(3-way chamfer) · ADR-273(self-intersection 검사기) ·
메타-원칙 #4 #6 #9 #10

---

## 1. Context — 위상 검사가 통과하는 silent corruption

ADR-268 이 확립한 핵심 교훈: **topology verification ≠ orientation/semantic
verification**. `verify_face_invariants`(I1–I5) · `face_set_manifold_info`
(is_closed_solid) 같은 위상 검사는 **면별/위상**만 보고, winding/normal/watertight
/connectivity 가 깨진 상태를 놓칠 수 있다.

본 ADR 은 이 교훈을 **체계적 감사**로 확장한다. 검증 안 된 커널 op(transform /
merge / split_edge / chamfer / fillet 등)을 **적대적으로(falsify) probe** 하여,
"기존 검사는 통과하지만 기하가 조용히 깨지는" 버그를 재현·수정했다.

### 적대적 검증 방법 (canonical)

- `packages/axia-wasm-node/dist` 로 bridge export 직접 호출 (raycast 없이),
  fresh `AxiaEngine` 마다.
- **진짜 버그 signal**: `verifyInvariants`(I1–I5) + `verifyVolumeIntegrity` 는
  valid 인데 `meshManifoldInfo().is_closed_solid` false, 또는 정점 이동 후
  중복 정점, 또는 fan-walk op 의 인접 면 누락.
- **주의**: `verifyOutwardNormals` 는 볼록 heuristic → 2-box 등 비볼록 mesh 에서
  오탐. 자체 centroid 로 재확인 필요.

## 2. 발견 — silent-corruption 6건

| # | op | 패턴 | 증상 (모두 위상 검사 통과) | commit |
|---|----|------|------|--------|
| 1 | `scale_verts`/`scale_faces` | winding | det<0(반사) 배율 시 전 면 normal 안쪽 | `10f428f` |
| 2 | `merge_faces_by_edge` | collinear | 이웃이 참조하는 T-junction 정점 collapse → closed solid 열림 | `49ccdbb` |
| 3 | `split_edge` | connectivity | 새 정점의 `v_next` fan 미배선 → fan-walk op(move/transform/fillet/offset) 인접 면 조용히 누락 | `fc38469` |
| 4 | `translate/rotate/scale_verts`+`move_vertex` | index | 정점 이동 시 spatial_hash 미갱신 → 이후 add_vertex weld 실패 → 중복 정점 | `9b0db8e` |
| 5 | `chamfer_vertex_3way` | 공유 edge | corner 를 면 bisector 내부점으로 대체 → 공유 edge 깸 → solid 열림 | `579a26e` |
| 6 | `fillet_edge` | 공유 edge | endpoint arc 를 F3 에 항상 forward splice → 절반 모서리서 arc 역순 → solid 열림 | `891404d` |

**공통 근본**: 6건 전부 **손으로 loop/HE 를 재구성하는(hand-rolled) op** + closure
를 검사하지 않는 테스트. `verify_face_invariants` 는 면별이라 closed→open 전환을
못 봄.

### 견고 검증 (거짓 수정 회피)

`mirror_faces`(winding 반전 + add_vertex spatial-hash weld), `erase_edge_
resynthesize`(ADR-019 의도된 파괴, DCEL valid), `array_linear/radial`(translation
winding 불변), `add_edge`(create_halfedge_pair 가 insert_into_v_ring 호출),
`push_pull`/`offset_face`(MoveOnly), `slice`/`drill`(ADR-267 게이트 fail-loud),
`boolean` demo(미지원 config fail-loud) 는 적대 검증 결과 **견고**. split_edge 가
fan 유지 헬퍼(`insert_into_v_ring`)를 우회한 **유일한 예외**였음.

## 3. Decision

### D1 — 근본 원인 개별 수정 (6 commit)

각 op 의 실제 결함을 root 에서 수정. 대표:
- #3 `split_edge`: 기존 `insert_into_v_ring`/`remove_from_v_ring` 헬퍼로 v-ring
  재구성 (fan-walk 무결성 복구). radial-chain 에 의존 안 하고 loop 스캔으로
  merge 수정 보강.
- #5 `chamfer`: bisector 내부점 → **edge 위 trim점**(v+radius·unit(neighbor−v)).
  공유 edge trim점은 양면 동일 → add_vertex dedup weld → 닫힘 유지.
- #6 `fillet`: `active_shared` 두 면 중 v_a→v_b 순회 면을 F1 으로 명시 선택.

### D2 — Closure-Preserving Gate (systemic 방어선, `e34a1e5`)

**진단**: 기존 `integrity_gate_passed`(ADR-267)는 `IntegrityScope::OpenMesh` →
`open_boundary_edges = 0` 강제 (mesh_invariants.rs:451). 즉 crack + I1–5 만 잡고
**closed→open(boundary edge) 전환은 구조적으로 못 봄**. slice/drill 은 우연히
crack 을 만들어 잡혔지만 merge/chamfer/fillet 은 boundary 를 만들어 안 잡혔고,
애초에 이들 WASM 노출은 게이트 호출이 0.

**해법**: `closure_preserving_gate_passed`(axia-wasm) — `IntegrityScope::
ClosedSolid` 전체 활성 face 검사, **입력이 완전 watertight(before_boundary==0)일
때만** 결과가 연 경우(open_boundary_edges>0) reject + snapshot 롤백 + txn cancel.
이미 열린 sheet 입력은 boundary 로 거부 안 함 → **false rejection 0**. crack /
invariant 는 모든 입력 차단. merge / chamfer / fillet WASM 3곳 wiring. (ADR-273
에서 self-intersection 검사도 추가.)

### D3 — fillet 실사용 복구 (`574a12b`)

`fillet_edge` 의 F1/F2 방향 임의 순서로 절반 모서리가 `"F1 loop doesn't contain
the edge"` 로 조용히 no-op 했던 것을 D1 #6 와 함께 수정 → box 12 모서리 전부
fillet.

### D4 — chamfer flap 입력 검증 guard (`d3d1bf8`)

브라우저 시연 게이트가 발견: fillet-strip 정점(짧은 arc edge)을 큰 radius 로
chamfer 하면 trim 삼각형이 이웃을 overshoot 해 self-intersecting flap 생성 —
closed/manifold/crack/winding + closure gate 전부 통과(열림 아니라 self-
intersection). `edge_trim_points` 에 **radius < 각 incident edge 길이** 강제
(overshoot 차단, 파괴 전 fail-loud). clean corner 아닌 정점 reject. (self-
intersection 자체의 결과 검증은 ADR-273.)

## 4. Lock-ins

- **L1** — 새 hand-rolled face-rebuild op 은 반드시 closure/watertight 회귀 포함.
- **L2** — radial-chain(v_next) 무결성에 의존 금지 → loop 스캔 등 robust 경로 선호.
- **L3** — 게이트는 op 이 *새로* 만든 손상만 reject(before/after 비교) → open-mesh
  입력 false rejection 0.
- **L4** — 커밋 전 axia-geo 뿐 아니라 axia-core 도 실행 (49ccdbb 가 axia-geo 만
  통과시켜 grid 회귀를 놓친 사례).
- **L5** — node-WASM probe 는 camelCase export 명 사용 (snake_case 오타 = throw →
  "no-op" 오판).

## 5. 회귀 자산 (절대 #[ignore] 금지)

- `scale_negative_determinant_keeps_normals_outward`
- `merge_after_split_keeps_solid_closed_tjunction`
- `split_edge_rebuilds_vertex_fan`
- `translate_vertex_updates_spatial_hash_for_weld`
- `chamfer_3way_keeps_cube_closed` · `chamfer_3way_rejects_radius_exceeding_edge`
- `fillet_cube_edge_keeps_closed`(12/12 모서리)

## D. Acceptance Log (2026-07-03)

| commit | 내용 |
|--------|------|
| `10f428f` | scale det<0 winding 보정 (#4 winding) |
| `49ccdbb` | merge collinear T-junction 보존 (#2) |
| `fc38469` | split_edge v_next fan 재구성 + merge coplanarity (#5) |
| `9b0db8e` | 정점 이동 후 spatial_hash reindex (#6) |
| `579a26e` | chamfer edge-trim (#2) |
| `891404d` | fillet arc F3 방향 (#2) |
| `e34a1e5` | closure-preserving gate (systemic) |
| `574a12b` | fillet F1/F2 방향 실사용 복구 |
| `d3d1bf8` | chamfer flap 입력 검증 guard |

**검증**: axia-geo 2124+ · axia-core 429 · axia-wasm 82 PASS. node + 브라우저
end-to-end. 회귀 자산 7건. 브라우저 시연 게이트가 자동 검사(+closure gate)가
놓친 chamfer flap 을 발견 — 시각 검증의 가치 재확인.

## 6. 남은 트랙

- self-intersection 자체 검출 → **ADR-273**.
- make_box(push_pull) 첫-모서리 fillet 결함(bnd=8) — 게이트가 잡지만 근본
  fillet 원인 미해결, 별도 조사.
