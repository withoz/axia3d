# ADR-212 — 2D Corner Fillet + Chamfer (valence-2 wire corner)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: 2D CAD 편집 명령 (ADR-210 메뉴 재구성 후속 C2) / Foundation
- **Depends on**: ADR-211 (Trim/Extend, `edit_2d.rs` 모듈) / ADR-028
  (`AnalyticCurve::Arc` + `add_edge_with_curve`) / ADR-172 (wire auto-split) /
  ADR-207 · ADR-209 (3D vertex/edge fillet/chamfer — 구분 대상)

## 1. Context

ADR-210 메뉴 재구성 후 2D CAD 편집 명령 C2. 두 와이어 edge가 만나는 **코너**
(valence-2 꼭짓점)를 둥글게(fillet, 호) 또는 모따기(chamfer, 직선)하는 연산.
de-risk audit: 모든 building block (`split_edge` / `remove_edge_and_halfedges` /
`add_edge` / `add_edge_with_curve` / `AnalyticCurve::Arc` / `count_incident_edges`)
이 이미 존재 → 신규 = 코너 식별 + arc 기하 + surgery 합성 (신규 kernel 0).

코너는 valence-2 — 두 edge가 *끝점*에서 만나므로(교차 아님) ADR-172 자동분할이
간섭하지 않음 (C1 trim 의 자동분할 발견과 직교).

## 2. Decision

사용자 결재 (2026-06-22): **Option A — 신규 Corner Fillet + Corner Chamfer 2도구**.

- **Engine** (`operations/edit_2d.rs`, ADR-211 모듈 확장):
  - `Mesh::fillet_corner_2d(corner, radius)` — 트림 거리 `R/tan(θ/2)`, 양 edge를
    트림 + 접선 `AnalyticCurve::Arc`(center = 내부 bisector 위 `R/sin(θ/2)`) 삽입.
  - `Mesh::chamfer_corner_2d(corner, dist)` — 양 edge를 `dist` 트림 + 직선 연결.
  - 공유 `corner_2d_geom`(각도/방향) + `cut_corner`(split→stub 제거→연결 edge).
  - `Mesh::two_edges_at_corner(vid)` (mesh.rs) — valence-2 코너의 두 edge 헬퍼.
- **WASM/bridge**: `filletCorner2d(vert, radius)` / `chamferCorner2d(vert, dist)`.
- **Tools**: `CornerFilletTool` / `CornerChamferTool` — valence-2 코너 꼭짓점
  클릭(findVertexIdAt) + VCB 반지름/거리. ADR-207 `chamfer_vertex_3way`(3D
  valence-3) / ADR-209 `filletEdge`(3D 엣지)와 **별개 도구**(additive).

## 3. Lock-ins

- **L-212-1** fillet/chamfer corner = 기존 primitive 합성 + arc 기하 (신규
  geometric kernel 0, Pattern-12).
- **L-212-2** valence-2 코너 한정 (`two_edges_at_corner` 가드). valence-3 solid
  코너는 ADR-207 (별개). collinear/radius 과대는 거부.
- **L-212-3** Fillet = 접선 호 (`R/tan(θ/2)` 트림, center on bisector). Chamfer =
  직선. 접선성 회귀로 검증 (center가 양 trim점에서 정확히 R).
- **L-212-4** 신규 CornerFilletTool/CornerChamferTool (`tool-corner-fillet` /
  `tool-corner-chamfer`), 기존 3D tool-fillet/tool-chamfer 무손상 (ADR-046 P31 #4).
- **L-212-5** `AnalyticCurve::Arc` (ADR-028) + `add_edge_with_curve` 재사용.
- **L-212-6** ADR-172 자동분할은 코너(valence-2 끝점 meet)에 무영향.
- **L-212-7** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `edit_2d.rs` (fillet_corner_2d / chamfer_corner_2d / corner_2d_geom /
  cut_corner) + `mesh.rs` (two_edges_at_corner pub(crate)).
- **WASM** (`lib.rs`): filletCorner2d / chamferCorner2d (+ export_baseline 2).
- **Bridge** (`WasmBridge.ts`): 2 wrappers (graceful fallback).
- **Tools**: CornerFilletTool / CornerChamferTool + ToolManager 등록 +
  AxiaCommands(tool-corner-fillet/chamfer) + MenuBar case + index.html Modify 메뉴.

## 5. 회귀 + 검증

- **회귀**: axia-geo +5 (chamfer / fillet+접선 / non-corner reject / radius 과대 /
  collinear reject) · axia-wasm +2 (filletCorner2d/chamferCorner2d, baseline) ·
  vitest +24 (CornerFillet 10 + CornerChamfer 10 + bridge 4). 절대 #[ignore] 금지.
  tsc clean · 전체 워크스페이스 axia-geo 1977 / axia-core 392 PASS.
- **브라우저** (real WASM): FILLET L-코너 → arc edge (curveKind=3=Arc) ·
  CHAMFER → line edge (curveKind=0, 길이 √18=4.2426 정확) · 비-코너(valence-1) -1 거부.

## 6. 후속 (별도 ADR, ADR-210 §6 로드맵)

- **C3** Join (폴리라인 결합) + edge transform (Copy/Mirror/Array on edges).
- **C5** Point 도구 + Dimension (Linear/Aligned/Angular/Radial) — Dimension 메뉴.
- (선택) 코너 fillet/chamfer live preview (geometric ghost).
- (선택) 두 edge가 안 만나는(gap) 경우 자동 연장 후 fillet (AutoCAD trim-extend).

## 7. Cross-link

- ADR-211 (Trim/Extend — 같은 `edit_2d.rs` 모듈, 2D 편집 트랙)
- ADR-210 (메뉴 재구성 — Modify 메뉴 home)
- ADR-028 (`AnalyticCurve::Arc` + `add_edge_with_curve`)
- ADR-207 (3D vertex 3-way chamfer) / ADR-209 (3D edge fillet) — 구분
- ADR-172 (wire auto-split — 코너 valence-2 에 무영향)
- ADR-087 K-ζ (사용자 시연 게이트 — 브라우저 검증)
- ADR-046 P31 #4 (additive only) / 메타-원칙 #4 #5 #6 #14
- LOCKED #44 (Complete Meaning per Merge — 단일 atomic PR)
