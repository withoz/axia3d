# ADR-216 — Angular Dimension (driving / parametric)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Dimension 확장 (ADR-215 후속) / Foundation
- **Depends on**: ADR-215 (Linear Dimension, DimensionTool/Manager, setConstraintValue) /
  Constraint Solver Level 2 (`resolve_edge_pair` Perpendicular 솔버) / `DimensionLabel`

## 1. Context

ADR-215 Linear Dimension(driving) 후속. 사용자 결재: **driving 개별 (Angular →
Radial → Reference)**. 본 ADR = **Angular(구동)**.

사전검토(시뮬레이션): `ConstraintKind`에 Angle 없음. 하지만 Perpendicular 솔버
(`resolve_edge_pair`, 90° 특수 case)가 `dir_a×dir_b` 평면에서 target_dir 계산 →
driven 엣지 회전. **Angle = 이것의 일반화** (target_dir = dir_a를 θ만큼 Rodrigues
회전). 신규 솔버 알고리즘 0.

사용자 결재 (2026-06-22): **Q1=a 공유 정점 우선 pivot** (코너 친화) + **Q2=a
DimensionLabel 호 확장** (editable 각도 라벨).

## 2. Decision

**Angular Dimension = driving Angle constraint + editable DimensionLabel arc** (Pattern-12).

- **Engine** (`constraint.rs`): `ConstraintKind::Angle` 추가. `resolve_edge_pair`에
  `target_angle` param + Angle target_dir (`dir_a*cosθ + (plane_n×dir_a)*sinθ`) +
  **pivot = 공유 정점**(없으면 midpoint). `constraint_residual`에 Angle (|현재−target|).
- **WASM**: `addAngleConstraint(eAvA,eAvB, eBvA,eBvB, angleRad)` (0<θ<π). 편집은
  ADR-215 `setConstraintValue` 재사용 (value=radians).
- **Tool** (`AngularDimensionTool`): 2 엣지 클릭(edgeMap pick) → 현재 각도 계산 →
  addAngleConstraint. Dimension 메뉴.
- **Render**: `DimensionLabel`에 `DimLine.angular{apex,dirA,dirB,radius,valueDeg}`
  필드 + `renderAngular`(호 + apex 연장선 + 호 중점 editable 라벨). 편집 시 degrees
  입력 ↔ radians 내부. `DimensionManager`가 angle 제약 → angular DimLine.

## 3. Lock-ins

- **L-216-1** `ConstraintKind::Angle` = `resolve_edge_pair` 일반화 (Rodrigues target_dir).
  신규 geometric solver 알고리즘 0.
- **L-216-2** pivot = 공유 정점 우선(코너 유지, Q1=a), 없으면 midpoint. Parallel/
  Perpendicular/Collinear 는 midpoint 보존 (Angle 만 공유정점).
- **L-216-3** DimensionLabel 호 확장 (Q2=a) — angular DimLine + renderAngular,
  editable onEdit 기계 재사용 (degrees↔radians).
- **L-216-4** DimensionManager = distance + angle 처리. angular 편집 degrees↔radians.
- **L-216-5** addAngleConstraint + setConstraintValue (ADR-215 재사용). value=radians (0,π).
- **L-216-6** 영구(snapshot)+solve+undo/redo = 기존 constraint 직렬화.
- **L-216-7** 한계: 각도 모호성(tool 은 acos 각도 0..π 측정). 코너 외 자유 엣지는 midpoint.
- **L-216-8** additive — 신규 tool-angular-dimension + Dimension 메뉴 (ADR-046 P31 #4).
- **L-216-9** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `constraint.rs` (Angle enum + resolve_edge_pair target_angle/pivot +
  residual).
- **WASM**: `addAngleConstraint` (+ baseline) + list_constraints "angle".
- **Bridge**: addAngleConstraint wrapper.
- **Tool**: AngularDimensionTool + ToolManager 등록.
- **Render**: DimensionLabel angular(arc) + DimensionManager angleLine.
- **UI**: AxiaCommands tool-angular-dimension + Dimension 메뉴 + MenuBar case.

## 5. 회귀 + 검증

- **회귀**: axia-core +1 (`adr216_angle_constraint_drives_corner` — 코너 각도 90°→
  target 구동, set_value 편집 재구동, 공유 정점 고정) · axia-wasm +1 (addAngleConstraint,
  baseline) · vitest +10 (AngularDimensionTool 6 + DimensionManager 2 + bridge 2).
  절대 #[ignore] 금지. tsc clean. 전체 워크스페이스 axia-geo 1986 / axia-core 394 PASS.
- **브라우저** (real WASM): 2 엣지 코너(공유 정점) 26.57° → addAngleConstraint → id 1 →
  **setConstraintValue(π/2) → 엣지 B 코너 중심 회전, 26.57°→90°** (driving + 공유 정점
  pivot 확인).

## 6. 후속 (Dimension 확장 다음)

- **ADR-217 Radial(구동)** — Circle/Arc 반지름. 신규 ConstraintKind::Radius (원 크기조절).
- **ADR-218 Reference(읽기전용)** — 비구동 측정값 영구 annotation.
- (선택) Angular live preview / 각도 모호성(reflex angle) UX / 코너 외 pivot 개선.

## 7. Cross-link

- ADR-215 (Linear Dimension — DimensionTool/Manager/setConstraintValue 재사용)
- Constraint Solver Level 2 (`resolve_edge_pair` Perpendicular — 일반화 source)
- `DimensionLabel` (editable onEdit — 호 확장)
- ADR-210 (Dimension 메뉴) / ADR-046 P31 #4 (additive)
- ADR-087 K-ζ (사용자 시연 게이트) / 메타-원칙 #4 #5 #6 #13
- LOCKED #44 (Complete Meaning per Merge)
