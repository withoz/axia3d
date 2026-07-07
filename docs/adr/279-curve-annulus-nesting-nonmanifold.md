# ADR-279 — Curve-Annulus Nesting Non-Manifold Fix (α spec)

- **Status**: Accepted (β implemented + demo-verified 2026-07-07)

## Context

Drawing a Path B (kernel-native, analytic self-loop) circle **inside another
circle that is itself a hole in a solid face** produces a **non-manifold edge
(nm=1)** — the solid opens (`is_closed_solid=false`). This is the
"annulus 곡선 한계" (curve-annulus limit) also observed in a sibling engine.
Downstream extrude/carve on the resulting cracked region is then correctly
rejected by the ADR-267/272/273 integrity gate — so the user sees "곡선
annulus를 못 만든다" and "그 위에서 관통이 안 된다".

Single arc-bounded carve (one circle on a face) and polygon annuli (concentric
rectangles) are **fine** — the defect is specific to **curve (self-loop) holes
nested at depth ≥ 2** in the coplanar re-derive path.

## Measurement (de-risk simulation — `sim_curve_annulus_nonmanifold_diagnosis`)

Production path = all 3 flags ON (`face_rederive_on_draw` +
`auto_intersect_on_draw` + `auto_face_synthesis_on_draw`), matching the browser.
`crates/axia-core/src/scene.rs` scene test, measured 2026-07-07:

| Step | active | closed | boundary | nm | face inners |
|---|---|---|---|---|---|
| **Case A** — 2 concentric circles, NO box | | | | | |
| after R40 | 1 | false | 1 | 0 | disk40: 0 |
| after R20 (annulus) | 2 | false | 1 | **0** | ring: 1, disk20: 0 |
| **Case B** — box + 2 concentric circles on top | | | | | |
| box | 6 | true | 0 | 0 | — |
| after R40 on top | 7 | true | 0 | 0 | **box-top: 1**, disk40: 0 |
| after R20 (annulus on solid) | 8 | **false** | 0 | **1** | **box-top: 2** ⚠, disk40-ring: 1, disk20: 0 |

**Decisive finding:** after R20, the **box-top face gains a SECOND inner loop
(inners 1 → 2)**. The R20 circle is assigned as a hole to BOTH its immediate
container (disk40 → ring, correct) AND its grandparent (the box-top face,
WRONG). The R20 self-loop edge then ends up referenced by 3 face-bearing
half-edges → non-manifold (nm=1) → the solid opens.

- Case A (no box → no grandparent) is **clean (nm=0)** — the annulus itself is
  fine; the defect needs a **nesting depth ≥ 2** (circle inside a face that is
  itself a hole).
- Concentric **rectangles** (polygon annulus) through the SAME re-derive path
  are clean (browser-measured nm=0) — the defect is specific to the **curve
  (self-loop) hole** path.

## Wiring map (current, measured + code-read)

```
DrawCircleAsCurve (R20)                                   scene.rs:3411
  → Scene::exec_draw_circle_as_curve                      scene.rs:7025
      add_face_closed_curve → 1 anchor + 1 self-loop edge (Circle) + face
      → intersect_faces_inner(&[R20 face])                scene.rs:7098
          face_rederive_on_draw == true →                 scene.rs:3062
          → Scene::rederive_coplanar_on_draw              scene.rs:2423
              → face_rederive::rebuild_coplanar_faces[_analytic]  face_rederive.rs:531 / 1139
                  Phase 1: Path B circles POLYGONISED into the planar graph
                           (tessellate_full, chord = r·0.05)   face_rederive.rs:592-604
                  Phase 3: resolve_and_extract_nested / arrange()  face_rederive.rs:719 / 1470
                           → ArrFace { outer, holes }
                  Hole assignment — TWO mechanisms both fire for the inner circle:
                    (1) arrange nesting via `innermost_parent`     face_rederive.rs:52,768,1529
                        + ArrFace.holes reconcile                  face_rederive.rs:753,1610
                    (2) A2 freeform-containment reparent           face_rederive.rs:1712
                        (inner self-loop twin HE → outer hole)
```

**Root cause (hypothesis, measurement-anchored):** at nesting depth ≥ 2 the
containment/parent resolution does not converge on a single innermost parent for
the inner curve — the arrange `.holes` nesting and/or the A2 reparent path assign
the inner circle's loop to **more than one enclosing face** (immediate container
*and* grandparent). The `innermost_parent` probe (face_rederive.rs:52) is meant
to pick exactly one parent; for a polygonised circle nested inside another
polygonised circle inside a polygon, it (or the parallel A2 path) double-assigns.
Result: the inner self-loop edge is claimed by 3 face-bearing HEs → nm=1.

(β will pin the exact double-assignment line by instrumenting which of the two
mechanisms adds the second box-top inner loop; the sim already localises it to
"box-top gains a 2nd inner after R20".)

## Decision (proposed — β needs 결재)

**Single-parent hole assignment via a canonical containment tree.** In the
coplanar re-derive, build one containment nesting tree over all loops (outer +
all curve/polygon holes) and assign each inner loop as a hole to **exactly its
innermost enclosing face**. Eliminate the double-assignment by making the
arrange-nesting path and the A2 freeform-containment reparent path mutually
exclusive (one canonical path owns curve-hole nesting), so a curve hole at any
depth is referenced by exactly 2 half-edges (its disk + its one parent's hole)
→ manifold.

**Q for 결재:**
- **Q1** Fix locus — (a) unify hole assignment inside `rebuild_coplanar_faces`
  so arrange-nesting is the sole owner and the A2 reparent path is gated off for
  circles already nested by arrange (recommended); (b) fix `innermost_parent` to
  return the true innermost among nested curve holes and drop the duplicate
  assignment; (c) post-pass that de-duplicates a loop referenced by >1 face.
- **Q2** Scope — (a) circles only (Path B `AnalyticCurve::Circle`) this ADR;
  (b) all closed curves (Bezier/BSpline/NURBS self-loops) now. Recommend (a),
  mirror to (b) as a follow-up (same nesting mechanism).
- **Q3** Depth — (a) fix all nesting depths generally (recommended); (b) only
  depth-2 (box-top + one circle hole) as MVP.
- **Q4** Gate discrepancy — separately, `verify_face_invariants().valid == true`
  while `face_set_manifold_info().nm == 1` for this case (I5 self-loop counting
  vs ManifoldInfo). (a) fix I5 to catch self-loop non-manifold in this ADR;
  (b) separate ADR. Recommend (b) — it is orthogonal (a checker-completeness
  gap, not the annulus fix), but note it so the β regression asserts on
  `face_set_manifold_info` (authoritative), not only `verify_face_invariants`.

## Lock-ins (α — carried into β)

- **L-279-1** Measure-first: the de-risk sim
  (`sim_curve_annulus_nonmanifold_diagnosis`, scene.rs) is a **characterization
  test** — it currently asserts the defect (Case B nm ≥ 1). The β fix flips it to
  `Case B nm == 0` + `closed == true` + `box-top inners == 1`. Do not delete it.
- **L-279-2** β regressions assert on `face_set_manifold_info` (authoritative
  `non_manifold_edge_count` / `is_closed_solid`), NOT only
  `verify_face_invariants` (Q4 discrepancy).
- **L-279-3** No regression to: single arc carve (ADR-252 pocket/through, LOCKED
  #82 sibling findings — 22/23-face watertight), polygon annulus (concentric
  rects nm=0), lone concentric circles (Case A nm=0).
- **L-279-4** Each curve hole referenced by exactly one parent face → exactly 2
  face-bearing HEs on its self-loop edge (manifold by construction).
- **L-279-5** Additive per ADR-046 P31 #4 — no public API / UI / menu change;
  fix lives entirely in the coplanar re-derive hole-nesting.
- **L-279-6** 절대 #[ignore] 금지.

## De-risk evidence (this α)

- `sim_curve_annulus_nonmanifold_diagnosis` (axia-core scene.rs) — **passes**,
  reproducing Case A nm=0 (clean) and Case B nm=1 (defect) in the production
  rederive path. Localises the defect to "box-top face gains a 2nd inner loop
  after the inner circle".

## Acceptance Log

- **2026-07-07 β (`assign_circle_holes_innermost`)** — single-parent hole
  assignment landed (Q1=(a) unify, Q2=(a) circles, Q3=(a) all depths, Q4=(b)
  gate discrepancy separate).
  - **Root cause pinned (β diagnosis):** the defect is NOT the arrange (it skips
    full-circle holes, `face_rederive.rs:1611`) — it is the Scene **post-process
    containment loop** (`rederive_coplanar_on_draw`). On a scoped re-derive of a
    2nd concentric circle, the box-top's existing R40 hole + `disk40` are
    preserved untouched; the old order-dependent pairwise loop then RE-assigned
    `disk40` (the R40 circle, already box-top's hole-disk) as ANOTHER box-top hole
    → duplicate → the R40 rim edge gains a 3rd face-bearing HE → nm=1.
  - **Fix:** replaced the pairwise loop with the canonical
    `axia_geo::operations::annulus::assign_circle_holes_innermost` (메타-원칙 #4
    SSOT): sort candidate faces by enclosed area ascending, assign each circle to
    its FIRST (smallest ⇒ innermost) container and stop; **skip a circle whose rim
    twin is already a container's hole** (`circle_already_hole` — the ring+disk
    already exists, re-assigning duplicates it). A circle may still serve as a
    container for a smaller one, so perfect nesting at any depth → one-hole-per-
    parent.
  - **Verification (3 layers):**
    - Engine sim `adr279_curve_annulus_nested_is_manifold` (axia-core, production
      4-flag path incl. `freeform_overlap`): Case A nm=0, **Case B (box + R40 +
      R20) now nm=0, closed=true, box-top inners 2→1**. Unit tests
      `adr279_assign_innermost_three_level_nesting_manifold` (R30⊃R20⊃R10, each
      innermost-only, nm=0) + `adr279_assign_innermost_idempotent_skips_already_
      hole` (axia-geo).
    - Suite: axia-geo 2189 pass / 0 ignored, axia-core 434 pass / 0 ignored.
    - Browser (real Chromium, fresh WASM): box + `drawCircleAsCurve(…,40)` +
      `drawCircleAsCurve(…,20)` → 8 faces, **closed=true, nm=0, boundary=0,
      invValid=true** (was nm=1). Single-circle carve/through unaffected (42-face
      through-hole, closed, nm=0). *(Note: my first browser probe mis-called the
      7-arg bridge `drawCircleAsCurve` with 10 args → radius defaulted to the 7th
      arg (1.0) → two coincident r=1 circles, a false nm=1. The engine sim, always
      authoritative, correctly used radii 40/20.)*
  - **Wiring audit (β, all callers of the containment splits):**
    - **Production circle path** (`rederive_coplanar_on_draw`, face_rederive ON =
      browser default) → now `assign_circle_holes_innermost`. **Fixed** (nm=0).
    - **Legacy circle path** (`intersect_faces_inner` face_rederive **OFF**,
      scene.rs ~3161, old `detect_circle_containment` + `split_face_by_inner_
      circle` per-fid loop) → measured: NO nm=1 bug (nm=0), but it does not form
      circle annuli at all (circle-in-polygon unhandled in legacy) — a
      pre-existing legacy behavior, NOT this defect. Production never takes it
      (face_rederive default ON); left untouched to avoid destabilizing the 245+
      legacy regressions. SSOT note: two hole-assignment impls coexist; only the
      production one matters.
    - **A2 freeform path** (`face_rederive.rs:1719`, `split_face_by_inner_closed_
      curve_generic` for Bezier/BSpline/NURBS self-loops, circles excluded) →
      first-match-per-inner scan, so an analogous innermost/depth≥2 risk exists
      for FREEFORM annuli — but the generic split requires a POLYGON outer (no
      freeform-in-freeform), so it is much milder and untriggered by the circle
      case. **Follow-up:** generalize `assign_circle_holes_innermost` to freeform
      closed curves (separate curve type, separate ADR).
  - **Out of scope (follow-up):** extruding the INNERMOST disk of an annulus
    through is still gate-REJECTED (clean byte-identical rollback, no corruption)
    — a separate nested-region extrude limitation, NOT the annulus formation this
    ADR fixed. Q4 checker discrepancy (`verify_face_invariants` valid vs
    `face_set_manifold_info` nm) left to a separate ADR; β regressions assert on
    the authoritative `face_set_manifold_info` (L-279-2).

## Cross-link

- LOCKED #82 sibling / `project-curve-annulus-limit` memory (E:/AXiA3D
  verification of the sibling `axia-carve-pocket-curve-limit` note).
- ADR-186 (`rederive_coplanar_on_draw` / `rebuild_coplanar_faces` — production
  coplanar re-derive; the fix locus).
- ADR-089 Phase 2 (closed-curve self-loop face — `add_face_closed_curve`).
- ADR-145 / ADR-185 (`operations::annulus` — legacy face_rederive-OFF annulus
  path; single-level annulus reference, its unit test is manifold-clean).
- ADR-101 §L6 (3-way overlap deferred — sibling "multi-region curve/arrangement"
  family; overlapping rects leave cracks too).
- ADR-252 (carve pocket/through — the downstream op the gate blocks on a cracked
  annulus, LOCKED #82).
- ADR-267 / ADR-272 / ADR-273 (integrity gate — correctly rejects extrude on the
  cracked annulus; the gate is right, the annulus is the bug).
- 메타-원칙 #4 (SSOT — single canonical hole-nesting path) / #6 (Preventive —
  measure-first) / #14 (면은 닫힌 경계로부터).
- LOCKED #44 (Complete Meaning per Merge — α spec + sim = one meaning; β separate).
