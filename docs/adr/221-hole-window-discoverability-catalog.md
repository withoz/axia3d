# ADR-221 — Hole/Window Discoverability (CommandCatalog + ActionCatalog)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Roadmap ③ 24-도구 Phase 5 (Hole) / Foundation
- **Depends on**: ADR-194 (hole punch / drill-through) / ADR-191 (ring face Push/Pull) /
  ADR-220 (Sweep/Loft discoverability — 동일 패턴) / ADR-133 (AC ⊇ CC) / ADR-210 (메뉴)

## 1. Context

24-도구 로드맵 ③의 Hole 항목. de-risk (4 서브시스템 병렬 + 적대적 검증 + 브라우저)가
**Pattern-12 확정**: Hole/Window 도구가 **이미 완전 존재·작동** (ADR-220 Sweep와 동일).

**에이전트 충돌 해소**: 한 에이전트가 "ABSENT" 주장했으나 **직접 grep + 브라우저 검증으로
오류 확인** — Hole/Window는 전 레이어 present (적대적 검증의 가치).

- 엔진 `punch_circular_hole`/`punch_rect_hole`/`drill_circular_through_hole` (8 회귀, ADR-194).
- WASM `punchHole`/`drillThroughHole`/`punchRectHole` + bridge 래퍼.
- `DrawHoleTool`(2-click)/`DrawWindowTool` + ToolManager 등록 + MenuBar case + index.html
  메뉴(Draw "⊘ 구멍" / MoDeling "창").
- Push/Pull → through-tube (ADR-191).
- **브라우저 검증**: 100×100 면에 r20 구멍 punch → inner_loop_count 1 (ring-with-hole),
  invariants valid 0 violations (manifold).

**진짜 gap**: `tool-hole`/`tool-window`이 **CommandCatalog(AxiaCommands.ts) + ActionCatalog
미등록** → Command Palette / 단축키 도움말에 안 보임 (ADR-220 Sweep와 정확히 동일).

**사용자 결재 (2026-06-23)**: Scope **A — Discoverability 마무리** (등록 + 검증 + 문서).
커널 심화(circle metadata + Boolean/Offset 다중-루프 허용)는 별도 후속 ADR.

## 2. Decision

**Hole/Window discoverability closure** (Pattern-12, 도구/엔진 코드 0 — ADR-220 답습).

- **CommandCatalog** (`AxiaCommands.ts`): `tool-hole`(category draw, Draw 메뉴 위치) +
  `tool-window`(category modify, MoDeling 메뉴) 추가. 단축키 없음(H=sphere 점유).
- **ActionCatalog** (`catalog.ts`): AC ⊇ CC 정합 — `tool-hole`(bridge `punchHole`) +
  `tool-window`(bridge `punchRectHole`) 추가 (tier 2 modificative).
- **dist 재빌드** (ADR-133 L-133-8, gitignored — build가 재생성).
- **테스트 정정**: CatalogConsistency CC count 164→166.
- **엔진/WASM/도구/메뉴 변경 0** — DrawHoleTool/DrawWindowTool·punch*·MenuBar·index.html 보존.

## 3. Lock-ins

- **L-221-1** Hole/Window = 이미 작동 (Pattern-12, 브라우저 r20 구멍 → ring inner_loop 1
  manifold). 본 ADR은 discoverability만.
- **L-221-2** AC ⊇ CC invariant 정합 (ADR-133) — CommandCatalog 추가와 ActionCatalog 동기.
- **L-221-3** 적대적 검증 가치 — 에이전트 "ABSENT" 주장을 직접 grep + 브라우저로 반증.
- **L-221-4** tool-hole=draw(Draw 메뉴) / tool-window=modify(MoDeling) — 메뉴 위치 정합.
- **L-221-5** 단축키 없음 (H 점유) — 표시 메타데이터만, dispatch는 메뉴/도구.
- **L-221-6** dist 재빌드 필수 (web import source).
- **L-221-7** 커널 심화(circle metadata / multi-loop downstream Boolean/Offset) 별도 ADR
  — LOCKED #1/ADR-016 Q2 정책은 사용자 명시 변경 필요.
- **L-221-8** ADR-046 P31 #4 additive only — 도구/엔진/메뉴 surface 보존.
- **L-221-9** 절대 #[ignore] 금지.

## 4. 회귀 + 검증

- **회귀**: CommandCatalog +2 (hole/window), ActionCatalog +2. CatalogConsistency 3/3
  PASS (AC ⊇ CC + count 166). 패키지 catalog 24/24. web commands 26/26. tsc clean.
  엔진(cargo) 무변경.
- **브라우저** (real WASM, de-risk): 100×100 면 r20 punchHole → faceId, inner_loop_count 1,
  invariants valid 0 violations (manifold ring-with-hole). hole/window 도구 ToolManager 등록 확인.

## 5. 후속 (별도 ADR per LOCKED #44)

- **Hole 커널 심화**: 구멍 inner loop에 AnalyticCurve::Circle 부착(self-loop kernel-native,
  ADR-089 답습) → Boolean/Offset 다중-루프 면 허용(ADR-016 Q2 완화, 사용자 결재 필요).
- axia-core scene/command 통합 테스트 (현재 엔진 kernel-level only).
- 3P-Plane / NURBS surface / Wall / Window(고급) → 24-도구 잔여.

## 6. Cross-link

- ADR-194 (hole punch / drill-through 엔진) / ADR-191 (ring face Push/Pull through-tube)
- ADR-220 (Sweep/Loft discoverability — 동일 패턴, 직전 ADR) / ADR-133 (AC ⊇ CC)
- ADR-045 D1 (ActionCatalog SSOT) / ADR-210 (메뉴) / ADR-016 Q2 (multi-loop face 정책)
- ADR-089 (closed-curve self-loop — 커널 심화 anchor) / ADR-046 P31 #4 (additive)
- LOCKED #44 (Complete Meaning per Merge) / 메타-원칙 #4 #5 #6
