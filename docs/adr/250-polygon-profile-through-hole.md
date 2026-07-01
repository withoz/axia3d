# ADR-250 — Phase 2 P5: Arbitrary-Profile Through-Hole (`drill_polygon_through_hole` + Polygon-Hole tool)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 2 (Punch 확장) — P5 (임의-profile 관통). 시뮬레이션
  정제 순서 P1(ADR-249) → **P5** → P6 의 둘째.
- **Depends on**: ADR-249 (`drill_rect_through_hole` + 공유 `bridge_through_loops`
  + `drill_extract_new_hole_loop` — generalization source) / `punch_rect_hole`
  (host-search body shape) / ADR-190 P0.2 / ADR-007 / ADR-133 (AC ⊇ CC) /
  ADR-046 P31 #4 / DrawWindowTool (UI mirror)

## 1. Context

ADR-249 시뮬레이션이 P5 substrate 를 결정: `Mesh::boolean()` Subtract 는 관통/pocket
을 못 만듦 (rect box−box 통제조차 0 intersection → OPEN) → **P5 도 P1 과 같은
punch + topology-driven bridge 경로** (Boolean 아님). 사용자 결재 **UI = interactive
polygon-hole 도구** (Hole/Window 도구 mirror).

P5 engine 은 P1 의 자연 확장 — ADR-249 가 추출한 공유 자산이 모두 loop-agnostic:
`bridge_through_loops` (loop-size-agnostic, P1 검증) + `drill_extract_new_hole_loop`
(newest-id, 임의 loop OK). 남은 것은 임의 loop 를 punch 하는 `punch_polygon_hole`.

## 2. Decision

**Engine**:
- `Mesh::punch_polygon_hole(loop_pts: &[DVec3], normal_hint) -> Result<FaceId>`
  (mesh.rs) — `punch_circular_hole` / `punch_rect_hole` 의 일반화 (mesh.rs:10358
  "common helper" 실현): 유일한 shape-specific step (positions 계산) 을 **caller
  loop 를 host plane 에 투영**으로 대체. host 검색은 loop centroid 사용. 나머지
  (host-search / boundary+overlap 검증 / `add_face_with_holes` 재유도) 는 circle/rect
  와 동일. self-contained (circle/rect 본체 미변경 — 3-way 통합 helper 는 future
  refactor).
- `Mesh::drill_polygon_through_hole(loop_pts, normal) -> Result<DrillThroughResult>`
  (carve.rs) — `drill_rect_through_hole` 의 polygon 아날로그: depth → `punch_polygon_
  hole` entry/exit (exit = loop projected `-n*depth`) → 공유 `bridge_through_loops`.
  convex straight-through MVP.

**UI (신규 interactive 도구, Hole/Window mirror)**:
- `DrawPolygonHoleTool` (web/src/tools) — 면 위 N-점 capture (1st click = 면 hit →
  plane 캡처, clicks 2..N = ray ∩ plane, 닫기 = 첫 점 근처 클릭 / Enter / 더블클릭).
  **CCW 정규화** (`ccwLoop` shoelace signed area, 엔진은 hole loop CCW around +n 기대)
  + drill-then-punch fallback (솔리드=관통 / sheet=면 구멍, DrawWindowTool mirror).
- WASM `punchPolygonHole` / `drillPolygonThroughHole` (`&[f64]` flat xyz, snapshot+
  rollback) + bridge wrappers (`flattenLoop` helper, graceful -1).
- 6-layer wiring (ToolManager `polygon-hole` + CommandCatalog `tool-polygon-hole`
  + ActionCatalog (AC ⊇ CC) + MenuBar case + index.html 메뉴 + CatalogConsistency
  count 175→176). dist 재빌드 (CATALOG_SIZE 188→189).

## 3. Lock-ins

- **L-250-1** P5 substrate = punch + bridge (ADR-249 시뮬레이션 Boolean 반증). NOT
  Boolean.
- **L-250-2** `punch_polygon_hole` = circle/rect 일반화, self-contained host-search
  (circle/rect 본체 미변경, risk 격리). positions = caller loop 투영.
- **L-250-3** `drill_polygon_through_hole` = `drill_rect` 의 polygon 아날로그 + 공유
  `bridge_through_loops` (ADR-249 자산 재사용, 새 bridge 0).
- **L-250-4** UI CCW 정규화 강제 (`ccwLoop` shoelace) — 사용자 클릭 방향 무관, 엔진
  hole-loop CCW around +n 보장.
- **L-250-5** drill-then-punch fallback (DrawWindowTool mirror) — 솔리드=관통/sheet=면.
- **L-250-6** convex straight-through MVP (`bridge_through_loops` anti-parallel guard
  공유). 비볼록 솔리드 = P6. (profile 자체는 simple non-convex 가능 — add_face_with_
  holes / bridge 가 임의 simple loop 처리.)
- **L-250-7** AC ⊇ CC (ADR-133), CommandCatalog 176, dist 재빌드. ADR-046 P31 #4
  additive (신규 도구, 기존 무변경).
- **L-250-8** snapshot rollback (ADR-190 P0.2). ADR-007 manifold 검증 (bridge 끝).
- **L-250-9** 절대 #[ignore] 금지.

## 4. 회귀 / 검증

- **axia-geo** carve `adr249_p5_*` 5 (punch_polygon pentagon window 5-vert / punch
  rejects <3 + outside-boundary / drill triangle through 3 quads / drill pentagon
  5 quads 양 cap holed / drill rejects <3 + no-opposite-wall). axia-geo lib **2006**
  (2001 → +5).
- **axia-wasm** punchPolygonHole / drillPolygonThroughHole exports (baseline additive
  PASS) — **64**.
- **vitest** 161 files **2414 passed** / 1 skipped, tsc 0. CatalogConsistency
  175→**176** (AC ⊇ CC). action-catalog dist CATALOG_SIZE 188→**189**.
- **브라우저 e2e** (실제 rebuilt WASM, ADR-087 K-ζ 게이트): box 6 faces → triangle
  `drillPolygonThroughHole` → 3 tube quads (9 faces) → pentagon → 5 tube quads
  (14 faces), invariants valid 0 violations, console clean.

## 5. Lessons

- **L1 시뮬레이션 자산의 직접 dividend** — ADR-249 시뮬레이션이 P5 substrate (punch+
  bridge, Boolean 아님) 를 미리 확정 + ADR-249 가 추출한 공유 helper (`bridge_through_
  loops` loop-agnostic, `drill_extract_new_hole_loop` newest-id) 가 P5 에 무수정
  재사용. P5 engine = `punch_polygon_hole` 한 함수 + 얇은 drill wrapper. de-risk-first
  + Pattern-12 의 복리 효과.
- **L2 일반화 = special-case 흡수** — circle/rect/polygon punch 의 유일한 차이는
  positions 계산 (원주 / bbox / 임의 loop). mesh.rs:10358 "common helper" 가 정확히
  이 구조. self-contained 로 추가 (risk 격리), 3-way 통합은 future refactor.
- **L3 UI winding 정규화** — 엔진은 hole loop CCW around +n 기대 (punch_circular 관습).
  interactive 도구는 사용자 클릭 방향 무관 → tool-side shoelace signed area 로 CCW
  정규화. 향후 free-form 입력 도구의 표준.
- **L4 UI 5+1-layer 패턴 (Window mirror + 신규 도구)** — ADR-249 는 Window 재사용
  (catalog 0), P5 는 신규 도구라 6-layer wiring (ToolManager+CC+AC+MenuBar+html+
  count). 두 경로 (도구 재사용 vs 신규 도구) 모두 reproducible.

## 6. 후속 (시뮬레이션 정제 순서)

- **P6 (별도 ADR, multi-week)** — 다면/비볼록 솔리드 관통: multi-solid drill loop
  (anti-parallel guard 완화 + 각 shell 순차) 또는 polygonal CSG SSI 0-intersection
  수정. 현 substrate 둘 다 실패 (ADR-249 시뮬레이션). 추가 시뮬레이션 권장.
- **punch 3-way 통합 helper** — circle/rect/polygon 의 host-search 본체 공유 추출
  (mesh.rs:10358 follow-up). refactor only.
- **footgun (ADR-249 시뮬레이션 P5b)** — topology op(punch)이 face id 무효화. drill/
  punch 는 world-point 로 host 재계산하므로 영향 없음 (DrawPolygonHoleTool 도 commit
  시 fresh 계산).

## 7. Cross-link

- ADR-240 (Phase 2 로드맵) / ADR-249 (P1 — 공유 bridge + drill_extract + 시뮬레이션
  substrate source) / `punch_circular_hole`·`punch_rect_hole` (일반화 source) /
  ADR-190 P0.2 / ADR-007 / ADR-133 (AC ⊇ CC) / ADR-046 P31 #4 / DrawWindowTool
  (UI mirror) / DrawHoleTool (fallback mirror) / 메타-원칙 #5 #6 #16 / LOCKED #44.
