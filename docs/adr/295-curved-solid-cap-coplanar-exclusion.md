# ADR-295 — Curved-solid caps are not coplanar candidates (auto-intersect self-hole fix)

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Kernel Robustness / Auto-behaviours

## 1. Problem

Found while auditing IFC export wiring (2026-07-27, main @ `dfb1b52`). With the
production defaults `auto_intersect_on_draw` + `face_rederive_on_draw` **ON**
(LOCKED #76), creating a Path B curved primitive into a scene that already holds
coplanar geometry **damaged that primitive**:

| symptom | measured |
|---|---|
| re-derive path (both flags ON) | the cone's base cap gained an **inner loop anchored at its own outer anchor** — a self-referential hole |
| legacy path (intersect ON, re-derive OFF) | the cap was **deactivated and replaced** by a polygonal face; the Cone side's loop was left dangling (`verts=0`) |
| both | `verify_outward_normals().is_closed_solid` **true → false** |
| both | `verify_face_invariants()` stayed **valid** — topology checks do not catch it |

User-visible as "the first curved primitive exports parametric, later ones fall back
to a brep" — but the real trigger is **coplanarity with existing geometry**, not order.

### Root cause

A Path B primitive's **curved side and its own planar cap share one rim circle**. So
`face_containment_size` reports an identical value for both (π·500² for a r=500 cone),
and the containment pass treats them as container/contained:

```
face 0  base cap   Plane  extract_circle=true  size=785398.16
face 1  cone side  Cone   extract_circle=true  size=785398.16   ← same rim
assign_circle_holes_innermost → assigned = 1 → closed_solid = false
```

ADR-197 already keeps the curved **wall** out of the coplanar passes, but its planar
**cap** passed the filter — and the cap belongs to the same kernel-native solid, so it
is not a free coplanar region either.

Pre-existing: `intersect_faces_inner` is axia-core and predates the ADR-203 §42–§44
export work. That work only *surfaced* it, because the new parametric detectors gate on
`!face.inners().is_empty()` and correctly refuse to export a holed cap as a solid sweep.

## 2. Decision

Two complementary guards, each mutation-verified.

**(a) One structural gate for every containment split** (메타-원칙 #4 SSOT) —
`reject_non_nestable` runs at the top of `split_face_by_inner_circle`,
`split_face_by_inner_circle_generic` and `split_face_by_inner_polygon`, so both the
pairwise legacy path (`detect_*_containment` → split) and the innermost-parent pass
(`assign_*`) are covered by one rule:

1. neither face may carry a curved surface — a wall is not a planar region;
2. the outer must be **strictly larger** than the inner — equal-size regions cannot
   nest, which is exactly the shared-rim case.

**(b) A curved solid's cap is not a coplanar candidate.** New mesh query
`Mesh::face_touches_curved_neighbour(fid)` walks the face's outer loop and each
boundary edge's radial chain (correct for a closed-curve face: one anchor + one
self-loop edge). `Scene::intersect_faces_inner` extends its ADR-197 filter with it, so
the cap never reaches either pass. Guard (a) alone fixes the re-derive path; only (b)
also fixes the legacy path, where the cap was being destroyed outright.

## 3. Lock-ins

- **L-295-1** `reject_non_nestable` is the single gate for all three
  `split_face_by_inner_*` entry points; per-caller copies were removed so there is one
  SSOT.
- **L-295-2** Curved surfaces (Cylinder / Sphere / Cone / Torus / BezierPatch /
  BSplineSurface / NURBSSurface) are never a container or a hole.
- **L-295-3** Equal-size regions never nest (`strictly_larger`, relative 1e-9).
- **L-295-4** A face sharing a boundary edge with a curved face is excluded from
  `intersect_faces_inner` — the ADR-197 rule extended from the wall to its cap.
- **L-295-5** Legitimate containment is untouched: a strictly smaller coplanar circle
  still becomes a hole (ADR-185 / ADR-279 behaviour), asserted by a positive test.
- **L-295-6** The IFC export gates stay strict — do **not** relax them to "fix" this,
  that would fill real bores (ADR-203 §43).
- **L-295-7** 절대 #[ignore] 금지.

## 4. Verification

Regression tests (+4):

- `axia-geo` `curved_cap_selfhole_tests`
  - `a_cones_cap_never_becomes_a_hole_of_its_own_side` — 0 assigned, no face gains a hole
  - `a_cylinders_caps_never_nest_in_each_other` — inner counts unchanged (the Path B
    side legitimately owns one inner loop, ADR-094, so it asserts *no delta*)
  - `a_smaller_coplanar_circle_still_becomes_a_hole` — the guards do not over-block
- `axia-core` `a_cone_stays_a_closed_solid_through_auto_intersect` — all three flag
  combinations a caller can produce ((off,off), (on,off), (on,on)); asserts no spurious
  hole, `is_closed_solid`, and invariants.

**Mutation-checked**: removing both annulus guards fails 2 of the 3 geo tests with the
defect message; removing the cap filter fails the legacy configuration of the core test.

Full workspace **3243 pass / 0 fail**; vitest 2947; tsc 0; web-ifc external validator
exit 0.

**Live** (real Chromium, rebuilt WASM, production defaults ON) — the audit scene that
first exposed this:

```
Box:SweptSolid | Cylinder:SweptSolid | Cone:SweptSolid | Sphere:AdvancedBrep
extruded=2 rect=1 circle=1 revolved=1 advBrep=1
isClosedSolid = true   (was false)   invariants = valid
```

## 5. Remaining

- The carve APIs (`punch_circular_hole`, `drill_circular_through_hole`) still cannot
  operate on a closed-curve cap ("no coplanar face contains the hole center" / "no
  opposite wall along -normal"). Same family (polygon-loop assumptions vs Path B
  self-loops), separate track.
- A genuinely bored curved solid (annulus cap) remains out of scope for the parametric
  IFC export by design — it correctly falls back to a brep.

## Cross-link

ADR-197 (curved faces excluded from the coplanar re-derive — extended here),
ADR-185 / ADR-279 (circle containment), ADR-283 (polygon containment),
ADR-176 / LOCKED #76 (auto-behaviours default ON), ADR-094 (Path B annulus side),
ADR-203 §42–§44 (the export gates that surfaced this), 메타-원칙 #4 / #6.
