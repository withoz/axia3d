# ADR-304 — Degenerate faces: permissive creation, detection at the verifier

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Kernel / correctness
**Found while**: checking ADR-303 §2's premise before merging it

## 1. How this was found

ADR-303 argued that `merge_coplanar_faces_geometric`'s post-removal bail is
unreachable "because ADR-003 rejects degenerates at creation." Before merging it
I checked that premise. **It is false**, and the check is the only reason this
ADR exists.

`add_face_with_holes` accepts:

| input | result |
|---|---|
| collinear zero-area triangle | **Ok**, `normal = (NaN, NaN, NaN)` |
| loop repeating a vertex, `[a,b,a,b]` | **Ok**, `normal = (NaN, NaN, NaN)` |
| face with a NaN coordinate | **Ok**, `normal = (NaN, NaN, NaN)` |
| loop of 2 vertices | Err — *the only creation guard* |

ADR-303's conclusion survived for a different, measured reason (a degenerate
operand's NaN normal fails the coplanarity gate, which runs before the removal),
and it was corrected in place rather than merged as written.

## 2. Two NaN-comparison traps, one root

`compute_normal` looks like it rejects degenerates:

```rust
let len = normal.length();
if len < NORMAL_EPSILON {
    …fallback cross product…
    bail!("Degenerate polygon — cannot compute normal");
}
Ok(normal / len)
```

`NORMAL_EPSILON = 0.0`. `len < 0.0` is **never true** — not for a zero-length
normal, and not for NaN, since every comparison with NaN is false. So the entire
fallback branch, `bail!` included, is dead code, and a zero-length Newell normal
falls through to `Ok(normal / 0.0)` = NaN.

`verify_face_invariants` then had the same shape:

```rust
let cached = face.normal();
if cached.length_squared() > 1e-10 {   // NaN > 1e-10 is FALSE
    …the entire winding check…
}
```

A NaN normal skipped the check and the face was reported **valid**. Measured:
`InvariantReport { checked_faces: 1, violations: [] }` for a face whose normal is
`(NaN, NaN, NaN)`.

That is the part that matters. Every gate in this codebase asks this verifier —
ADR-267's integrity gate, ADR-272's closure-preserving gate, and the ADR-302/303
mutation sims. A blind spot here is a blind spot in all of them. It is the second
known one, after ADR-298's finding that the verifier does not walk inner loops.

## 3. Decision

**Creation stays permissive.** `NORMAL_EPSILON = 0.0` is commented "keep at 0 to
avoid missing thin faces", and ADR-064's own test notes degenerate handling as
"delegated to existing `compute_normal` engine behavior … strict
degenerate-rejection is outside scope". Adding a creation-time guard would change
behaviour on live paths — a valid quad merged with a collinear sliver currently
*succeeds* — and that is a policy decision, not a bug fix.

**Detection is added.** `verify_face_invariants` gains I6: a face whose cached
normal is not finite is a violation. Zero-length is deliberately *not* flagged —
the existing check treats it as "no cached normal, skip", and that is
load-bearing. `verify_face_invariants_rev2` inherits it: it filters only messages
matching "cached normal opposite to winding", and a NaN normal is wrong
regardless of the sheet winding exemption.

**The tests that claimed to cover this now do.** Three ended in
`let _ = result;`. They assert the measured contract at both ends: accepted at
creation, caught by the verifier, and *which* invariant catches it — the
duplicate-vertex loop trips I1 rather than I6, because the DCEL folds `[a,b,a,b]`
into a 2-vertex loop before the normal is consulted. A valid-quad control test
guards against the verifier simply calling everything broken.

**The documentation that was wrong is corrected.**
`docs/PRACTICALITY_REPORT.md` rows 2/3/9 presented the absent guard as verified;
they now describe what actually happens. Its test counts were stale by an order
of magnitude (`cargo test` 267 → 3254, `npm run test` 1013 → 2967). ADR-003's
checklist item for degenerate-rejection tests is marked partial, with the reason:
the contract is acceptance-then-detection, not rejection.

## 4. Lock-ins

- **L-304-1** Never compare a possibly-NaN quantity with `<` or `>` as a guard.
  Both traps here were `NaN < eps` / `NaN > eps` silently evaluating false and
  skipping the protection. Use `is_finite()` explicitly.
- **L-304-2** Creation is permissive; the verifier is where degenerate geometry
  is caught. Changing the first half is a policy decision with its own ADR.
- **L-304-3** I6 flags non-finite only. Zero-length stays unflagged.
- **L-304-4** A test that pins a contract names *which* invariant enforces it.
- **L-304-5** A report claiming "✅ verified" must name a test that asserts it.
- **L-304-6** 절대 #[ignore] 금지.

## 5. Verification

`cargo test --workspace` → **3254 passed / 0 failed / 1 ignored**, with **zero
regressions from I6** — nothing in the suite creates a NaN-normal face in normal
operation, so the new check is pure detection.

**Mutation-checked**: disabling I6 fails two tests with
`verify_face_invariants reported degenerate face FaceId(0) (normal
DVec3(NaN, NaN, NaN)) as VALID`.

`practicality_edge_cases.rs`: 7 → **10** tests, every one asserting.

## 6. Remaining

- The dead fallback in `compute_normal` — the cross-product rescue path can never
  run. It is unreachable rather than wrong, and removing it is cleanup, not a
  fix. Left in place, now documented.
- Whether creation *should* reject degenerates is still open, and is the policy
  question this ADR deliberately does not answer.

  > **Answered 2026-07-29 (사용자 결재): no — keep creation permissive, and remove
  > what depended on the ambiguity instead.** Measured downsides of switching to
  > reject: (1) `normalize_for_import` carries a `degenerate_removed` counter, so
  > the import pipeline is *built* on accept-then-repair — real OBJ / STL / DXF /
  > IFC files routinely contain zero-area triangles, and strict creation fights
  > that design rather than serving it; (2) it would make ADR-307's two refuted
  > findings live again — `merge_faces_by_edge` ×4 and `subdivide` ×1 are
  > unreachable *because* `compute_normal` cannot fail, so a strict guard
  > re-opens paths that tear down before rebuilding with no rollback, i.e.
  > choosing "reject" **creates** the work ADR-307 measured away; (3) 199
  > `add_face_with_holes` call sites gain a failure mode, 16 files of which do
  > teardown-then-rebuild.
  >
  > Worth separating, because the code comment conflates them: rejecting **only
  > NaN / exactly-zero** normals needs no tolerance and would not touch thin
  > faces at all — the `NORMAL_EPSILON` comment's "avoid missing thin faces"
  > concern applies only to the *area-threshold* variant. The dead `bail!`
  > intends the first. That nuance is why the decision is a genuine choice rather
  > than a foregone one.
  >
  > The decision is to break the **coupling** instead: ADR-307 already did it for
  > `punch_*` by restoring the snapshot that was already captured. Doing the same
  > for the remaining teardown ops means the safety argument stops resting on a
  > dead guard, and this policy question becomes free to answer either way, later.
- ~~`verify_face_invariants` still does not walk inner loops (ADR-298)~~ —
  **wrong, and closed by ADR-306.** I3 *has* walked inner loops since before
  ADR-298; the real asymmetry was that **I4** checked half-edge ownership on the
  outer loop only, so a repointed inner half-edge was invisible to this verifier
  *and* to `collect_non_manifold_edges` *and* to `detect_self_intersections`.
  ADR-306 extends I4 over inner loops. I repeated ADR-298's stated reason here
  without checking it — exactly what L-306-3 was written about.

## Cross-link

ADR-303 (whose premise this refuted — corrected before merge), ADR-298 (the
verifier's other blind spot), ADR-007 (the invariant policy I6 extends), ADR-003
(the validity principle whose creation-time guard does not exist), ADR-064
(the test comment that recorded NORMAL_EPSILON = 0.0 as deliberate), ADR-267 /
ADR-272 / ADR-302 / ADR-303 (the gates that ask this verifier), LOCKED #5 (the
dedup floor the sub-millimetre tests now pin), 메타-원칙 #4 / #6.
