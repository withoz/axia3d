# ADR-307 — The `cancel()` sweep, measured: 82 sites, 3 worth touching

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Kernel / correctness
**Follows**: ADR-302 §6, ADR-303 §6

## 1. The deferred sweep, sized

`TransactionManager::cancel()` ends a recording and restores nothing. ADR-302 and
ADR-303 both deferred a sweep of "~85 Err arms in the WASM layer" on the grounds
that most never mutated anyway. That number was remembered, not measured.

Measured: **82** `cancel()` calls in `crates/axia-wasm/src/lib.rs`.

| bucket | count | why it needs nothing |
|---|---|---|
| already restore a snapshot | 11 | correct pattern (ADR-190 P0.2) |
| closed by ADR-302 / ADR-303 | 4 | validate-before-mutate landed |
| self-protect in the engine | 2 | op rolls itself back |
| never destroy geometry before their last fallible step | ~56 | all validation precedes mutation |
| **destroy-then-fallible shape** | **9** | across 6 ops |

Adversarial verification then shrank the 9 further:

- **`merge_faces_by_edge` (4 sites) — refuted.** The proposed failure cannot
  occur: `create_halfedge_pair` (mesh.rs:6780-6812) has no `?` and no `bail!` and
  returns `Ok(())` unconditionally, so `add_edge` cannot fail. The reachable
  subtree's three remaining bail sites are all pre-guarded.
- **`subdivide_catmull_clark` (1 site) — refuted**, but for a reason that matters
  below: its post-teardown `add_face_with_holes?` has two doors, and both are
  shut — one by construction, the other because `compute_normal`'s degenerate
  `bail!` is **dead code** (ADR-304: `NORMAL_EPSILON = 0.0`, so
  `len < NORMAL_EPSILON` is never true).
- **`punch_*` (3 sites)** — same shape, and treated below.
- `merge_coplanar_containing` (1 site) — not adversarially checked.

So the campaign that read as "~85 error paths need rollback" is really **three**,
and the correct-by-construction majority is the finding.

## 2. The coupling worth recording

Two of those "unreachable" verdicts rest on the same fact: **`compute_normal` can
never return `Err`.** That is not a design guarantee — it is ADR-304's dead-code
finding. `NORMAL_EPSILON = 0.0` makes the guard unreachable, and ADR-304
deliberately left open whether creation *should* reject degenerate geometry.

**Answering that question "yes" re-opens these doors.** A safety argument that
depends on a guard being broken is not a safety argument; it is a coincidence with
good timing.

## 3. Decision — use the snapshot that is already there

All three `punch_*` entries already capture `integrity_snapshot` at the top, for
the integrity gate. Their `Ok` path restores from it when the gate rejects. Their
`Err` path threw it away and called `cancel()`.

The Err arms now restore first. One line each, no new machinery, and it removes
the dependence on §2's coupling entirely — the answer no longer changes if
ADR-304's policy question is answered.

`merge_coplanar_containing` is **left alone**: not verified, and ADR-303's
precedent is that a reorder near a helper with a documented precondition needs its
own de-risk rather than a drive-by.

## 4. Lock-ins

- **L-307-1** A safety argument may not rest on a guard that is dead. If the only
  reason a door is shut is a bug elsewhere, close the door.
- **L-307-2** When an op already captures a rollback snapshot, its Err arm uses it.
  `cancel()` alone is correct only where nothing was mutated.
- **L-307-3** A deferred sweep is sized before it is deferred again. "~85 sites"
  and "3 sites" are different decisions, and only one of them was true.
- **L-307-4** 절대 #[ignore] 금지.

## 5. Verification

`cargo test --workspace` → **3260 passed / 0 failed / 1 ignored**.

`adr307_punch_err_arms_restore_the_snapshot` (step6) pins all three: each must
capture the snapshot *and* restore from it in the Err arm. It is a source guard
because the failure it protects against is measured unreachable — it pins the
**restore**, not the reachability.

**Mutation-checked**: removing one of the three restores fails it.

## 6. Remaining

`merge_coplanar_containing` (1 site). And the real follow-up is not more rollback
plumbing — it is ADR-304's open question, because answering it changes which of
these paths are reachable at all.

## Cross-link

ADR-302 (validate-before-mutate; §6 deferred this sweep), ADR-303 (§6 deferred it
again, on evidence that turned out to hold), ADR-304 (the dead `compute_normal`
bail these verdicts lean on), ADR-190 P0.2 (the snapshot-restore pattern),
ADR-298 §3b (the punch host filter that shuts the other door), 메타-원칙 #4 / #6.
