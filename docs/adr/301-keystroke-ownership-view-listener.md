# ADR-301 — The view listener is a third keystroke owner

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Input / UX
**Amends**: ADR-300

## 1. ADR-300 introduced the defect it was written to remove

ADR-300 returned bare `O` to the Offset tool and bare `J` to Slice, moved the two
panels to `Shift+O` / `Shift+J`, and moved the constraint-**indicator** toggle —
which had held `Shift+J` — to `Shift+K`, "verified free".

`Shift+K` is not free. It is Back view. `KeyboardShortcuts.ts` says so in its own
comment, four lines above the code:

```ts
// Shift+F is freehand, not Front — this guard was missing, so it was
// both. Shift+K stays Back view (not in SHIFT_TOOL_MAP), which is what
// the help sheet promises with 'F / Shift+K'.
if (e.shiftKey && isShiftToolKey(key)) return;
if (key === 't') mode = 'top';
…
else if (key === 'k') mode = 'back';
```

The view listener runs under `!e.ctrlKey && !e.altKey` with exactly one Shift
escape — letters in `SHIFT_TOOL_MAP`. `K` is not one, so the listener claims both
bare `k` **and** `Shift+K`. Measured live on the merged build: one press moved the
camera `3d → back` *and* flipped the indicator canvas `none → block`.

**Why the guard shipped alongside ADR-300 did not catch it.** That test compared
`main.ts` against `TOOL_MAP` and `SHIFT_TOOL_MAP`. The view listener is a third
owner, in neither map, expressed as a `key === 'x') mode = '…'` chain rather than
an `e.key ===` comparison. The extractor could not see it, so the check passed on
a set that was missing a third of the truth. The same session's live probe asked
"did the panel toggle? did the tool change?" and never asked "did the *view*
change" — so both the automated and the manual verification looked past it.

Separately, the ADR-300 hint sweep was incomplete: `view-constraints` still
printed bare `J`, which arms the Slice tool. The guard's hint check only examined
`Shift+…` hints, so a bare-letter hint was invisible to it.

## 2. Decision

**Relocating the toggle again would move the landmine, not remove it.** The
constraint overlay draws the constraints this panel lists, so the toggle belongs
in the panel. It becomes a `👁` button in the Constraint Panel header, next to
`⟳` and `✕ ALL`, and it holds **no global key at all**.

That is a net removal of one global binding, and it makes a toggle that appeared
in no menu and no help text visible for the first time. The button is only
rendered when a `toggleVisual` callback is supplied — a button that does nothing
is the dead-menu defect of ADR-299.

`view-constraints`' hint becomes `Shift+J`.

## 3. Lock-ins

- **L-301-1** Three maps own keystrokes, not two: `TOOL_MAP`, `SHIFT_TOOL_MAP`,
  and the view listener's `t / b / f / k` — which are claimed in **both** bare and
  Shift form for every letter not in `SHIFT_TOOL_MAP`. Check all three before
  placing a binding.
- **L-301-2** A UI toggle that belongs to one panel lives in that panel, not on a
  global letter. Global key space is the scarce resource.
- **L-301-3** A menu hint must name the key that performs **that** action. "Bound
  to something" is not enough — bare `J` was bound, to the wrong thing.
- **L-301-4** A guard's extractors are asserted non-empty. An extractor that
  silently matches nothing turns every check beneath it into a pass.
- **L-301-5** 절대 #[ignore] 금지.

## 4. Verification

`KeyBindingCollisions.test.ts` is rewritten around one owner map built from all
three sources plus `main.ts`, so a contested key is reported as
`Shift+K → view:back AND main.ts` — naming both owners. The hint check now covers
bare letters and derives the *expected* owner from the action id (`tool-*` must
name a tool key; a view letter on a non-view item is the bare-`J` shape).

**Mutation-checked** — each new assertion was proven to bite:

| mutation | failure |
|---|---|
| re-add the `Shift+K` indicator binding | `these keystrokes fire more than one handler: Shift+K → view:back AND main.ts` |
| restore the bare-`J` constraints hint | `menu hints that lie: view-constraints prints J, which arms the slice tool` |

**Live** (real Chromium, production build):

```
Shift+K → view 3d → back, overlay display UNCHANGED     ONE OWNER
panel 👁 → overlay block → none → block, aria-pressed true → false → true,
           dimmed while off
menu    → view-constraints hint "Shift+J"
```

vitest **2967 pass / 0 fail** (+1), tsc 0.

## 5. What this says about the previous fix

ADR-300's finding was right and its principle stands. What failed was the
verification: a guard whose input set was incomplete, and a manual probe that
checked the two surfaces I had just been thinking about. A guard is only as good
as its extractors, which is why L-301-4 now asserts they found something.

## Cross-link

ADR-300 (the fix this amends), ADR-299 (dead menus — the "a button that does
nothing" rule), ADR-294 (the i18n guard, which caught the new string and a Korean
comment inside a CSS template), ADR-292 (`stopImmediatePropagation` is still not
the answer), ADR-046 P31 #4, 메타-원칙 #4 / #5 / #6.
