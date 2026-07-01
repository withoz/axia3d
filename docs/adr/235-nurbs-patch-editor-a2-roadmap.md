# ADR-235 — NURBS Patch Editor: A2 Roadmap (panel + drag coexisting)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS surface 고급화 A2 (ADR-232/233/234 후속 — roadmap only, 구현 0)
- **Depends on**: ADR-232 (control-net overlay + getNurbsSurfaceParams) / ADR-233 (CP weight
  edit) / ADR-234 (CP position edit, 통합 prompt) / ADR-033 (NURBS surfaces engine)

## 1. Context

ADR-233/234 가 제어점을 **클릭 → prompt → re-create** 로 편집한다 (weight + 위치). 사용자가
"슬라이더/드래그/인라인 편집 같은 개선 UX 로 설계 확장이 가능한가?" 를 물었고, 결재는
**전체 A2 에디터 계획(단계적)** — panel(정밀) + drag(직관)이 공존하는 통합 NURBS 패치
에디터를 SketchUp/Rhino 식으로, 각 단계 별도 ADR + 결재로 점진 구현. 본 ADR 은 **roadmap +
de-risk 발견 봉인** (구현 0, ADR-141/190/222 plan 패턴 — docs only).

## 2. De-risk 발견 (canonical — 후속 ADR 가 재발견하지 않도록)

| # | 발견 | 위치 | 의미 |
|---|---|---|---|
| D1 | `Mesh::set_face_surface(fid, Some(surface))` 가 **NURBSSurface 포함 모든 AnalyticSurface** 수용 | mesh.rs:1144 | 엔진에 in-place surface 교체 hook **이미 존재** (WASM 은 primitive 5종만 노출) |
| D2 | `tessellate_face_surface` 는 **"면의 파라미터 범위에서 tessellate"** (DCEL 경계 아님) | mesh.rs:1661 | surface 만 교체하면 다음 render 에서 곡면 전체 live 재생성 |
| D3 | ITool `onMouseDown/onMouseMove/onMouseUp` plumbing 존재 | ITool.ts:240-246 | 커스텀 드래그 가능, **TransformControls 불필요** |
| D4 | `getNurbsSurfaceParams` 가 Face.surface 에서 read-back | ADR-232 | set_face_surface 교체 시 panel/overlay 가 새 control net 자동 반영 |
| D5 | re-create 메커니즘 (createNurbsSurface + deleteFace) 검증됨 | ADR-233/234 | DCEL 경계/picking 정확성의 commit 경로 |

**핵심 함의 (ADR-232 추정 수정)**: ADR-232 de-risk 는 "A2-full(라이브 편집) = L-XL" 로 봤고
근거가 "setFaceSurface* 가 NURBS 업데이트 경로 없음" 이었다. 실제 막혔던 건 *엔진 능력*이
아니라 *WASM 래퍼 한 개* (D1) — `set_face_surface` 가 NURBS 를 받고 render 가 surface
파라미터에서 tessellate (D2) 하므로, 얇은 WASM 래퍼 하나로 **라이브 곡면 변형이 tractable**.

## 3. Vision — 통합 NURBS 패치 에디터

같은 선택 패치 / 같은 overlay(ADR-232) / 같은 re-create commit 위에서:
- **정밀** — 인라인 패널 (CP 목록 표: x/y/z 숫자 + weight 슬라이더).
- **직관** — 드래그 (CP 마커 잡고 3D 끌기, 라이브 곡면 변형).
- 둘이 **공존** — 패널 편집 ↔ 드래그가 같은 overlay/패치에 반영. SketchUp/Rhino 식.

## 4. Roadmap (각 단계 = 별도 ADR + 결재 + atomic PR)

| 단계 | 제목 | 드래그 중 곡면 | 엔진/WASM | 비용 | ADR(가칭) |
|---|---|---|---|---|---|
| ✅ A2-MVP-1 | Control-net overlay (visualize) | — | 0 (read-back) | done | ADR-232 |
| ✅ A2-MVP-2 | CP weight edit (pick+prompt) | — | 0 | done | ADR-233 |
| ✅ A2-MVP-3 | CP position edit (통합 prompt) | — | 0 | done | ADR-234 |
| A2-MVP-4 | **Drag-on-release** (마커 잡고 끌기 → 놓으면 re-create) | 마커/네트만 live | **0** | M | ADR-236 |
| A2-MVP-5 | **Inline panel** (CP 표 + weight 슬라이더 + x/y/z 숫자) | — (값 편집→재생성) | **0** | M | ADR-237 |
| A2-full-1 | **Live surface drag** (끌면 곡면 매 프레임 변형) | **곡면 전체 live** | `setFaceSurfaceNurbs` 1개 (D1) | S-M 엔진 + M TS | ADR-238 |
| A2-full-2 | **Unified editor polish** (단일 Undo wrap + panel↔drag 공존 + axis-lock) | — | 0~S | M | ADR-239 |

**순서 근거** (위험 격리 우선, LOCKED #44 + 메타-원칙 #6):
- A2-MVP-4(drag-on-release) 먼저 — **엔진 0** 으로 드래그 인터랙션(화면→평면 투영, plane 선택,
  axis-lock) de-risk. 가장 직관적 win.
- A2-MVP-5(panel) — **엔진 0**, 정밀 편집 보완. drag 와 독립.
- A2-full-1(live) — D1/D2 로 tractable 해진 엔진 WASM 추가. drag-on-release 위에 라이브 전환.
- A2-full-2 — 단일 Undo + 통합 polish.
- 순서는 결재로 조정 가능 (live 우선 등).

### 단계별 핵심 설계 (요지)

- **A2-MVP-4 Drag-on-release**: onMouseDown(`pickControlNetPoint` grab) → onMouseMove(화면 delta
  → plane 투영, overlay 마커 + 네트 라인 live 이동) → onMouseUp(`_recreate`(ADR-234) with 새 위치).
  Plane = 화면 평행 default + MoveTool 식 axis-lock(X/Y/Z) modifier. 곡면은 놓을 때 갱신.
- **A2-MVP-5 Inline panel**: XiaInspector/DraggablePanel 패턴. getNurbsSurfaceParams 로 CP 표
  채움 → x/y/z 숫자 + weight 슬라이더(0..~5) → commit(또는 throttle) 시 re-create. overlay 동기.
- **A2-full-1 Live surface drag**: 신규 WASM `setFaceSurfaceNurbs(fid, ctrlPts, nU, nV, weights,
  knotsU, knotsV, degU, degV)` → `mesh.set_face_surface(fid, Some(NURBSSurface{...}))`(D1).
  onMouseMove 마다 setFaceSurfaceNurbs → syncMesh → 곡면 live(D2). onMouseUp re-create 로 DCEL
  경계/picking re-sync + 단일 Undo. **Perf**: 작은 패치 OK, 큰 scene 은 ADR-111/112/135 +
  throttle. **Caveat**: 드래그 중 DCEL 경계(edge wireframe) stale → 놓을 때 re-sync (preview 허용).
- **A2-full-2 Unified polish**: re-create 단일 Undo wrap(createNurbsSurface + deleteFace → 1
  transaction) + deleteFace orphan cleanup 검증 + panel↔drag 공존 + (future) degree elevation /
  knot insertion(CP 행/열 추가).

## 5. Lock-ins

- **L-235-1** 모든 단계 = ADR-232 overlay + ADR-233/234 pick+re-create 토대 위 **additive** 확장.
- **L-235-2** panel + drag **공존** — 같은 선택 패치 / 같은 overlay / 같은 re-create commit.
- **L-235-3** **re-create = commit 메커니즘**(DCEL 정확성). live update(set_face_surface)는 drag
  **preview 전용** → onMouseUp 에 re-create 로 re-sync.
- **L-235-4** 각 단계 = 별도 ADR + 결재 + atomic PR (LOCKED #44 Complete Meaning per Merge).
- **L-235-5** **엔진 변경은 A2-full-1 에 국한** (`setFaceSurfaceNurbs` WASM 래퍼 — 엔진
  `set_face_surface` 는 D1 로 이미 존재). A2-MVP-4/5 는 TS-only.
- **L-235-6** 드래그 = ITool onMouseDown/Move/Up(D3), TransformControls 불필요.
- **L-235-7** ADR-046 P31 #4 additive (메뉴/단축키 제거 0) / 메타-원칙 #16 (silent skip 차단).
- **L-235-8** 절대 #[ignore] 금지.
- **L-235-9** 본 ADR 은 roadmap only — 구현 0, 회귀 0. 각 단계 ADR 이 구현/회귀/시연.

## 6. Lessons

- **L1** de-risk 가 ADR-232 의 L-XL 추정을 수정 — 막힌 건 엔진 능력(set_face_surface 는 NURBS
  수용 + render 가 surface 에서 tessellate)이 아니라 WASM 노출 한 개. **추정은 코드 확인으로
  재검증** (메타-원칙 #6).
- **L2** re-create(commit) + set_face_surface(live preview) 2-layer — 정확성(DCEL)과 반응성(live)
  분리. 향후 라이브 편집 도구의 canonical 패턴.
- **L3** 단계적 위험 격리 — 엔진 0 단계(drag-on-release/panel) 먼저 인터랙션 de-risk, 엔진 추가
  (live)는 그 위에. ADR-094 §E L1 additive-first 답습.

## 7. Cross-link

- ADR-232 (overlay + getNurbsSurfaceParams) / ADR-233 (weight) / ADR-234 (position) — A2 토대.
- ADR-033 (NURBS surfaces engine, createNurbsSurface, set_face_surface) / ADR-031 Phase D
  (tessellate_face_surface).
- ADR-111/112/135 (syncMesh perf — live drag throttle 근거) / ADR-046 P31 #4 (additive) /
  메타-원칙 #6 (Preventive — 추정 재검증) / #16 (silent skip 차단) / LOCKED #44 (Complete Meaning
  per Merge).
- 후속 ADR-236(drag-on-release) / ADR-237(inline panel) / ADR-238(live surface drag) /
  ADR-239(unified polish) — 가칭, 각 별도 결재.
