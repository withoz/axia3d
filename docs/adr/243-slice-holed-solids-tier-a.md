# ADR-243 — Slice 견고화 Phase 1 (C2 Tier A — 홀 있는 솔리드 slice, 홀 한쪽)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 1 (Slice 견고화) — sub-step 3 of 3 (C2 Tier A)
- **Depends on**: ADR-240 (로드맵) / ADR-241 (C5 trim) / ADR-242 (C1 비볼록) /
  slice_volume_by_plane (`slice.rs`) / face_set_manifold_info (`mesh.rs:8796`) /
  add_face_with_holes (`mesh.rs:5017`)

## 1. Context

`slice_volume_by_plane` 은 진입 게이트(slice.rs:111)에서 **inner loop(hole)을 가진
모든 면을 거부**했다. C2 는 이 금지를 해제한다. de-risk(병렬 6-investigator
feasibility workflow)로 3 tier 식별:

- **Tier A**: 홀이 cut 한쪽에 완전히 있음 (홀 면이 all-above/all-below).
- **Tier B**: 평면이 홀 면의 *outer* 를 절단, 홀은 한쪽 (inner-loop 재배치 필요).
- **Tier C**: 평면이 홀을 *관통* → annular 단면 (holed cap 필요).

사용자 결재 **A — 홀 한쪽 MVP** (complete-meaning 단위, B/C 는 후속).

**핵심 수학적 근거 (affine)**: 평면 면 위에서 signed plane distance `d(v) =
(v−origin)·normal` 는 affine functional → 연결 평면 영역의 극값은 경계 vertex 에서
달성. 따라서 **outer loop 의 모든 vertex 가 plane 위쪽이면 모든 hole vertex 도
위쪽** (hole 은 outer 안에 포함). ⟹ 기존 분류(outer loop 만 검사)가 holed 면에도
정확. all-above/all-below holed 면은 step 5.5(below-detach)에서 On vert 가 없어
cut-vertex set 에 안 들어가므로 그대로 보존 → **홀 자동 보존, 추가 처리 0**.

## 2. Decision

**`slice_volume_by_plane` (slice.rs)**:
- 진입 게이트(line 111)의 일괄 hole 거부 **제거**.
- 분류 루프에 **Tier A gate**: 면이 inners() 보유 **AND** on_verts 비어있지 않으면
  (= cut 을 절단 또는 graze) bail — 정확한 사용자 facing 메시지("holed face
  touches the cut plane … position the cut clear of the hole"). Edge-split(step 2)
  후 crossing 면은 항상 On vert 보유 → crossing+grazing holed 면 모두 포착.
  strictly 한쪽 holed 면(On vert 없음)만 통과.

**`face_set_manifold_info` (mesh.rs:8796) — hole-aware 화 (GLOBAL bug fix)**:
- 기존엔 `face_outer_edges` 만 계수 → holed 면의 inner loop edge 미계수 → **닫힌
  holed 솔리드를 "안 닫힘"으로 오판** (boundary_edge_count 거짓 양수).
- inner loop edge(`collect_loop_hes(inner.start)`)도 계수 추가. 비-holed 면은
  inners() 빈 배열 → 무변경 (회귀 안전). slice step 8 self-check 가 holed half 를
  올바르게 닫힘으로 인식하게 됨.

**step 5.5 hole-preservation 패치 불필요** (Tier A gate 가 cut-접촉 holed 면을
원천 차단 → 살아남는 holed 면은 모두 strictly 한쪽 → step 5.5 가 as-is 유지). B/C
진입 시 추가 (cut 접촉 holed 면 처리가 본격 필요해질 때).

**Nesting guard (annular cross-section, 적대 검증 후 추가)**: per-face gate 는 holed
*면* 만 검사한다. 그러나 cut 이 hole *영역* (non-holed pocket 벽으로 둘러싸인)을
가로지르면, holed top 면은 all-above 라 gate 를 통과하지만 pocket 벽들이 절단되어
**nested(annular) 단면** (outer 박스 loop + inner pocket loop) 이 형성된다. step 6 의
per-loop simple cap 은 이를 2개 별도 cap 으로 잘못 생성(큰 cap 이 hole 영역 덮음,
boundary=0/invariants=valid 이지만 geometrically 틀림). → step 5(assemble_loops) 후
`cut_loops_are_nested`(2D 투영 + point-in-poly 로 loop 포함 검사)로 **nested loop bail**
(annular = Tier C). Disjoint loop (분리된 simple 단면, 예: 2 prong)는 허용 유지.

**trim_volume_by_plane (ADR-241)**: slice 호출 → 비-홀 trim 자동 상속 (코드 0).

## 3. Lock-ins

- **L-243-1** Tier A scope: holed 면은 STRICTLY 한쪽(On vert 없음)일 때만 허용.
  crossing/grazing holed → bail (B/C 미지원).
- **L-243-2** Affine 근거: outer all-above ⟹ 전체 면(홀 포함) above. 분류 무수정.
- **L-243-3** `face_set_manifold_info` hole-aware (outer + inner edge 계수) —
  GLOBAL bug fix, 비-holed 무영향.
- **L-243-4** step 5.5 패치는 Tier B 로 deferral (gate 가 Tier A 에서 불필요화).
- **L-243-5** Gate 메시지는 사용자 facing(C2 Tier B/C 한계 명시, silent 손상 아님).
- **L-243-6** add_face_with_holes(mesh.rs:5017)는 테스트 fixture 에서만 사용
  (engine 본체는 Tier A 에서 holed 면을 *읽기*만 — 새 holed 면 생성 안 함).
- **L-243-7** trim 자동 상속 (Pattern-12).
- **L-243-8** 메타-원칙 #6(de-risk) / #16(명시 trigger) / ADR-046 P31 #4 additive.
- **L-243-9** 절대 #[ignore] 금지.
- **L-243-10** Nesting guard: cut 이 nested(annular) 단면을 만들면 bail (per-face
  gate 가 non-holed 벽으로 둘러싸인 hole 영역 관통을 못 잡으므로 loop-level 보강).
  disjoint loop 는 허용.
- **L-243-11 (known limitation)** Grazing holed 면 (outer vertex 1개가 정확히 plane
  위, 나머지 strictly 한쪽)은 conservative 하게 bail. all-above grazing 은 실제론
  안전하나(above 미rebuild), all-below grazing 은 step 5.5 hole-loss 위험이라 둘 다
  거부 = "strictly 한쪽" scope 의 의도된 경계. 정제는 Tier B (step 5.5 패치 동반).

## 4. 회귀

- axia-geo `slice_volume` +4 → **22 PASS**:
  - `slice_box_with_pocket_clear_of_hole_preserves_hole` — box-with-blind-pocket
    (top 면 holed) 를 pocket 아래 z=3 수평 절단 → 1 cut loop + holed top 면이
    above half 에 그대로(inners().len()==1 보존) + 양 절반 closed + invariants.
  - `slice_through_holed_face_bails_tier_bc` — 같은 솔리드 x=0 수직 절단(홀 면
    관통) → 정확한 bail("holed face … hole").
  - `trim_box_with_pocket_keep_above_preserves_hole` — trim keep-above → kept
    upper half closed + holed top 면 + 홀 보존.
  - `slice_through_hole_region_annular_bails` (nesting guard, 적대 검증 후) — 같은
    솔리드 z=8 (pocket floor z=6 ↔ opening z=10 사이) 수평 절단 → pocket 벽 절단으로
    annular 단면(nested loop) → 정확한 bail("annular … hole"). 경험적 probe 가
    이 false-allow 를 노출 (수정 전엔 cap 2개 별도 생성으로 잘못 성공).
  - 신규 fixture `make_box_with_top_pocket` (11-face genu-0 솔리드, add_face_with_holes
    holed top + 4 pocket walls + floor).
- **GLOBAL 회귀 검증** (face_set_manifold_info hole-aware): axia-geo 1993 lib + 21
  slice + 기타 = 0 / axia-core 399 / axia-wasm / axia-transaction = 0. WASM
  재빌드(SIMD 11080).

## 5. 검증 (engine + workspace)

- **Engine**: 위 3 integration 테스트 (Tier A slice/trim + Tier B/C bail gate).
- **Workspace regression**: face_set_manifold_info GLOBAL 변경에도 2400+ 테스트 0
  실패 (hole-aware 가 비-holed 무영향임을 실증).
- **Browser 한계 (정직 명시)**: holed CLOSED 솔리드의 headless 구성은 비현실적 —
  `add_face_with_holes`가 bridge 미노출, pushpull-pocket 은 XIA 소유권 불확실 +
  multi-step fragile, preview canvas 0×0(mouse flow 불가). slice/trim WASM
  round-trip 자체는 ADR-242 C1(U-prism) 브라우저 smoke 에서 이미 입증됨. C2 Tier A
  는 engine-only(신규 WASM/TS surface 0)라 engine 통합 + 워크스페이스 회귀가
  적절한 검증 layer.

## 6. Lessons

- **L1 affine 근거가 Tier A 를 거의 공짜로 만듦**: outer 분류가 holed 면에도
  엄밀히 정확(극값=경계 vertex) → all-above/below holed 면은 추가 처리 0.
  de-risk 가 이를 수학적으로 확정.
- **L2 hole-aware gate 의 단순화**: "holed 면이 cut 접촉(On vert)하면 bail" 한
  줄이 crossing+grazing 양쪽을 포착 + step 5.5 hole-loss 함정을 원천 차단 →
  step 5.5 패치 deferral. workflow 가 제시한 두 옵션(step 5.5 fix vs restrict)
  중 restrict 가 MVP 에 더 단순·안전.
- **L3 latent global bug 발견**: feasibility de-risk 가 face_set_manifold_info 의
  inner-blind 계수(닫힌 holed 솔리드 오판)를 노출 → Tier A 의 부수 효과로 정석
  수정. 회귀 0(비-holed 무영향). de-risk 의 후행 가치.
- **L4 engine-only robustness = surface 무변경**: slice/trim 경로(scene/WASM/TS)는
  ADR-241 에서 wired — C2 Tier A 는 `slice_volume_by_plane` 내부 + manifold metric
  만 변경. 브라우저 새 동작은 WASM 재빌드만 필요.
- **L5 적대 검증 — 경험적 probe > LLM 추론 (geometry)**: 2개 적대 워크플로우 실행.
  per-face gate 의 진짜 결함(annular false-allow, hole 영역을 non-holed 벽으로 관통)은
  **LLM 리뷰가 놓치고 경험적 probe(z=8 절단 직접 실행)가 잡음** → nesting guard 로
  수정. LLM 워크플로우의 2 "confirmed" 는: (a) grazing over-reject = over-restriction
  (안전, documented limitation L-243-11) (b) inner-loop 가 outer 없이 plane 가로지름 =
  **planar 에서 affine-impossible false positive** (verify agent 가 non-planar
  "spline dips" 가정 — slice 의 planar 도메인 밖). 교훈: 기하 알고리즘의 진짜 결함은
  실행 가능한 probe(ground truth)로 확인 — LLM reasoning 은 보조. (C1 의 false-positive
  + C2 의 miss 양쪽이 같은 교훈.)

## 7. 후속 (Phase 1 완료 → Phase 2+)

- **C2 Tier B** (crossed holed-face outer, 홀 한쪽): Phase G case-(a) skeleton
  (face_split.rs:479-524) + `reassign_loop_face`/`point_in_face`/`classify_holes`
  포팅 + step 5.5 hole-preservation 패치. 별도 ADR.
- **C2 Tier C** (홀 관통 annular cap): add_face_with_holes cap + cut-loop nesting
  분류(point_in_polygon_even_odd / nest_loops 패턴) + annular below-detach. 별도 ADR.
- Phase 1 (Slice 견고화) = C5(241) + C1(242) + C2 Tier A(243) closure. 이후
  Phase 2 Punch 확장(P1+P5+P6) / Phase 3 Extrude 완성(E1+E2+E3) — ADR-240 로드맵.

## 8. Cross-link

- ADR-240 (로드맵 Phase 1) / ADR-241 (C5 trim) / ADR-242 (C1 비볼록 — 자매 sub-step) /
  slice_volume_by_plane (slice.rs) / face_set_manifold_info (mesh.rs:8796 hole-aware) /
  add_face_with_holes (mesh.rs:5017) / ADR-007 (manifold invariants) / face_split.rs
  Phase G (Tier B 후속 재사용 자산).
- 메타-원칙 #6 (de-risk) / #16 (명시 trigger) / ADR-046 P31 #4 (additive) /
  LOCKED #44 (Complete Meaning per Merge).
