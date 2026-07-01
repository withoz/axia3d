# ADR-217 — Radial Dimension (driving / parametric, Circle + Arc)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Dimension 확장 (ADR-216 후속) / Foundation
- **Depends on**: ADR-215 (Dimension 인프라, setConstraintValue) / ADR-216 (constraint
  pattern) / ADR-089 (Path B Circle self-loop) / ADR-028 (Arc curve) / `DimensionLabel`

## 1. Context

ADR-216 Angular 후속. 사용자 결재 driving 개별 — 본 ADR = **Radial(구동)**.

사전검토: Radial 은 Linear/Angular(정점/엣지)와 달리 **원/호의 `AnalyticCurve`
radius**. Path B 원 = anchor 정점 at `center + basis_u·radius` + self-loop edge
with `Circle{center, radius, normal, basis_u}`. 호 = 2 endpoint + `Arc{..,radius,
start/end_angle}`. `edge.set_curve()` 로 갱신 가능.

사용자 결재 (2026-06-22): **Q1=B Circle + Arc** + **Q2=A center 고정** (anchor 이동).

## 2. Decision

**Radial Dimension = driving Radius constraint + straight DimensionLabel ("R" 라벨)** (Pattern-12).

- **Engine** (`mesh.rs` + `constraint.rs`):
  - `Mesh::set_curve_radius(edge, R)` — Circle: anchor → `center + basis_u·R`.
    Arc: 양 endpoint → angle 점 at R (현재 위치로 start/end 매칭). **center 고정**.
  - `Mesh::find_curve_edge_at(vert)` (pub) — 정점의 Circle/Arc edge 찾기.
  - `Mesh::edge_curve_radius(edge)` — 반지름 조회.
  - `ConstraintKind::Radius` (ref = 정점, value = radius) + `resolve_radius`
    (find_curve_edge_at → set_curve_radius) + residual.
- **WASM**: `addRadiusConstraint(refVert, radius)` + `edgeCurveRadius(edge)` +
  `radiusDimAt(refVert)→[cx,cy,cz,radius]` + list "radius". setConstraintValue 재사용.
- **Tool** (`RadialDimensionTool`): 원/호 edge 클릭 → refVert(`getEdgeEndpoints[0]`)
  + radius(`edgeCurveRadius`) → addRadiusConstraint.
- **Render**: **직선 DimLine 재사용** (center→원점, text `"R5"`). 신규 render mode
  불요 (placeholder `|center−point|` = radius 자동). DimensionManager radiusLine.

## 3. Lock-ins

- **L-217-1** `ConstraintKind::Radius` + `set_curve_radius` (Circle+Arc, center 고정,
  Q1=B/Q2=A). 신규 엔진 = curve 갱신 솔버.
- **L-217-2** Circle: anchor → center+basis_u·R. Arc: 양 endpoint → start/end angle
  점 at R (현재 위치 거리로 매칭).
- **L-217-3** Constraint ref = 곡선 edge 위 정점; resolve 가 `find_curve_edge_at` 로
  edge 찾음. find_curve_edge_at 는 axia-core 호출 위해 `pub`.
- **L-217-4** Render = 직선 DimLine 재사용 (center→point "R"). 신규 DimensionLabel 모드 0.
- **L-217-5** addRadiusConstraint + edgeCurveRadius + radiusDimAt + setConstraintValue 재사용.
- **L-217-6** 영구(snapshot)+solve+undo/redo = 기존 constraint 직렬화.
- **L-217-7** RadialDimensionTool = 원/호 edge 단일 클릭.
- **L-217-8** additive — 신규 tool-radial-dimension + Dimension 메뉴 (ADR-046 P31 #4).
- **L-217-9** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `mesh.rs` (set_curve_radius + find_curve_edge_at + edge_curve_radius) +
  `constraint.rs` (ConstraintKind::Radius + resolve + residual).
- **WASM**: addRadiusConstraint + edgeCurveRadius + radiusDimAt (+ baseline) + list "radius".
- **Bridge**: 3 wrappers.
- **Tool**: RadialDimensionTool + ToolManager 등록.
- **Render**: DimensionManager radiusLine. **DimensionLabel 변경 0** (직선 재사용).
- **UI**: AxiaCommands tool-radial-dimension + Dimension 메뉴 + MenuBar case.

## 5. 회귀 + 검증

- **회귀**: axia-core +2 (`adr217_radius_constraint_drives_arc` — Arc 크기조절 center
  고정 + 제약 구동 + 편집 / `adr217_set_curve_radius_circle` — Circle self-loop) ·
  axia-wasm +3 (addRadiusConstraint/edgeCurveRadius/radiusDimAt, baseline) · vitest +10
  (RadialDimensionTool 4 + DimensionManager 2 + bridge 4). 절대 #[ignore] 금지. tsc clean.
  전체 워크스페이스 axia-geo 1986 / axia-core 396 PASS.
- **브라우저** (real WASM): Path B 원 r5 → addRadiusConstraint → id 1 →
  **setConstraintValue(12) → 원 r5→12, anchor (12,0,0) 이동, center (0,0,0) 고정**
  (driving + center 고정 확인).

## 6. 후속 (Dimension 확장 마지막)

- **ADR-218 Reference(읽기전용)** — 비구동 측정값 영구 annotation (Linear/Angular/
  Radial 읽기전용). Dimension 확장 마무리.
- (선택) Radial dimension leader line(원 밖 라벨) / 호 외 곡선(타원 등).

## 7. Cross-link

- ADR-215/216 (Linear/Angular Dimension — DimensionTool/Manager/setConstraintValue 재사용)
- ADR-089 (Path B Circle self-loop) / ADR-028 (Arc curve)
- `DimensionLabel` 직선 (재사용, 변경 0)
- ADR-210 (Dimension 메뉴) / ADR-046 P31 #4 (additive)
- ADR-087 K-ζ (사용자 시연 게이트) / 메타-원칙 #4 #5 #6 #13
- LOCKED #44 (Complete Meaning per Merge)
