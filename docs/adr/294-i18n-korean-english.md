# ADR-294 — i18n: Korean + English (α design + infrastructure)

- **Status**: Accepted (13 batches closed, survey 0 — §3.1 Acceptance)
- **Date**: 2026-07-16 (α) / 2026-07-17 (rollout closure — §3.1)
- **결재**: 사용자 "다국어를 지금 할까 : 진행합니다"
- **Cross-link**: ADR-046 Q7 (한국어 + 영어, Phase 2) · ADR-035 P20.C #2 (initial
  bundle 0MB) · ADR-129 §3.1 (i18n listed as not started) · ADR-095 §E L3 /
  ADR-100 L7 (humanize at boundary)

---

## 1. Scope, measured

> **§1 correction (batch 2).** The count below is TS string literals, and on
> that basis I called the command/menu catalogs "the app's chrome". They are
> not. The chrome — the menu bar, the toolbars and their tooltips — lives as
> **static markup in `index.html`**, which this count excluded entirely: 344
> Korean text nodes (306 unique) and 44 Korean `title` tooltips. The 64 Hangul
> literals in `ui/MenuBar.ts` are its handlers' toasts, not its labels. See D8.

ADR-129 and my own earlier note said "~3,271 Hangul literals across 182 files".
That counted **comments**, which this codebase has a great many of and which are
not user-facing. Measured properly — Hangul inside string literals only,
excluding comment lines, tests and generated `src/wasm/`:

**1,731 literals across 98 files.** Concentrated:

| file | count |
|---|---|
| `tools/ToolManagerRefactored.ts` | 298 |
| `commands/AxiaCommands.ts` | 263 |
| `ui/CommandRegistry.ts` | 74 |
| `ui/ShortcutHelpModal.ts` | 74 |
| `ui/MenuBar.ts` | 64 |
| **top 5** | **773 (45%)** |

The command/menu catalogs dominate, and they are label/description pairs — the
most mechanical possible migration. That shape is what makes a batched rollout
realistic rather than a rewrite.

## 2. Decisions

### D1 — In-house `t()`, not a framework

**ADR-035 P20.C #2 makes initial bundle growth a locked constraint.** i18next is
~40 kB minified; the whole of `i18n/` here is under 100 lines. The repo's own
idiom is a small SSOT module (`AutoIntersectSettings`, `DrawCurveSettings`,
`MergeSettings`…), and there are currently zero i18n dependencies. A framework
would buy plural rules and lazy namespaces we do not need for two languages.

Measured cost, built with and without: the machinery is **+1.67 kB** (gzip
+0.52). The `en.ts` table is the part that grows — batch 2's 350 strings cost
**+18.2 kB** (gzip +7.6). That is kB, not MB, so it is within P20.C #2 today,
but it is worth naming the trend: at ~2,000 strings the table lands near
+100 kB, and a Korean user never needs a byte of it. The exit is a lazy chunk —
`main.ts` awaits the table when, and only when, the locale is English. Not worth
doing at 350 strings; worth doing before the table is finished. Deliberately not
done now, so the change lands with a number behind it rather than a guess.

### D2 — The Korean source text IS the key

Not `toast.pushpull.tooShort`. `t('돌출 거리가 너무 짧습니다')`.

Inventing 1,731 stable key names is the expensive half of an i18n migration, and
it is the half that produces nothing a user can see. Source-as-key means:

- **`ko` needs no locale file at all** — it is the identity function. Half the
  translation work does not exist.
- **Migration is a wrap**, not a rewrite: `'…'` → `t('…')`. The Korean stays
  legible at the call site, so the code does not get worse to read.
- **Untranslated = Korean**, automatically. There is no "missing key" state that
  renders `toast.pushpull.tooShort` at a user.

The cost is honest and worth naming: **editing the Korean silently orphans the
English.** That is what `i18n/en.ts` being a plain object checked by a test is
for — an orphaned entry is dead weight the guard can find, and a missing one
falls back to Korean rather than breaking.

### D3 — `{param}` interpolation

Many strings carry runtime values: `` `두께 ${limit}mm 에서 멈췄습니다` ``. With
source-as-key an interpolated template has no stable key — the key would vary
with the value. Those become `t('두께 {limit}mm 에서 멈췄습니다', { limit })`.

Chosen over `${}`-preserving cleverness because the placeholder must survive
translation: an English translator needs `Stopped at {limit}mm thickness`, and
`{limit}` is the only part they must not touch.

### D4 — Locale: `navigator.language`, overridable, persisted

Mirrors the repo's Settings-module idiom exactly (`getLocale` / `setLocale` /
localStorage `axia:locale`). Default = Korean unless the browser says otherwise,
because that is the current behaviour and this must not surprise existing users.

### D5 — `innerHTML` templates keep working

`t()` returns a string, so `` `<h2>${t('AXiA 3D 정보')}</h2>` `` needs nothing
new. No template compiler, no DOM directive.

### D6 — Module-scope constants are safe (measured; the α draft was wrong)

**The α draft claimed** a `t()` at module load "freezes whatever locale was
current at import time — before main.ts runs", and concluded every module-level
catalog "must become getters rather than constants". That was reasoned from the
spec, not measured, and **it is wrong**.

ES modules evaluate depth-first: `i18n/index.ts`'s body — including its
`detect()` — finishes before the body of any module that imports it. So a
module-scope `t()` already sees the persisted locale.

Measured, not asserted: `i18n/__fixtures__/moduleScope.ts` is a module that
calls `t()` at module scope, and `i18n.test.ts` imports it under each persisted
locale. The load-bearing case is Korean — jsdom reports
`navigator.language = 'en-US'`, so only `ko` proves the persisted choice beat
the browser default rather than coinciding with it. Mutation-verified: making
`detect()` ignore localStorage fails the test.

This matters because it is the difference between the bulk migration being a
wrap and being a restructure. **A module-level catalog can be wrapped in place.**

What module scope genuinely cannot do is follow a *runtime* switch — which
leads to D7.

### D7 — Switching the language reloads the page

Measured: `initMenuBar`, `registerAxiaCommands` and `initCommandRegistry` are
all init-once functions, and the panels build their `innerHTML` once. Switching
locale live would repaint nothing — the user would get new error toasts in
English over a menu bar still in Korean. That mixed state is worse than either
language alone.

So `SettingsPanel` sets the locale and calls `location.reload()`, and says so in
the hint. This is what VS Code does for the same reason, and it is what makes D6
safe: after a reload, module scope re-reads the persisted choice.

**Auto-detection made this urgent.** `detect()` honours `navigator.language`, so
the moment the first slice landed, a user on an English browser saw English
engine errors inside an otherwise-Korean UI. A switch the user controls is not a
nicety here; it is what keeps the feature coherent while the migration is
partial.

### D8 — Static markup translates in place at boot; the DOM already holds the keys

The chrome is static markup in `index.html`. The obvious options were to add
`data-i18n` attributes to 350 elements, or to move the menus into TS. Neither is
needed: **source-as-key means the DOM already contains the keys.** So
`translateDom()` walks the markup once at boot and rewrites it in place —
`index.html` is untouched.

Two details that decide the implementation:

- It translates **text nodes, not `textContent`**. A menu row is
  `<div class="menu-action">새로 만들기<span class="mk">Ctrl+N</span></div>`;
  `textContent` would fuse label and shortcut into one unkeyable string, and
  writing it back would destroy the span.
- It runs **before any panel is constructed**, so its scope is exactly the
  static markup. Panels build their own DOM from TS and re-render, so a
  boot-time sweep would only paint over them until their first repaint. They
  get `t()` in their own batch.

Two things the markup forced out into the open:

- **Keys are what the DOM holds, not what the markup spells.** `index.html`
  writes `&#9633; 직사각형`; the key is the decoded `□ 직사각형`. The guard
  therefore *parses* index.html rather than reading it as text.
- **A sentence split across nodes reorders.**
  `재질을 부여하면 이 객체는 <strong>XIA (특성)</strong>로 승격됩니다` is three
  text nodes, and English puts the strong in a different place. The trailing
  fragment's correct translation is `''` — which exposed a real bug: `t()` used
  `(table[key] || key)`, so an empty translation fell back to Korean and the
  sentence rendered half-translated. It now checks for `undefined` explicitly
  (L-294-10).

## 3. Rollout

Not one pass. In batches, each its own commit, each independently green:

1. **Batch 1** — infrastructure + `humanizeEngineError` as the first slice
   (self-contained, entirely user-facing, and already the funnel every engine
   error goes through), + drift guards. `c6dda5f`.
1b. **Locale switch** (D7) + the D6 correction. Auto-detect made this urgent:
   until a user can choose, an English browser gets English errors in an
   otherwise-Korean UI. `b3cdcde`.
2. **The static chrome** (D8) — `index.html`'s 306 text nodes + 44 tooltips,
   plus `translateDom` and its drift guard. This is the whole menu bar and
   toolbar, i.e. what the app looks like. Done.
3. `ui/` panels and modals. **§3 correction:** I wrote that the 280 leftover
   nodes were "all of them TS-built panels". Measured per-container, **210 of
   the 280 are inside one panel — the Capability Explorer — and they are not
   its chrome at all. They are ActionCatalog labels**, i.e. catalog *data*
   rendered in a panel, which belongs with batch 4. The panels' own chrome is
   ~70 nodes: Settings 28, XiaInspector's material section 12, console 6, and
   1–3 each across the rest. Every panel is hidden until opened, so the default
   view stays fully English throughout this batch. `SettingsPanel` done — it is
   where the language switch lives, so a Korean panel there was the sharpest
   version of the mixed-UI problem.
4. The catalogs — `packages/axia-action-catalog` (214 labels, rendered by the
   Capability Explorer) and `web/src/commands/AxiaCommands.ts` (190 labels +
   190 shorts, rendered by the Command Palette). **349 unique Korean strings
   across the two.** ActionCatalog must NOT import `t()`: the MCP server (Node)
   reads that package, and pulling `web/src/i18n` into it would invert the
   layering. Source-as-key makes that unnecessary — the *panels* call
   `t(action.label)` at render and the catalog stays pure data. 32 of the
   Explorer's 206 labels are already translated, because they are the same
   Korean strings as the menu's.

   Measured on the way past, out of scope here: of the 190 ids both catalogs
   define, **only 87 (45%) carry the same label** ('선형 배열' vs
   '선형 배열 (Array Linear)…'). LOCKED #61 guards the ids, not the fields;
   field-level drift is ADR-134's problem, not i18n's.
5. `tools/` Toasts. `891fc24`.

English translation of each batch is a separate concern from wrapping it: a
wrapped-but-untranslated string renders Korean, which is exactly today's
behaviour, so batches can land before their translations do.

### 3.1 What actually landed (2026-07-16/17)

Thirteen batches, survey to 0. The plan above stopped at 5; the rest is what
the measurements led to rather than what I predicted.

| # | scope | commit |
|---|---|---|
| 1 | infrastructure + `humanizeEngineError` + drift guards | `c6dda5f` |
| 1b | locale switch (D7) + the D6 correction | `b3cdcde` |
| 2 | index.html static chrome (D8) + `translateDom` | landed |
| 3 | SettingsPanel (where the language switch lives) | `dfdd78b` |
| 3 | F1 shortcut sheet | `5f00c37` |
| 3b | VCB, status bar, XiaInspector — what you touch while modelling | `24fef03` |
| 4 | both catalogs + D9 transliteration | `6f6e498` |
| — | D10: material names are data, not keys — guarded | `0366392` |
| 5 | everyday tool Toasts | `891fc24` |
| 6 | menu + right-click feedback (deleted a help screen that was lying) | `f3bd419` |
| 7 | ToolManagerRefactored — the biggest single file | `5f31326` |
| 9 | the panels you have to open to find | `20b5fae` |
| 10 | command line | `50eebc0` |
| 11 | file I/O | `caf89a0` |
| 12 | the long tail — **and the guard becomes the only survey** | `91083b3` |
| 13 | the status bar (see below) | `8289b1e` |

Three corrections along the way, each because a guard or a browser disagreed
with what I had written:

- **`2261e75`** — the Capability Explorer was never wired; the previous commit
  claimed it was.
- **`7c90fcd`** — the guard only read single quotes, so 185 backtick templates
  sat in files the ledger called done.
- **`1671d33`** — a Korean lookup table (`GEOMETRY_STATES`) read without `t()`
  renders Korean in every locale. It lives in one file and is rendered by
  another, so three guards could not see it.

### 3.2 Two ways "survey: 0" lied

The number means "no raw Korean **of the shapes the scanner knows**". Twice
that was not the same as done:

- **batch 13 (`8289b1e`)** — `TOOL_DISPLAY_NAMES` was hard-coded *English*. A
  scanner hunting Korean finds nothing in an English string, so 54 tool names +
  7 view names scanned clean while a Korean user clicked 「사각형」 and the
  status bar answered "Rectangle". Found by clicking, not by scanning. 52 of
  the 64 keys already existed in en.ts — a tool name is the same word the menu
  uses, which is also why reusing the key is what keeps them in sync.
- **`63df586`** — 16 `return '한글'` inside `humanize*()`. The survey passes
  anything that HAS an en.ts key and all 16 did; the sink guard reads
  `Toast.error('한글')`, but here the Korean is assembled one call further out
  (`Toast.error(humanizeBoundaryError(e))`). Invisible to every guard.

Both now have guards: `DISPLAY_MAPS` names the maps whose values reach the
screen and requires `t()`, and the sink list reads `return '한글';`.

### 3.3 The locale trap, twice

`detect()` falls back to `navigator.language`. jsdom reports en-US, so 19
vitest files asserting Korean copy were reading raw literals and passing by
accident; they are pinned with `setLocale('ko')`.

The same trap caught Playwright, but only on CI (`348b8ce`). A dev machine
carries an `axia:locale` from clicking around; a clean runner does not — so
the two were running different apps, and specs asserting Korean passed locally
and failed on CI. `playwright.config` now pins `ko-KR`.

`tool-label-in-statusbar` is the sharp version: it asserted `#tool-label` reads
"Select" / "Rectangle" / "Work Plane" — the exact strings batch 13 changed —
and passed through that commit without noticing, because en-US made the old
values true.

### D9 — Hard CAD terms use the transliteration in Korean too (사용자 결재 2026-07-16)

> 사용자: "어려운것은 영어발음을 사용합니다."

This is a change to the **Korean** UI, not a translation decision, and it is
user-visible. It is recorded here because i18n is what surfaced it: writing 266
English labels put every Korean label side by side, and the inconsistency was
impossible to miss.

The convention already existed — 로프트, 스윕, 스플라인, 테이퍼, 오프셋, 미러,
스냅 are all transliterations. The translated ones were the deviation.

"Difficult" is defined as: **the Korean is a coined literal or a descriptive
phrase that a CAD user would not actually say.** Everyday Korean stays Korean —
선, 원, 사각형, 이동, 회전, 삭제 are not renamed.

| was | now | why |
|---|---|---|
| 모깎기 | 필렛 | coined; users say 필렛 |
| 모따기 | 챔퍼 | coined |
| 코너 둥글리기 | 코너 필렛 | coined |
| 홈파기 | 포켓 | coined |
| 매끄럽게 분할 | 서브디비전 | a description, not a term |
| 두께 부여 | 셸 | a description, not a term |
| 선 병합 | 조인 | a description, not a term |
| 면 합치기 / 면 통합 / 기하 병합 | 면 머지 / 기하 머지 | **three Korean words for one concept** |
| 자르기 (Trim) | 트림 | 자르기 vs 잘라내기(Cut) read the same |
| 연장 (Extend) | 익스텐드 | pairs with 트림 |

**Substring replacement would have broken two things**, both found by reading
the occurrences rather than trusting the term list:

- `연장 (Extend)` is the tool, but **`연장선` and `연장(X)` are snap modes**
  (extension). Renaming the substring would have renamed the snap panel.
- `자르기 (Trim)` is the tool, but **`평면으로 자르기` is Slice.**

So the renames are full-label, and the two bare labels are matched only when
quote- or tag-delimited.

Source-as-key made this safe rather than dangerous: changing the Korean changes
the key, so every rename had to move its `en.ts` entry with it — and the orphan
guard fails on any that did not. It also caught a real collision: `선 병합` and
its palette abbreviation `선병합` both became `조인`, because a transliteration
is already short enough not to need an abbreviation. One key, one English.

**ADR-046 P31 #4** says menu changes are additive only, muscle-memory-preserving.
A label rename is a muscle-memory change, so it needs a decision on the record:
this is it. Positions, ids, shortcuts and toolbar order are untouched — only the
words change.

### D10 — Material names are data, not keys (사용자 결재 2026-07-16)

> 사용자: "재질은 사용자가 직접 임의 입력도 가능하기때문에 재질은 사용자 입력에
> 맞춥니다."

The 12 built-in materials render as `콘크리트 (Concrete)`, `철강 (Steel)`,
`유리 (Glass)`. They are **not** translated, and this is the reason to write it
down rather than leave it to whoever runs the next batch — the strings sit right
next to panel chrome and look exactly like something a batch should sweep up.

Measured, and it turns out there is a stronger reason than the one given:

- **Users author them.** `MaterialLibrary.addCustom` takes any name; Quick Color
  mints one per use. A translated built-in list next to a user's own names is a
  list in two voices.
- **They are persisted.** `FileManager` saves `getCustom()` into
  `metadata.materials` and restores by the same object on load. Translating them
  would mean a material's name depends on the language it was saved in, and a
  file saved in English would restore English names into a Korean session. A
  name that round-trips through a file is data, whatever it looks like on screen.

Guarded, not just documented: `i18n.test.ts` asserts no built-in material name
is an `en.ts` key. The next batch cannot translate them by reflex.

**Open, deliberately not done:** Quick Color mints `색상 #ff0000`. It is a
generated default rather than user input, so arguably it should be born in the
locale of whoever creates it — and then stay data forever after. That is a
different decision from this one and is left alone.

## 4. Lock-ins

- **L-294-1** No i18n dependency. ADR-035 P20.C #2.
- **L-294-2** Korean source text is the key; `ko` is identity and has no file.
- **L-294-3** Unknown/untranslated key → return the key (Korean). Never throw,
  never render a key name at a user.
- **L-294-4** Interpolation is `{name}`, not `${}` — the placeholder must be
  translatable text.
- **L-294-5** `t()` resolves the locale at CALL time. Module-scope constants are
  safe to wrap in place — measured, not assumed (D6). The α draft's "must become
  getters" is superseded.
- **L-294-9** Switching the locale reloads the page (D7). A live switch would
  leave every init-once catalog stale.
- **L-294-10** An empty translation is honoured, not treated as missing — a
  sentence split across DOM nodes reorders in English and its trailing fragment
  has no counterpart (D8).
- **L-294-11** `translateDom` covers static markup ONLY, and runs before any
  panel is built. TS-built DOM re-renders, so it must be wrapped with `t()`
  rather than swept (D8).
- **L-294-13** Hard CAD terms are transliterated in Korean (D9). "Hard" = a
  coined literal or a descriptive phrase; everyday Korean stays Korean.
- **L-294-15** Material names are data, not keys (D10) — user-authored and
  persisted to the .axia file. Guarded: no built-in material name may be an
  `en.ts` key.
- **L-294-14** Renames are full-label. `연장선`/`연장(X)` (snap) and
  `평면으로 자르기` (Slice) are homographs a substring pass would destroy.
- **L-294-12** Keys are what the DOM holds, not what the markup spells
  (`&#9633;` → `□`). Guards parse `index.html`; they never read it as text.
- **L-294-6** Locale = `navigator.language`, overridable via `setLocale`,
  persisted at `axia:locale`. Default Korean.
- **L-294-7** Migration is batched and additive; an unmigrated string keeps
  working unchanged.
- **L-294-8** 절대 #[ignore] 금지.
