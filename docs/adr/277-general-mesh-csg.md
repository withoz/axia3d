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
