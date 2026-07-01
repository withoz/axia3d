# ADR-236 — NURBS CP Drag-on-release (A2-MVP-4)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS Patch Editor A2 (ADR-235 roadmap MVP-4 — 첫 drag 인터랙션)
- **Depends on**: ADR-235 (A2 roadmap) / ADR-234 (CP position edit, re-create) / ADR-233 (weight) /
  ADR-232 (control-net overlay + pickControlNetPoint)

## 1. Context

ADR-235 로드맵의 **MVP-4 (drag-on-release)** — 제어점 마커를 잡고 끌어 이동, 놓으면 패치 재생성.
ADR-233/234 의 클릭→prompt 편집을 보완하는 직관 편집. **엔진 0** (놓을 때 ADR-234 re-create
재사용). 코드베이스 첫 continuous-drag 인터랙션.

**de-risk (canonical 발견)**:
- **카메라 orbit = 휠(중간) 버튼, pan = 우 버튼 (Viewport.ts:643-655)** → **좌-버튼은 도구 전용,
  드래그가 orbit 과 0 충돌** (orbit 억제 코드 불필요). ADR-232 추정과 달리 드래그가 깨끗.
- ToolManager 가 mousedown/**mousemove(버튼 무관 항상)**/mouseup 모두 도구로 전달 (continuous-drag
  완전 지원, ToolManagerRefactored.ts:4159/4177/4305).
- `Viewport.raycaster` + `ray.intersectPlane` 존재 → CP 깊이의 화면-평행 평면에 투영 가능.

**사용자 결재 (2026-06-23)**: A — 화면-평행 평면(CP 깊이) 자유 드래그 + X/Y/Z axis-lock
(ctx.axisLock 재사용, MoveTool 식).

## 2. Decision — drag-on-release (좌-버튼, click vs drag 통합)

- **Viewport 헬퍼** (mock-safe plain-array I/O, THREE math 은 Viewport 에 격리):
  - `cameraForward(): [x,y,z]` — 카메라 world forward (drag-plane normal).
  - `rayToPlane(e, planePoint, planeNormal): [x,y,z]|null` — 마우스 ray ∩ 평면 (setFromCamera +
    `ray.intersectPlane`).
- **NurbsEditTool** (drag 재구조화):
  - **onMouseDown**: `pickControlNetPoint` → CP 잡기. CP start pos + 화면-평행 평면 anchor
    (`rayToPlane(e, cp, cameraForward())`) + grab clientX/Y 저장. **prompt 안 함** (commit 은 up 에서).
  - **onMouseMove**: grab clientX/Y 에서 **DRAG_PX(4px) 초과** 시 dragging. `rayToPlane` → delta →
    axis-lock(ctx.axisLock X/Y/Z 면 다른 성분 0) → liveCP = start + delta. 오버레이(마커 + 네트
    line) live 갱신 (`updateNurbsControlNet({...params, ctrlPts: edited})`). **곡면은 그대로**.
  - **onMouseUp**: dragging 이면 `_recreate(idx, liveCP, weight)` (위치만, weight 불변). 아니면
    (이동 없는 click) `_promptEdit(idx)` (ADR-234 통합 prompt).
- **Labels** (catalog/AxiaCommands/index.html "클릭=입력 / 드래그=이동", CC count 173 불변, adrs += ADR-236).

## 3. Lock-ins

- **L-236-1** MVP-4 = drag-on-release. 곡면은 **놓을 때 re-create** (드래그 중엔 오버레이 마커/네트만
  live). 라이브 곡면 변형은 A2-full-1 (ADR-238, setFaceSurfaceNurbs).
- **L-236-2** 좌-버튼 드래그 (orbit=중간/pan=우, 충돌 0 — Viewport.ts:643). orbit 억제 불필요.
- **L-236-3** click vs drag = DRAG_PX(4px) 임계. click → ADR-234 prompt / drag → re-create. 한 도구 통합.
- **L-236-4** 화면-평행 평면(normal = cameraForward, point = CP start) + axis-lock X/Y/Z (ctx.axisLock).
- **L-236-5** Viewport rayToPlane/cameraForward = plain-array I/O (THREE math Viewport 격리 → 도구
  mock-safe).
- **L-236-6** 엔진/WASM 0 (re-create = ADR-234 그대로). ADR-046 P31 #4 additive (신규 catalog entry 0).
- **L-236-7** 메타-원칙 #16 silent skip 차단 (no-CP grab → no-op / prompt invalid → reject).
- **L-236-8** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+0 net** (NurbsEditTool 9→11: +drag / +axis-lock-z / +tiny-move-stays-click; position-only
  test → drag/click 으로 재구성). 2399 유지.
- 패키지 catalog 24/24 · CatalogConsistency CC count **173 불변** · tsc 0 · 엔진/WASM 0 (TS-only).

## 5. 후속 (ADR-235 로드맵)

- **MVP-5 inline panel** (ADR-237, 엔진 0) — CP 표 + weight 슬라이더 + x/y/z 숫자.
- **A2-full-1 live surface drag** (ADR-238) — `setFaceSurfaceNurbs` WASM (D1 set_face_surface) →
  드래그 중 곡면 매 프레임 변형. 본 ADR 의 drag 위에 onMouseMove 에서 live update 추가.
- **A2-full-2 unified polish** (ADR-239) — re-create 단일 Undo wrap + panel↔drag 공존.

## 6. Lessons

- **L1** de-risk 가 drag 인터랙션 위험을 제거 — orbit=중간/pan=우 라 좌-드래그 자유 (Viewport.ts:643).
  추정(orbit 충돌) → 코드 확인으로 무효화 (메타-원칙 #6).
- **L2** click vs drag 통합 (DRAG_PX 임계) — 한 도구가 정밀(prompt) + 직관(drag) 둘 다. onMouseDown
  =grab, onMouseUp=commit(분기) 패턴.
- **L3** Viewport 헬퍼 plain-array I/O — THREE math 을 render layer 에 격리해 도구 unit-test mock-safe
  (pickControlNetPoint 답습).

## 7. Cross-link

- ADR-235 (A2 roadmap — MVP-4) / ADR-234 (re-create) / ADR-233 (weight) / ADR-232 (overlay).
- Viewport.ts:643 (orbit=중간/pan=우, 좌-드래그 자유) / ITool onMouseDown/Move/Up (ITool.ts:240) /
  MoveTool (axis-lock ctx.axisLock 패턴).
- 메타-원칙 #6 (Preventive — 추정 재검증) / #16 (silent skip 차단) / ADR-046 P31 #4 (additive) /
  LOCKED #44 (Complete Meaning per Merge).
