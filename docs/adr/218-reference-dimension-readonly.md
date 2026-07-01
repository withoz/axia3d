# ADR-218 — Reference Dimension (read-only / non-driving)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Dimension 확장 마무리 (ADR-215/216/217 후속) / Foundation
- **Depends on**: ADR-215 (Dimension 인프라) / ADR-216 (Angle) / ADR-217 (Radius) /
  `ConstraintGraph` / `DimensionManager` / `DimensionLabel`

## 1. Context

Dimension 확장 (driving 개별: Angular → Radial → **Reference**) 의 마지막. 215~217 은
모두 **driving**(라벨 편집 → 솔버가 기하 이동). Reference 는 **읽기전용 측정값** —
기하는 고정, 라벨은 현재 측정값만 표시·영구저장.

사전검토 핵심 통찰: DimensionManager 의 표시값은 **이미 기하에서 계산**(거리=정점간,
각도=엣지간, 반지름=곡선). driving 이든 reference 든 표시 동일. 차이는 **편집 가능
여부뿐**. → reference = 기존 `ConstraintKind`(Distance/Angle/Radius) 에 **`value = None`**.
`resolve*` 가 모두 `value=Some` 을 요구하므로 **None 이면 자동으로 not solved**(측정만).
`Parallel/Perpendicular/Collinear` 이 이미 value=None 선례. **신규 struct/section/bincode
위험 0**.

사용자 결재 (2026-06-22): **Tool UX = Option A — 단일 ReferenceDimensionTool(클릭 유형
dispatch)**.

## 2. Decision

**Reference Dimension = `value = None` 제약(Distance/Angle/Radius) + 괄호 read-only 라벨**
(Pattern-12).

- **Engine** (`constraint.rs`): Angle residual 만 `value=None` 가드 추가
  (`match c.value { Some(t) => |current−t|, None => 0.0 }`). Distance/Radius residual 은
  이미 `target.unwrap_or(0) <= 0 → 0`. resolve 3종 모두 `value=Some` 요구 → None = no-op
  (변경 0). **즉, 가드 1줄이 전부**.
- **WASM**: `addReferenceDistance(vA,vB)` / `addReferenceAngle(eAvA,eAvB,eBvA,eBvB)` /
  `addReferenceRadius(refVert)` — 같은 kind 를 `value=None` 으로 생성(공유 private
  `add_reference_constraint`, 단일 undo transaction, 기하/topology 변경 0). `addReferenceRadius`
  는 곡선 edge 검증(find_curve_edge_at). `listConstraints` 는 None → `value` 필드 생략.
- **Tool** (`ReferenceDimensionTool`, 단일 dispatch): 첫 클릭이 **정점(tight pick)→linear /
  원·호 edge→radial(즉시) / 직선 edge→angular**. linear/angular 는 둘째 클릭 대기.
- **Render** (DimensionManager): `editable = (c.value != null)`. reference(value 없음) →
  CAD 관습 괄호 `(10mm)` / `(90.0°)` / `(R5mm)` + non-editable. DimensionLabel 은
  `editable=false` 시 클릭/편집 핸들러를 안 붙임 — **DimensionLabel 변경 0**.

## 3. Lock-ins

- **L-218-1** Reference = 기존 ConstraintKind(Distance/Angle/Radius) + `value=None`.
  신규 struct/snapshot section/bincode 변경 0.
- **L-218-2** `value=None` → resolve 3종 모두 not solved(no-op) + residual 0
  (Angle 가드 추가). reference 가 솔버 수렴 방해 안 함.
- **L-218-3** WASM 3 add* = 공유 private `add_reference_constraint`(value=None, 단일 undo,
  기하/topology 변경 0). addReferenceRadius 는 곡선 edge 검증.
- **L-218-4** `listConstraints` 는 None → `value` 필드 생략 → JS `c.value===undefined`.
- **L-218-5** DimensionManager `editable = value 존재`; reference → 괄호 + read-only.
  **DimensionLabel 변경 0**(editable=false 자연 처리).
- **L-218-6** Tool = 단일 ReferenceDimensionTool, 클릭 유형 dispatch(정점→linear /
  원·호→radial / 직선→angular). Option A.
- **L-218-7** 영구(snapshot)+undo/redo = 기존 constraint 직렬화.
- **L-218-8** additive — 신규 tool-reference-dimension + Dimension 메뉴 (ADR-046 P31 #4).
- **L-218-9** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `constraint.rs` Angle residual None 가드.
- **WASM**: addReferenceDistance/Angle/Radius + 공유 private helper (+ baseline 3).
- **Bridge**: 3 wrappers (graceful).
- **Tool**: ReferenceDimensionTool (dispatch) + ToolManager 등록.
- **Render**: DimensionManager 3 line builder 에 `ref` 분기(괄호 + editable false).
- **UI**: AxiaCommands tool-reference-dimension + Dimension 메뉴 + MenuBar case.

## 5. 회귀 + 검증

- **회귀**: axia-core +1 (`adr218_reference_dimensions_never_drive_geometry` — 3 kind
  reference 모두 not solved + geometry 불변 + max_residual 0) · axia-wasm +3 (baseline) ·
  vitest +11 (ReferenceDimensionTool 6 + DimensionManager +1 + WasmBridge +4). 절대
  #[ignore] 금지. tsc clean. 전체 axia-core 397 PASS.
- **브라우저** (real rebuilt WASM): Path B 원 r5 + 직선 → addReferenceRadius/Distance →
  **listConstraints 에 value 필드 없음(=None)**, **resolveAllConstraints moved 0**(reference
  안 구동), 원 r5·거리 20mm 불변, 비-곡선 정점 addReferenceRadius → 0(validation).

## 6. 결과 — Dimension 확장 완결

| ADR | 치수 | 종류 |
|---|---|---|
| 215 | Linear | driving |
| 216 | Angular | driving |
| 217 | Radial (Circle+Arc) | driving |
| **218** | **Reference (Linear/Angular/Radial)** | **read-only** |

후속(별도 ADR): Point 도구(standalone vertex) → 24-도구 Phase 5+(Sweep → Loft → Hole →
3P-Plane → NURBS surface → Wall → Window).

## 7. Cross-link

- ADR-215/216/217 (Dimension 인프라 — Tool/Manager/Label/constraint 재사용)
- `ConstraintGraph` (value=None = reference, Parallel/Perp/Collinear 선례)
- `DimensionLabel` (editable=false 자연 처리, 변경 0)
- ADR-210 (Dimension 메뉴) / ADR-046 P31 #4 (additive)
- ADR-087 K-ζ (사용자 시연 게이트) / 메타-원칙 #4 #5 #6 #13
- LOCKED #44 (Complete Meaning per Merge)
