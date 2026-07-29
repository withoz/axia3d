# ADR-308 — SplitTool gets a browse surface

**Status**: Accepted
**Date**: 2026-07-29
**Category**: UI / discoverability

## 1. The one tool of 57 you could not browse to

`SplitTool` is registered (`ToolManagerRefactored.ts:452`) and bound to bare **X**
(`KeyboardShortcuts.ts:510`), and the shortcut sheet lists it. It appeared in no
menu, no `CommandCatalog`, no `ActionCatalog` — so it could be *used*, but only by
someone who already knew it existed.

A sweep of all 57 registered tools found it is the only one in that state, and the
inverse sweep found nothing advertising a tool that does not exist.

## 2. Why the reachability guard passes on it

`ActionWiring.test.ts` asserts every registered tool is reachable, and it counts a
key binding as a reachability surface. That is **deliberate and mutation-verified**
— commit `553aedb` records the rationale and the test (menu + catalog removed,
`Ctrl+B` kept → passes; `Ctrl+B` removed too → fails with `['boundary']`). The
guard asks *"can this tool be invoked at all?"*, and for a key-bound tool the
answer is yes.

So this was never a guard gap. It is a **product question** — should "browsable"
be required, not just "invocable" — and the answer here is yes, on the same
grounds as the BoundaryTool fix: a tool nobody can find is a tool nobody uses.

## 3. What was added

Additive only (ADR-046 P31 #4). The **X binding is unchanged**.

| surface | change |
|---|---|
| `web/index.html` | menu item in 다듬기 · 자르기, beside Trim / Extend / Join, with an `X` hint |
| `web/src/ui/MenuBar.ts` | `case 'tool-split': setActiveTool('split')` |
| `web/src/commands/AxiaCommands.ts` | `tool('tool-split', 'split', 'modify', …, 'X', …)` |
| `packages/axia-action-catalog/src/catalog.ts` | `tool-split` entry, tier 2, `surfaces: ['menu']` |
| `web/src/i18n/en.ts` | two entries — the menu string and the catalog label |

## 4. The guards caught the interesting half

Adding the menu item first, without the MenuBar `case`, failed
`ActionWiring.test.ts` with

```
Dead menu items: clicking does nothing, and the palette/Explorer record them as
successful … expected [ 'tool-split' ] to deeply equal []
```

That is exactly the defect **ADR-299** closed, and it would have shipped as a new
instance of it. Two more caught the rest: the catalog count (187 → 188, bumped
with its reason) and the ADR-294 i18n sweep, twice — once for the menu string and
again for the catalog `label`, which the Capability Explorer renders.

**Three guards, three real catches, on a change that looked like four lines.**

## 5. Lock-ins

- **L-308-1** A menu item ships with its dispatcher `case` in the same change.
  A `data-action` with no handler is the ADR-299 dead-menu defect, not a stub.
- **L-308-2** "Invocable" and "browsable" are different properties. The
  reachability guard tests the first by design; requiring the second is a product
  decision, made per tool, not a guard change.
- **L-308-3** A new user-facing string needs **two** i18n entries when it is both
  menu markup and a catalog label — the Explorer renders the label separately.
- **L-308-4** 절대 #[ignore] 금지.

## 6. Verification

vitest **2967 passed / 1 skipped / 0 failed**, tsc 0, catalog dist rebuilt.

**Live** (real Chromium, production build): the menu item reads
`엣지 분할 (Split) X`; clicking it switches the active tool `select → split`, so it
is not a dead menu; bare `X` still switches `select → split`, unchanged.

## Cross-link

ADR-299 (the dead-menu class this nearly re-created, and the guard that caught it),
ADR-294 (the i18n guards), ADR-133 (the catalog count invariant), ADR-148 §8 (the
BoundaryTool precedent — worked on Ctrl+B, surfaced nowhere), ADR-046 P31 #4
(additive only), 메타-원칙 #5.
