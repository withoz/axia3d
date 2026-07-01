# ADR-246 — Push/Pull → "Extrude/Cut (Volume)" rename + 단축키 P↔V swap

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-046 P31 Pillar 1 (Discoverability) / ADR-045·133 (ActionCatalog SSOT)
- **Depends on**: ADR-087 K-ζ (createSolidExtrude canonical entry) / ADR-196
  (Push/Pull MoveOnly = extrude out / cut in) / ADR-045 D1 / ADR-133 (AC ⊇ CC) /
  ADR-046 P31 #4 (additive only — 본 ADR 이 명시 예외)

## 1. Context

사용자 보고: "엔진에서 extrude 명령이 메뉴에 없음". 7-agent UI surface audit +
실측 — extrude(=Push/Pull)는 모든 surface 에 이미 wired (메뉴 / 툴바 / 단축키 P /
Cmd-K / ActionCatalog → `createSolidExtrude`). 문제는 라벨에 "Extrude"/"돌출"
문자열이 0건 → 사용자가 "Extrude" 로 못 찾음.

사용자 결재 (명시 directive): Push/Pull 명령을 **"Extrude/Cut (Volume)"** 로
rename + 단축키를 **P → V** 로 변경. AxiA 에서 Push/Pull = 면을 노멀 방향으로
밀어 extrude(밖, 부피 추가) 또는 cut(안, 부피 제거) (ADR-196 MoveOnly) → "Extrude/
Cut (Volume)" 가 양방향 + 3D 부피 연산을 더 정확히 표현.

## 2. Decision

**Rename (user-facing only)**:
- 메뉴 (`web/index.html:1959`): `밀기/당기기 (Push/Pull)` → `돌출/잘라내기
  (Extrude/Cut · Volume)`.
- 툴바 tooltip (`web/index.html:2178`): → `Extrude/Cut · Volume (V)`.
- Cmd-K (`web/src/commands/AxiaCommands.ts:114`): label `'Push/Pull'` →
  `'Extrude/Cut (Volume)'`, short `'P/P'` → `'Ex/Cut'`.
- ActionCatalog (`packages/axia-action-catalog/src/catalog.ts`): label `'밀기/당기기'`
  → `'돌출/잘라내기'`, description 갱신, `aliases.legacy: ['extrude','cut','push-pull',
  'pushpull']` (back-search). dist 재빌드 (ADR-133 L-133-8).

**단축키 P↔V swap** (V 가 Select 였으므로 swap 으로 충돌 해소 — orphan 없음):
- Extrude/Cut: `P` → **`V`** (`KeyboardShortcuts.ts:476`, AxiaCommands:114, 메뉴
  배지, tooltip).
- Select: `V` → **`P`** (`KeyboardShortcuts.ts:468`, AxiaCommands:85, 툴바 tooltip
  2081). P=Pick mnemonic.

**불변 (변경 금지)**:
- Engine API 이름 (`push_pull` / `pushPull` / `createSolidExtrude`) — ADR-087/041/050
  governed. ActionCatalog aliases.bridge/wasm/mcp 그대로 (`push_pull`).
- DOM identity (`data-action="tool-pushpull"` / `data-tool="pushpull"` / tool key
  `'pushpull'` / command id `'tool-pushpull'`) — 단일 identity 유지 (ADR-045/133
  SSOT). 별도 `tool-extrude` 신설 거부.

## 3. Lock-ins

- **L-246-1** User-facing label + 단축키만 변경; engine API / DOM identity / tool
  key 불변.
- **L-246-2** 단축키 conflict 는 **swap** 으로 해소 (V↔P) — orphan 금지 (두 도구
  모두 단축키 유지).
- **L-246-3** legacy aliases (extrude/cut/push-pull/pushpull) 로 구 vocabulary
  검색 보존 (AC ⊇ CC invariant 유지 — 새 entry 0).
- **L-246-4** ADR-046 P31 #4 (additive only) **명시 예외**: rename + 단축키 변경 =
  muscle-memory 변경. 사용자 명시 directive + 본 ADR 로 정책 예외 문서화.
- **L-246-5** Extrude/Cut = canonical Push/Pull (ADR-087 K-ζ createSolidExtrude +
  ADR-196 extrude/cut 양방향).
- **L-246-6** catalog.ts 변경 → dist 재빌드 (ADR-133 L-133-8).

## 4. 회귀 / 검증

- KeyboardShortcuts.test **2건 갱신** ("V→select"/"P→pushpull" → "P→select (swap)"/
  "V→pushpull (swap)") → 41 PASS. CommandPalette 7 / CatalogConsistency (AC⊇CC) 3 /
  action-catalog D1 (alias no_collision: cut/push-pull/pushpull 충돌 0) 24 / web
  commands 26 PASS. tsc 0 errors.
- 브라우저 (real, dev server): 메뉴 "돌출/잘라내기 (Extrude/Cut · Volume)" + 배지 V /
  툴바 tooltip "Extrude/Cut · Volume (V)" / "Select (P)" / **키 V → pushpull /
  키 P → select** end-to-end 확인.
- 엔진/Rust 변경 0.

## 5. Lessons

- **L1 단축키 충돌 = swap (orphan 금지)**: 원하는 키가 점유돼 있으면 (V=Select) 그
  도구를 vacate 되는 키로 swap (Select→P, P 는 pushpull 이 비움) → 두 도구 모두
  단축키 유지. silently 뺏거나 orphan 시키지 않음. cascade(S=Scale 도 점유)는 사용자
  결재로 회피.
- **L2 muscle-memory 변경은 사용자 directive + ADR**: ADR-046 P31 #4 additive-only
  를 깨는 rename/shortcut 변경은 사용자 명시 요청을 override 조건으로 + 새 ADR 로
  문서화 (LOCKED 변경 절차 정합).
- **L3 user-facing rename vs engine identity 분리**: 라벨/단축키만 바꾸고 engine API
  (push_pull/createSolidExtrude) + DOM identity (tool-pushpull) 는 불변 → SSOT
  (ADR-045/133) + MCP capability (ADR-041) 무영향.
- **L4 audit-first**: "메뉴에 없음" → 7-surface audit 으로 *이미 있음(라벨/단축키
  문제)* 진단 → 별도 entry 신설(중복) 회피, rename 으로 정확 해소.

## 6. Cross-link

- ADR-046 P31 Pillar 1 (Discoverability) + P31 #4 (additive only — 명시 예외) /
  ADR-045 D1 (identity SSOT) / ADR-133 (AC ⊇ CC + dist rebuild) / ADR-087 K-ζ
  (createSolidExtrude) / ADR-196 (extrude/cut 양방향) / ADR-041 (MCP capability
  push_pull 불변) / ADR-240 (extrude family 로드맵). 메타-원칙 #4 (SSOT) / #5
  (사용자 편의).
