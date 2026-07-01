# ADR-225 — Pie / RotRect / Spline draw-tool Discoverability Sweep

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: 24-도구 catalog drift sweep (ADR-224 follow-up)
- **Depends on**: ADR-224 (3P-Plane/Wall/NURBS discoverability) / ADR-220 (Sweep/Loft
  + catalog drift restoration) / ADR-133 (AC ⊇ CC invariant) / ADR-186 (24-tool toolbar phases)

## 1. Context

ADR-224 closure 시 발견한 추가 catalog drift(pie / rotrect)의 follow-up. 정식
audit(ToolManager `tools.set` ↔ CommandCatalog `tool()` diff)로 **동일-패턴 drift 전체
집합**을 확정한 결과 — pie / rotrect 외 **spline(DrawSplineTool, ADR-186 Phase 2)**도 같은
패턴(메뉴+툴바 존재, 카탈로그만 누락). 사용자 결재 Scope A — 3개 일괄 sweep(완결성).

## 2. Audit — ToolManager ↔ CommandCatalog diff

ToolManager 등록 도구 중 CommandCatalog 누락 = {boundary, group, pie, rotrect, spline, split}.
각 성격 분류:

| tool | ToolManager | 메뉴/툴바 | CommandCatalog | 판정 |
|---|---|---|---|---|
| **pie** | ✅ | ✅ (data-action 1884 / toolbar 2160) | ❌ | **draw-tool drift → 등록** |
| **rotrect** | ✅ | ✅ (1879 / 2137) | ❌ | **draw-tool drift → 등록** |
| **spline** | ✅ | ✅ (1889 / 2118) | ❌ | **draw-tool drift → 등록** (추가 발견) |
| group | ✅ | ✅ | ✅ (`action('group')` AxiaCommands:196) | 이미 cataloged — drift 아님 |
| boundary / split | ✅ | ❌ (메뉴 미배선) | ❌ | 내부 도구 — 별개 카테고리, 제외 |

**역방향 latent (본 sweep scope 밖)**: `tool-explode` / `tool-text3d` 는 CommandCatalog +
메뉴에 있으나 ToolManager `tools.set`에 미등록 → 클릭 무동작. 선재 이슈(별도 — 도구 등록
또는 CC 엔트리 제거 결정 필요).

## 3. Decision — pie + rotrect + spline 등록 (Scope A)

ADR-224와 동일 discoverability closure. 엔진 0, 신규 도구 0.

- **CommandCatalog** (`AxiaCommands.ts`) +3: `tool-rotrect` / `tool-pie`(shortcut 'I' 표시) /
  `tool-spline` (group=`draw`).
- **ActionCatalog** (`catalog.ts`) +3:
  - `tool-rotrect` — `status:'delegated'`, `aliases:{}` (composite draw, drawRectAsShape; cf. tool-polygon).
  - `tool-pie` — `status:'delegated'`, `aliases:{}`, `surfaces:['menu','keyboard']` (단축키 I).
  - `tool-spline` — `aliases:{ bridge:'drawBSplineWithCurve', wasm:'drawBSplineWithCurve' }` (dedicated, cf. bezier/arc).
- **CatalogConsistency.test.ts** CC count `169 → 172`.

## 4. Lock-ins

- **L-225-1** Scope A — pie + rotrect + spline (동일-패턴 draw-tool drift 완전 sweep). spline은
  audit 추가 발견(사용자 결재 A로 포함).
- **L-225-2** Pattern-12 — 세 도구 모두 이미 구현/dispatch/메뉴/툴바 완비(DrawRotRectTool /
  DrawPieTool / DrawSplineTool, ADR-186 phases). **엔진·WASM·도구·메뉴·툴바 변경 0**
  (ADR-046 P31 #4 additive — identity 등록만).
- **L-225-3** pie `shortcut:'I'` (CC 표시용; 실제 dispatch SSOT = KeyboardShortcuts keyMap
  `'i':'pie'`). rotrect/spline 단축키 없음.
- **L-225-4** rotrect/pie = `status:'delegated'` + `aliases:{}` (composite draw, tool-polygon
  선례). spline = `drawBSplineWithCurve` bridge/wasm alias (dedicated curve, bezier/arc 선례).
- **L-225-5** AC ⊇ CC invariant(ADR-133) 보존 — CC +3 ⇒ AC +3 동시. CatalogConsistency 강제.
- **L-225-6** dist 재빌드 필수.
- **L-225-7** group/boundary/split/explode/text3d는 본 sweep 제외(각각 별개 사유 — group은
  이미 cataloged, boundary/split 미배선, explode/text3d 역방향 latent).
- **L-225-8** 절대 #[ignore] 금지.

## 5. 회귀

- ActionCatalog +3 / CommandCatalog +3 (CC count 169 → 172).
- CatalogConsistency 3/3 (AC ⊇ CC + count 172 + AC ≥ 161) + 패키지 catalog 24/24
  (self-consistency) PASS.
- 엔진(axia-geo/core/wasm) / WASM export / 도구 / 메뉴 / 툴바 변경 **0**.

## 6. Lessons

- **L1** "sweep" = 완결성 — 사용자가 2개(pie/rotrect)를 지목했으나 audit-first가 동일-패턴
  3번째(spline)를 노출. 부분 정리보다 동일-패턴 일괄 sweep이 drift 재발 방지.
- **L2** ToolManager `tools.set` ↔ CommandCatalog `tool()` toolName diff가 drift 탐지의
  canonical 방법. AC ⊇ CC(CatalogConsistency)는 CC→AC 방향만 강제하므로, **양쪽 모두 누락한
  working tool**은 별도 diff로만 탐지됨.
- **L3** 역방향 drift(CC 엔트리 + tool 미등록 = explode/text3d)도 동일 diff로 노출 — 클릭
  무동작 latent bug. 별도 트랙(도구 등록 vs CC 제거).

## 7. 후속 (별도)

- **explode / text3d 역방향 latent** — `tool-explode`/`tool-text3d` CC+메뉴 엔트리가 가리키는
  toolName이 ToolManager에 미등록 → 클릭 무동작. ExplodeTool/Text3dTool 등록 또는 CC/메뉴
  엔트리 제거 결정(별도 ADR/audit).
  > **정정 (ADR-226, 2026-06-23)**: "클릭 무동작"은 **부정확**. `MenuBar.setActiveTool`
  > (165-178)의 integrity 가드(audit 2026-05-02 Finding 3)가 `hasTool` false → "준비 중"
  > Toast + return 으로 **이미 정직하게 처리** (dead-click 아님). 실제 상태: text3d=정확한
  > `status:'stub'` placeholder(text capability 0, 변경 불필요) / explode=`status:'ui-only'`
  > 오라벨(도구 없음 + 기존 `ungroup`과 동의어 중복). ADR-226 이 explode→ungroup 재배선
  > (분해 live) + status `ui-only`→`redirect` 정정. 자세히는 ADR-226.
- 24-도구 잔여 폭 (Window 등) / Phase 0.5 smooth hole render / ADR-197 곡면 Boolean edge case.

## 8. Cross-link

- ADR-224 (3P-Plane/Wall/NURBS discoverability — 직계 follow-up, 동일 패턴) / ADR-220
  (Sweep/Loft + catalog drift restoration) — discoverability closure 패턴 source.
- ADR-133 (AC ⊇ CC invariant) / ADR-045 D1 (ActionCatalog identity SSOT).
- ADR-186 (24-tool toolbar — DrawRotRectTool/DrawPieTool/DrawSplineTool phases).
- ADR-046 P31 #4 (additive only) / 메타-원칙 #4 (SSOT) / #5 (discoverability) / #6 (Preventive) /
  LOCKED #44 (Complete Meaning per Merge).
