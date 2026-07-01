# ADR-226 — Explode → Ungroup Re-point (phantom tool resolution)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: 24-도구 catalog 정리 (ADR-225 follow-up — explode/text3d 역방향 latent)
- **Depends on**: ADR-225 (draw-tool drift sweep — explode/text3d 발견) / ADR-133 (AC ⊇ CC) /
  integrity guard audit 2026-05-02 Finding 3 / ADR-046 P31 #4 (menu additive / muscle memory)

## 1. Context — audit 정정

ADR-225 가 발견한 "explode / text3d 역방향 latent (CC+메뉴 엔트리인데 ToolManager 미등록)"의
follow-up. 정밀 audit 결과 **ADR-225 §7 의 "클릭 무동작" 기재가 부정확**함을 확인:

- `MenuBar.setActiveTool`(165-178)에 **integrity 가드 존재** (audit 2026-05-02 Section A
  Finding 3): `if (!toolManager.hasTool(tool)) { Toast.warning("...준비 중입니다") ; return; }`.
  explode/text3d 는 `data-action`(menu-action)이라 이 경로 경유 → 클릭 시 **"준비 중" Toast +
  return** (setTool 미호출). **dead-click 아님** — 정직하게 placeholder 로 surfaced.

정밀 audit 으로 드러난 실제 상태:

| | status (audit 전) | 실제 | 판정 |
|---|---|---|---|
| **text3d** | `'stub'` + "(Stub) not yet implemented" | text 엔진/bridge capability 0 | **정확** — 계획된 미구현 placeholder, **변경 불필요** |
| **explode** | `'ui-only'` ("works as UI tool") | 도구 클래스 0 (phantom) + 기존 `ungroup` 과 의미 중복 (분해 = 그룹 해제; ungroup 은 메뉴 1975 + 컨텍스트 2755 + dropdown 2586 + Ctrl+Shift+G 모두 작동) | **부정확** — `ui-only` 인데 도구 없음 → 'stub'/'redirect' 이어야; ungroup 과 동의어 |

ExplodeTool / Text3dTool **클래스 자체 부재** 확인(파일 0, class 정의 0, tools.set 0) → "등록"은
도구 빌드(feature work)라 본 정리 범위 밖.

## 2. Decision — explode → ungroup 재배선 (사용자 결재 Option 3)

사용자 결재: **"분해 재배선 + ungroup 단축키 유지"** — 분해(Explode)를 작동하는 ungroup 에
재배선(분해 live) + ungroup 은 Ctrl+Shift+G / 메뉴 현행 유지 → **분해·그룹 해제 동의어 공존**
(CAD 관례: Explode = Ungroup synonym). muscle memory(Ctrl+Shift+G, SketchUp 표준) 보존.

- **MenuBar** (`MenuBar.ts`): `case 'tool-explode': setActiveTool('explode')` →
  `case 'tool-explode': toolManager.executeAction('ungroup')` (tool-ungroup 패턴 답습).
- **CommandCatalog** (`AxiaCommands.ts`): `tool('tool-explode', 'explode', ...)` (mode) →
  `action('tool-explode', 'modify', '분해 (Explode · = 그룹 해제)', '분해', undefined, false,
  deps, () => deps.toolManager.executeAction('ungroup'))` (일회성 action, customExecute=ungroup).
- **ActionCatalog** (`catalog.ts`): tool-explode `status:'ui-only'` → `'redirect'` (분해 = ungroup
  동의어, executeAction('ungroup') 재배선). description 정정.
- **text3d**: **변경 없음** — 이미 정확한 `status:'stub'` placeholder, integrity 가드가 "준비 중"
  처리. 3D 텍스트는 향후 feature ADR.
- index.html 메뉴(1977)/dropdown(2597) 엔트리 **유지** (라벨 "분해 (Explode)" 그대로, 이제 작동) —
  ADR-046 P31 #4 additive 보존(제거 0).

## 3. Lock-ins

- **L-226-1** 분해(Explode) = ungroup 동의어 재배선 (분해 live, executeAction('ungroup')). ungroup
  단축키(Ctrl+Shift+G) + 메뉴 + 컨텍스트 현행 유지 (사용자 결재 Option 3, muscle memory 보존).
- **L-226-2** explode 는 일회성 action (tool mode 아님) — CC `action()` customExecute = ungroup.
- **L-226-3** AC tool-explode `status:'redirect'` (ui-only 오라벨 정정). 도구 클래스 빌드 0.
- **L-226-4** text3d 변경 없음 — 정확한 `status:'stub'` placeholder, integrity 가드 처리.
- **L-226-5** ADR-225 §7 "클릭 무동작" 기재 정정 — integrity 가드(2026-05-02 Finding 3)가 이미
  "준비 중" Toast 로 처리. dead-click 아니었음.
- **L-226-6** ADR-046 P31 #4 additive — index.html 메뉴/dropdown 엔트리 제거 0 (라벨 유지, 작동만
  복원). CatalogConsistency CC count 변화 0 (tool→action, 여전히 1 엔트리 = 172).
- **L-226-7** 절대 #[ignore] 금지.

## 4. 회귀

- CommandCatalog: tool-explode tool()→action() (count 변화 0, 여전히 172).
- ActionCatalog: tool-explode status 정정 (count 변화 0).
- CatalogConsistency 3/3 (AC ⊇ CC + count 172) + 패키지 catalog 24/24 PASS.
- 엔진/WASM 변경 0. dist 재빌드.

## 5. Lessons

- **L1** Audit-first 가 audit 자체를 정정 — ADR-225 §7 "무동작 버그" 가정이 정밀 audit(integrity
  가드 경로 추적)에서 부정확으로 드러남. menu-action 경로(MenuBar.setActiveTool hasTool 가드)와
  raw setTool 경로 구분 필수. 가정 전 dispatch 경로 끝까지 추적.
- **L2** Phantom tool ≠ dead-click — integrity 가드(2026-05-02 Finding 3)가 미등록 tool-id 를
  "준비 중" Toast 로 surface. 기존 방어선 확인 우선(메타-원칙 #6).
- **L3** 동의어 재배선 > 신규 빌드 — explode 는 ungroup 과 동의어이므로 재배선이 빌드보다
  정확/저비용. CAD 관례(Explode = Ungroup) 정합.
- **L4** status 정확성 — `'ui-only'`(works as UI tool, no engine alias) vs `'stub'`(unimplemented)
  vs `'redirect'`(synonym dispatch) 구분. 도구 없는데 'ui-only' = 오라벨.

## 6. 후속 (별도)

- text3d 3D 텍스트 기능 구현 (Text3dTool + engine text capability) — feature ADR.
- 24-도구 잔여 폭 (Window 등) / Phase 0.5 smooth hole render (ADR-092) / ADR-197 곡면 Boolean.

## 7. Cross-link

- ADR-225 (draw-tool drift sweep — explode/text3d 발견; §7 정정 대상) / ADR-224 (plane/wall/nurbs
  discoverability) / ADR-133 (AC ⊇ CC) / ADR-045 D1 (ActionCatalog identity SSOT).
- integrity guard audit 2026-05-02 Section A Finding 3 (hasTool 가드 — "준비 중" Toast).
- ADR-046 P31 #4 (menu additive / muscle memory — Ctrl+Shift+G 보존) / 메타-원칙 #6 (Preventive —
  기존 방어선 확인) / LOCKED #44 (Complete Meaning per Merge).
