# ADR-245 — Slice 견고화 C2 Tier C (annular cross-section — holed caps)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 1 후속 (Slice 견고화 C2 Tier C)
- **Depends on**: ADR-243 (C2 Tier A — nesting guard) / ADR-244 (C2 Tier B) /
  slice_volume_by_plane (`slice.rs`) / `add_face_with_holes` (mesh.rs:5017) /
  `orient_loop_for_normal` / `point_in_poly_2d`

## 1. Context

ADR-243 (Tier A) 가 도입한 nesting guard 는 cut 이 **annular(nested-loop) 단면** 을
만들면 bail 했다 — cut 이 hole *영역* 을 관통하면 outer loop 안에 inner loop 가 생겨
step 6 의 per-loop simple cap 이 2개 cap 으로 잘못 생성(큰 cap 이 hole 영역 덮음).
**Tier C** 는 이를 해제하여 **holed cap** 으로 올바르게 봉인한다.

**핵심 통찰 — annular 2-sub-case**:
- **C-i (hole 영역을 non-holed 벽으로 관통)**: 예 box-with-pocket 를 z=8 (pocket
  floor z=6 ↔ opening z=10 사이) 절단 → 4 box walls(outer loop) + 4 pocket walls
  (inner loop) 절단. crossed 면은 모두 non-holed 정상 crossing, holed top 면은
  all-above(Tier A 보존). → **본 ADR MVP**.
- **C-ii (holed 면 자체의 inner loop 가 crossed)**: 예 x=0 절단 → holed top 의
  inner pocket loop 가 잘림. step 1 의 inner edge split 필요 + holed-face-crossed-
  inner 처리 → **deferral** (Tier B gate 가 "crossed inner → Tier C bail" 유지).

C-i 는 step 1 변경 불필요 (inner loop chords 가 non-holed 벽의 outer edge crossing
에서 나옴). 순수 "nested cut loops → holed cap" 작업.

## 2. Decision

**Step 5.4 (신규) — nesting classifier (guard 대체)**: `classify_loop_nesting`
이 cut loops 를 nesting 그룹 `Vec<(outer_idx, Vec<hole_idx>)>` 으로 분류 (2D 투영
+ point-in-poly containment depth). depth 0 = outer, depth 1 = hole(parent =
containing outer), depth ≥ 2 = **bail** (>1-level nesting, MVP single-level).
disjoint outer 들은 각자 그룹 (U-prism 2 prong 등 보존).

**Step 6 — group 별 cap (per-loop 대체)**: lone outer (no holes) → simple cap
(`add_face`). annular group (outer + holes) → **holed cap** (`add_face_with_holes`).
winding: cap_above outer = `orient_loop_for_normal(−plane.normal)`, holes =
opposite(+normal); cap_below outer = +normal, holes = −normal (add_face_with_holes
가 holes 를 반대 방향으로 읽도록).

**Below-detach (기존)**: 이미 모든 cut vert (outer + inner) 를 dup + cut_loops_below
을 동일 index 로 빌드 → 그룹이 index 참조라 자연 정합 (변경 0).

## 3. Lock-ins

- **L-245-1** C-i scope: annular from nested cut loops (hole 영역을 non-holed 벽 OR
  all-above/below holed 면 옆 관통). holed 면의 inner loop 가 crossed (C-ii)는 Tier B
  gate 가 여전히 bail (step 1 inner edge split 미구현).
- **L-245-2** nesting classifier: depth 0 outer / depth 1 hole / depth ≥ 2 bail
  (single-level only). disjoint outer 각자 그룹.
- **L-245-3** holed cap: outer + holes 그룹 → add_face_with_holes (above + below
  duplicates). lone outer → simple add_face (cube/U-prism/comb 보존).
- **L-245-4** winding: hole loops 는 cap outer 의 반대 방향 (add_face_with_holes 계약).
- **L-245-5** below-detach 변경 0 (모든 cut vert dup + index-aligned cut_loops_below).
- **L-245-6** Tier A (strictly one side) + Tier B (crossed outer, hole 한쪽) + grazing
  conservative bail 모두 보존.
- **L-245-7** multi-hole (1 outer + N holes) + multi-group (disjoint outers) 지원.
- **L-245-8** 메타-원칙 #6(de-risk) / #16(명시 trigger) / ADR-046 P31 #4 additive.
- **L-245-9** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo `slice_volume` +2 → **25 PASS**:
  - `slice_through_hole_region_annular_caps` — box-with-pocket z=8 절단 → outer +
    1 hole → holed cap (above/below 각 inners==1) + 양 절반 closed + invariants.
    (ADR-243 의 `_annular_bails` → `_annular_caps` 전환: bail → success.)
  - `slice_through_two_hole_regions_multi_hole_cap` — 2-pocket box z=8 → outer +
    2 holes → 1 group, cap inners==2 (multi-hole grouping). 신규 fixture
    `make_box_with_two_pockets`.
- 전체 axia-geo 1993 lib + 25 slice + axia-core 399 = 0 실패. WASM 재빌드.

## 5. 검증 (engine + workspace)

- **Engine**: annular (1 hole + 2 holes) + Tier A/B/disjoint/bail 모두 통과.
- **Workspace regression**: step 5.4/6 변경에도 axia-geo 1993 lib + axia-core 399 = 0.
- **Browser 한계**: holed CLOSED 솔리드 headless 구성 비현실적 (ADR-243/244 동일 —
  engine-only, 신규 surface 0). engine + workspace 회귀가 검증 layer.

## 6. Lessons

- **L1 guard → classifier 전환**: ADR-243 의 nesting guard (bail) 가 Tier C 에서
  자연스럽게 classifier 로 진화 (같은 containment 로직, bail 대신 group). 보수적 guard
  를 먼저 두고 나중에 해제하는 패턴 (additive-first).
- **L2 annular 2-sub-case 분리 (C-i vs C-ii)**: hole 영역 관통(non-holed 벽, step 1
  무변경) vs holed 면 inner crossed(step 1 inner edge split 필요)를 명확 분리 →
  C-i 를 깔끔한 MVP 로, C-ii 를 deferral. 같은 "annular" 도 구현 경로가 다름.
- **L3 multi-hole probe (probe > review)**: 1-hole annular 만으로 충분해 보였으나
  multi-hole(2-pocket) + multi-group grouping 을 별도 probe 로 검증 (C1/C2 교훈 답습).
  grouping 로직의 untested 조합을 경험적으로 확인.
- **L4 below-detach 의 index-aligned 설계가 Tier C 를 거의 공짜로**: 기존 below-detach
  가 모든 cut vert dup + cut_loops_below 를 동일 index 로 빌드 → nesting group 이
  index 참조라 holed cap_below 가 변경 0 으로 정합. 기존 자산의 후행 가치 (Pattern-12).

## 7. 후속

- **C-ii (holed 면 inner crossed)**: step 1 에 inner loop edge 추가 (crossing inner
  edge split) + holed-face-with-crossed-inner split (Phase G case-(b) hole-eaten
  recipe 재사용 가능) → 별도 ADR. (현재 Tier B gate 가 안전 bail.)
- **>1-level nesting** (hole within a hole): multi-level cap + below-detach → 별도 ADR.
- Phase 1 Slice 견고화 = C5(241) + C1(242) + C2 Tier A(243) + Tier B(244) +
  Tier C C-i(245). 이후 Phase 2 Punch / Phase 3 Extrude (ADR-240 로드맵).

## 8. Cross-link

- ADR-243 (C2 Tier A — nesting guard, 본 ADR 이 classifier 로 진화) / ADR-244 (C2
  Tier B) / ADR-240 (로드맵) / ADR-241 (C5 trim) / ADR-242 (C1 비볼록) /
  `add_face_with_holes` (mesh.rs:5017 — annular cap primitive) / `orient_loop_for_normal` /
  `point_in_poly_2d` / Phase G case-(b) (face_split.rs — C-ii 후속 재사용 anchor) /
  ADR-007 (manifold invariants).
- 메타-원칙 #6 (de-risk) / #16 (명시 trigger) / ADR-046 P31 #4 (additive) /
  LOCKED #44 (Complete Meaning per Merge).
