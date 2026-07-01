# ADR-244 — Slice 견고화 Phase 1+ (C2 Tier B — crossed holed face, 홀 한쪽)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 1 후속 (Slice 견고화 C2 Tier B)
- **Depends on**: ADR-243 (C2 Tier A) / slice_volume_by_plane (`slice.rs`) /
  Phase G case-(a) (`face_split.rs:483-516`) / `Mesh::split_face` (mesh.rs:8230) /
  `point_in_face` / `reassign_loop_face` / `add_face_with_holes`

## 1. Context

ADR-243 (Tier A) 는 홀이 cut 한쪽에 strictly 있을 때만 허용하고, 홀 면이 cut 을
절단/graze 하면 bail 했다. **Tier B** 는 그중 *outer 가 절단되지만 홀(inner loop)은
여전히 한쪽* 인 경우를 활성화한다 — 예: 중앙 top pocket 이 있는 box 를 pocket 옆
(x=7) 수직 절단 → top 면의 outer 는 절단되나 pocket(x∈[−4,4]) 은 x<7 쪽에 통째로.

de-risk: Phase G `split_face_by_line` 의 **case-(a) skeleton** (face_split.rs:483-516)
이 정확히 이 작업 — detach inners → split outer → 각 hole 의 sample vert 를
`point_in_face` 로 두 sub-face 와 검사 → `reassign_loop_face` + `add_inner`. slice 는
이미 authoritative On verts 를 가지므로 `split_face_by_line` *entry* 가 아닌 그
*helper + recipe* 를 재사용한다 (line endpoint 재투영 회피).

## 2. Decision

**분류 루프 (slice.rs)** — crossing(outer above & below) holed 면 처리:
- **convex 강제**: outer 가 정확히 2 On (`dedup_on.len() == 2`) — non-convex crossed
  holed 면은 bail (Tier B+).
- **inner one-side 강제**: 각 inner loop 의 verts signed-distance 검사 → 한쪽이면
  OK, 양쪽(crossed inner)이면 bail (annular = Tier C).
- 통과 시 `holed_crossings` 에 기록.
- all-above/all-below grazing holed (On vert 보유)는 여전히 conservative bail
  (Tier A 한계 L-243-11 보존 — all-below grazing 의 step 5.5 위험).

**Step 4c (신규)** — Phase G case-(a) recipe 포팅:
- 각 holed crossing: inner LoopRef + sample vert 저장 → `inners_mut().clear()` +
  `bump_boundary_version_after_inners_mut()` → `split_face(cut_a, cut_b)` →
  side_of_face 분류 → 각 saved hole: `point_in_face(fa, sample)` → target →
  `reassign_loop_face(target)` (target != ci.face 시) + `add_inner`.

**Step 5.5 hole-preservation 패치** (ADR-243 에서 deferral 한 것, 이제 필요):
- below sub-face 재구성 시 inner loops 를 remove_face *전* 캡처 → cut-vert 치환
  (hole verts 는 below=non-cut → identity) → `add_face_with_holes` 로 재구성.
  hole 이 below sub-face 에 재배치되면 detach rebuild 에서 보존.

**`reassign_loop_face` (face_split.rs) `pub(crate)` 승격** — slice 재사용 (SSOT).

**trim 자동 상속** (ADR-241).

## 3. Lock-ins

- **L-244-1** Tier B scope: crossed holed 면 = convex outer (2 On) + 모든 inner loop
  strictly 한쪽. non-convex crossed holed → bail (Tier B+). crossed inner → bail
  (Tier C / annular).
- **L-244-2** Phase G case-(a) recipe 재사용 (detach → split_face → point_in_face →
  reassign_loop_face → add_inner). `split_face_by_line` entry 아닌 helper 재사용.
- **L-244-3** Step 5.5 hole-preservation: below sub-face inner loops 를 remove_face
  전 캡처 + add_face_with_holes 재구성. hole verts 는 non-cut → identity 치환.
- **L-244-4** `reassign_loop_face` pub(crate) (SSOT, 복제 금지).
- **L-244-5** 양 분기 검증: hole on below (reassign to fb, step 5.5 보존) + hole on
  above (reassign to fa, above 미rebuild 자연 보존).
- **L-244-6** Tier A gate (strictly one side) + nesting guard (annular) + grazing
  conservative bail 모두 보존.
- **L-244-7** trim 자동 상속.
- **L-244-8** 메타-원칙 #6(de-risk) / #16(명시 trigger) / ADR-046 P31 #4 additive.
- **L-244-9** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo `slice_volume` +2 → **24 PASS**:
  - `slice_box_with_pocket_vertical_tier_b_reassigns_hole` — box-with-pocket 를
    x=7 수직 절단 → top outer 절단 + pocket(x∈[−4,4]) below(x<7) 재배치 → below half
    inners==1 보존 / above inners==0 / 양 절반 closed + invariants.
  - `slice_box_with_pocket_vertical_hole_above` — x=−7 → pocket 이 above(x>−7)
    재배치 → above inners==1 (above 미rebuild 자연 보존) / below 0.
  - `slice_through_holed_face_bails_tier_bc` (갱신) — x=0 → pocket crossed (inner
    양쪽) → Tier C annular bail.
- 전체 axia-geo 1993 lib + 24 slice + axia-core 399 = 0 실패. WASM 재빌드(SIMD).

## 5. 검증 (engine + workspace)

- **Engine**: 위 3 integration 테스트 (Tier B 양 분기 + Tier C bail).
- **Workspace regression**: face_split.rs pub(crate) 변경 + slice step 4c/5.5 변경에도
  axia-geo 1993 lib + axia-core 399 = 0 실패.
- **Browser 한계**: holed CLOSED 솔리드 headless 구성 비현실적 (ADR-243 동일 — engine-only,
  신규 surface 0). engine + workspace 회귀가 검증 layer.

## 6. Lessons

- **L1 Phase G case-(a) recipe 의 직접 재사용**: 같은 "detach → split outer →
  containment 재배치" 패턴이 line-based(face_split) 과 plane-based(slice) 양쪽에
  적용. helper(`point_in_face`/`reassign_loop_face`) 만 재사용, entry 는 분리
  (slice 는 authoritative On vert 보유). Pattern-12 확장.
- **L2 step 5.5 패치는 Tier B 에서 genuine 필요**: ADR-243 Tier A 에서 gate 가 원천
  차단해 deferral 했으나, Tier B 는 below sub-face 에 hole 이 재배치되므로 hole-
  preservation 이 필수. de-risk 의 "deferral until genuinely needed" 정확.
- **L3 양 분기 명시 검증 (probe > review)**: hole-below(fb 재배치) + hole-above(fa
  재배치, above 미rebuild) 양쪽을 별도 테스트. C1/C2 의 "기하는 경험적 probe" 교훈
  답습 — 두 분기가 다른 코드 경로(step 5.5 통과 여부).
- **L4 crossed inner = Tier C 경계 명확화**: outer crossed + inner crossed = annular
  → Tier C. Tier B 는 inner strictly 한쪽만. 경계가 inner loop classification 으로
  깔끔히 분리.

## 7. 후속 (Tier C)

- **C2 Tier C** (홀 관통 annular cap): inner loop 가 crossed → annular 단면. 필요:
  inner edge 도 step 1 에서 split (crossing inner edge) + cut loop nesting 분류
  (outer vs inner) + holed cap (add_face_with_holes) + annular below-detach. 별도 ADR.
  (현재 nesting guard 가 annular 를 안전하게 bail — Tier C 가 이를 해제.)

## 8. Cross-link

- ADR-243 (C2 Tier A — 직전, gate/manifold/nesting guard 기반) / ADR-240 (로드맵) /
  ADR-241 (C5 trim) / ADR-242 (C1 비볼록) / Phase G case-(a) (face_split.rs:483-516,
  recipe source) / `Mesh::split_face` / `point_in_face` / `reassign_loop_face` /
  `add_face_with_holes` / ADR-007 (manifold invariants).
- 메타-원칙 #6 (de-risk) / #16 (명시 trigger) / ADR-046 P31 #4 (additive) /
  LOCKED #44 (Complete Meaning per Merge).
