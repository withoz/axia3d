# ADR-224 — 3-Point Plane / Wall / NURBS surface Discoverability Closure

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: 24-도구 폭 확장 (3P-Plane → NURBS surface → Wall) — discoverability closure
- **Depends on**: ADR-133 (AC ⊇ CC invariant) / ADR-220 (Sweep/Loft discoverability) /
  ADR-221 (Hole/Window discoverability) / ADR-166 (plane lock) / ADR-079 (create_solid
  extrude) / ADR-033 (NURBS surfaces — BezierPatch)

## 1. Context

24-도구 폭 확장에서 사용자가 지정한 3개 도구(**3P-Plane → NURBS surface → Wall**)를
검토. de-risk(3-agent workflow + 직접 코드 확증)가 **Pattern-12 확정** — 세 도구 모두
이미 완전 구현 + 메뉴/툴바 노출 + MenuBar dispatch 작동. ADR-220(Sweep/Loft) /
ADR-221(Hole/Window)과 **정확히 동일한 discoverability 갭** (CommandCatalog +
ActionCatalog identity 미등록).

## 2. De-risk findings (Pattern-12)

| 도구 | ToolManager | MenuBar dispatch | index.html 메뉴+툴바 | 엔진 경로 | CommandCatalog | ActionCatalog |
|---|---|---|---|---|---|---|
| **plane** (3P-Plane) | ✅ :423 | ✅ :401 | ✅ 1967/2186 | 3점 → `lockPlane` (ADR-166) | ❌ | ❌ |
| **wall** | ✅ :425 | ✅ :402 | ✅ 1965/2189 | `drawRectAsShape` → `createSolidExtrude` (ADR-079) | ❌ | ❌ |
| **nurbs** | ✅ :443 | ✅ :410 | ✅ 1956/2220 | `createBezierPatch` (`BezierPatch` kernel-native, ADR-033) | ❌ | ❌ |

- **세 도구 모두 스텁 아님** (코드 확증):
  - `DrawPlaneTool` — 3-click → degenerate 가드(`normal.lengthSq() < 1e-9`) → `lockPlane({origin, normal, up, source:'manual'})` + preview 삼각형 + snap + Escape cancel.
  - `DrawWallTool` — baseline → `drawRectAsShape` → `getShapeFaceIds` → `createSolidExtrude` (ADR-079/087).
  - `DrawNurbsTool` — 2-click 사각형 → 4×4 control grid bicubic patch → `createBezierPatch` → `AnalyticSurface::BezierPatch` kernel-native face (메타-원칙 #14, 렌더 ADR-038 P23 tessellation).
- 갭의 실질: **Command Palette(Cmd-K) + 키보드 도움말 미노출 + AC ⊇ CC(ADR-133) 외부**.
  메뉴/툴바로는 이미 발견·사용 가능.
- **추가 drift 발견**(본 ADR scope 외, 별도 follow-up): `pie`, `rotrect`도 동일하게
  CommandCatalog/AC 누락 (작동은 함).

## 3. Decision — 사용자 지정 3개 discoverability closure (Scope A)

ADR-220/221과 **1:1 mirror**인 discoverability closure. 엔진 0, 신규 도구 0.

- **CommandCatalog** (`web/src/commands/AxiaCommands.ts`) +3:
  `tool-plane` / `tool-wall` / `tool-nurbs` (group=`modify`, 단축키 없음).
- **ActionCatalog** (`packages/axia-action-catalog/src/catalog.ts`) +3:
  - `tool-plane` — `status:'ui-only'`, `aliases:{}` (pure TS `lockPlane`, 엔진 호출 0).
  - `tool-wall` — `status:'ui-only'`, `aliases:{}` (composite of existing bridge calls).
  - `tool-nurbs` — `aliases:{ bridge:'createBezierPatch', wasm:'createBezierPatch' }`.
- **CatalogConsistency.test.ts** CC count `166 → 169`.
- pie/rotrect drift는 **별도 follow-up** (사용자 결재: "drift sweep은 별도 follow-up").

## 4. Lock-ins

- **L-224-1** Scope A — 사용자 지정 3개(plane/wall/nurbs)만 등록. pie/rotrect drift는
  별도 follow-up (LOCKED #44 — 사용자 지정 의미 단위).
- **L-224-2** Pattern-12 — 세 도구 모두 이미 구현/dispatch/메뉴/툴바 완비. **엔진·WASM·도구·
  메뉴·툴바 변경 0** (ADR-046 P31 #4 additive only — identity 등록만).
- **L-224-3** `tool-plane`/`tool-wall` = `status:'ui-only'` + `aliases:{}` (plane=pure TS
  lockPlane, wall=composite). catalog.test.ts "non-stub has alias OR exempt status" 정합.
- **L-224-4** `tool-nurbs` = `createBezierPatch` bridge/wasm alias (단일 kernel-native
  capability, ADR-033). 중복 0.
- **L-224-5** AC ⊇ CC invariant(ADR-133 L-133-3) 보존 — CommandCatalog +3 ⇒ ActionCatalog
  +3 동시 등록. CatalogConsistency 강제.
- **L-224-6** dist 재빌드 필수 (`packages/axia-action-catalog` tsc — web의 import source).
- **L-224-7** 절대 #[ignore] 금지.

## 5. 회귀

- ActionCatalog +3 / CommandCatalog +3 (CC count 166 → 169).
- CatalogConsistency 3/3 (AC ⊇ CC + count 169 + AC ≥ 161) + 패키지 catalog 24/24 (self-
  consistency `CATALOG_SIZE === ALL_ACTIONS.length`) PASS.
- 엔진(axia-geo/core/wasm) / WASM export / 도구 / 메뉴 / 툴바 변경 **0**.

## 6. Lessons

- **L1** Pattern-12 4번째 누적(ADR-220 Sweep/Loft / ADR-221 Hole/Window / ADR-219 Point
  ToolManager-only → 본 ADR plane/wall/nurbs). 24-도구 폭 항목 다수가 "이미 구현됨,
  catalog identity만 누락" — de-risk-first가 신규 구현 추정을 반복 정정.
- **L2** discoverability ≠ 신규 기능. 메뉴/툴바 노출(사용자 발견 가능) ↔ Command Palette +
  AC ⊇ CC identity(SSOT 정합)는 별개 surface. 후자만 누락된 케이스가 24-도구 catalog drift의
  주 패턴.
- **L3** `status:'ui-only'`는 pure-TS/composite 도구(엔진 단일 capability 없음)의 canonical
  alias 면제 표식 — wall(composite)/plane(state-only) 정합.

## 7. 후속 (별도 ADR / follow-up)

- pie / rotrect catalog drift sweep (동일 패턴, 별도 follow-up — 사용자 결재).
- 24-도구 잔여 폭 항목 (3P-Plane/NURBS/Wall 이후 — Hole까지 완료 기준 잔여).
- NURBS surface 고급화 (rational patch / draggable control net — `createNurbsSurface`
  이미 존재, 별도 ADR).

## 8. Cross-link

- ADR-220 (Sweep/Loft discoverability — 직계 패턴 source) / ADR-221 (Hole/Window) —
  Pattern-12 + discoverability closure 1:1 mirror.
- ADR-133 (AC ⊇ CC invariant, dual catalog) / ADR-045 D1 (ActionCatalog identity SSOT).
- ADR-166 (plane lock — DrawPlaneTool commit) / ADR-079 (create_solid extrude — wall) /
  ADR-033 (NURBS surfaces, BezierPatch — nurbs) / ADR-038 P23 (surface tessellation render).
- ADR-046 P31 #4 (additive only) / ADR-210 (menu reorganization).
- 메타-원칙 #4 (SSOT) / #5 (사용자 편의·discoverability) / #6 (Preventive) / LOCKED #44
  (Complete Meaning per Merge — 사용자 지정 의미 단위 scope).
