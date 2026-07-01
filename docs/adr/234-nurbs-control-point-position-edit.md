# ADR-234 — NURBS Control-Point Position Edit (A2-MVP-3, unified x/y/z/weight prompt)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS surface 고급화 A2 (ADR-233 후속, A2-MVP-3)
- **Depends on**: ADR-233 (CP weight edit — pick + prompt + re-create) / ADR-232 (control-net
  overlay + getNurbsSurfaceParams) / ADR-033 (NURBS surfaces engine, createNurbsSurface)

## 1. Context

ADR-233(A2-MVP-2) 가 제어점 weight 편집을 추가했다 — CP 마커 클릭 → weight prompt → 패치
재생성. A2-MVP-3 는 **제어점 위치(x/y/z) 편집** 을 추가한다. 메커니즘은 ADR-233 과 동일
(createNurbsSurface 에 넘기는 배열 요소 편집 → re-create) — weights[idx] 대신 ctrlPts[idx]
편집뿐.

**런타임 de-risk (real WASM, 위험 0)**: createNurbsSurface(vault A) → getNurbsSurfaceParams(A)
→ CP idx2 (500,0,500) z+300 → createNurbsSurface(edited ctrlPts) → deleteFace(A) →
**CP (500,0,800) z+300 정확 / weight 0.7071 보존 / 새 패치 kind 8 / invariants valid 0
violations / old face -1 삭제 / faces=1**. weight 편집과 동일 경로 — 엔진이 control net 에서
face 재구성, manifold valid.

**사용자 결재 (2026-06-23)**: UX = **A 통합 입력창** — CP 클릭 → 단일 prompt "x, y, z, weight"
현재값 pre-fill → 원하는 값만 고쳐서 Enter. 위치든 weight든 한 흐름. ADR-233 의 weight-only
prompt 를 통합 prompt 로 supersede (weight 만 바꾸려면 마지막 값만, 위치만 바꾸려면 x/y/z 만).

## 2. Decision — unified "x, y, z, weight" prompt

- **NurbsEditTool.onMouseDown** (ADR-233 weight-only → 통합):
  - `pickControlNetPoint(e)` → CP idx (ADR-233, 변경 0)
  - 현재 CP 값으로 `"cx, cy, cz, cw"` pre-fill → `prompt("제어점 {idx} — x, y, z, weight:", def)`
  - 4개 쉼표-구분 float 파싱 — 개수≠4 / NaN → reject Toast (silent skip 차단). weight≤0 → reject.
  - `_recreate(idx, [nx, ny, nz], nw)`.
- **_recreate(idx, pos, newWeight)** (ADR-233 weight-only → position+weight):
  - `ctrlPts[idx*3..+3] = pos` + `weights[idx] = newWeight`
  - `createNurbsSurface(editedCtrlPts, nU, nV, editedWeights, knotsU, knotsV, degU, degV)` → newFid
  - `deleteFace(oldFid)` (create-then-delete) → syncMesh → getNurbsSurfaceParams(new) →
    selectFaces(new) → updateNurbsControlNet(new)
- **Labels** (위치·weight 반영, CC count 불변 173): catalog tool-nurbs-edit
  ("NURBS 제어점 편집 (위치·weight)", adrs += ADR-234) + AxiaCommands + index.html.

## 3. Lock-ins

- **L-234-1** A2-MVP-3 = CP 위치 편집. ADR-233 weight 편집과 **통합 prompt** 로 합침 (UX 결재 A).
  drag 없음 (값 편집). 전체 패널(2b) + draggable handle(A2-full)은 후속.
- **L-234-2** Unified prompt "x, y, z, weight" — 4 float 쉼표-구분, 현재값 pre-fill. 개수≠4 /
  NaN / weight≤0 → reject (silent skip 차단). ADR-233 weight-only prompt supersede.
- **L-234-3** _recreate 가 ctrlPts[idx] + weights[idx] 동시 편집 → createNurbsSurface(edited) →
  deleteFace(old, create-then-delete). 신규 WASM/엔진 0 (ADR-233 메커니즘 그대로, ctrlPts 도 편집).
- **L-234-4** 위치 이동 후 invariants valid (moved-CP 패치 manifold) — de-risk 봉인.
- **L-234-5** re-create 후 newFid 재선택 + 오버레이 refresh (ADR-233 그대로).
- **L-234-6** keepSelection 'nurbs-edit' (ADR-233 그대로).
- **L-234-7** ADR-046 P31 #4 additive — 도구/메뉴 label 만, 신규 entry 0 (CC count 173 불변).
  엔진/WASM 0 (TS-only).
- **L-234-8** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+1** (NurbsEditTool 8→9: position-only edit 추가; weight 테스트 → 통합 prompt 포맷
  갱신; invalid 테스트 → 통합 포맷 4-case) 2398→2399.
- 패키지 catalog 24/24 · CatalogConsistency CC count **173 불변** · tsc 0 · 엔진/WASM 0 (TS-only).

## 5. 후속 (별도 트랙)

- **전체 편집 패널 (2b)** — CP 목록 + weight/xyz editable 표 (마커 클릭 없이 표에서 직접 편집).
- **re-create 단일 Undo** — createNurbsSurface + deleteFace 2 transaction → 1 (engine wrap).
- **deleteFace orphan cleanup** 검증.
- **A2-full draggable** — TransformControls/drag-handle (신규 인터랙션 패러다임 + 라이브
  setFaceSurfaceNurbs WASM, L-XL).

## 6. Lessons

- **L1** A2-MVP-2(weight) → A2-MVP-3(position) = 동일 re-create 메커니즘의 배열만 교체 — 위험
  0, de-risk 가 commit 전 확정 (CP z+300 정확, invariants valid).
- **L2** 통합 prompt(현재값 pre-fill + 4 float) > modifier/mode — 클릭 1 + 입력 1, 외울 규칙
  없음. weight-only 를 자연 supersede (끝 값만 고치면 weight 편집, x/y/z 만 고치면 위치 편집).
- **L3** silent skip 차단 — 개수≠4 / NaN / weight≤0 모두 명시 Toast reject (메타-원칙 #16).

## 7. Cross-link

- ADR-233 (CP weight edit — 본 ADR 이 통합 prompt 로 확장) / ADR-232 (control-net overlay +
  getNurbsSurfaceParams) / ADR-033 (NURBS surfaces engine, createNurbsSurface).
- 메타-원칙 #2 (render-only overlay pick) / #16 (silent skip 차단) / ADR-046 P31 #4 (additive) /
  LOCKED #44 (Complete Meaning per Merge).
