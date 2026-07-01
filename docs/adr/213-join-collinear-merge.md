# ADR-213 — Join (collinear merge of 2D wire edges)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: 2D CAD 편집 명령 (ADR-210 메뉴 재구성 후속 C3 part 1) / Foundation
- **Depends on**: ADR-211 (`edit_2d.rs` 모듈) / ADR-212 (`two_edges_at_corner`) /
  ADR-172 (wire auto-split — Join 이 정리하는 fragment source) /
  `split_edge` (Join 은 그 역연산)

## 1. Context

ADR-210 메뉴 재구성 후 2D CAD 편집 C3. 사용자 결재(2026-06-22) Option A —
C3 를 **ADR-213 Join (collinear merge)** + ADR-214 (edge transform) 으로 분리,
Join 먼저.

de-risk audit: 엔진에 join/weld/merge/dissolve 없음 (신규). 가장 well-defined
한 의미 = **collinear merge** — valence-2 꼭짓점에서 만나는 일직선 두 edge 를
하나로 병합 (`split_edge` 의 역연산). auto-split(ADR-172) / Trim 이 남긴 일직선
fragment 정리에 자연스러움.

## 2. Decision

- **Engine** (`operations/edit_2d.rs`, ADR-211 모듈 확장):
  `Mesh::join_collinear_at(vertex)` — valence-2 꼭짓점의 두 직선 edge 가
  일직선(`dir1·dir2 ≤ −0.999`, ~2.5°)이고 둘 다 곡선 없음이면, 두 edge 제거 +
  먼 두 끝점을 잇는 단일 edge 추가, 공유 꼭짓점 dissolve. `two_edges_at_corner`
  (ADR-212) 재사용.
- **WASM/bridge**: `joinCollinearAt(vert) -> i32`.
- **Tool**: `JoinTool` — valence-2 일직선 꼭짓점 클릭 → 즉시 병합 (VCB 없음).
  `tool-join`, Modify 메뉴.

## 3. Lock-ins

- **L-213-1** Join = collinear merge (valence-2, `split_edge` 의 역). 기존
  primitive 합성 (`remove_edge_and_halfedges` + `add_edge`), 신규 kernel 0.
- **L-213-2** 가드: valence-2 (`two_edges_at_corner`) + 일직선 (`dot ≤ −0.999`)
  + 곡선 없음 (line only) + degenerate self-edge 거부.
- **L-213-3** JoinTool = 꼭짓점 클릭 즉시 병합 (parameterless, no VCB).
- **L-213-4** `two_edges_at_corner` (ADR-212, mesh.rs) 재사용.
- **L-213-5** 신규 `tool-join` (additive, ADR-046 P31 #4). 기존 도구 무손상.
- **L-213-6** ADR-172 auto-split / Trim fragment 정리 — split 의 역연산.
- **L-213-7** 절대 #[ignore] 금지.

## 4. 구현

- **Engine**: `edit_2d.rs` (`join_collinear_at`).
- **WASM** (`lib.rs`): `joinCollinearAt` (+ export_baseline).
- **Bridge** (`WasmBridge.ts`): wrapper (graceful fallback).
- **Tool**: JoinTool + ToolManager 등록 + AxiaCommands(tool-join) + MenuBar case
  + index.html Modify 메뉴.

## 5. 회귀 + 검증

- **회귀**: axia-geo +4 (collinear merge / non-collinear reject / curved reject /
  non-valence2 reject) · axia-wasm +1 (joinCollinearAt, baseline) · vitest +7
  (JoinTool 4 + bridge 3). 절대 #[ignore] 금지. tsc clean. 전체 워크스페이스
  axia-geo 1981 / axia-core 392 PASS.
- **브라우저** (real WASM): collinear A—V—B → joinCollinearAt → edges 2→1,
  merged 길이 10 정확, V dissolve · 90° 코너 -1 거부.

## 6. 후속 (별도 ADR, ADR-210 §6 로드맵)

- **ADR-214 C3 part 2** — Edge Copy/Mirror/Array (face op 의 edge 변형:
  `array_linear_edges` / `array_radial_edges` / `mirror_edges`, curve 보존).
- **C5** Point 도구 + Dimension (Linear/Aligned/Angular/Radial) — Dimension 메뉴.
- (선택) Weld near-coincident endpoints (tolerance-based) — collinear 외 Join.
- (선택) Chain join (한 클릭으로 전체 일직선 chain 병합).

## 7. Cross-link

- ADR-211 (Trim/Extend — 같은 `edit_2d.rs` 모듈) / ADR-212 (Corner fillet/chamfer,
  `two_edges_at_corner`)
- ADR-210 (메뉴 재구성 — Modify 메뉴 home)
- ADR-172 (wire auto-split — Join 이 정리하는 fragment source)
- ADR-087 K-ζ (사용자 시연 게이트 — 브라우저 검증)
- ADR-046 P31 #4 (additive only) / 메타-원칙 #4 #5 #6
- LOCKED #44 (Complete Meaning per Merge — 단일 atomic PR)
