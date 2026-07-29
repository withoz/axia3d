# ADR-298 — A shared-rim closed-curve face is not a punch host (ADR-297 correction)

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Kernel / Carve
**Corrects**: ADR-297

## 1. What ADR-297 got wrong

ADR-297 claimed a Path B solid's cap became punchable. Measurement says otherwise, and
its test was too shallow to notice:

| case | ADR-297 result | truth |
|---|---|---|
| top cap punch | `verify_face_invariants().is_valid()` → **passed** | the annulus band's **inner** loop was destroyed; the checker does not walk it |
| bottom cap punch | not covered by any test | `face FaceId(2): cannot collect outer loop: HalfEdge HeId(1) not found` — the band's **outer** loop destroyed |

Root cause: a Path B cap's self-loop boundary edge is **shared** — the annulus band
holds its radial twin. `polygonize_closed_curve_face` deactivates the anchor and the
self-loop and builds fresh vertices, so whatever else was standing on that rim is
orphaned. Measured discriminator: `closed_curve_rim_face_count` returns `2` for every
face of a Path B cylinder and `1` for a standalone drawn-circle sheet.

A second, independent defect shipped with it: the annulus **band** could be selected as
the punch host. It is a one-vertex closed-curve face, so ADR-297's outline lookup
resolved it; its placeholder normal is ±Z, so the same-plane gate passed; and
`analytic_face_area` gives it `2πrh`, which beats a cap's `πr²` whenever `h < r/2`. On
`r=500, h=200` the band actually won the smallest-area tiebreak and a planar punch was
applied to a curved face.

## 2. Decision

1. **A closed-curve face whose rim is shared is never a punch host.** New
   `Mesh::closed_curve_rim_face_count` counts the active faces on the self-loop edge;
   the host search in all three `punch_*` variants skips any face where that is `> 1`.
   The band is excluded by the same rule, so the separate curved-surface filter added
   alongside it was redundant — a mutation test proved it unpinned, and it was removed
   rather than left as protective-looking dead weight.
2. **`carve_face_plane` rejects on boundary shape, not surface kind.** ADR-297 rejected
   any curved surface, which also excluded **Path A facets** — genuine planar quads that
   merely carry a Cylinder/Sphere surface and have always been legitimate ray targets.
   Measured effect: a radial drill on a Path A pipe died at "no opposite wall" instead of
   reaching its real answer. The test is now "curved surface **and** a closed-curve
   boundary", which still rejects a Path B barrel side (genuinely non-planar) and
   restores Path A.

## 3. What ADR-297 actually delivers, restated

A **standalone** closed-curve face — a drawn circle sheet, rim used by one face — is
punchable, and that is verified. Path B solid caps are refused, cleanly, leaving the
mesh untouched.

## 3b. A refused punch used to mutate anyway

A follow-up audit found the same code path leaking a third way. `polygonize_closed_
curve_face` is destructive, and nothing rolls it back: the WASM entry points handle
`Err` with `TransactionManager::cancel`, which clears recording state and restores no
geometry. So a punch that passed host selection and then bailed — an oversized radius,
an opening off the face — returned "failed" while the user's analytic circle had already
become a polygon. Reproduced: a radius-500 sheet punched at radius 600 went from
`outer_loop=1, verts=1` to `outer_loop=23, verts=23` and still reported the error.

Fixed by validating first: new `Mesh::probes_inside_host_outline` tests the opening's
probe points (circle rim / rect corners / polygon points) against the **read-only**
outline, and all three variants call it *before* the polygonize. The authoritative
post-rebuild check is untouched, so this only moves the refusal earlier.

## 4. Lock-ins

- **L-298-1** `polygonize_closed_curve_face` must not run on a shared rim; the host
  search is where that is enforced.
- **L-298-2** Only a standalone closed-curve face may be rewritten by a punch.
- **L-298-3** Planarity guards key on the **boundary shape**, never on surface kind
  alone — a Path A facet is planar and must stay a valid ray target.
- **L-298-4** A refused punch or drill leaves the mesh byte-unchanged (asserted).
- **L-298-5** `verify_face_invariants` cannot by itself prove a rewrite was safe; assert
  loop walkability on the *neighbours* too.

  > ⚠ **Corrected by ADR-306 (2026-07-29).** As written this said the checker "does not
  > walk a face's inner loops". It does — **I3** walks them (null start, collect failure,
  > `< 3` verts, with the ADR-089 1-vert exemption) and has since the `155e127` baseline,
  > i.e. *before* this ADR. The real hole was an **asymmetry**: **I4** verified that
  > half-edges point back at their face for the **outer loop only**, so an inner half-edge
  > repointed at another face was invisible — measured `valid=true, violations=[]`, with
  > `collect_non_manifold_edges` and `detect_self_intersections` both reporting 0 on the
  > same mesh. ADR-306 extends I4 over inner loops. The lock-in's *advice* stands; its
  > stated reason did not.
- **L-298-6** Validate before mutating: the punch family must reject an ill-fitting
  opening *before* `polygonize_closed_curve_face`, because nothing rolls that back.
- **L-298-7** 절대 #[ignore] 금지.

## 5. Verification

Seven regressions in `closed_curve_cap_carve`, replacing ADR-297's two optimistic ones:
`a_shared_rim_cap_is_never_a_punch_host` (both caps, mesh untouched, **every neighbour's
outer loop still walkable** — the assertion ADR-297 lacked),
`a_standalone_closed_curve_face_is_punchable` (the real capability),
`a_curved_band_is_never_a_punch_host` (the `h < r/2` tiebreak regime),
`rect_and_polygon_punches_also_refuse_a_shared_rim_cap`,
`a_path_a_facet_stays_a_valid_ray_target` (axial drill still `Ok`; radial reaches its own
rejection, not "no opposite wall"),
`a_polygon_host_is_untouched_by_the_closed_curve_path`, `a_refused_punch_leaves_the_host_untouched`
(§3b: the face survives, still one anchor vertex, no leaked tessellation verts), and the
drill baseline.

**Mutation-checked**: removing the rim guard fails four; widening `carve_face_plane` back
to surface-kind fails the Path A pin.

Workspace **3251 pass / 0 fail**.

## 6. Remaining

Through-drilling a Path B solid is still refused — now at the entry punch. Beyond a
punchable cap representation it also needs a trustworthy outward normal: every face of a
Path B cylinder reports a cached `+Z`, bottom cap included, and its `Plane` surface says
`+Z` too, so `bridge_through_loops`' anti-parallel guard cannot be satisfied. Cone and
sphere caps do carry `NEG_Z`, so "prefer the analytic surface" would be right for them
and silently wrong for cylinder — the fix belongs at the cylinder constructor, and
`polygonize_closed_curve_face` would have to honour the face normal rather than
re-deriving winding from the shared curve. Separate ADR.

## Cross-link

ADR-297 (corrected here), ADR-089 (closed-curve faces), ADR-105 / ADR-267
(`polygonize_closed_curve_face`, `face_outline_points`), ADR-183 (outward caps),
ADR-269 (cross-drill guard), 메타-원칙 #4 / #6.
