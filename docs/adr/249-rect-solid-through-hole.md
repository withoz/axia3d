# ADR-249 — Phase 2 P1: Rectangular SOLID Through-Hole (`drill_rect_through_hole`)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 2 (Punch 확장) — P1 (사각 관통). 시뮬레이션이
  정제한 순서 P1 → P5 → P6 의 첫째.
- **Depends on**: ADR-194 (`drill_circular_through_hole` — bridge source) /
  `punch_rect_hole` (face-level window) / ADR-007 (winding/manifold) /
  ADR-190 P0.2 (snapshot rollback) / ADR-046 P31 #4 (additive)

## 1. Context — audit + 상세 시뮬레이션

ADR-240 Phase 2 Punch 확장. audit (workflow + 직접 read) + **9-probe 실제 엔진
시뮬레이션** (codebase 교훈: empirical probe > LLM 추론, 기하는 실행으로 확인)
으로 substrate 를 ground-truth:

**capability matrix (실측)**:

| | 원형 | 사각 | 임의 | 다면 |
|---|---|---|---|---|
| FACE-LEVEL hole | ✅ `punch_circular_hole` | ✅ `punch_rect_hole` (Window 도구) | ❌ | ❌ |
| SOLID 관통 | ✅ `drill_circular_through_hole` | ❌ **P1** | ❌ P5 | 🚫 P6 |

**핵심 시뮬레이션 발견**:
1. **P1 = TRIVIAL** — box top/bottom 에 `punch_rect_hole` × 2 + drill 의 generic
   bridge (loop-size-agnostic) → whole-mesh manifold, 0 violations. drill 의
   tube-bridge 는 profile-agnostic 이었고 원형 가정은 단 (a) `punch_circular_hole`
   호출 (b) radial-band loop 추출 둘 뿐.
2. **P5 substrate 반전 (audit conflict 해소)** — `Mesh::boolean()` Subtract 는
   *rect box−box 통제(P5e)조차* "Intersections found: 0" → OPEN garbage. legacy
   polygonal CSG 는 면을 자르지 못함 (enclosed void 만 OK, 관통/pocket 전부 실패).
   → "Boolean shortcut for P5" 가설 완전 반증. P5 도 P1 과 같은 punch+bridge 경로.
   (이것이 ADR-197/198 이 곡면 Boolean 을 전용 bore/pierce 빌더로 만든 이유.)
3. **P6 = 진짜 별개 multi-week** — drill = single-solid (P6a depth=box1 only),
   boolean 두-shell = OPEN. 현 substrate 둘 다 실패.

본 ADR 은 **P1 (사각 관통)** 만 — 시뮬레이션이 manifold 로 증명한 atomic win.

## 2. Decision

**Engine (`operations/carve.rs`)**:
- `drill_rect_through_hole(corner_a, corner_b, normal) -> Result<DrillThroughResult>`
  — `drill_circular_through_hole` 의 rect 아날로그. depth 측정(carve ray) → entry/exit
  `punch_rect_hole` → 공유 bridge.
- **공유 bridge 추출 (Pattern-12, mesh.rs:10358 "common helper" 정신)**:
  `bridge_through_loops(entry, exit, e_loop, b_loop, axis, depth)` — anti-parallel
  exit guard + size match + reverse/nearest-align + quad 브리지 + manifold 검증.
  circular + rect 둘 다 사용. circular 거동 불변 (회귀 테스트 봉인).
- `drill_extract_new_hole_loop(face)` — 갓 punch 된 inner loop = NEWEST (최대 min
  VertId) 추출. 이미 hole 있는 face 도 robust (rect 는 radial-band 불가하므로 id
  단조성 활용). circular 은 기존 radial 유지.
- convex straight-through MVP (anti-parallel guard 유지). 비볼록/다면 = P6.

**UI (Window 도구 재사용 — catalog 변경 0)**:
- WASM `drillRectThroughHole(ax..bz, nx..nz)` (snapshot+rollback, `drillThroughHole`
  mirror) + bridge `drillRectThroughHole(...)` (graceful -1).
- `DrawWindowTool.commitWindow` 에 **try-drill-then-fallback** (DrawHoleTool 패턴
  mirror): 솔리드면 → `drillRectThroughHole` (관통 창), 실패(-1, sheet/미지원) →
  `punchRectHole` (면 창). Window 도구가 이제 솔리드면 = 관통, sheet = 면 창.

## 3. Lock-ins

- **L-249-1** P1 = `drill_rect_through_hole` (engine), Window 도구 재사용 (신규
  action/catalog 0 — ADR-046 P31 #4 additive). count 175 unchanged.
- **L-249-2** 공유 `bridge_through_loops` — circular + rect SSOT. circular 거동
  불변 (`adr249_circular_drill_unchanged_after_refactor` 봉인).
- **L-249-3** `drill_extract_new_hole_loop` = newest-id loop (rect 의 loop 추출;
  이미-holed face robust). circular 은 radial-band 유지.
- **L-249-4** convex straight-through MVP — anti-parallel exit guard 유지. 비볼록
  = P6 (별도 ADR).
- **L-249-5** snapshot rollback (ADR-190 P0.2) — drill 부분 실패 시 깨끗 복원.
- **L-249-6** Window try-drill-then-fallback (DrawHoleTool mirror) — 솔리드면 관통
  / sheet 면 창 자동 분기.
- **L-249-7** ADR-007 manifold 검증 (bridge 끝에서 verify_face_invariants, 실패 →
  Err → rollback). 메타-원칙 #6.
- **L-249-8** 절대 #[ignore] 금지.

## 4. 회귀 / 검증

- **axia-geo** carve `adr249_*` 5 (drill_rect_through_box_manifold / caps_have_rect
  _hole_loop 4-vert × 2 / no_opposite_wall_errors / degenerate_normal_errors /
  two_drills_existing_hole_manifold) + `adr249_circular_drill_unchanged_after_refactor`
  (refactor 회귀 가드). axia-geo lib **2001** (1995 → +6).
- **axia-wasm** `drillRectThroughHole` export (baseline additive PASS) — **64**.
- **vitest** 161 files **2414 passed** / 1 skipped, tsc 0. CatalogConsistency
  175 unchanged (Window 도구 재사용).
- **브라우저 e2e** (실제 rebuilt WASM, ADR-087 K-ζ 시연 게이트): create_box 6 faces
  → `drillRectThroughHole([-30,-20,100],[30,20,100],[0,0,1])` → **4 tube quads,
  10 faces, invariants valid 0 violations, console clean**. 엔진 테스트와 동일.

## 5. Lessons

- **L1 empirical 시뮬레이션 > audit 추론 (canonical 재확인)** — audit 의 P5 "Boolean
  shortcut" conflict 를 9-probe 실행으로 결정적 해소 (rect box−box 통제조차 0
  intersection → OPEN). audit synthesis 는 가설, 실행이 truth. 큰 scope (D) 진입
  전 시뮬레이션이 substrate 를 P1=P5=punch+bridge / P6=별개 로 재정렬.
- **L2 Pattern-12 + 공유 helper** — drill bridge 가 이미 loop-agnostic (시뮬레이션
  증명) → circular 에서 추출한 `bridge_through_loops` 를 rect 가 무수정 재사용.
  mesh.rs:10358 이 예고한 "common helper" 실현. 새 기하 0.
- **L3 newest-id loop 추출** — circular 의 radial-band 는 rect 에 부적합 → VertId
  단조성으로 "갓 punch 된 loop = 최신 id" robust 추출 (이미-holed face 도). 향후
  임의-profile (P5) punch 추출에도 재사용 가능.
- **L4 도구 재사용 = catalog 무변경** — Window 도구가 이미 존재 → commit 만 smarter
  (try-drill-then-fallback). 신규 action/catalog 없이 솔리드 관통 unlock. P31 #4.

## 6. 후속 (시뮬레이션 정제 순서)

- **P5 (ADR-250 가칭)** — 임의-profile 관통: `punch_polygon_hole`(임의 loop) +
  `drill_rect_through_hole` 의 generalize (bridge 는 이미 loop-agnostic, L3 추출
  재사용). Boolean 아님 (시뮬레이션 반증). medium.
- **P6 (별도 ADR, multi-week)** — 다면/비볼록: multi-solid drill loop (anti-parallel
  guard 완화 + 각 shell 순차) 또는 polygonal CSG SSI 0-intersection 수정. 추가
  시뮬레이션 권장.
- **footgun (시뮬레이션 P5b)** — topology op(punch)이 face id 무효화 → caller 가
  op 후 id 재취득 필수. (drill 은 world-point 로 host 재계산하므로 영향 없음.)

## 7. Cross-link

- ADR-240 (Phase 2 로드맵) / ADR-194 (`drill_circular_through_hole` — bridge +
  anti-parallel guard + DrillThroughResult source) / `punch_rect_hole` (face-level,
  Window) / ADR-197/198 (전용 bore/pierce — 왜 일반 boolean 이 관통 못 하는지) /
  ADR-190 P0.2 (snapshot rollback) / ADR-007 (manifold) / ADR-046 P31 #4 (additive,
  Window 재사용) / DrawHoleTool (try-drill-then-fallback mirror) / 메타-원칙 #5 #6
  #16. LOCKED #44 (Complete Meaning per Merge — P1 = 1 complete meaning).
