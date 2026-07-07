# ADR-280 — Solid-Top Re-tile on Crossing Draw (imprint, preserve wall boundary) (α spec)

- **Status**: Accepted (Level 1 fail-closed landed 2026-07-07; Level 2 full re-tile deferred)

## Context

Drawing a shape that **crosses** (partially overlaps) another coplanar shape on a
**solid's top face** opens the solid — a side/top face appears to "disappear"
(사용자 "옆면이 사라짐"). Measured: box + circle, then a rect that crosses the
circle → the top de-tiles into pieces that don't cover the full top → **10 open
boundary edges, `is_closed_solid=false`** (walls actually intact; the TOP
partition opens).

This is NOT the ADR-279 curve-annulus bug (that is fixed) and NOT a regression —
it reproduces with a **single** circle + crossing rect. Contained (non-crossing)
shapes work; only **crossing** shapes on a solid top break it. It is the same
family as the overlapping-rects cracks (ADR-101 §L6 partial-overlap deferred).

## Root cause (measured — confirmed by instrumentation)

The production coplanar re-derive (`rederive_coplanar_on_draw` →
`rebuild_coplanar_faces_analytic_scoped`, face_rederive.rs):

1. **`reconstruct_input_curves` excludes `volume_edges`** (face_rederive.rs:337
   `if !edge.is_active() || volume_edges.contains(&eid) { continue; }`). The
   box-top's **outer square edges are shared with the side walls → they ARE
   `volume_edges`** → excluded from the arrange input.
2. Instrumented the arrange call for box + circle + crossing-rect:
   **`input_curves = 5`** = rect(4 lines) + circle(1) — **the square boundary is
   absent**. The arrange tiles only rect∪circle (Σarea ≈ 7270), so the
   square-minus-shapes region is never reproduced → open top.
3. `part_of_solid` (face_rederive.rs:1383) protects the box-top FACE from
   removal, but its interior hole edge (the circle) is not a `volume_edge` → it
   IS removed by `edges_to_remove` → the protected face's hole dangles.

The "draw-onto-solid guard" (face_rederive.rs:1285) that protects solid walls is
**deliberate** — its comment documents that naively re-deriving around a solid
face dangles the wall loop ("Entity HeId not found" panic → wasm "recursive use"
spam) or makes edges 3-way non-manifold. So the exclusion is intentional
wall-protection; the gap is that it also prevents re-tiling the top.

## De-risk simulation (`adr280_sim_arrange_tiles_full_square_with_boundary`)

Pure-2D `arrange` test (boundary_kernel/analytic_arrange.rs), square 120×120
(area 14400) + circle r40 + rect crossing:

| input | faces | tiled area | verdict |
|---|---|---|---|
| **(A) circle + rect only** (current) | 3 | Σouter ≈ **7270** | only rect∪circle — top opens |
| **(B) + square boundary** (fix) | 4 (1 with holes) | net (outer−holes) = **14400** | **tiles the FULL square** ✅ |

**Fix direction validated:** feeding the solid-top's outer boundary to the
arrange makes it net-tile the entire square — 1 square-with-holes face (holes =
rect+circle) + 3 sub-faces (rect-only / circle-only / lens) filling the holes.
The arrange ALREADY handles the 3-way tiling; the missing piece is only that the
boundary is withheld.

## Wiring map (current)

```
DrawRectAsShape / DrawCircleAsCurve (crossing)
  → Scene::exec_draw_* → intersect_faces_inner            scene.rs
      face_rederive_on_draw ON →
      → Scene::rederive_coplanar_on_draw                  scene.rs:2423
          → face_rederive::rebuild_coplanar_faces_analytic_scoped   :1187
              affected_face_component (BFS AABB)          :259  ← box-top IS included
              reconstruct_input_curves(…, volume_edges)   :300  ← EXCLUDES square (volume) ✗
              part_of_solid → skip box-top removal         :1383 ← protected face
              edges_to_remove (excl. volume)               :1420 ← removes the circle hole edge
              arrange(input_curves)                        :1470 ← gets 5 curves (no square)
              Phase 4: ArrFace → add_face_with_holes       :1657
```

## Decision (proposed — β needs 결재): imprint the solid-top, preserve its wall boundary

When the affected coplanar region includes a **solid-top** (`part_of_solid`,
sheet) face and a crossing shape re-tiles it:

1. **Feed the solid-top face's outer boundary** (its square edges, even though
   `volume_edges`) into the arrange input so the arrange net-tiles the full
   region (sim (B)).
2. **Remove the solid-top face** (so it is replaced by the tiled sub-faces) BUT
   **do NOT remove its shared outer edges** (they belong to the walls;
   `edges_to_remove` already excludes `volume_edges` — keep that).
3. **Materialize** the ArrFaces via `add_face_with_holes`, relying on the
   existing `add_vertex` spatial-hash dedup + `find_edge` so the new
   square-with-holes face's OUTER loop **reuses the existing wall-top edges**
   (not new duplicates) → walls stay connected → watertight. (Mirrors the
   ADR-277 boolean v2 shared-vertex imprint: seam watertight by construction, no
   weld, no dangling.)

**Q for 결재:**
- **Q1** Trigger scope — (a) only when a crossing shape (≥2 boundary crossings
  with the solid-top's interior loops) is present (minimal, recommended); (b) any
  draw touching a solid-top. Recommend (a) so contained-shape + plain-sheet cases
  keep their existing (working) paths untouched.
- **Q2** Materialization safety — (a) reuse `add_face_with_holes` + dedup and add
  a post-materialize check that every new outer edge equals a pre-existing
  wall-top edge id (assert no duplicate wall edge) (recommended); (b) explicit
  edge-reuse wiring. Recommend (a) first, fall to (b) if dedup misses.
- **Q3** Fail-closed gate — (a) wrap the solid-top re-tile so that if the result
  opens a previously-closed solid, roll back the draw byte-identically (mirror
  ADR-267) — a hard safety net so this can NEVER reopen the "disappearing face"
  regression (strongly recommended). 
- **Q4** curved-top (cylinder/sphere face) crossing — (a) out of scope this ADR
  (planar solid-top only); (b) include. Recommend (a).

## Lock-ins (α — carried into β)

- **L-280-1** De-risk sim `adr280_sim_arrange_tiles_full_square_with_boundary`
  (axia-geo) is a kept α artifact — it asserts the arrange net-tiles the full
  square when given the boundary (fix-direction invariant), and that the
  no-boundary input does NOT (the bug shape). Do not delete.
- **L-280-2** Preserve the wall-shared boundary — NEVER remove `volume_edges`;
  the new outer loop must REUSE them via dedup (no duplicate wall edges).
- **L-280-3** Fail-closed — a solid that was closed before the draw must be
  closed after (or the draw rolls back). β regressions assert on
  `face_set_manifold_info().is_closed_solid` + `non_manifold_edge_count == 0`
  (authoritative), and must cover: box+circle+crossing-rect, box+2-circles
  (annulus, ADR-279)+crossing-rect, box+overlapping-rects.
- **L-280-4** No regression to the WORKING paths: contained circle (ring+disk),
  ADR-279 concentric annulus, single-circle through, plain flat-sheet re-derive,
  the 245+ solid-wall-protection regressions.
- **L-280-5** Additive per ADR-046 P31 #4 — no public API / UI / menu change.
- **L-280-6** 절대 #[ignore] 금지.

## β investigation (2026-07-07) — runtime findings + blocker

Started β; instrumented the actual runtime (STR_DEBUG on the "draw-onto-solid
guard", face_rederive.rs:1297). Findings that MUST inform the β (my initial model
was incomplete):

- The guard `region_touches_solid` (returns `RebuildReport::default()` early when
  an affected face shares an edge with a wall) is CENTRAL:
  - **Circle draw**: `affected = [disk(6, touches=false), box-top(1, touches=TRUE)]`
    → region_touches_solid=**true** → EARLY RETURN (no arrange). The ring is then
    formed by the Scene POST-PROCESS (`assign_circle_holes_innermost`). This is
    why the contained circle works — it never goes through the arrange at all.
  - **Crossing-rect draw**: `affected = [rect(7), disk(6)]` — **the box-top is
    ABSENT from `affected_faces`**, so region_touches_solid=**false** → the
    arrange re-derive PROCEEDS on {rect, disk} without the box-top boundary →
    tiles only rect∪circle → 10 open edges.
- Unresolved contradiction (the β blocker): at the moment of the rect re-derive,
  `all solid-touching active faces` = only [bottom + 4 walls] — the **box-top
  ring is not reported as solid-touching**, contradicting "the circle draw made a
  ring whose outer square is shared with the walls". The intermediate face/edge
  state between the circle draw and the rect draw needs full step-tracing
  (add_vertex dedup / face id churn / post-process reparent effects) before a
  safe β. Building on this incompletely-understood state risks the documented
  wall-dangling panic + the 245+ solid-protection regressions (메타-원칙 #9).

**β decision (honest):** the fix is a genuine multi-session effort. The de-risk
sim proves the arrange tiles correctly GIVEN the boundary; the hard part is the
runtime path (guard + scope + post-process + shared-edge materialization). Two
concrete continuations, either needs its own careful pass:

- **Level 1 (safe interim, low-risk, recommended first):** widen the guard —
  fire `region_touches_solid` also when the affected region's 2D AABB overlaps
  ANY solid-touching face (not only faces already in `affected_faces`). Then a
  crossing shape on a solid top EARLY-RETURNS → the solid stays CLOSED (the shape
  is left as an un-split coplanar sheet) instead of opening. This eliminates the
  "옆면이 사라짐" broken-solid symptom immediately, aligned with the guard's
  existing intent. It does NOT split (Level 2 does).
- **Level 2 (full re-tile / split, the ADR-280 imprint):** feed the solid-top
  boundary + remove the face + materialize reusing wall edges (this doc's
  Decision). Needs the intermediate-state tracing above + the fail-closed gate
  (Q3) as backstop.

## Level 1 landed (2026-07-07) — fail-closed guard (safe interim)

Implemented the safe interim: a face-creating draw that would OPEN a closed solid
is rejected + rolled back (the solid stays closed) instead of breaking it.

- **Locus:** `Scene::guard_imprint` (scene.rs) — the existing ADR-258 β-1 wrapper
  around face-creating `*AsShape` draws that already rolled back on a
  non-manifold increase. Extended it: capture `closed_before = is_closed_solid`
  over all active faces; after the draw, if `closed_before && !closed_after`
  (the closed solid opened), restore the pre-draw snapshot + `discard_last_undo`
  + return the Korean "면 안쪽에 그려주세요" error (same path as the nm guard).
  This is the correct locus/timing — the earlier attempt to gate inside
  `rederive_coplanar_on_draw` was too late (the crossing shape had already opened
  the top before the re-derive ran; boundary was 9 at rederive entry).
- **Behavior (browser + engine verified):**
  - Crossing rect on box+circle → REJECTED, box stays **closed** (was: 10 open
    edges). "옆면이 사라짐" eliminated.
  - Rect on a plain box top (no crossing shape) → COMMITS (split), stays closed —
    guard does NOT over-fire.
  - ADR-279 concentric-circle annulus → NOT regressed (stays closed, committed).
  - Rect drawn INSIDE a circle-on-a-solid → also rejected (it genuinely opens the
    top too — same re-derive fragility). Declined safely; Level 2 makes it split.
  - Flat-sheet / free-wire draws → unaffected (guard only fires when
    `closed_before` AND only wraps face-creating `*AsShape` draws).
- **Regression:** axia-core 435 (+1 `adr280_l1_crossing_shape_on_solid_stays_
  closed`) / axia-geo 2190, 0 ignored. Known limitation of Level 1: the
  multi-component case (a closed solid coexisting with an open sheet →
  `is_closed_solid` already false → guard can't fire) is not covered — Level 2 /
  a per-component check handles it.
- **Trade-off (honest):** Level 1 makes crossing/inside-shape draws on a solid
  top a safe *decline* (Toast), NOT a split. The actual re-tile/split is Level 2
  (this doc's Decision — imprint, preserve wall boundary).

## Cross-link

- ADR-279 (curve-annulus nesting) + LOCKED — sibling; fixed the annulus formation
  this ADR builds a crossing shape onto.
- ADR-277 (general mesh CSG — shared-vertex imprint) — the materialization
  pattern (imprint, preserve boundary, dedup, no weld) this reuses.
- ADR-186 (`rederive_coplanar_on_draw` / `rebuild_coplanar_faces_analytic` — the
  re-derive; the fix locus) + the "draw-onto-solid guard" (face_rederive.rs:1285).
- ADR-101 §L6 (partial-overlap / 3-way deferred — same family: crossing shapes).
- ADR-267 (integrity gate — the fail-closed pattern for Q3).
- ADR-275 (planar-solid scope warning — the honest-degradation precedent).
- 메타-원칙 #4 (SSOT) / #6 (Preventive — measure-first) / #9 (회귀 없음) /
  #14 (면은 닫힌 경계로부터).
- LOCKED #44 (Complete Meaning per Merge — α spec + sim = one meaning; β separate).
