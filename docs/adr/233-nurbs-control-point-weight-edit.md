# ADR-233 — NURBS Control-Point Weight Edit (A2-MVP-2, pick + prompt + re-create)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS surface 고급화 A2 (ADR-232 후속, A2-MVP-2)
- **Depends on**: ADR-232 (control-net overlay + getNurbsSurfaceParams read-back) / ADR-231
  (NURBS vault) / ADR-033 (NURBS surfaces engine) / ADR-037 P22 (selection)

## 1. Context

ADR-232(A2-MVP-1) control-net 오버레이 + read-back 위에, **per-control-point weight 편집**
(drag 없이 값 편집, A2-MVP-2). 사용자 결재 **2a (CP pick + weight prompt + re-create)** —
ADR-232 오버레이 마커를 클릭 가능하게 만들어 "보이는 CP 클릭 → weight 입력 → 패치 재생성".

**런타임 시뮬레이션 (re-create 메커니즘 확정, real WASM)**: createNurbsSurface(vault, faceId A)
→ getNurbsSurfaceParams(A) → corner weight 1/√2→0.5 → createNurbsSurface(edited, faceId B) →
deleteFace(A) → **B에 weight 0.5 적용 + A 제거 + 두 op 성공**. → re-create 메커니즘 feasible
(신규 WASM/엔진 0 — createNurbsSurface/deleteFace/getNurbsSurfaceParams 전부 존재).

## 2. Decision — pick CP marker → weight prompt → re-create

- **Viewport.pickControlNetPoint(e)** — ADR-232 control-net 오버레이의 CP 마커 중 마우스에
  가장 가까운 것의 row-major index 반환 (px-tolerance NDC 0.045 이내, else null). **screen-
  projection nearest** (raycaster Points.threshold 아님 — 마커가 sizeAttenuation:false
  constant screen-size 라 world-space threshold 부적합).
- **NurbsEditTool** (`web/src/tools/NurbsEditTool.ts`, ITool) —
  - onActivate: 선택된 단일 NURBS-class face(faceSurfaceKind 6/7/8) 읽기 + getNurbsSurfaceParams
    + 오버레이 표시. 비-NURBS / 0 또는 2+ 선택 → Toast reject.
  - onMouseDown: `viewport.pickControlNetPoint(e)` → CP index → `prompt(weight, 현재값)` →
    양수 검증 → re-create.
  - **_recreate(idx, newWeight)**: weights[idx]=newWeight → `createNurbsSurface(ctrlPts, nU,
    nV, editedWeights, knotsU, knotsV, degU, degV)` → newFid → `deleteFace(oldFid)` (create-
    then-delete 순서로 newFid 유효) → syncMesh → getNurbsSurfaceParams(newFid) → selectFaces
    ([newFid]) → updateNurbsControlNet(newParams).
- **ToolManager**: `tools.set('nurbs-edit')` + keepSelection 에 'nurbs-edit' 추가 (도구 활성
  시 패치 선택 보존).
- **Catalog + menu**: tool-nurbs-edit (CommandCatalog + ActionCatalog status:'ui-only' +
  MenuBar case + index.html, tool-nurbs 인접). CC count 172→173.

## 3. Lock-ins

- **L-233-1** A2-MVP-2 = weight 편집 (pick CP marker + prompt + re-create). drag 없음 (값 편집).
  CP 위치 편집 + 전체 패널(2b) + draggable handle(A2-full)은 후속.
- **L-233-2** re-create 메커니즘 — getNurbsSurfaceParams(read) → createNurbsSurface(edited) →
  deleteFace(old). 신규 WASM/엔진 0 (ADR-232 read-back + 기존 create/delete 재사용). create-
  then-delete 순서 (newFid 유효).
- **L-233-3** pick = screen-projection nearest (NDC 0.045 tolerance), NOT raycaster Points
  threshold (constant screen-size 마커 대응). Viewport.pickControlNetPoint.
- **L-233-4** weight 양수 검증 (≤0 / NaN reject). prompt cancel → no-op.
- **L-233-5** re-create 후 newFid 재선택(selectFaces) + 오버레이 refresh (ADR-232 selection
  onChange + 직접 updateNurbsControlNet 양쪽).
- **L-233-6** keepSelection 'nurbs-edit' — 도구 활성 시 패치 선택 보존 (pushpull/move 답습).
- **L-233-7** ADR-046 P31 #4 additive — 신규 도구/메뉴만, 기존 무변경. 엔진/WASM 0 (TS-only).
- **L-233-8** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+8** (NurbsEditTool: name/activate(load·non-NURBS reject·multi reject)/pick+weight→
  re-create·no-CP·cancel·invalid weight) 2390→2398.
- 패키지 catalog 24/24 (tool-nurbs-edit AC⊇CC) · CatalogConsistency CC count 173 · tsc 0 ·
  엔진/WASM 변경 0 (TS-only).

## 5. 후속 (별도 트랙)

- **CP 위치 편집** — pick CP → prompt new x/y/z (또는 A2-full draggable handle, 신규 인터랙션
  패러다임 + 라이브 setFaceSurfaceNurbs WASM).
- **전체 편집 패널 (2b)** — CP 목록 + weight/xyz editable 필드.
- **단일 Undo** — re-create(createNurbsSurface + deleteFace) 2 transaction → 단일 Undo wrap.
- **deleteFace orphan cleanup** 검증 — re-create 시 old 패치 verts/edges 잔존 확인.
- **A2-full draggable** + Track B 곡면 Boolean (deep SSI).

## 6. Lessons

- **L1** re-create 메커니즘 = ADR-232 read-back + 기존 create/delete 재사용 (Pattern-12). 신규
  WASM 없이 편집 unlock — 시뮬레이션이 commit 전 확정.
- **L2** screen-projection pick (NDC nearest) > raycaster Points threshold — constant screen-
  size 마커(sizeAttenuation:false)는 world threshold 부적합. 향후 overlay 마커 pick canonical.
- **L3** create-then-delete 순서 — newFid 먼저 할당 후 oldFid 제거 → newFid 유효 보장.
- **L4** keepSelection 확장 — 선택-의존 편집 도구(NurbsEdit)는 활성 시 선택 보존 필수.

## 7. Cross-link

- ADR-232 (control-net overlay + getNurbsSurfaceParams — 본 ADR 의 read-back source) / ADR-231
  (NURBS vault) / ADR-033 (NURBS surfaces engine, createNurbsSurface) / ADR-037 P22 (selection).
- 메타-원칙 #2 (render-only overlay pick) / ADR-046 P31 #4 (additive) / Boolean β-4 defer 패턴
  (시뮬레이션 → 단계화) / LOCKED #44 (Complete Meaning per Merge).
