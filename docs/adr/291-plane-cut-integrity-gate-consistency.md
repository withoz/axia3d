# ADR-291 — Plane-Cut Integrity Gate Consistency (trim / curved knives)

- **Status**: Accepted
- **Date**: 2026-07-12
- **Category**: Kernel Robustness / CSG
- **Related**: ADR-267 (Universal Watertight Production Gate — `integrity_gate_passed`),
  ADR-272/273 (adversarial sweep + self-intersection checker + closure gate),
  ADR-241 (polygonal trim), ADR-197/205 (curved knives), CLAUDE.md LOCKED #88
  (Phase 3 gate coverage — the "measure, only gate what corrupts" lesson)

## 1. Context

ADR-267 established the invariant that **every plane-cut op is gated**: on the
`Ok` arm it calls `integrity_gate_passed(baseline, snapshot, label, manual_txn)`,
which re-measures `verify_volume_integrity(OpenMesh).damage_count()` and, if the
op introduced NEW damage, rolls back to a byte-identical snapshot + surfaces a
Toast. `slice_volume_by_plane` carries this gate.

A post-track audit flagged that slice's plane-cut siblings shipped **ungated**:
`trim_volume_by_plane` (which literally calls `slice`'s core then removes one
half), and the Path B curved knives `cut_curved_by_z_plane` /
`trim_curved_by_plane`. The audit called this a "confirmed silent-corruption
bug" and recommended copying slice's gate.

## 2. Measurement (measure-first, 메타-원칙 #6)

A de-risk simulation (`phase3_gate_sim` `adr291_*`) ran the ungated ops against
closed solids with **degenerate / grazing planes** (tangent to a face, through a
vertex, thin slivers, tilted-through-corner, compound cuts) — the inputs the gate
exists for. Findings:

1. **trim / cut_curved / trim_curved are robust.** Every degenerate case either
   **SELF-REJECTS** (the slice core `Err`s before mutating) or stays **SAFE**
   (clean, valid, manifold). No trim corruption was reproducible.
2. **`slice` (keep BOTH halves) produces self-intersections** on a thin sliver
   (z=49.9 → SI 0→17) and a tilted-near-corner cut (SI 0→12), committing `Ok`.
3. **Decisive discriminator**: `trim` (keep ONE half) at the *same* planes is
   **SI-clean** (SI 0, valid). So the SI slice reports is **between the two
   halves touching/coincident at the cut plane** — an inherent property of
   keep-both, NOT corruption of any resulting solid. (Verified: the flagged
   pairs are perpendicular wall×cap, and the strict share-aware SI checker's
   report reflects the two shells sharing the cut plane, not a fold within a
   solid.)
4. **`damage_count()` excludes self-intersection** (`= invariant_violations +
   geometric_cracks + open_boundary_edges`). So `integrity_gate_passed` cannot
   see SI anyway — slice's own gate would not catch its keep-both SI, and adding
   an SI-aware gate to the cut ops would **false-reject legitimate slices** whose
   two halves touch at the cut plane (the over-gating meta-principle #6 forbids).

**Conclusion**: there is no reproducible plane-cut corruption. The "bug" is a
*consistency gap*, not a live corruption. An SI-aware gate is **contraindicated**.

## 3. Decision

Mirror slice's **`integrity_gate_passed`** (crack / invariant / open-boundary,
baseline-relative, **NOT self-intersection**) onto `trim_volume_by_plane`,
`cut_curved_by_z_plane`, and `trim_curved_by_plane` — on their routed (mutating)
`Ok` arm, `manual_txn=false` (each Scene method commits internally). This:

- gives the plane-cut siblings the **same safety net** `slice` has (catches any
  future regression in the shared slice core that starts producing cracks —
  ADR-267's invariant that every plane-cut is gated);
- adds **zero UX risk** — the gate is a no-op passthrough on clean results
  (measured `damage_count 0`), and does NOT check SI, so it never false-rejects
  a legitimate slice whose halves touch at the cut plane;
- closes the **regression-guard omission** (`step6` `adr267_gamma_…` enumerated
  punch/drill/carve/slice/door/split but omitted trim/curved).

The merge ops (`merge_coplanar_faces_geometric` / `merge_coplanar_containing`)
are **out of scope** — no corruption was measured (they SELF-REJECT the
mis-merge cases), and they belong to the closure-gate (SI-aware) family, not the
plane-cut family.

## 4. Lock-ins (L-291-1 ~ L-291-8)

- **L-291-1** trim/cut_curved/trim_curved call `integrity_gate_passed(...,
  "trim"|"cut curved"|"trim curved", false)` on the routed `Ok` arm, mirroring
  slice.
- **L-291-2** Gate scope = crack / invariant / open-boundary only (OpenMesh
  `damage_count`, baseline-relative). **NOT** self-intersection.
- **L-291-3** No SI gate on plane-cut ops (contraindicated — inter-half touching
  is not corruption; would false-reject legitimate slices). Measured rationale
  locked by `phase3_gate_sim` `adr291_derisk_classify_slice_self_intersections`
  (trim keep-one SI-clean at the SI-producing planes — a hard assertion).
- **L-291-4** `manual_txn=false` (Scene commits internally → gate uses
  `discard_last_undo` on rejection; not-routed/error arms leave no frame).
- **L-291-5** Baseline-relative — pre-existing Path B rim artifacts on a curved
  solid do not false-reject (gate fires only on NEW damage).
- **L-291-6** Source-grep regression guard (`step6` `adr267_gamma_…`) extended:
  ≥13 gate call sites + explicit `"trim"|"cut curved"|"trim curved"` labels, so a
  future edit can't silently drop them the way trim originally shipped.
- **L-291-7** Merge ops out of scope (no measured corruption; SI-aware family).
- **L-291-8** 절대 #[ignore] 금지.

## D. Acceptance Log

- **α (measure-first)** — `crates/axia-geo/tests/phase3_gate_sim.rs`:
  `adr291_derisk_degenerate_cut_trim` (degenerate trim/curved → SELF-REJECT/SAFE,
  none corrupt) + `adr291_derisk_classify_slice_self_intersections` (slice
  keep-both SI classified; **decisive assertion**: trim keep-one is SI-clean +
  invariants valid at the SI-producing planes). Established: no reproducible
  corruption; SI-aware gate contraindicated.
- **β (implement)** — `crates/axia-wasm/src/lib.rs`: `integrity_gate_passed`
  mirrored onto `trim_volume_by_plane` / `cut_curved_by_z_plane` /
  `trim_curved_by_plane` (routed `Ok` arm, `manual_txn=false`). axia-wasm builds
  clean.
- **γ (regression + 시연)** — `crates/axia-wasm/tests/step6_additive_only.rs`
  `adr267_gamma_verify_volume_integrity_endpoint_wired`: ≥13 gate sites + explicit
  trim/curved label guard. Real-Chromium E2E
  `web/e2e/adr-291-cut-trim-integrity-gate.spec.ts` (2/2): clean polygonal trim
  (box, z=0, keepAbove → 6→6 faces, valid, integrity clean) + clean curved knife
  (Path B cylinder, mid-z slice → routed, valid) both succeed — gate wired and
  transparent, no regression.

**Regression totals**: axia-geo +2 (phase3_gate_sim de-risk asserts),
axia-wasm regression-guard extended (+3 label checks, count 10→13), Playwright
+2. Workspace cargo unchanged otherwise (axia-geo/axia-core/axia-wasm all green,
1 pre-existing slow-channel ignored). 절대 #[ignore] 금지 준수.

## §Lessons

- **L1 measure-first overturned the premise twice.** The survey's "confirmed
  trim corruption, trivial drop-in" did not survive measurement: trim is robust,
  and the corruption signal (slice SI) is inter-half touching, not a defect. The
  user-directed "verify the real bug first, then gate" sequence is exactly what
  surfaced this — and what stopped an SI-aware gate that would have regressed
  legitimate slices.
- **L2 `damage_count` ≠ "all damage".** It excludes self-intersection by design;
  SI is the closure-gate's concern. Knowing which gate sees what is essential
  before choosing one — an SI problem cannot be fixed by the integrity gate, and
  a touching-halves "SI" must not be fixed by any gate.
- **L3 consistency-without-over-gating.** When an op shares a gated sibling's
  core, giving it the *same* gate (no-op on clean, fires only on the sibling's
  proven failure class) is defensible defense-in-depth — distinct from adding a
  *new* stricter gate that changes behavior. ADR-267's "every plane-cut gated"
  invariant justifies the former; meta-principle #6 forbids the latter.
- **L4 lock the rationale as a hard test.** The "trim keep-one is SI-clean"
  assertion is load-bearing for the "no SI gate" decision — it's a regression
  assert, not a print, so a future change that makes a trimmed solid genuinely
  fold will fail and force a revisit.

## Cross-link

- ADR-267 (`integrity_gate_passed`, the mirrored gate) / ADR-272/273
  (closure gate + SI checker — the SI-aware family, deliberately NOT used here)
- ADR-241 (polygonal trim) / ADR-197 / ADR-205 (curved knives) — the gated ops
- CLAUDE.md LOCKED #88 Phase 3 ("measure, only gate what corrupts")
- 메타-원칙 #6 (Preventive / measure-first) / #4 (SSOT) / LOCKED #44 (Complete
  Meaning per Merge) / #66 (STATUS-POLICY)
