# ADR-300 — One keystroke, one owner

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Input / UX

## 1. What was wrong

Two window keydown listeners claimed the same bare letters, in different files, so
nothing compared them and both fired on every press:

| key | claimed by | and by |
|---|---|---|
| **O** | Offset tool (`KeyboardShortcuts.ts` `TOOL_MAP`) | Component Panel (`main.ts`) |
| **J** | Slice tool (`TOOL_MAP`, `AxiaCommands`) | Constraint Panel (`main.ts`) |

One press switched tools *and* toggled a panel. `ShortcutHelpModal` documented the
collision without resolving it — it listed **O twice**, once as "Offset" and once as
"Outliner".

Two menu hints named keystrokes nothing binds: `view-components` printed **Shift+G**
(only `Ctrl+Shift+G` exists, for ungroup) and `view-home` printed **H** (bare H is the
Sphere tool; camera-home lost its binding on 2026-07-03 and the hint stayed).

## 2. Decision

**Bare letters belong to tools.** `O` for Offset is the CAD/SketchUp convention this app
already follows across its tool map, and tool keys are the ones in muscle memory. Panels
move to a modifier.

- Offset keeps bare **O**; Component Panel (Outliner) → **Shift+O**.
- Slice keeps bare **J**; Constraint Panel → **Shift+J**.
- The constraint *indicator* toggle, which held Shift+J, → **Shift+K**. Between the two,
  the panel is the documented user-facing surface (it is in `ShortcutHelpModal`) and the
  indicator appears in no menu and no help text, so the panel takes the near-miss key.
- `view-components` hint: Shift+G → **Shift+O** (the truth). `view-home` hint: dropped —
  the menu item works, it simply has no key.

Not used: `stopImmediatePropagation`. Suppressing one listener leaves both bindings in
the source for the next reader to trip over, and this codebase has already been bitten
by it (tests that re-init listeners without teardown silence each other).

## 3. Lock-ins

- **L-300-1** A keystroke has exactly one owner. Bare letters are tool keys; a panel or
  overlay takes a modifier.
- **L-300-2** `ShortcutHelpModal` and the menu `.mk` hints state what is actually bound.
  A hint for an unbound key is a defect, not decoration.
- **L-300-3** When two features want one key, the documented user-facing one keeps it and
  the undocumented/dev one moves.
- **L-300-4** Collisions are caught by a test, not by review: `KeyBindingCollisions.test.ts`
  reads `main.ts`, `KeyboardShortcuts.ts` and `index.html` and compares them.
- **L-300-5** 절대 #[ignore] 금지.

## 4. Verification

Four regressions in `KeyBindingCollisions.test.ts`: main.ts claims nothing the tool
keymap owns; no keystroke is claimed twice within main.ts; the help modal does not
advertise one bare key for two things; every `Shift+…` menu hint is really bound. The
last one initially failed on Shift+C/L/F — bound through `SHIFT_TOOL_MAP` *lookups*
rather than `e.key ===` comparisons, so the extractor could not see them; that was the
test's gap and it now folds both keymaps in.

**Mutation-checked**: putting the Component Panel back on bare O fails two of the four.

**Live** (real Chromium, production build):

```
bare O   → tool 'offset',  Outliner NOT toggled
bare J   → tool 'slice',   Constraint NOT toggled
Shift+O  → Outliner flips both ways (none → flex → none), tool stays 'select'
Shift+J  → Constraint flips both ways (none → block → none), tool stays 'select'
Shift+K  → indicator only; the panel is untouched
menu     → view-components "Shift+O", view-home no hint
```

vitest **2966 pass / 0 fail** (+4), tsc 0.

## 5. Remaining

`SplitTool` is registered and bound to **X** with no toolbar button, menu item or catalog
entry — invisible in the palette and Capability Explorer. That is a discoverability gap
rather than a collision (bare X is otherwise the extension snap), and it needs a product
call on where it belongs in the menu.

## Cross-link

ADR-292 (OSNAP re-introduction — the `K` / `Tab` handlers and the
`stopImmediatePropagation` lesson), ADR-294 (i18n guards that catch stale UI strings),
ADR-299 (the audit closure that surfaced these), ADR-046 P31 #4 (menu changes additive,
muscle memory), 메타-원칙 #4 / #5.
