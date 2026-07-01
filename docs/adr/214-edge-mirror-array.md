# ADR-214 — Edge Mirror / Array (Copy on edges)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: 2D CAD 편집 명령 (ADR-210 메뉴 재구성 후속 C3 part 2) / Foundation
- **Depends on**: ADR-211 (`edit_2d.rs` 모듈) / ADR-053 (`AnalyticCurve::transform`) /
  ADR-208 (CopyTool) / ADR-209 (Mirror/Array tools) / `mirror_faces` ·
  `array_linear_faces` · `array_radial_faces` (face 변형 구조 source)

## 1. Context

ADR-213 (Join) 후속, 2D CAD 편집 C3 part 2. 사용자 결재 Option A (C3 분리,
edge transform 후속). 기존 mirror/array/copy 도구는 **face 전용**(`mirror_faces`
/ `array_*_faces`). 2D 스케치(wire edge)에서 프로필 미러링/배열이 불가능했음.

de-risk audit: `AnalyticCurve::transform(&DMat4)` (ADR-053) 가 Line(no-op)/
Circle/Arc(rigid)/Bezier/BSpline/NURBS 변환 지원 → edge 변형 시 curve 보존
가능. face 변형 구조(add_vertex dedup, source 무손상)를 edge 로 재사용.

## 2. Decision

- **Engine** (`operations/edit_2d.rs`, ADR-211 모듈 확장):
  - `replicate_edges(edge_ids, transforms: &[DMat4])` — 공유 핵심. 각 transform
    마다 source 정점을 변환(`m.transform_point3`) + curve 를 `AnalyticCurve::
    transform` 으로 보존(실패 시 chord line 으로 graceful degrade) + `add_edge` /
    `add_edge_with_curve`. shared endpoint 는 spatial-hash dedup 으로 copy 내 공유.
  - `mirror_edges(plane)` — Householder reflection DMat4 (1 copy).
  - `array_linear_edges(count, offset)` — translate DMat4 × count.
  - `array_radial_edges(count, axis, total_angle)` — rotate DMat4 × count.
- **WASM/bridge**: `mirrorEdges` / `arrayLinearEdges` / `arrayRadialEdges`.
- **Tools** (additive 확장, 신규 도구 0): 기존 MirrorTool / ArrayLinearTool /
  ArrayRadialTool (ADR-209) / CopyTool (ADR-208) 가 **faces-or-edges dispatch** —
  면 선택 시 face op (기존), 면 없이 엣지만 선택 시 edge op (신규). MoveTool/
  RotateTool/ScaleTool 의 faces-or-edges 패턴 답습.

## 3. Lock-ins

- **L-214-1** 3 edge 변형 = 단일 `replicate_edges(&[DMat4])` 통일 (mirror=reflection
  / linear=translate / radial=rotate). 신규 geometric kernel 0 (Pattern-12).
- **L-214-2** Curve 보존 = `AnalyticCurve::transform` (ADR-053). rigid 변환 시 보존,
  reflection of arc / non-uniform → chord line graceful degrade (op 실패 안 함).
- **L-214-3** 기존 Mirror/Array/Copy 도구 faces-or-edges dispatch (MoveTool 패턴).
  신규 도구/명령/메뉴 0 — 기존 도구가 edge 지원 획득 (additive, ADR-046 P31 #4).
- **L-214-4** Faces 우선, 면 없이 엣지만 선택 시 edge op fallback.
- **L-214-5** 구조 = `array_op.rs`/`mirror.rs` face 버전 답습 (add_vertex dedup,
  source 무손상, per-copy vert map).
- **L-214-6** Radial full-circle (2π, count N) 의 N번째 copy (360°) 는 source 와
  일치 → dedup (기존 `array_radial_faces` 동일 semantics).
- **L-214-7** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `edit_2d.rs` (replicate_edges + mirror_edges + array_linear_edges +
  array_radial_edges).
- **WASM** (`lib.rs`): mirrorEdges / arrayLinearEdges / arrayRadialEdges (+ baseline 3).
- **Bridge** (`WasmBridge.ts`): 3 wrappers (graceful fallback).
- **Tools**: MirrorTool / ArrayLinearTool / ArrayRadialTool / CopyTool 의 commit
  경로 faces-or-edges dispatch. 기존 도구 테스트 mock 에 `selection.getSelectedEdges`
  추가.

## 5. 회귀 + 검증

- **회귀**: axia-geo +5 (mirror / array linear / array radial / arc curve 보존 /
  guards) · axia-wasm +3 (3 exports, baseline) · vitest +5 (bridge 4 + MirrorTool
  edge-path 1; 4 도구 mock 갱신). 절대 #[ignore] 금지. tsc clean. 전체 워크스페이스
  axia-geo 1986 / axia-core 392 PASS.
- **브라우저** (real WASM): mirrorEdges → 1 copy · arrayLinearEdges ×3 → 3 copies ·
  arrayRadialEdges ×4 → 4 (4번째=360° source dedup).

## 6. 🎉 C3 (Join + edge transform) 완료

- **C3 part 1** ADR-213 Join (collinear merge) ✅
- **C3 part 2** ADR-214 Edge Mirror/Array ✅

후속 (ADR-210 §6 로드맵):
- **C5** Point 도구 + Dimension (Linear/Aligned/Angular/Radial) — Dimension 메뉴.
- (선택) Edge transform live ghost preview (array 복제본 미리보기).
- (선택) Reflection of arc 의 정확한 curve 보존 (현재 chord degrade) — 별도.

## 7. Cross-link

- ADR-211 (Trim/Extend) / ADR-212 (Corner fillet/chamfer) / ADR-213 (Join) —
  같은 `edit_2d.rs` 2D 편집 모듈
- ADR-053 (`AnalyticCurve::transform` — curve 보존 source)
- ADR-208 (CopyTool) / ADR-209 (Mirror/Array tools) — edge dispatch 확장 대상
- ADR-210 (메뉴 재구성) / ADR-046 P31 #4 (additive only)
- ADR-087 K-ζ (사용자 시연 게이트) / 메타-원칙 #4 #5 #6 #14
- LOCKED #44 (Complete Meaning per Merge)
