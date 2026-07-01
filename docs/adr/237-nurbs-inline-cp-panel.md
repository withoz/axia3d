# ADR-237 — NURBS Inline Control-Point Panel (A2-MVP-5)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS Patch Editor A2 (ADR-235 roadmap MVP-5 — 정밀 인라인 편집)
- **Depends on**: ADR-235 (A2 roadmap) / ADR-236 (drag) / ADR-234 (position) / ADR-233 (weight) /
  ADR-232 (control-net overlay + getNurbsSurfaceParams)

## 1. Context

ADR-235 로드맵 **MVP-5 (inline panel)** — 제어점 목록 표에서 x/y/z 숫자 + weight 슬라이더로 정밀
편집. 클릭→prompt(233/234) + 드래그(236)의 정밀 보완. **엔진 0** (편집 commit 시 re-create).
사용자 결재 **A** — NURBS 면 선택 시 자동 표시(오버레이 wiring 답습) + commit(숫자 Enter/blur,
슬라이더 release) 시 재생성.

**de-risk**: ConstraintPanel 이 정확한 템플릿 — 생성자(container, bridge, cb) + panelEl
(createElement+innerHTML) + injectStyles + show/hide/toggle/refresh, `refresh()`→data→`renderList()`
(per-row DOM + change/click 핸들러). 오버레이는 main.ts:469 selection.onChange wiring → 패널도 동일.

## 2. Decision

- **공유 re-create SSOT** — `web/src/tools/nurbsRecreate.ts` `recreateNurbsPatch(bridge, oldFid,
  params, editedCtrlPts, editedWeights, hooks)` → {newFid, newParams}. createNurbsSurface(edited) +
  deleteFace(old, create-then-delete) + syncMesh + selectFaces + updateOverlay. **NurbsEditTool +
  NurbsPatchPanel 양쪽이 사용** (DRY, full-2 단일 Undo wrap 이 여기 들어올 자리). NurbsEditTool._recreate
  를 이 헬퍼로 리팩터 (동작 동일, 11 테스트 보존).
- **NurbsPatchPanel** (`web/src/ui/NurbsPatchPanel.ts`, ConstraintPanel 패턴) — CP 목록 표:
  per-CP row `[#idx (u,v)] [x] [y] [z] [weight 슬라이더] [weight 숫자]`. 숫자 'change'(Enter/blur)
  / 슬라이더 'change'(release) → `editCP(i, pos, weight)` → recreateNurbsPatch. 슬라이더 'input' 은
  숫자 표시 sync 만(재생성 안 함). weight ≤ 0 / NaN → reject Toast.
- **자동 표시** — main.ts selection.onChange 확장: 단일 NURBS-class face(6/7/8) → 오버레이
  (ADR-232) + `panel.showFor(faceId)`; 그 외 → 둘 다 clear/hide.
- **focus-loss 방지** — `recreating` 가드: editCP 의 recreate 가 selectFaces→onChange→showFor 재진입
  시 showFor 가 skip (editCP 가 직접 render). 재생성당 정확히 1회 render.

## 3. Lock-ins

- **L-237-1** MVP-5 = inline panel (정밀, 표). 드래그(236)/클릭 prompt(233/234)와 **공존** — 같은
  선택 패치 / 같은 오버레이 / 같은 recreateNurbsPatch SSOT.
- **L-237-2** recreateNurbsPatch = re-create 단일 SSOT (tool + panel). full-2 (ADR-239) 단일 Undo
  wrap 이 여기 들어옴.
- **L-237-3** 자동 표시 — NURBS 면 선택 시 (오버레이 wiring main.ts:469 답습). 비-NURBS/multi → hide.
- **L-237-4** commit on change (숫자 Enter/blur, 슬라이더 release) — 편집-당-재생성 churn 최소
  (라이브 입력마다 재생성 아님). 라이브 곡면 변형은 full-1 (ADR-238).
- **L-237-5** `recreating` 가드 — 자기 re-create 의 selectFaces→onChange→showFor 재진입 차단
  (focus-loss + 이중 render 방지).
- **L-237-6** weight ≤ 0 / NaN reject (메타-원칙 #16 silent skip 차단).
- **L-237-7** 엔진/WASM 0 (TS-only, re-create 재사용). 신규 catalog entry 0 (auto-show, command
  아님) → CC count 173 불변. ADR-046 P31 #4 additive.
- **L-237-8** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+9** (NurbsPatchPanel: hidden/showFor rows/null hide/hide/pos edit/weight number/weight
  slider sync/invalid weight/non-finite pos). NurbsEditTool 11 보존 (리팩터 후 동일). 2401→2410.
- 패키지 catalog 24/24 · CatalogConsistency CC count **173 불변** · tsc 0 · 엔진/WASM 0 (TS-only).

## 5. 후속 (ADR-235 로드맵)

- **A2-full-1 live surface drag** (ADR-238) — `setFaceSurfaceNurbs` WASM (D1 set_face_surface) →
  드래그/슬라이더 중 곡면 매 프레임 변형. recreateNurbsPatch 는 commit(release) SSOT 유지.
- **A2-full-2 unified polish** (ADR-239) — recreateNurbsPatch 에 단일 Undo wrap (createNurbsSurface
  + deleteFace → 1 transaction) + deleteFace orphan cleanup 검증 + panel↔drag 공존 다듬기.

## 6. Lessons

- **L1** re-create SSOT 추출 시점 — tool(236) + panel(237) 두 consumer 생기는 순간 헬퍼 추출이
  자연 (DRY + full-2 단일 Undo wrap 의 단일 진입점 확보). ADR-091 §E L4 pure helper 답습.
- **L2** `recreating` 가드 — selection-driven auto-show 패널이 자기 re-create 로 재진입(selectFaces
  →onChange→showFor)하는 cycle 은 flag 로 차단. selection-coupled 패널의 canonical 패턴.
- **L3** commit-on-change (release) — 슬라이더 'input'(live sync 표시)과 'change'(release 재생성)
  분리로 churn 최소 + 반응성 유지.

## 7. Cross-link

- ADR-235 (A2 roadmap — MVP-5) / ADR-236 (drag) / ADR-234 (position) / ADR-233 (weight) /
  ADR-232 (overlay + getNurbsSurfaceParams).
- ConstraintPanel (패널 패턴 source) / main.ts:469 (overlay selection.onChange wiring 답습).
- ADR-091 §E L4 (pure helper extraction) / 메타-원칙 #16 (silent skip 차단) / ADR-046 P31 #4
  (additive) / LOCKED #44 (Complete Meaning per Merge).
