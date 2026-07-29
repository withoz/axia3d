# ADR-306 — I4 covers inner loops too

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Kernel / correctness
**Corrects**: ADR-298 L-298-5

## 1. The claim I inherited was wrong

ADR-298 L-298-5 says `verify_face_invariants` "does not walk a face's inner loops."
It does. **I3** walks them — null start, collect failure, `< 3` verts, with the
ADR-089 one-vertex closed-curve exemption — at `mesh_invariants.rs:280-313`, and
`git log -S "I3: inner loops"` puts it in the `155e127` baseline, **before**
ADR-298. The claim was imprecise when written, not describing a since-fixed state.

I carried it into a session summary without checking. Measuring it is what found
the actual defect.

## 2. The actual defect: I4 was outer-only

**I3** walks inner loops but only asks whether they are *collectable and long
enough*. **I4** asks the different question — does every half-edge point back at
its own face — and asked it of the **outer loop only**.

So an inner half-edge repointed at another face was invisible. Measured on a
200×200 quad with a 60×60 hole plus a second face to point at:

```
baseline                  valid = true
OUTER he repointed   ->   valid=false  ["face FaceId(0): outer HE HeId(0) points to wrong face FaceId(1)"]
INNER he repointed   ->   valid=true   []                              <<<
  collect_non_manifold_edges  -> 0
  detect_self_intersections   -> 0
```

Not merely missed by this verifier — **invisible to every gate**. ADR-267's
integrity gate, ADR-272's closure gate and the ADR-302/303 sims all resolve to
`verify_face_invariants`, `collect_non_manifold_edges` or
`detect_self_intersections`, and on that mesh all three are clean.

This is the second such asymmetry in two days. ADR-304's was that a NaN normal
skipped I2 because `NaN > 1e-10` is false; this one is that a whole class of
half-edge was never asked the question at all.

## 3. Decision

I4 iterates the outer loop **and** every inner loop, reporting which one:
`face F: inner[0] HE H points to wrong face G`. Null starts are skipped — I1 and
I3 already report those, and duplicating them would just make one fault read as
two.

Nothing else changes. Creation, I3's shape and the closed-curve exemptions are
untouched; this is detection, not policy.

## 4. Lock-ins

- **L-306-1** A per-face invariant states whether it covers inner loops. If it
  applies to half-edges or vertices, it applies to *all* of the face's loops
  unless there is a reason in the code saying otherwise.
- **L-306-2** A regression that pins a blind spot also pins **that the other
  gates cannot see it**, so it stays load-bearing rather than becoming a
  duplicate of whichever gate later grows the same check.
- **L-306-3** A lock-in inherited from another ADR is verified before being
  repeated. L-298-5's advice was sound; its stated reason was not, and repeating
  the reason is how the wrong model spreads.
- **L-306-4** 절대 #[ignore] 금지.

## 5. Verification

`cargo test --workspace` → **3258 passed / 0 failed / 1 ignored**, **zero
regressions** — no mesh anywhere in the suite has an inner half-edge pointing at
the wrong face, so the check is pure additive detection.

Two regressions in `mesh_invariants::adr306_tests`: one asserting both halves of
the symmetry (outer *and* inner flagged, with the specific message), one asserting
that `collect_non_manifold_edges` and `detect_self_intersections` still report 0
on the corrupted mesh — the measurement that makes the first test load-bearing.

**Mutation-checked**: restricting I4 back to the outer loop fails with
`a repointed INNER half-edge must be flagged. Nothing else sees it…`.

## 6. What this does not do

It does not make the verifier complete. It closes the one asymmetry that was
measured; other invariants may have their own. ADR-298's advice — assert loop
walkability on the neighbours of a rewrite, not just on the face you touched —
still stands on its own merits.

## Cross-link

ADR-298 (the corrected lock-in), ADR-304 (the sibling asymmetry — NaN skipped I2),
ADR-267 / ADR-272 (the gates that resolve to this verifier), ADR-089 (the
closed-curve exemptions I3 and I1 carry), 메타-원칙 #4 / #6.
