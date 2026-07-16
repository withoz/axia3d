# ADR-294 — i18n: Korean + English (α design + infrastructure)

- **Status**: Accepted (α design + infrastructure + first slice; bulk migration batched)
- **Date**: 2026-07-16
- **결재**: 사용자 "다국어를 지금 할까 : 진행합니다"
- **Cross-link**: ADR-046 Q7 (한국어 + 영어, Phase 2) · ADR-035 P20.C #2 (initial
  bundle 0MB) · ADR-129 §3.1 (i18n listed as not started) · ADR-095 §E L3 /
  ADR-100 L7 (humanize at boundary)

---

## 1. Scope, measured

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

## 3. Rollout

Not one pass. In batches, each its own commit, each independently green:

1. **Batch 1** — infrastructure + `humanizeEngineError` as the first slice
   (self-contained, entirely user-facing, and already the funnel every engine
   error goes through), + drift guards. `c6dda5f`.
1b. **Locale switch** (D7) + the D6 correction. Auto-detect made this urgent:
   until a user can choose, an English browser gets English errors in an
   otherwise-Korean UI.
2. `ui/` panels and modals — the visible surface.
3. `commands/AxiaCommands.ts` + `ui/MenuBar.ts` + `ui/CommandRegistry.ts` — the
   45%, mechanical label/description pairs. Watch D6 here.
4. `tools/` Toasts.

English translation of each batch is a separate concern from wrapping it: a
wrapped-but-untranslated string renders Korean, which is exactly today's
behaviour, so batches can land before their translations do.

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
- **L-294-6** Locale = `navigator.language`, overridable via `setLocale`,
  persisted at `axia:locale`. Default Korean.
- **L-294-7** Migration is batched and additive; an unmigrated string keeps
  working unchanged.
- **L-294-8** 절대 #[ignore] 금지.
