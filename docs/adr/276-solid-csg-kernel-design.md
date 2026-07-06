# ADR-276 — Solid CSG Kernel: Design + Phased Plan (α spec)

- **Status**: Accepted (design accepted; Phase 1 β landed 2026-07-06; Phases 2–5 gated)
- **Date**: 2026-07-06
- **Context**: Follows ADR-275 (planar/solid box boolean is unimplemented; honest
  no-op guard shipped). User approved route **(a′) — implement the real solid-CSG
  kernel, starting from a design ADR**.
- **This ADR is α (spec + audit + phased plan) only.** No production engine change.
  Each β phase is a separate atomic PR gated by user 결재 (Path Z / LOCKED #44).

## Problem

Box/planar solid boolean (subtract/union/intersect) does not cut (ADR-275). A
real triangle-mesh CSG is needed: collect surface-surface intersections, split
faces along them, classify sub-faces inside/outside, assemble the kept shell,
merge coplanar, and guarantee a watertight manifold result.

## Audit finding (the reframe — measurement-first)

**The classic `Mesh::boolean` already has a full 6-stage pipeline, and the
general intersection collector already exists and works — it is simply NOT
WIRED into `boolean()`.**

| Stage | Function (`boolean.rs`) | Status |
|-------|-------------------------|--------|
| 0 prepare solid (fan-tri) | `prepare_solid` | exists |
| 1 **general** tri-tri crossing | `find_intersections` → `boolean_geo::triangle_triangle_intersection` | **exists, works, but wired only to "Intersect with Model" — NOT to `boolean()`** |
| 1′ coplanar overlap | `detect_coplanar_faces` | exists; **the ONLY thing `boolean()` Stage 1 uses** |
| 2 split faces by segments | `split_faces_by_intersections` | exists (2D project + insert crossings + sub-polygon split) |
| 3 classify in/out | `classify_split_faces` + `point_in_solid` | exists (centroid ray test, per-op logic) |
| 5 assemble + flip | inline + `flip_face` | exists |
| 6 merge coplanar | `merge_coplanar_result_faces` | exists |

`boolean()` line ~1613: `let intersections = coplanar_intersections;` — it ignores
`find_intersections` entirely. (Git history is squashed at baseline `155e127`, so
the reason for the disconnection is not recoverable; treat the general path as
**never validated for `boolean()`** — hence the safety gates below.)

### Phase 0 de-risk simulation (measured, `adr276_phase0_sim_general_intersection_and_split`)

Wiring `find_intersections` into Stage 1, box A [0,0,50] 100³ − box B:

| config | `find_intersections` segs | split faces | invariants valid |
|---|---|---|---|
| corner-poke | **6** (coplanar 0) | 12→18 | **valid** ✅ |
| top-center notch | **8** | 12→16 | **valid** ✅ |
| through-slot | **16** | 12→16 | **INVALID** ⚠ |
| enclosed cavity | **0** (no surface crossing — correct) | 12→12 | valid |

**The collector and split stages work** for surface-crossing configs (segs>0,
faces grow, topology valid for corner-poke/notch). This is NOT a from-scratch
kernel. The remaining work is bounded:

1. **Wire Stage 1** (`find_intersections` ∪ coplanar) into `boolean()`.
2. **Harden split robustness** — through-slot yields an invalid result (fan-tri
   convexity assumption and/or multi-segment-per-face split ordering).
3. **Enclosure/void case** — 0-seg subtract (B ⊂ A) must produce an internal
   shell (hollow), not a no-op; disjoint UNI; enclosed INT.
4. **Coplanar coincidence** — shared-plane faces (fold in `detect_coplanar_faces`
   + merge/dedup so results stay manifold).
5. **Safety + verify end-to-end** — classify/assemble produce a correct CUT
   solid, guarded by the existing gates.

## Decision (proposed — needs 결재 per phase)

Implement the kernel by **completing the existing pipeline**, not rewriting it,
in fail-closed atomic phases. Each phase wires more of the pipeline and is gated
by the existing safety infrastructure so an incorrect result rolls back instead
of corrupting the mesh:

- **ADR-267** `verify_volume_integrity` / watertight gate,
- **ADR-272** `closure_preserving_gate_passed` (closed→open reject),
- **ADR-273** `detect_self_intersections` gate.

### Proposed phased plan (each = separate atomic PR + 결재)

- **Phase 1 — Wire + fail-closed** (S–M): call `find_intersections` and union
  with coplanar in Stage 1; run the full pipeline; wrap the whole `boolean()` in
  a snapshot + the three gates with byte-identical rollback on any
  invalid/opened/self-intersecting result. Success criterion: corner-poke +
  notch cut end-to-end (browser-verified, manifold valid); through-slot &
  enclosed **safely roll back** (clear message) rather than corrupt. This alone
  makes the common convex-overlap cut work.
- **Phase 2 — Split robustness** (M): fix the through-slot invalid result
  (evaluate: non-convex face triangulation, multi-segment split ordering,
  chained crossings). Removes a rollback case.
- **Phase 3 — Enclosure / void** (M): 0-seg subtract (B ⊂ A) → internal shell;
  disjoint/enclosed UNI/INT semantics.
- **Phase 4 — Coplanar coincidence** (M): shared-plane operands — fold the
  coplanar path in cleanly + merge/dedup.
- **Phase 5 — Routing + default + demo** (S): decide UI routing (see Q2), set
  default on/off, browser demo across the config matrix, full regression + a
  proper regression suite replacing the print-only sim.

### Decision points needing user 결재

- **Q1 — Phase 1 gate policy**: fail-closed rollback on any invalid result
  (recommended — no corruption, honest "couldn't cut this config yet") vs
  best-effort commit. Recommend fail-closed.
- **Q2 — UI routing**: keep classic planar CSG as a separate path and dispatch
  by surface kind (all-Plane operands → classic CSG; curved → existing ADR-197
  DCEL), vs unify. Recommend surface-kind dispatch (Plane→classic, curved→DCEL),
  reusing `classify_dispatch_eligibility`.
- **Q3 — Default**: engine default off + production localStorage opt-in during
  hardening (ADR-049 P-5e-α pattern), flip on after Phase 2–4 land. Recommend.
- **Q4 — Triangulation**: keep fan-tri (convex assumption) and reject non-convex,
  vs earcut (`boolean_geo::project_to_2d` + earcut already used by ADR-273).
  Decide in Phase 2 with data.

## Consequences

- Reframes (a′) from "weeks, from-scratch CSG kernel" to "complete + harden an
  existing, mostly-working pipeline in gated phases." Lower risk than feared.
- Fail-closed gates mean each phase is safe to ship: unsupported configs roll
  back cleanly (never corrupt), matching the ADR-275 honesty principle.
- LOCKED Boolean lineage (064/066/074/075/076) untouched until Q2 routing is
  decided; the curved-analytic path (ADR-197) stays the path for curved operands.

## Regression

- `crates/axia-geo/src/operations/boolean.rs` — `adr276_phase0_sim_general_intersection_and_split`
  (measurement + regression guard: the general collector must find box-box
  crossings and split must grow faces for surface-crossing configs). Kept as the
  Phase 0 evidence; Phase 5 replaces the print-only parts with assertion suites.
- Existing scoping assets (ADR-275): `boolean_scoping.rs`, `boolean_planar_probe.rs`.

## Acceptance Log

### Phase 1 β — landed 2026-07-06 (user-approved: proceed, fail-closed / Q1)

- **Separate entry, zero regression** — rather than change `Mesh::boolean`
  universally (which broke 24 existing tests: `boolean_union_with_face_split`
  + 13 `boolean_dispatch` routing tests that use `boolean()` as an oracle /
  mesh-fallback and only assert routing, not geometry), the solid-CSG path is a
  new entry. `boolean_impl(…, use_general: bool)`; `boolean()` → `false`
  (byte-identical pre-ADR-276); **`boolean_solid()` → `true`** (general tri-tri
  Stage 1 + fail-closed gate). All existing callers untouched.
- **Stage 1 wiring** — `boolean_solid` unions `find_intersections` (general
  non-coplanar) with the coplanar overlaps.
- **Fail-closed gate** — snapshot (`self.clone()`) before mutation; after Stage 6,
  if `verify_face_invariants().is_valid()` is false OR
  `detect_self_intersections().is_clean()` is false → `*self = backup` +
  `bail!` (byte-identical rollback). Closed-solid NOT required (2D/sheet operands
  legitimately open).
- **Measured (Rust `adr276_phase1_box_box_subtract_cuts_and_never_corrupts` +
  browser via `demoBooleanSolidTwoBoxes`)**: corner-poke SUB → **cuts** (faces
  12→9, verts 16→22, invariants valid, non-manifold 0); notch → cuts, valid;
  through-slot → **fail-closed rollback** (Err, byte-identical, valid); enclosed
  → no-op (0 segs). Browser end-to-end (Rust→WASM→bridge) confirms corner-poke =
  9 faces, invariants valid, 0 non-manifold.
- **HONEST limitation** — the cut result is **valid + non-corrupting but NOT yet
  watertight** (corner-poke: `is_closed_solid=false`, 6 boundary edges — the
  notch walls are not fully stitched). Phase 1 delivers a valid, non-corrupting
  cut and proves the pipeline; **watertight sealing is Phase 2/3.** The gate
  guarantees no corruption, not completeness.
- **Regression**: axia-geo 2158 / axia-core / axia-transaction all green (0
  regression). New: `boolean_solid` / `boolean_impl`, `demoBooleanSolidTwoBoxes`
  (verification harness, not UI-wired), `adr276_phase0_sim_*` + `adr276_phase1_*`.
- **Not done in Phase 1** (per plan): watertight sealing (Phase 2), through-slot
  robustness (Phase 2), enclosure/void (Phase 3), coplanar coincidence (Phase 4),
  UI routing + default (Phase 5, Q2/Q3). `boolean_solid` is not reachable from
  the UI yet — the UI still shows the ADR-275 honest no-op warning.

### Phase 2 (partial) — audit + fail-closed-correct gate (2026-07-06, user-approved "bounded 해결 시도")

- **Root cause of the open cut (Q4 confirmed).** Audit (`adr276_phase2_audit_
  open_seam_duplicate_verts`) on corner-poke: verts ARE shared (0 coincident
  duplicates — LOCKED #5 dedup works), yet 6 boundary edges remain. Dumping the
  face loops + boundary-edge owners showed the box-box cut is produced as a
  **diagonal / tetrahedral notch, not the true rectangular notch**: A's three
  bitten faces (top / +x / +y) are each cut with a single DIAGONAL across the
  corner (e.g. +y face `…(20,50,100)→(50,50,70)…`, missing the real inner corner
  `(20,50,70)`), and the notch walls are TRIANGLES (half-quads). The 6 open edges
  are those diagonals. **`prepare_solid` fan-triangulates every face (convex-
  assumption MVP), so `find_intersections` computes tri-tri segments along
  triangle diagonals → the box-box intersection curve is topologically wrong.**
- **Not a bounded fix.** Correcting this requires reworking the intersection to
  preserve polygon loops (face-face intersection, not tri-tri on fan-tri'd
  faces) — core CSG, deferred to Phase 2 proper.
- **Safe bounded action taken — closed→closed gate.** The prior gate
  (invariants + SI) did NOT catch "valid-but-open", so `boolean_solid` was
  COMMITTING a geometrically-wrong open cut. Added: when BOTH operands are
  watertight solids, the result must be watertight too (`face_set_manifold_info
  (&merged_faces).is_closed_solid`), else roll back byte-identically.
  `boolean_solid` is now **fail-closed-correct**: for the current box configs it
  cleanly declines (rolls back) instead of shipping a wrong cut, and will only
  commit once the intersection rework produces watertight results.
- **Behavior change:** the Phase 1 "corner-poke cuts" result (9 faces, open) now
  rolls back — that cut was geometrically wrong, so declining it is more correct.
  `boolean()` (use_general=false) is unaffected (gate only runs for
  `boolean_solid`). Curved-analytic path (ADR-197) unaffected (early return).
- **Regression:** axia-geo 2159 pass / 0 fail. Test renamed
  `adr276_phase12_box_box_never_commits_open_or_invalid` — asserts every config
  is valid + (committed⇒watertight) OR (Err⇒byte-identical rollback). Audit test
  kept as the Phase 2 characterization asset.
- **Remaining Phase 2 core (deferred):** intersection-curve rework so box-box
  produces the true rectangular notch (watertight) — then the gate starts
  admitting real cuts. Then through-slot robustness, Phase 3 enclosure/void.

## Lock-ins (for the β phases)

- **L-276-1** Complete the existing pipeline; do NOT rewrite `find_intersections`
  / `split_faces_by_intersections` / `classify_split_faces` unless a phase proves
  a specific one is unfixable.
- **L-276-2** Every phase is fail-closed: snapshot + ADR-267/272/273 gates +
  byte-identical rollback on invalid/opened/self-intersecting results.
- **L-276-3** Each phase = separate atomic PR + user 결재 (Path Z / LOCKED #44).
- **L-276-4** Curved operands keep routing to ADR-197 DCEL; classic CSG is for
  all-Plane operands (pending Q2).
- **L-276-5** 절대 #[ignore] 금지.

## Cross-link

- ADR-275 (planar boolean scope + no-op guard) — direct predecessor.
- ADR-064 / 066 / 074 / 075 / 076 (NURBS Boolean → DCEL lineage) — untouched.
- ADR-197 (curved analytic Boolean dispatch) — the curved path.
- ADR-267 (watertight gate) · ADR-272 (closure-preserving gate) · ADR-273
  (self-intersection checker) — the fail-closed safety infrastructure.
- ADR-049 P-5e-α (engine-off + production opt-in) — Q3 default pattern.
- 메타-원칙 #4 (SSOT) · #5 (사용자 편의) · #6 (Preventive) · #9 (회귀 없음) · #16.
- Memory: `project-boolean-runtime-finding`.
