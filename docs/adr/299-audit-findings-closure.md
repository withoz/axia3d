# ADR-299 — Audit findings closure: dead menus, dishonest tests, a missed gate

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Wiring / Test integrity / Kernel

Closes the three items the 2026-07-28 engine-wide audit left verified but unfixed.
Each was re-measured before being touched.

## 1. Four dead Window menu items — and the audit trail that called them successes

Three View items read globals nothing ever assigned; a fourth pointed at a class that
does not exist.

| item | global | reads / writes | reachable only by |
|---|---|---|---|
| `view-components` | `__axia_componentPanel` | 1 / **0** | bare `O` |
| `view-xia-inspector`, `view-materials` | `__axia_xiaInspector` | 2 / **0** | bare `I` |
| `view-sun-panel` | `__axia_sunPanel` | 1 / **0** | nothing — no `SunPanel` class exists |

Control: `__axia_constraintPanel` (6/1) and `__axia_scenes` (2/1) *do* get writers.

**What hid it**: `dispatchMenuAction` returned `true` as soon as it found a
`[data-action]` element and clicked it, so the Capability Explorer logged
`result:'ok'` — "Launched (menu dispatch)" — for a handler that had already bailed.
The same class as ADR-069 one layer lower: that fix covered *unknown* ids, not ids
whose handler cannot proceed.

**Fixed**: `initXiaInspector` returns a handle and `main.ts` exposes both globals beside
the existing `__axia_constraintPanel` writer. `view-sun-panel` is removed from menu,
palette and catalog together — a panel needs the class first, then the entry.
`MenuBar` gains `markActionUnavailable` / `consumeActionUnavailable`; the flag resets at
the start of each dispatch and the dispatcher reports `!consumeActionUnavailable()`.
`dispatchMenuAction` moved to `ui/dispatchMenuAction.ts` so its contract is testable
without importing `main.ts` (which boots the app).

## 2. Three Playwright specs that passed unconditionally

`adr-092/093/094-demo.spec.ts` imported only `{ test }` and had **zero `expect`, zero
`throw`** — they ran full engine flows and `console.log`ged the results. ADR-093 in
particular computed `siblingCount` and `selectedCount` and never compared them, while
**LOCKED #93 cites "siblings=23 → selectedCount=23" as a gate this file proved**.

Adding real assertions made **two of the three fail immediately** — and the failures
were informative:

- **ADR-093** was not an engine regression: the spec builds its cylinder through
  `drawCircleAsCurve` + extrude, and with Path B now the production default
  (ADR-094/117) that is a 3-face solid whose side is ONE face, so `siblings = 1` is the
  correct answer. The "23" figure describes Path A. The spec now pins Path A
  (`setCylinderPathBDefault(false)`) so the grouping under test has more than one
  member — measured 23, exactly the LOCKED #93 figure — and asserts
  `selectedCount === siblingCount`.
- **ADR-092** failed on a stale variable name in my own new assertion; the underlying
  demo was sound (multi-segment arc rim confirmed).
- **ADR-094** passed unchanged: Path B 3/2/3 vs Path A, 88% face reduction.

Two Rust tests whose names outran their bodies were also corrected:

- `pushpull_result_is_closed_solid` asserted only that each active face had a non-null
  outer-loop pointer — `is_closed_solid` appeared nowhere in the file except the
  function's own name. It now calls `verify_outward_normals()` and
  `collect_non_manifold_edges()`.
- `adr262_door_box_wall_manifold` could not see a boundary edge, so an unwelded
  jamb seam — what LOCKED #86 claims is welded — would have passed. Measuring showed
  the notch leaves a **fully closed solid** (0 boundary, 0 non-manifold), which is
  stronger than the claim; that is what it now asserts.

## 3. A closed-curve face could not be a coplanar container

`find_larger_coplanar_container_face` gated host candidates on
`collect_loop_verts(...).len() >= 3`. A kernel-native closed-curve face (ADR-089) has
one anchor vertex, so it was skipped: a smaller circle inside a bigger one reported no
host, `wall_thickness_from_source_face` returned `None`, and a through-cut silently
degraded to a blind pocket. The **profile** side of that same function already used
`face_outline_points`; this is the identical fix ADR-297 made to the punch host search,
never propagated. `face_outline_points` is read-only — the mesh keeps its analytic form.

## Lock-ins

- **L-299-1** A menu handler that cannot proceed calls `markActionUnavailable`; the
  dispatcher reports what happened, never mere element existence.
- **L-299-2** A menu entry requires an implementation first. Removing the entry is the
  correct fix when none exists.
- **L-299-3** A demo spec asserts. `import { test }` with no `expect` is not a gate, and
  must not be cited as evidence in a LOCKED note.
- **L-299-4** A test's name is a claim: `…_is_closed_solid` calls a closed-solid check,
  `…_manifold` calls a manifold check.
- **L-299-5** Boundary-shape gates use `face_outline_points`, never a raw `>= 3` vertex
  count — that skips every closed-curve face.
- **L-299-6** 절대 #[ignore] 금지.

## Verification

vitest **2962 pass / 0 fail** (+8), cargo **3252 pass / 0 fail** (+1), tsc 0, all three
Playwright demo specs pass **with assertions**.

Mutation-checked: removing the in-dispatch reset fails the stale-flag test; restoring
the old always-true contract fails two; removing the Path A pin drops ADR-093 to
`siblings = 1` and fails; reverting the container gate to `>= 3` fails the new
container test.

Live (real Chromium, production build): both panel globals are functions, the sun-panel
item is gone, and the Inspector menu item now toggles the panel — previously a warning
toast.

## Cross-link

ADR-069 (audit trail recording successes — same class, one layer up), ADR-089
(closed-curve faces), ADR-093 (owner-id grouping — its cited evidence now real),
ADR-094 / ADR-117 (Path B default, which made the ADR-093 demo vacuous), ADR-262
(door notch), ADR-297 / ADR-298 (the punch host fix this propagates), 메타-원칙 #4 / #6.
