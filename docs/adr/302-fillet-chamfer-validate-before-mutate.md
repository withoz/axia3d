# ADR-302 — A failed fillet/chamfer must leave the mesh untouched

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Kernel / correctness

## 1. The defect

`fillet_edge` and `chamfer_edge` tear down their operand faces —

```rust
for fid in &faces_to_remove {          // f1, f2, f3_a, f3_b
    let _ = self.remove_face(*fid);
    if self.faces.contains(*fid) { self.faces.remove(*fid); }
}
```

— and only *afterwards* run eight fallible steps: five `add_face_with_holes(…)?`,
two `orient_arc_for_f3(…)?`, two `splice_vertex_replacement(…)?`, and a `bail!`.

Reaching any of them left the mesh mutated with no way back. The WASM Err arm
calls `transactions.cancel()`, which is:

```rust
pub fn cancel(&mut self) {
    self.is_recording = false;
    self.current_frame = TransactionFrame::default();
    self.touched_entities.clear();
}
```

It restores nothing, and it discards the recording — so there is no undo frame
either. The closure-preserving gate (ADR-272) runs only on the Ok arm. The op
reports `-1` and the solid is permanently open.

## 2. Measuring it, and what the measurement corrected

The structure above is plain from reading. Whether any *input* reaches a
post-teardown failure was not, and two prior descriptions of it were wrong:

- The finding proposed "drill a through-hole, then **fillet** a tube-wall edge."
- An adversarial review corrected that to material-**convex** geometry (a boss),
  on the grounds that a hole rim is reflex and bails early at fillet.rs:184.

`tests/fillet_rollback_sim.rs` sweeps every edge of eight geometries — box, Path
A cylinder, Path B cylinder/sphere/cone/torus, a drilled box, and two boxes
genuinely fused by a union — through both ops, and prints every case that failed
while changing the mesh. It settles the question:

**Both were half right.** The geometry was the drilled hole, as originally
claimed; the op was **chamfer**, not fillet. Under `fillet` the hole rim does
bail early, exactly as the review said. Under `chamfer` it does not:

```
[drilled-box/chamfer] edge EdgeId(22): Err("fillet: endpoint 11 not in F3 loop")
    — faces 10 -> 8, closed true -> false   <<< MUTATED ON FAILURE
[drilled-box/chamfer] edge EdgeId(20): … endpoint 10 …   <<< MUTATED ON FAILURE
[drilled-box/chamfer] edge EdgeId(23): … endpoint 8  …   <<< MUTATED ON FAILURE
[drilled-box/chamfer] edge EdgeId(21): … endpoint 9  …   <<< MUTATED ON FAILURE
```

All four hole-rim edges. Nothing else in the sweep mutated on failure — the
other 100-odd failures across those geometries are early, clean bails.

Worth recording: the sweep's first two runs found **zero**, because the shapes
were too easy — two `create_box` calls are two disjoint solids, not a fused boss.
A "no defect found" result is only as strong as the inputs behind it.

## 3. Decision — validate before mutate

Both reachable failures are decidable from data captured *before* the teardown.
`f3_a_info` / `f3_b_info` already hold each F3 face's vertex loop, and
`orient_arc_for_f3` fails precisely when the endpoint is absent from that loop.

A shared `preflight_f3(op, f3_a_info, f3_b_info, v_a, v_b)` runs immediately
before the teardown in both ops and checks:

1. the endpoint appears in its F3 loop (what `orient_arc_for_f3` needs), and
2. F3 at the two endpoints are distinct (what the existing post-teardown `bail!`
   checked far too late).

This is ADR-298 §3b's `probes_inside_host_outline` pattern — refuse on the facts
you already have, rather than discovering them with the mesh half torn down. It
costs nothing at run time and fixes the defect at the **engine** layer, so direct
Rust callers, MCP, and the browser are all covered by one change.

Side benefit: chamfer's message no longer says `fillet:` (it shared the helper's
hard-coded prefix).

## 4. Lock-ins

- **L-302-1** No face is removed until every check decidable from pre-teardown
  data has run.
- **L-302-2** The fix lives in `axia-geo`, not in a WASM wrapper. A rollback in
  one entry point protects one caller; a refusal in the op protects all of them.
- **L-302-3** `tests/fillet_rollback_sim.rs` is a **gate**, not a report. If a
  failed fillet/chamfer changes the face count or opens a closed solid, it fails.
- **L-302-4** A sweep that finds nothing must state the inputs it used. Two runs
  here found zero against shapes that could not reach the path.
- **L-302-5** 절대 #[ignore] 금지.

## 5. Verification

| | before | after |
|---|---|---|
| inputs that failed **and** mutated | 4 | **0** |
| total Err across the sweep | unchanged | unchanged |

The refusals are the same; only the debris is gone.

**Mutation-checked**: removing the chamfer `preflight_f3` call fails the gate
with `a failed fillet/chamfer mutated the mesh — see the MUTATED lines above`.

**Regression**: `cargo test --workspace` → **3253 passed / 0 failed / 1 ignored**
(the pre-existing slow-channel test).

## 6. Not in this change

`transactions.cancel()` restoring nothing is **systemic**, not a fillet quirk —
there are ~85 `cancel()` Err arms across the WASM layer. Making them restore is a
sweep across 85 error paths, most of which never mutated in the first place, and
it needs its own measurement rather than being carried along here. The engine
layer is the better place for this defect anyway, and that is where it is fixed.

Two neighbours from the same audit remain open and are the natural next
measurement: `chamfer_vertex_3way`'s sanity check firing after its corner is torn
down and rebuilt, and `merge_coplanar_faces_geometric` destroying both operands
before its degeneracy bail.

## Cross-link

ADR-298 (validate-before-mutate; the refused-but-mutated defect this repeats),
ADR-272 (the closure-preserving gate, which runs only on Ok), ADR-190 P0.2 and
ADR-262 §β-2 (snapshot rollback at the WASM layer — the other available pattern),
ADR-249 (`drill_rect_through_hole`, the reproduction's input), 메타-원칙 #4 / #6.
