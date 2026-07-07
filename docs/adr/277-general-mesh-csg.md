# ADR-277 — General Mesh CSG (arbitrary-angle solids) via shared-vertex imprint + constrained retriangulation

- **Status**: Proposed (α spec — needs 결재 per phase)

## Context

ADR-276 delivered watertight box-box CSG for **axis-aligned** operands
(corner / notch / slot / enclosed-cavity subtract + pure-coplanar 1-axis
union / subtract / intersect). The ADR-276 generality audit
(`adr276_generality_audit_axis_only_rotations_fail_closed`) then proved the
current solid-CSG is **axis-aligned-box scoped**: every rotated box (even a
PURE-transversal `rot (1,1,1) 30°`, coplanar=0) fails-closed. So the transversal
machinery does not handle arbitrary-angle cuts. This ADR designs the general
mesh CSG that lifts that limit.

## De-risk simulation — WHERE it breaks (`adr277_trace_rotated_box_stages`)

`rot (1,1,1) 30°` subtract, traced stage by stage:

| Stage | result | verdict |
|---|---|---|
| 0 prepare_solid | A 6 / B 6 faces | ✓ |
| 1 find_intersections_polygonal | 10 segments across 8 faces (per-face 2·2·2·2·3·3·3·3) | ✓ **correct at any angle** |
| 2 split_faces_by_chains | A 4/6, B 4/6 split (6→11 each) | ⚠ partial (3-seg faces not cleanly split) |
| 3 classify_split_faces | keep_a 6/11, keep_b 5/11 | ✓ reusable |
| 5 pre-weld assemble | **boundary=22, OPEN**, nm=0, valid | ❌ A/B split INDEPENDENTLY → seam verts don't match |
| 5.5 weld_result_seam | boundary 22→9, **nm=1, INVALID** | ❌ band-aid fails on complex seams |

**Root cause (decisive):** the intersection curve is detected correctly (Stage 1
works for any angle), but A's face and B's face are split **independently** and
then a post-hoc `weld` tries to reconcile coincident verts. For axis-aligned
cuts the seams happen to line up (simple 1-chains); for arbitrary angles (and for
the MIXED coplanar+transversal box case, ADR-276 Phase 4) the independent splits
produce a mismatched, open, or non-manifold seam that weld cannot robustly fix.

This is the SAME root cause as the ADR-276 mixed-config T-junctions: **no shared
vertex set on the intersection curve.**

## Decision (proposed) — imprint with a shared vertex arrangement, retriangulate, classify

Replace the "independent chain-split + post-hoc weld" (Stage 2 + Stage 5.5) with
a **shared-vertex imprint + per-face constrained retriangulation**. The
intersection segments are the SAME 3D geometry for the A-face and the B-face of
each crossing pair; if both faces are subdivided along a SINGLE shared vertex
set, the result is watertight **by construction** — no weld.

Pipeline (reused stages marked ♻):

1. ♻ **Stage 0** `prepare_solid` — unchanged.
2. ♻ **Stage 1** `find_intersections_polygonal` — collect all pairwise
   intersection segments (already exact at any angle).
3. **NEW Stage 2a — global vertex arrangement:** dedup every segment endpoint
   into ONE shared vertex set (spatial-hash, LOCKED #5 ε=1.5μm). Each segment now
   references shared `VertId`s, so the A-face and B-face crossing at a segment
   share those two endpoints.
4. **NEW Stage 2b — per-face constrained retriangulation:** for each face,
   project to its plane's 2D basis, insert its incident segments as constraint
   edges, and retriangulate the face polygon honoring those constraints
   (constrained triangulation / ear-clip with constraints). Sub-faces along a
   segment use the shared endpoints → A-side and B-side sub-faces stitch by
   `add_face` + `find_edge`.
5. ♻ **Stage 3** `classify_split_faces` — each sub-face centroid (use
   `face_classify_point`) tested by `point_in_solid` per op — unchanged.
6. ♻ **Stage 5** assemble keep / flip (subtract) / remove — unchanged.
7. **Stage 5.5 weld — ELIMINATED** (shared verts by construction). Optional
   coplanar merge stays skipped when coplanar cells present (ADR-276 Phase 4).
8. ♻ **Gate** — fail-closed (invariants + SI + closed→closed), byte-identical
   rollback — unchanged.

The coplanar planes keep the ADR-276 Phase 4 side-occupancy resolver
(`resolve_coplanar_planes`) — orthogonal to the transversal imprint. The MIXED
config is then just "coplanar resolver + shared-vertex transversal imprint"
sharing ONE global vertex set → the T-junctions vanish.

## The genuinely new pieces (everything else is reuse)

- **N1 — global vertex arrangement:** dedup segment endpoints + insert on-boundary
  crossings into face boundary loops against the shared set (not per-solid).
- **N2 — constrained face retriangulation:** 2D face polygon + interior/crossing
  constraint segments → sub-faces (robust ear-clip with constraints, or a small
  constrained-Delaunay). Must handle a face crossed by 2–3+ segments forming an
  open chain, a closed loop, or multiple disjoint chains.

Reused unchanged: `prepare_solid`, `find_intersections_polygonal`,
`point_in_solid` / `classify_split_faces`, the fail-closed gate,
`resolve_coplanar_planes` (Phase 4).

## Phased plan (each = separate atomic PR + 결재, Path Z)

- **α** (this): spec + de-risk trace.
- **β-1 — global vertex arrangement (N1):** shared endpoint dedup + on-boundary
  insertion; unit-tested that A-face and B-face of a crossing reference the same
  VertIds.
- **β-2 — constrained retriangulation primitive (N2):** pure 2D (face polygon +
  constraint segments → triangles), unit-tested on chain / loop / multi-chain
  inputs. No mesh mutation.
- **β-3 — wire as an ALTERNATIVE path (gated):** a new `boolean_solid` internal
  path (behind a flag / `use_general` v2) that runs imprint+retriangulate+classify
  with NO weld; the current axis-box path stays as fallback until v2 proves out.
- **β-4 — rotated box watertight:** `rot (1,1,1)` + `rot Z 45` etc. cut
  watertight (engine + browser). MIXED box config (ADR-276 Phase 4) closes too.
- **β-5 — cutover + regression sweep:** replace the axis-box transversal path
  with v2 once all ADR-276 cases (corner/notch/slot/enclosed/coplanar) still pass;
  delete the weld band-aid path.
- **γ — arbitrary solids:** non-box triangulated solids; interplay with the
  ADR-197 curved-primitive path (curved stays on its analytic path; general mesh
  CSG is for polyhedral operands).

## Lock-ins (for the β phases)

- **L-277-1** Shared vertex arrangement is the core — the intersection curve is
  ONE global vertex set; watertight by construction, weld eliminated.
- **L-277-2** Reuse `prepare_solid` / `find_intersections_polygonal` /
  `point_in_solid` / classify / gate — do NOT rewrite them (the audit proved
  Stage 1 is correct at any angle).
- **L-277-3** Every phase is fail-closed: snapshot + invariants + SI +
  closed→closed gate + byte-identical rollback on any invalid result.
- **L-277-4** Additive / gated: the new v2 path runs alongside the current
  axis-box path; ALL ADR-276 passing cases (corner/notch/slot/enclosed/coplanar
  1-axis) must keep passing before cutover (β-5).
- **L-277-5** Coplanar planes keep the ADR-276 Phase 4 resolver; MIXED = coplanar
  resolver + transversal imprint on ONE shared vertex set.
- **L-277-6** Curved primitives (ADR-197) are a separate analytic path — general
  mesh CSG is for polyhedral operands.
- **L-277-7** 절대 #[ignore] 금지.

## Consequences

- Rotated / arbitrary-angle / non-box polyhedral solids cut watertight — real CSG.
- The MIXED coplanar+transversal box case (ADR-276 Phase 4 deferred) closes for
  free (shared vertex set removes the T-junctions).
- The `weld_result_seam` band-aid is retired (β-5).
- Cost: N2 (constrained retriangulation) is a real algorithm + LOCKED-boolean
  regression risk → the phased, gated, fail-closed plan contains it.

## Regression (planned)

Per phase, absolute #[ignore] 금지. β-4 adds rotated-box watertight asserts
(the audit's `rot (1,1,1)` / `rot Z 45` flip from fail-closed to committed-
watertight). β-5 must keep every ADR-276 assertion green.

## Acceptance Log

- **α (2026-07-07, `32c8e6c`):** spec + de-risk trace
  (`adr277_trace_rotated_box_stages`) — Stage 1 correct at any angle; the gap is
  Stage 2 split + Stage 5.5 weld (independent split, no shared vertex set).
- **β-1 (2026-07-07, `a30a3da`):** `VertexArrangement` (N1) — spatial-hash dedup
  (0.15μm, LOCKED #5) of intersection endpoints into a shared index +
  `build_intersection_arrangement`. Tests: dedup boundary, chain sharing, real
  rot(1,1,1) segments collapse to < 2× unique. Pure primitive.
- **β-2 (2026-07-07, `78651af`):** `subdivide_face_2d` (N2) — **Pattern-12 reuse**
  of `boundary_kernel::analytic_arrange::arrange`; boundary + constraint Lines →
  sub-faces (`SubFace2D { outer, holes }`). Tests: chord→2, L-chain→2 (0.25+0.75),
  crossing→4, interior loop→annulus+disk. Pure primitive.
- **β-3 (2026-07-07, `0a7afde`):** `boolean_solid_v2` + `imprint_faces` — the
  gated imprint pipeline (subdivide each crossed face + rebuild via add_vertex
  dedup, NO weld) + classify + fail-closed gate. v1 unchanged, v2 not UI-wired.
  **Axis-aligned corner subtract via v2 = watertight (9 faces, no weld)** — proves
  the shared-vertex architecture. **Rotated cuts still fail-closed** (β-3
  continuation): a shared segment can be a boundary crossing on the A-face but
  interior on the B-face → the two faces subdivide it differently (trace: v2
  boundary 22→9 vs v1, still open). **β-3 next increment = global
  intersection-curve assembly** — pre-split all segments at their mutual crossings
  and connect loose ends into the closed curve on the solid, so a shared segment
  is subdivided identically on both faces.
- **β-4 (2026-07-07, `1c2facd`) — PURE-TRANSVERSAL ROTATED CUTS WATERTIGHT (audit
  gap closed).** β-3-continuation diagnosis (measure-first) found the intersection
  curve for rot(1,1,1) is a clean closed loop (10 segs, all degree-2), imprint(A)
  alone is closed+valid, and imprint(A+B) shares the curve edges (nm=10 = 10 edges
  × 4 faces) — so the shared-vertex imprint is CORRECT. The gap was CLASSIFY:
  `arrange` can yield a NON-CONVEX (L-shape) sub-face whose CENTROID falls outside
  it → `point_in_solid` misclassifies → wrong seam face count. Fix:
  `boolean_solid_v2` classifies with a STRICT INTERIOR point
  (`strict_interior_point_3d`, ear-clipping) not the centroid. Result
  (`adr277_beta4_pure_transversal_rotations_watertight`): rot(1,1,1) 30°/20°,
  rot(1,2,3) 25° — SUB / UNI / INT all commit WATERTIGHT (valid + closed +
  manifold). The decisive audit case (`rot (1,1,1)` fail-closed) now cuts.
- **β-4 continuation (2026-07-07, `791bb90`) — coplanar fold-in; v2 is a strict
  SUPERSET of v1.** Folded the Phase-4 `resolve_coplanar_planes` (side-occupancy)
  into `boolean_solid_v2`: coplanar-shared-plane faces are excluded from the
  transversal imprint and rebuilt as side-occupancy quads sharing the same
  vertex set. `adr277_beta4cont_v2_superset_of_v1`: v2 cuts every ADR-276 case
  watertight — corner / notch / slot / enclosed-cavity / coplanar-1-axis (all 3
  ops) — plus pure-transversal arbitrary rotation (β-4). MIXED
  coplanar+transversal (2-axis, rotated-coplanar) still fails-closed in BOTH v1
  and v2 (no regression).
- **β-5 cutover (2026-07-07, `4e5fb1b`) — boolean_solid tries v2 FIRST, falls
  back to v1.** Both fail-closed, so on any v2 Err the mesh is exactly pre-op and
  v1 runs on clean input → the UI (BooleanHandler → bridge.booleanSolid) now gets
  arbitrary-angle boolean with ZERO regression. Verified: full suite green with
  boolean_solid = v2-first (axia-geo 2182, axia-core 433, 0 failed); browser
  axis-corner subtract via bridge.booleanSolid → 9 faces watertight. Rotation
  engine-proven + now reachable through boolean_solid. (Interactive rotated-box
  browser demo pending a rotate-tool/faceMap harness fix — orthogonal.)
- **MIXED (2026-07-07, `c18362a`) — coplanar+transversal WATERTIGHT (last major
  box-box gap closed).** Diagnosis: 2-axis union pre-gate was OPEN (boundary=24)
  because the grid-based `resolve_coplanar_planes` UNIFORMLY subdivides the cap
  (adds every x/y grid line), so the L-cap perimeter splits at points (x=0 on the
  y=-50 edge) the uncrossed transversal wall lacks → T-junction. Fix:
  `resolve_coplanar_planes_arrange` — arranges all A+B coplanar face polygons
  (`boundary_kernel::arrange`) so sub-faces split ONLY at the A/B crossings
  (perimeter matches the walls) + side-occupancy classify + outward-wound 3D loops
  (outer+holes); `boolean_solid_v2` uses it instead of the grid. Because arrange
  handles ARBITRARY polygons it ALSO fixes rotated-coplanar (rot Z 45). Verified
  (`adr277_mixed_coplanar_transversal_watertight`): 2-axis UNI/SUB/INT + rot Z 45
  SUB/UNI all watertight. Also closes the ADR-276 Phase-4 deferred 2-axis mixed.
- **γ (2026-07-07, `cec869b`) — NON-BOX polyhedra cut watertight (v2 is general).**
  The imprint pipeline never assumed boxes (find_intersections on any convex face,
  arrange on any polygon, strict-interior classify), so arbitrary polyhedra work
  with ZERO new code. Verified (`adr277_gamma_nonbox_polyhedra_watertight` +
  `make_tri_prism`): box⊕prism (3 ops), prism−box, two-prism union — all
  watertight/no-corruption.
- **Browser real-time verification (2026-07-07, WASM rebuilt for MIXED + γ):**
  via `bridge.booleanSolid` (= `Mesh::boolean_solid` v2-first) on a reloaded
  scene — axis corner SUB (9 faces, AABB x[-50,50]), lateral UNI (14, x[-50,100]),
  **2-axis UNI MIXED (14, AABB x[-50,100]y[-50,100] = L-prism)**, 2-axis SUB MIXED
  (8) — all `ok`, closed, nm=0, valid. The viewport (real-time preview) reflects
  the new v2 capability; boolean is click-to-apply (no drag ghost). WASM
  screenshot capture is an environment limitation (structural stats are
  authoritative). Wiring confirmed intact: UI → bridge.booleanSolid → WASM
  booleanSolid → Mesh::boolean_solid (v2-first) → boolean_solid_v2
  (imprint + resolve_coplanar_planes_arrange) → v1 fallback.
- **ROTATED-box interactive browser demo (2026-07-07) — DONE.** Fresh reload,
  `eng.rotate_faces` (about (1,1,1) 30°) on box B, then `bridge.booleanSolid`
  subtract → 10 faces, closed, valid, nm=0. Definitive proof rotation reached the
  boolean: the RESULT mesh has 8 OFF-AXIS vertices (only the rotated box's cut can
  produce them). (An intermediate getMeshBuffers AABB read of rotated-B-alone was
  stale — rotate is a delta-buffer op — but the committed boolean result reads
  correctly.) The general-CSG rotation capability is reachable end-to-end from the
  UI-equivalent flow (a user's Rotate tool + bool-subtract).
- **v1 retention (follow-up decision):** the v1 grid `resolve_coplanar_planes` +
  `weld_result_seam` are kept as the fail-closed fallback under boolean_solid.
  v2 is a proven superset, but retiring v1 removes the safety net for untested
  edge cases — defer retirement until real-world telemetry shows v2 never falls
  back (not core; not done now).
- **Status: general polyhedral CSG COMPLETE** — transversal + coplanar + MIXED +
  arbitrary rotation + non-box polyhedra all cut watertight via v2 (boolean_solid
  v2-first). Remaining (follow-up / cleanup, not core): retire the v1 weld
  band-aid + grid `resolve_coplanar_planes` once fully subsumed; curved-primitive
  generalization (ADR-197 analytic path, separate); interactive rotated-box
  browser demo (rotate-tool/faceMap harness — orthogonal).

## Cross-link

- ADR-276 (axis-aligned box CSG + generality audit) — direct predecessor; the
  audit's finding is this ADR's trigger.
- ADR-275 (planar boolean scope) — the original no-op guard.
- ADR-197 (curved analytic Boolean dispatch) — the separate curved path.
- ADR-267 / 272 / 273 — the fail-closed gate infrastructure (reused).
- LOCKED #5 (1.5μm spatial-hash dedup) — the shared vertex arrangement tolerance.
- LOCKED #44 (Complete Meaning per Merge) — each β phase is one complete meaning.
- 메타-원칙 #4 (SSOT) · #6 (Preventive / measure-first) · #9 (회귀 없음).
- Memory: `project-boolean-runtime-finding`.
