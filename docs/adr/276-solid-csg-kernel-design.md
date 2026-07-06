# ADR-276 — Solid CSG Kernel: Design + Phased Plan (α spec)

- **Status**: Proposed
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
