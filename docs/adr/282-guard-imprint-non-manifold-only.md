# ADR-282 — guard_imprint reduced to non-manifold-only (deformation over decline)

- **Status**: Accepted (2026-07-08)

## Context

`guard_imprint` (Scene, ADR-258 β-1) wraps every face-creating `*AsShape`/
`*AsCurve` draw. ADR-280 Level 1 extended it with an **`opened_solid`** check:
if the mesh was a closed solid before the draw and is not after, roll back +
reject (Korean Toast "…면 안쪽에 그려주세요"). It was a fail-closed backstop for
the "옆면이 사라짐" (a crossing shape opens a solid's top) finding while the real
re-tile was pending.

User feedback (2026-07-08):
> "안전망이 너무 확대가 되었네요. 그릴때 입체와 겹쳐지면 입체가 변형되는것이
> 정상인데. 깨지는것을 너무 우려해서 정상적인 상황도 막고 있습니다."

i.e. **overlapping a solid should DEFORM it** — the guard was declining
legitimate deformations. The user asked to remove the over-blocking net.

## Measured findings (before deciding scope)

With the whole guard removed, engine measurement (box + circle on top, then a
second shape) showed:

| Case | overlap kind | result w/o guard | verdict |
|---|---|---|---|
| **Crossing** rect over a circle-on-top | PARTIAL | **splits, closed, nm=0** | ✅ β-1 (ADR-281) already handles it — the guard was NOT blocking this |
| **Contained** rect inside a circle-on-top | CONTAINMENT | **opens, nm=0** (free coplanar sheet) | deformation, not corruption — auto-split disabled by LOCKED #1 (ADR-101 §L6) → ADR-283 |
| **Rect crossing the box's OWN top edge** (extends off-solid) | crosses solid boundary | **nm=2 (NON-MANIFOLD)** | ❌ genuine corruption — the guard's non-manifold check was NOT meaningless |

Key conclusion: `guard_imprint` had **two** checks with different value:
- **non-manifold** (ADR-258 original) — prevents genuine corruption (nm=2 above).
  Removing it ships a broken (non-manifold) mesh. **Keep.**
- **opened_solid** (ADR-280) — declines a draw that merely OPENS a closed solid.
  Opening is a valid deformation (the off-solid / contained portion is a sheet),
  not a break. **Remove** (the over-block the user hit).

## Decision (user-approved 2026-07-08 — option 가)

**`guard_imprint` rejects ONLY a NEW non-manifold edge (genuine corruption).**
The `opened_solid` (ADR-280 Level-1) decline is removed. `surfaceDrawReject`
(bridge Toast on the `-1` rejection sentinel) is **kept** — it now surfaces only
genuine corruption rejections.

Result:
- **Crossing** (partial overlap) → deforms + splits + closed (β-1). ✅
- **Contained** → deforms (opens as a free coplanar sheet), nm=0, NOT declined.
  Closed auto-split is the pending **ADR-283** work (containment auto-split, a
  user-approved LOCKED #1 change). ✅ (deform, not decline)
- **Corruption** (non-manifold, e.g. a shape crossing a solid's own edge and
  extending off it) → rolled back + rejected + Toast. ✅ (break prevented)

This **supersedes ADR-280 Level 1** (the `opened_solid` decline). ADR-280's
Level-2 goal (actually re-tile a crossing top) already landed as ADR-281 β-1.

## Lock-ins

- **L-282-1** `guard_imprint` rejects iff `collect_non_manifold_edges().len()`
  INCREASED across the draw. No watertight/closedness check.
- **L-282-2** A draw that merely opens a solid (deformation) passes through.
- **L-282-3** `surfaceDrawReject` retained — corruption rejections still Toast.
- **L-282-4** Rollback pattern unchanged (ADR-193 `restore_scene_snapshot` +
  `discard_last_undo`).
- **L-282-5** Line/Point draws remain unguarded (create no face — ADR-258).
- **L-282-6** Contained-open is a valid interim deformation; ADR-283 makes it
  auto-split (closed). Never a silent corruption (nm==0 asserted).
- **L-282-7** 절대 #[ignore] 금지.

## Acceptance

- `crates/axia-core/src/scene.rs` `guard_imprint` — `opened_solid` +
  `closed_before` removed; condition = `nm increased` only; message dropped the
  "솔리드를 열거나" clause. Net −12 lines; no call-site or bridge change.
- Regression: axia-core **436** / axia-geo **2190**, 0 failed, 0 ignored.
  vitest WasmBridge **332** passed (surfaceDrawReject intact).
  - `adr258_partial_overlap_imprint_rejected` — rect crossing the box's own
    edge (nm=2) still rejected + rolled back to the 6-face box (original
    assertion holds — the non-manifold check does this).
  - `adr280_l1_crossing_shape_on_solid_stays_closed` — crossing still closed
    (β-1, guard doesn't fire at nm=0).
  - `adr281_b1_crossing_shape_on_solid_splits_watertight` — crossing splits
    closed; contained now deforms (opens, nm=0) — assertion relaxed to `nm==0`
    with the closed-split deferred to ADR-283.
  - `adr258_contained_imprint_accepted` / `adr258_ground_rect_unaffected` —
    unaffected (nm=0 draws pass through as before).

## Wiring + menu/toolbar re-review (2026-07-08, post-ADR-282)

Full re-verification after the guard change (all read-only greps + tests):

**Engine/bridge wiring — consistent:**
- `guard_imprint` (non-manifold-only) wraps the **8 face-creating draws**
  (rect / circle-shape / polygon / circle-curve / ellipse / closed bezier /
  bspline / nurbs). Line / point / polyline / centerline are wire-draws (no
  face) → intentionally unguarded (ADR-258).
- `surfaceDrawReject` wraps the **matching 8 bridge methods** (1:1) — genuine
  corruption rejections still Toast `lastError()`.
- Error message is single-source and updated to non-manifold-only ("비-manifold
  (겹친 면)"); no stale "솔리드를 열거나". No stale `opened_solid`/`closed_before`
  references in code.

**Draw-tool → bridge routing — all kernel-aware (no legacy):**
- Rect/RotRect → `drawRectAsShape`; Circle → `drawCircleAsCurve`/`AsShape`
  (+ curved-surface `drawCircleOn{Sphere,Cylinder,Cone,Torus}`); Ellipse →
  `drawEllipseAsCurve`; Polygon → `drawPolygonAsShape`; Bezier/Spline →
  `drawClosed{Bezier,BSpline}AsCurve` (+ open with-curve variants); Line/
  Polyline/Freehand/Arc → `drawLineAsShape`/`drawPolylineAsShape`; Hole/Window/
  PolygonHole → `punch*`/`drill*`/`cutWallDoorOpening` (WASM integrity-gated).
- Legacy `bridge.drawCircle`/`drawRect` are DELETED (ADR-087 K-ζ) and NOT
  UI-exposed. Fixed a stale `DrawPolygonTool` doc comment that still named
  `bridge.drawCircle` (it actually calls `drawPolygonAsShape`).

**Menu/toolbar consistency — verified:**
- Every registered draw tool (line, polyline, rect, rotrect, circle, ellipse,
  polygon, arc, pie, point, bezier, spline, freehand, hole, polygon-hole,
  window, centerline) is reachable via MenuBar `tool-*`, toolbar `data-tool`,
  and/or Command Palette. `ellipse` is Command-Palette-only (AxiaCommands
  `tool-ellipse` + ActionCatalog) — reachable, not orphaned.
- ActionCatalog ⊇ CommandCatalog invariant intact (CatalogConsistency 3/3,
  LOCKED #60/#61).

**Verification:** axia-core 436 / axia-geo 2190 (0 failed / 0 ignored), vitest
2508 passed / 1 skipped, tsc 0 errors, ADR-catalog check pass.

## Cross-link

- ADR-258 β-1 (guard_imprint — the non-manifold check retained).
- ADR-280 Level 1 (`opened_solid` decline — **SUPERSEDED** by this ADR).
- ADR-281 β-1 (partial-overlap crossing re-tile — the crossing win).
- ADR-283 (가칭, containment auto-split — the contained-open root fix, approved
  LOCKED #1 change).
- ADR-101 §L6 / LOCKED #1 (containment auto hole-injection disabled — why
  contained doesn't auto-split yet).
- ADR-193 (rollback pattern).
- 메타-원칙 #6 (Preventive — measured before deciding) / #9 (회귀 없음) /
  #10 (LOCKED change → explicit consent + new ADR, applies to ADR-283).
