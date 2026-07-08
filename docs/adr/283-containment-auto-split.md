# ADR-283 — Containment Auto-Split (shape inside a shape on a solid) — α spec + de-risk sim

- **Status**: Proposed (α spec + de-risk simulation landed 2026-07-08; β implementation pending 결재)

## Context

ADR-282 reduced `guard_imprint` to non-manifold-only: a shape drawn INSIDE
another shape on a solid top now DEFORMS (passes through) instead of being
declined. But it deforms into an **un-integrated free coplanar sheet** — the
inner shape's edges are free boundary → the whole is not a closed solid. Example
(the user's "옆면 사라짐" contained case): a rect drawn fully inside a circle on
a box top → the rect is a free sheet, the top opens.

User approved (2026-07-08) making containment **auto-split** (a LOCKED #1 change):
the outer shape should split into outer-with-inner-hole + inner, staying closed —
"겹치면 변형이 정상" realized for containment, not just partial overlap (ADR-281
β-1 handles partial overlap / crossing).

Measure-first: this α verifies the wiring + de-risk-simulates the fix direction
before any production change (메타-원칙 #6).

## Wiring audit — where containment is (and isn't) handled

`Scene::intersect_faces_inner` (scene.rs:3123+) runs a pairwise coplanar scan.
For each pair it tries, in order:
1. `annulus::detect_circle_containment` + `split_face_by_inner_circle` (ADR-185)
   — **both must be Circles** (self-loop). Handles circle-in-circle.
2. `coplanar::auto_intersect_coplanar` — partial-overlap (2 crossings) → 3
   sub-faces (ADR-101). **Containment (0 crossings) → `Ok(None)` no-op**
   (coplanar.rs:551), after polygonizing Path B faces (Amendment 7 pre-check).

Existing containment coverage matrix:

| inner \ outer | circle (self-loop) | polygon |
|---|---|---|
| **circle** | ✅ `detect_circle_containment` + `split_face_by_inner_circle` | ✅ `split_face_by_inner_circle_generic` / `..._closed_curve_generic` |
| **polygon (rect)** | ❌ **GAP** (the failing case) | ❌ **GAP** |

The gap = a **polygon inner** (rect) contained in any outer. No detect/split
handles it → `auto_intersect_coplanar` containment no-op → free sheet → open.

## Root mechanism (why the existing split can't be reused as-is)

`split_face_by_inner_circle*` reparents the inner's **single self-loop twin**
(`he2 = he1.next_rad()`, annulus.rs:420-428): the inner circle's edge becomes
2-face (inner disk + outer hole) → manifold. A **polygon inner** has an N-HE
outer loop (rect = 4), not a single self-loop. Its N edge-twins form the free
"outside" loop — reparenting **all N twins** as the outer's hole generalizes the
circle case.

## De-risk simulation (landed, axia-geo `annulus::tests`)

- **`adr283_sim_rect_in_circle_gap_uncovered`** — characterizes the gap:
  `detect_circle_containment(circle, rect)` → None; `split_face_by_inner_circle_
  generic(circle, rect)` → `NotCircleFace{ role: "inner" }`; circle gains no hole.
- **`adr283_sim_rect_in_circle_reparent_manifold`** — proves the direction:
  reparenting the rect's 4 edge-twins into the circle as a hole loop →
  `circle.inners() == 1`, both faces active, `verify_face_invariants` 0
  violations, `face_set_manifold_info.non_manifold_edge_count == 0`. **Manifold
  closed by construction, boundary shared (no new geometry).**

Result: the twin-loop reparent is the correct, manifold-safe mechanism.

## β plan (pending 결재 — each its own gate)

- **β-1** engine: `annulus::split_face_by_inner_polygon(mesh, outer, inner)` —
  generalize the reparent to a multi-HE inner loop (self-loop stays via the
  circle path; polygon via the N-twin loop). Plus `detect_shape_containment`
  (general: inner representative point inside outer, point-in-polygon/circle;
  covers polygon-in-circle, polygon-in-polygon, and the existing circle cases as
  a superset). Reuse `assign_circle_holes_innermost`'s innermost-parent ordering
  to stay manifold at nesting depth ≥ 2 (ADR-279).
- **β-2** Scene wiring: `intersect_faces_inner` calls the general detect+split
  for containment BEFORE `auto_intersect_coplanar` (mirrors the ADR-185 circle
  branch at scene.rs:3163). Must run before the Path B polygonize (Amendment 7)
  so the analytic circle boundary is preserved (polygonizing the disk mismatches
  the ring's analytic hole → open; the reparent avoids polygonization entirely).
- **β-3** regression + browser: adr281_b1 contained → tighten to `closed1`;
  box+circle+contained-rect → closed nm=0; rect-in-rect → closed; corruption
  guard (ADR-282) unaffected. LOCKED #1 amendment (containment auto-split
  re-enabled on solid tops, user-approved) + CLAUDE.md.

## Lock-ins (α)

- **L-283-1** Twin-loop reparent is the manifold-safe mechanism (de-risk proven);
  no polygonization of an analytic (Path B) boundary.
- **L-283-2** General `detect_shape_containment` supersedes the circle-only
  `detect_circle_containment` as the Scene entry (circle case = subset).
- **L-283-3** Innermost-parent ordering (ADR-279) preserved for depth ≥ 2.
- **L-283-4** ADR-282 non-manifold guard remains the backstop (a bad reparent
  that corrupts is still rolled back).
- **L-283-5** LOCKED #1 change — explicit 결재 + new ADR + CLAUDE.md amendment
  (메타-원칙 #10). This α is spec + sim only (no production change).
- **L-283-6** 절대 #[ignore] 금지.

## Cross-link

- ADR-282 (guard non-manifold-only — the deform-not-decline predecessor).
- ADR-185 (`split_face_by_inner_circle` — circle-in-circle, the reparent pattern).
- ADR-279 (`assign_circle_holes_innermost` — innermost-parent, depth ≥ 2).
- ADR-101 §L6 / LOCKED #1 (containment auto hole-injection — this ADR re-enables
  it generally, user-approved).
- ADR-089 (Path B closed-curve self-loop — the analytic boundary to preserve).
- ADR-281 β-1 (partial-overlap crossing split — the sibling case).
- 메타-원칙 #6 (measure-first) / #10 (LOCKED change) / #14 (면은 닫힌 경계로부터).
