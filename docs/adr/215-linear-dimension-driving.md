# ADR-215 — Linear Dimension (driving / parametric)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: 2D CAD 편집 명령 (ADR-210 메뉴 재구성 후속 C5) / Foundation
- **Depends on**: Constraint Solver Level 2 (`ConstraintKind::Distance`, persisted +
  solved) / `DimensionLabel` (editable onEdit) / `ConstraintVisual` (snapshot-once
  cache 패턴) / ADR-210 (Dimension 메뉴 home)

## 1. Context

ADR-210 메뉴 재구성 후 2D CAD 편집 C5. 상세 시뮬레이션(5-angle 직접 audit) 결과:

- **Measure** (`MeasureTool` + `measure-selection`) = **transient** 측정 (Toast +
  guide line, 저장 안 됨). 일회성 측정은 이미 커버됨 → Dimension 의 가치 = **영구**.
- **DimensionLabel** = 완전한 치수 렌더러 (dim line + extension + tick + **editable
  onEdit** inline 편집). 영구 dimension 의 렌더·편집 레이어.
- **`ConstraintKind::Distance`** = 두 정점 고정 거리 (`value: Option<f64>`,
  Serialize→snapshot 영구저장 + `resolve_distance` 자동 solve + undo/redo). =
  **parametric driving dimension 의 저장·솔브 백엔드**.

사용자 결재 (2026-06-22): **driving(parametric)** + **Linear/Aligned MVP** +
**Point defer** (3/3 ⭐ 추천).

## 2. Decision

**Linear Dimension = driving Distance constraint + editable DimensionLabel** (Pattern-12).

- **Engine** (신규는 1개만): `ConstraintGraph::set_value(id, value)` (constraint.rs)
  — dimension 편집 시 target value 갱신. 나머지(add/solve/persist)는 기존.
- **WASM/bridge**: `setConstraintValue(id, value)` (add/re-solve, `_emitConstraintsChanged`).
- **DimensionTool**: 2 정점 픽(snap + `findVertexIdAt`→VertId, `getVertexPos`로 거리) →
  `addDistanceConstraint(vA, vB, dist)`. Dimension 메뉴(ADR-210) home.
- **DimensionManager**: `ConstraintVisual` 의 "snapshot once, render until invalidated"
  패턴 미러 — Distance 제약 LIST 는 `onConstraintsChanged` 이벤트로만 캐시 갱신,
  정점 위치는 매 프레임 `getVertexPos` (read-only, ConstraintVisual 답습). 전용
  `DimensionLabel` 인스턴스에 editable DimLine 렌더. onEdit → `setConstraintValue` →
  solver 가 기하 이동(driving) → syncMesh. `viewport.onFrame` 등록.

## 3. Lock-ins

- **L-215-1** Dimension = driving Distance constraint + DimensionLabel (Pattern-12,
  신규 geometric kernel 0).
- **L-215-2** `set_value` (ConstraintGraph) + `setConstraintValue` (WASM, resolve) —
  유일한 신규 엔진 코드.
- **L-215-3** DimensionManager = ConstraintVisual snapshot-once 패턴 (LIST 캐시는
  이벤트 갱신, position 은 per-frame). per-frame `listConstraints` 금지 (reentrancy).
- **L-215-4** 전용 DimensionLabel 인스턴스 (transient 도구 `ctx.dimLabel` 과 분리).
- **L-215-5** 영구(snapshot) + solved + undo/redo = 기존 constraint 직렬화 재사용.
- **L-215-6** 모든 active Distance 제약 = dimension 라벨 (별도 추적 불필요).
- **L-215-7** Linear/Aligned MVP. Angular/Radial/Point = defer (사용자 결재).
- **L-215-8** additive — 신규 `tool-dimension` + Dimension 메뉴. ConstraintVisual /
  기존 도구 무손상 (ADR-046 P31 #4).
- **L-215-9** 절대 #[ignore] 금지.

## 4. 구현

- **Engine** (`axia-core/src/constraint.rs`): `ConstraintGraph::set_value`.
- **WASM** (`lib.rs`): `setConstraintValue` (+ baseline).
- **Bridge** (`WasmBridge.ts`): `setConstraintValue` wrapper.
- **Tool** (`DimensionTool.ts`): 2-정점 픽 → addDistanceConstraint. ToolManager 등록.
- **Manager** (`DimensionManager.ts`): 캐시 + per-frame DimensionLabel 렌더 + onEdit.
  main.ts 인스턴스화 + onFrame tick.
- **UI**: AxiaCommands `tool-dimension` + Dimension 메뉴 항목 + MenuBar case.

## 5. 회귀 + 검증

- **회귀**: axia-core +1 (`adr215_set_value_drives_distance` — set_value+resolve →
  정점 이동) · axia-wasm +1 (setConstraintValue, baseline) · vitest +14 (DimensionTool
  6 + DimensionManager 5 + bridge 3). 절대 #[ignore] 금지. tsc clean. 전체 워크스페이스
  axia-geo 1986 / axia-core 393 PASS.
- **브라우저** (real WASM): line→2 verts(dist 10) → addDistanceConstraint → id 1,
  listConstraints value 10 → **setConstraintValue(1, 25) → vb (25,0,0) 이동, dist
  10→25** (parametric driving 확인). DimensionManager 인스턴스화 확인.

## 6. 후속 (별도 ADR, ADR-210 §6 로드맵)

- **Angular dimension** — 두 엣지 각도. ConstraintKind 에 Angle 없음 → 신규 백엔드
  또는 reference annotation.
- **Radial dimension** — Circle/Arc 반지름 (AnalyticCurve.radius 읽기). reference 또는 신규.
- **Reference (annotation) dimension** — 읽기전용 측정값 영구 표시 (별도 annotation store).
- **Point 도구** — standalone vertex (orphan cleanup 역풍, 별도 ADR).
- (선택) Dimension offset line / ConstraintVisual ↔ DimensionManager 중복 표시 정리.

## 7. Cross-link

- ADR-211/212/213/214 (Trim/Extend/Corner/Join/Edge transform — 2D 편집 트랙)
- ADR-210 (메뉴 재구성 — Dimension 메뉴 신설 home)
- Constraint Solver Level 2 (Distance constraint — driving 백엔드)
- `DimensionLabel` (editable onEdit) / `ConstraintVisual` (snapshot-once 패턴 source)
- ADR-087 K-ζ (사용자 시연 게이트) / 메타-원칙 #4 #5 #6 #13
- LOCKED #44 (Complete Meaning per Merge — 단일 atomic PR)
