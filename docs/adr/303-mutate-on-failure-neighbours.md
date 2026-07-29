# ADR-303 — The two neighbours ADR-302 left open, measured

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Kernel / correctness
**Follows**: ADR-302 §6

## 1. What was claimed

The audit that produced ADR-302 named two more ops with the same shape, neither
of which had been measured:

- **`chamfer_vertex_3way`** tears down f1/f2/f3 at step 7, rebuilds them at
  step 8, and only at step 9 asserts `uniq.len() == 3`. If that fires, the three
  originals are gone, three replacements are in with the corner spliced out, and
  the closing triangle is never added.
- **`merge_coplanar_faces_geometric`** removes *both* operands before its
  `bail!("merged loop degenerate after collinear simplification")` and a fallible
  `add_face_with_holes(…)?`. Its WASM entry has neither a restore nor a gate, and
  `transactions.cancel()` restores nothing.

Both readings are accurate. LOCKED #88 Phase 3's "self-reject, therefore ungated"
classification does not cover them: that measurement asked whether the op
corrupts *on success*, not whether it leaves debris *on refusal*.

## 2. What the measurement found

`tests/mutate_on_failure_sim.rs` sweeps every vertex (chamfer, r ∈ {5, 20, 60})
and every coplanar face pair (merge, tol ∈ {0.5°, 30°}) of a box, a Path A
cylinder, a drilled box, and two boxes fused by a union.

**624 failures. Zero left the mesh mutated.** Every one is an early bail:

| bail | count |
|---|---|
| `chamfer: radius … exceeds an incident edge length` | 15 kinds |
| `faces not coplanar (plane distance …)` | 13 |
| `faces not coplanar (… ° between normals)` | 13 |
| `chamfer: vertex N not in face loop` | 9 |
| `chamfer: MVP requires valence==3, got N incident faces` | 6 |
| `no collinear edge with geometric overlap` | 2 |

Neither post-teardown path was reached **once**.

### Why, and this is the part worth keeping

Both are defended upstream, by construction rather than by a guard:

- `uniq.len() == 3` cannot fail. `edge_trim_points` returns points measured along
  each *edge*, so the point on an edge shared by two faces is identical computed
  from either side and the spatial hash welds them. Its own doc-comment says so,
  crediting "adversarial sweep #2" — an earlier hardening whose purpose was
  keeping the solid closed, and which incidentally made this assertion
  unfireable. The step-9 `ensure!` is a leftover from before that fix.
- `simplified.len() < 3` needs the merged boundary to be *entirely* collinear,
  which takes at least one degenerate operand.

  My first draft of this ADR justified it with "ADR-003 rejects degenerates at
  creation." **That is false, and measuring it is what caught it.**
  `add_face_with_holes` accepts a collinear zero-area triangle, a loop that
  repeats a vertex (`[a,b,a,b]`), and a face with a NaN coordinate — the last one
  yielding `normal = (NaN, NaN, NaN)`. The only creation guard is
  `"Face requires at least 3 vertices"`.

  The conclusion survives for a different, measured reason: a degenerate face's
  normal is NaN or zero, and the coplanarity check runs **before** the removal, so
  the pair is refused there. Two collinear zero-area faces, and a NaN-vertex face
  against a valid quad, both return
  `Err("faces not coplanar (NaN° between normals, tol 30.00°)")` with the face
  count unchanged. A valid quad merged with a collinear sliver simply *succeeds*.
  Nothing tried reaches the post-removal bail.

  That the creation contract is missing is a separate finding, tracked below.

## 3. Decision

**Downgrade both from the audit's "high".** They are latent ordering hazards, not
live defects — the opposite of ADR-302's fillet/chamfer pair, which had a
reproducible input that permanently opened a solid.

Apply L-302-1 to `chamfer_vertex_3way` anyway: `uniq` depends only on the trim
vertices materialized at step 5, so its computation and check move there, ahead
of the splices and the teardown. Ordering hygiene, and honestly labelled as such
in the code — it fixes no reachable bug.

`merge_coplanar_faces_geometric` is **left alone**. Its bail sits after the
removal by necessity: `simplify_collinear_loop_preserving` is passed `[f1, f2]`
and documented to rely on them already being gone, so that any *active* incident
face it finds is a genuine external neighbour. Reordering it means reworking that
contract — real risk, for a path measurement says nothing reaches. Changing it
would need its own de-risk, not a drive-by.

## 4. Lock-ins

- **L-303-1** `chamfer_vertex_3way` computes and checks `uniq` before step 6.
- **L-303-2** `merge_coplanar_faces_geometric`'s ordering is deliberately
  unchanged, because the removal is a precondition of the simplification. Any
  future reorder starts by re-deriving that contract.
- **L-303-3** A defect that reads as real but that no input reaches is reported
  as exactly that. "Structurally present, measured unreachable, and here is why"
  is a finding; silently fixing it as though it were live is not.
- **L-303-4** `tests/mutate_on_failure_sim.rs` is a gate. It is weaker than
  ADR-302's — it locks in the status quo rather than proving a fix — and the code
  says so.
- **L-303-5** 절대 #[ignore] 금지.

## 5. Verification

624 failure cases, 0 mutating, before and after. **Mutation-checked**: forcing
the counter non-zero fails the gate with `a failed chamfer_vertex_3way /
geometric merge mutated the mesh`. `cargo test --workspace` → **3254 passed /
0 failed / 1 ignored**.

The chamfer hoist changes no behaviour — same condition, same message, earlier.
Existing `chamfer` tests: 20 passed.

## 6. Remaining

`transactions.cancel()` restoring nothing is still systemic — ~85 Err arms in the
WASM layer, most on paths that never mutated. ADR-302 §6 deferred that sweep and
this measurement is a reason to keep deferring it: two of the three ops that
looked most likely to need it turned out not to.

**New, found while checking §2's premise**: `add_face_with_holes` has no
degenerate guard. It accepts zero-area, self-repeating and NaN geometry, and only
refuses a loop of fewer than three vertices. `docs/PRACTICALITY_REPORT.md` rows
2/3/9 present that contract as "✅ ADR-003 degenerate guard" verified, resting on
`practicality_edge_cases.rs` tests that end in `let _ = result;` and check
nothing; ADR-003's own checklist still has "각 연산의 단위 테스트에 degenerate
거부 케이스 추가" unticked. Adding the guard would change behaviour on live paths
(a valid quad merged with a collinear sliver currently succeeds), so it needs its
own measurement and decision, not a drive-by. Its own ADR.

## Cross-link

ADR-302 (the reachable case, and §6 which named these two), ADR-298
(validate-before-mutate), ADR-272 (the closure-preserving gate), ADR-003
(degenerate rejection at creation — why the merge bail is unreachable),
LOCKED #88 Phase 3 (the on-success measurement this one complements),
메타-원칙 #4 / #6.
