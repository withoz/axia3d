# ADR-284 — Curved-Surface Polyline Split (line / crossing / freehand / bezier on sphere·cylinder·cone·torus) — α spec + de-risk sim

- **Status**: Proposed (α spec + de-risk sim landed 2026-07-08; β implementation pending 결재)

## Context

Curved-surface sketching today handles ONLY closed **circles** (ADR-202 sphere,
ADR-257 cylinder, ADR-263 cone/torus). The user asked to extend curved-face
division to **lines, crossing shapes, freehand, and bezier** (ADR-202 S3/S6
deferred). Measure-first audit + de-risk before any production change.

## Audit — draw tool × surface matrix (grep-verified 2026-07-08)

| draw tool | planar face | curved face (sphere/cyl/cone/torus) |
|---|---|---|
| Circle | ✅ | ✅ `drawCircleOn{Sphere,Cylinder,Cone,Torus}` (surfaceKind 1..4) |
| Rect / Polygon / Ellipse | ✅ | ❌ no curved dispatch |
| **Line** | ✅ | ❌ no curved dispatch (ADR-202 S3) |
| **Freehand / Bezier / Spline / Arc** | ✅ | ❌ no curved dispatch |

Only `DrawCircleTool` has curved-surface `surfaceKind` dispatch (4 branches).
Every other draw tool has ZERO curved awareness (grep: `surfaceKind===` count 0).

Engine curved-split capability: `split_{sphere,cylinder,cone,torus}_face_by_circle`
(mesh.rs). The planar `split_circle_face_by_{chord,line}` are for a flat disk,
NOT a 3D curved surface.

## Key finding (de-risk-proven) — the engine split is SHAPE-AGNOSTIC

`split_{surface}_face_by_circle(samples: &[DVec3])` is a **misnomer**: it splits
a curved face by ANY closed on-surface polyline — it validates the samples lie on
the surface, builds the loop, and reparents the N-edge twin ring as the host's
hole (cap + remainder, ADR-089 A-χ surface inheritance). The "circle" is only
because the CALLER (`circle_on_{surface}`) generates circle samples.

**De-risk sim `adr284_sim_rect_polyline_on_cylinder_splits`** (landed, mesh.rs):
a RECT-shaped geodesic loop (4 UV corners + edge-sampled, exactly how a rect /
freehand-closed / bezier-closed shape projects) fed to
`split_cylinder_face_by_circle` → cap + remainder, `verify_face_invariants`
valid, Cylinder inherited. **The split already works for non-circle closed
polylines.**

⇒ The gap for **closed** shapes on curved faces is NOT the split — it's:
1. **Projection**: a non-circle shape's points → on-surface geodesic samples
   (`circle_on_{surface}` unrolls to flat UV, samples, maps back — the SAME
   technique generalizes to a rect/polygon/freehand/bezier loop).
2. **Tool dispatch**: rect/polygon/freehand/bezier need `surfaceKind` dispatch
   → a `drawPolylineOn{surface}` bridge call (mirror of DrawCircleTool).

**Open line (S3)** is genuinely different: an open polyline rim-to-rim splits a
face into 2 halves — a NEW DCEL surgery (split the host boundary at the 2
endpoints + insert the polyline), not the closed-loop cap. ADR-202's degeneracy
(a line A→B on a full sphere with no boundary) lives here. → Phase 2.

## Phased β plan (pending 결재 — multi-week, each phase its own gate)

- **β-1 projection**: `polyline_on_{cylinder,sphere,cone,torus}(pts, closed)` —
  generalize `circle_on_{surface}` to an arbitrary drawn polyline (unroll → flat
  sample → map back), reusing each surface's project/evaluate. Closed = cap loop.
- **β-2 engine split (closed)**: reuse `split_{surface}_face_by_circle` as-is
  (proven shape-agnostic). Optionally rename to `..._by_polyline` for clarity.
- **β-3 tool dispatch (closed shapes)**: Rect / Polygon / Ellipse / Freehand
  (closed) / Bezier (closed) gain `surfaceKind` dispatch → `drawPolylineOn{surface}`
  bridge (mirror DrawCircleTool). Covers **S6** + closed freehand/bezier.
- **β-4 open line (S3)**: new rim-to-rim split (boundary-split + insert). Line /
  open freehand / open bezier. Sphere degeneracy handled per-surface.
- **β-5** regression + browser (draw a rect/freehand/bezier loop on a cylinder /
  sphere → cap + remainder, manifold) + LOCKED note.

## β progress

- **β-1 cylinder (landed 2026-07-08)**: `cylinder::polyline_on_cylinder(pts,
  closed, chord_tol)` — projects arbitrary world points onto the cylinder,
  unrolls `u` continuously, chord-samples each edge in flat UV, maps back. Wrap
  guard: CLOSED loops reject when the signed angular winding `> π` (a simple
  non-encircling loop winds ≈ 0, an encircling one ≈ ±2π); OPEN loops reject a
  ≥ full-turn span. Tests: `polyline_on_cylinder_rect_samples_on_surface` /
  `_rejects_full_wrap`; the de-risk sim `adr284_sim_rect_polyline_on_cylinder_
  splits` now drives the REAL function end-to-end (rect world corners →
  polyline_on_cylinder → split_cylinder_face_by_circle → cap + remainder,
  manifold, Cylinder inherited). axia-geo **2198**.
- **β-1 cone (landed 2026-07-08)**: `cone::polyline_on_cone(...)` — same
  technique (cone is developable); wrap guard = signed-`u`-winding (the
  `v·tanα` radius scaling doesn't change encircling). Tests:
  `polyline_on_cone_rect_samples_on_surface` + end-to-end sim
  `adr284_sim_rect_polyline_on_cone_splits` (rect → split_cone_face_by_circle →
  cap + remainder, manifold, Cone inherited). axia-geo **2200**.
- **β-1 torus (landed 2026-07-08)**: `torus::polyline_on_torus(...)` — torus is
  doubly-periodic, so BOTH `u` (major) and `v` (minor) are unrolled + the
  encircle guard checks BOTH windings. Reuses the samples-based
  `split_torus_face_by_circle`. Tests: `polyline_on_torus_rect_samples_on_surface`
  / `_rejects_major_wrap` + end-to-end `adr284_sim_rect_polyline_on_torus_splits`.
- **β-1 sphere (landed 2026-07-08)**: `sphere::polyline_on_sphere(...)` — `u`
  (longitude) wraps, `v` (latitude) is pole-bounded (reject `|v| → π/2`). The
  sphere `split_by_circle` takes an analytic `Circle`, so a polyline needs the
  NEW samples-based `Mesh::split_sphere_face_by_polyline` (N-edge loop +
  twin-reparent, the sphere analogue of the cylinder split). Tests:
  `polyline_on_sphere_rect_samples_on_surface` / `_rejects_pole` + end-to-end
  `adr284_sim_rect_polyline_on_sphere_splits`.
- **β-1 COMPLETE — all 4 surfaces** (cylinder / cone / torus / sphere) project +
  split a closed polyline (rect / polygon / freehand / bezier) → cap + remainder,
  manifold, surface inherited. axia-geo **2206**.
- **β-3 dispatch/bridge (landed 2026-07-08)** — the closed-shape path is now
  wired end-to-end:
  - Scene: `draw_polyline_on_{cylinder,cone,torus,sphere}(host, pts, closed)` +
    shared `finish_polyline_split` (dual-path owner reconcile + transaction,
    mirror of `draw_circle_on_*`).
  - WASM: `drawPolylineOn{Cylinder,Cone,Torus,Sphere}(face, flat, closed)`
    (flat `[x,y,z,…]`); `wasm_export_baseline_unchanged` still passes (additive).
  - Bridge: `drawPolylineOnCurved(kind, faceId, pts, closed)` dispatcher.
  - Tool: `DrawRectTool` — first click on a curved face (surfaceKind 2/3/4/5)
    captures the host + kind; second click projects the 4 tangent-plane corners
    + calls `drawPolylineOnCurved` (draw a rect ON a cylinder/sphere/cone/torus).
  - **Browser-verified** (Path B cylinder side face): `drawPolylineOnCurved(
    'cylinder', …)` → `{cap:3, annulus:2}`, `verifyInvariants` valid (0
    violations). (`meshManifoldInfo` nm=1 is the PRE-EXISTING Path B cylinder
    self-loop-rim discrepancy — present before the split too, not added by it.)
- **β-3 tools + projection fix (landed 2026-07-08)**:
  - **Projection relaxed**: `polyline_on_{surface}` no longer rejects a point
    that is off the surface by > 1e-3 — tool-drawn points lie on the TANGENT
    plane at the pick (off by the sagitta), so the function now PROJECTS them
    (its actual job); `project_to_{surface}` still returns None for
    un-projectable input (axis / apex / center / NaN). Without this the real
    tools (which pass tangent-plane points) would always be rejected.
  - **DrawPolygonTool / DrawFreehandTool / DrawBezierTool** wired (mirror
    DrawRectTool): first click on a curved face captures host + kind; on commit
    the shape's world boundary points → `drawPolylineOnCurved`. Freehand/Bezier
    only dispatch when the loop is CLOSED (freehand: ends within 20% of bbox
    diagonal; bezier: A-ψ P3≈P0 closure) — open strokes are β-4.
  - **Browser-verified** with TANGENT-PLANE (off-surface) corners:
    `drawPolylineOnCurved('cylinder', …)` → `{cap:3, annulus:2}`,
    `verifyInvariants` valid (0 violations) — confirms the projection fix makes
    the real tool path work.
  - **β-3 COMPLETE — closed shapes (rect / polygon / freehand / bezier) on all 4
    curved surfaces from the UI.** axia-geo 2206, vitest 2508.
## β-4 (open line S3) — measure-first findings (2026-07-08)

Two probe sims (axia-geo `mesh::tests`, landed) scoped β-4 before implementation:

- **`adr284_sim_curved_face_boundary_characterization`** — ALL curved faces have
  a **self-loop (1-HE) analytic rim** boundary: hemisphere = 1 equator (Sphere,
  inners=0); cylinder side = 1 outer rim + **inners=1** (2 rims = annulus); cone
  side = 1 base rim (Cone, apex degenerate); torus = 1 seam. So "open line
  rim-to-rim" must SPLIT a self-loop rim at 2 points — the planar
  `split_face_by_chain` polygonizes the rim (loses the surface), so it is NOT
  directly reusable.
- **`adr284_sim_open_chord_on_hemisphere_probe`** — the EXISTING
  `split_circle_face_by_chord` (trim rim → arcs via `trim_circle_face_at_
  crossings` (surface-preserving) → straight chord → `split_face_by_chain`)
  ALREADY works on a hemisphere: `Ok(Some(2))`, faces 2→3, **both pieces inherit
  Sphere** (`sphere_faces=3`). BUT `verify_face_invariants` = **1 violation** —
  the straight chord is a secant THROUGH the sphere (off-surface), and the seam
  is a chord, not a geodesic.

**β-4 design (scoped):** reuse the rim-trim + sub-face + surface-inherit machinery
(`trim_circle_face_at_crossings` + `split_face_by_chain`); replace the straight
chord with an on-surface **geodesic seam** — `sphere_great_circle_arc` (exists!)
for the sphere, `polyline_on_{cone}` (open) for the cone. That lands the seam on
the surface, which also should clear the 1 invariant violation.

**β-4 tractability by surface:**
- **hemisphere / cone side** (1 rim, inners=0) → a rim-to-rim seam splits into 2.
  TRACTABLE (chord machinery + geodesic seam).
- **cylinder side** (2 rims, annulus) → a top-rim→bottom-rim cut UNROLLS the
  annulus into 1 sheet (not 2); 2 pieces need 2 cuts, or a same-rim seam —
  different topology, separate sub-step.
- **torus** (seam) → similar multi-loop subtlety, separate sub-step.

**β-4 β plan:** β-4-1 sphere geodesic-seam rim split (hemisphere) + viol fix;
β-4-2 cone; β-4-3 dispatch (DrawLineTool / open freehand / open bezier on a
curved face); β-4-4 cylinder/torus multi-rim. Each measure-first + de-risk.

### β-4-1 attempt — deeper finding (2026-07-09): the shared self-loop rim

The β-4-1 geodesic-seam implementation (`split_sphere_face_by_seam`, attempted +
reverted) **disproved the scoped design's core assumption** that
`trim_circle_face_at_crossings` is reusable for a hemisphere. Two blockers, both
now measured:

1. **Shared self-loop equator (the real blocker).** A Path B sphere is TWO
   hemispheres sharing ONE equator self-loop edge (`create_sphere_kernel_native`
   mesh.rs:7367 — `he_fwd` = north outer, `he_bwd` = south outer, same edge).
   `trim_circle_face_at_crossings` (mesh.rs:8199-8201) does `remove_face(north)`
   **+ `remove_edge_and_halfedges(equator)`** — removing the shared edge breaks
   the SOUTH hemisphere's boundary (`verify_face_invariants` → "face FaceId(1):
   … HalfEdge HeId(1) not found"). The trim machinery is built for a STANDALONE
   self-loop circle (a flat cap / drawn disk), NOT a shared-boundary hemisphere.
   This is exactly why S9 (interior circle) works — the circle is interior, the
   equator is untouched — and S3 (rim-to-rim line) does not: it must operate ON
   the shared rim.
2. **Equator-geodesic degeneracy (ADR-202 S3 resurfacing).** The great circle
   between two equator points IS the equator (both lie in the z=0 plane through
   the center) → a slerp geodesic seam lands ON the rim, so its interior verts
   sit on the boundary loop → `split_face_by_chain` rejects ("intermediate chain
   vert on chosen loop"). Corollary: the seam is NOT a geodesic — it is the
   **user's drawn stroke** projected onto the sphere (rim → interior → rim), so
   the API must take seam POINTS, not endpoints + a computed geodesic.

**Revised β-4-1 scope (the real work).** A rim-to-rim split on a hemisphere must
**split the shared self-loop equator in-place at the 2 endpoints** (both twin HEs
→ 2 arc edges, so BOTH hemispheres see the 2 arcs) WITHOUT removing it, then
insert the projected interior seam into only the target hemisphere and re-tile
that one side. This is substantial shared-boundary DCEL surgery, distinct from
the standalone-circle trim machinery.

### β-4-1 ENGINE LANDED (2026-07-09): `split_sphere_face_by_open_seam`

De-risk sim (`adr284_beta4_sim_shared_equator_split`, in the transcript) proved a
**rebuild** realization of the shared-boundary split — manifold-clean on the first
iteration (contrary to the "several de-risk iterations" estimate). The mechanism
avoids a bespoke in-place self-loop split by reusing the standalone trim + a twin
rebuild:

1. Capture the **twin hemisphere** = the radial twin (`next_rad`) of the host's
   equator self-loop HE, plus its surface, BEFORE any mutation (the twin `FaceId`
   stays valid across the trim — trim breaks its loop but does not remove the face).
2. `trim_circle_face_at_crossings(host, [A, B])` → an arc ring (4-vert, D7
   midpoints) + the 2 crossing verts. This breaks the twin's loop as expected.
3. **Rebuild the twin**: deactivate the broken face + `add_face_with_holes(ring
   reversed)` — the reversed ring reuses the arc edges' FREE twin HE slots, so each
   arc edge ends up with 2 HEs (host-side + twin-side) → manifold.
4. Split the host arc-face by the drawn seam `[vA, interior…, vB]` via
   `split_face_by_chain`. Both host pieces inherit the host `Sphere`; the twin keeps
   its `Sphere` (ADR-089 A-χ).

Result: **3 faces (2 host pieces + 1 twin), `verify_face_invariants` valid, all 3
inherit Sphere.** Regression: `adr284_beta4_sphere_open_seam_splits_manifold`
(3 faces / valid / 3 sphere) + `adr284_beta4_open_seam_rejects_bad_input` (< 3
seam points, non-Sphere → graceful `None`). axia-geo 2208 → 2210, 0 fail.

### β-4-1 tool-dispatch geometry finding (deferred to β-4-3)

`split_sphere_face_by_open_seam` requires ≥ 3 seam points (2 rim + **≥ 1 interior
point arcing OVER the hemisphere**). This surfaces a UX subtlety for the draw
tools: **a straight 2-click `DrawLine` between two equator points is degenerate.**
Both endpoints lie in the z=0 plane, the straight chord between them lies in z=0,
and radial projection preserves z=0 → every projected point lands back on the
equator (no interior). This is the ADR-202 S3 degeneracy again: there is no
canonical geodesic between two equator points that arcs over the hemisphere (the
unique great circle through them IS the equator). So a valid interior seam must be
a **drawn multi-point stroke** that leaves the equator plane — the natural fit is
**open Freehand / open Bezier** (which already produce multi-point strokes), NOT a
straight `DrawLine`. β-4-3 tool dispatch should route open freehand/bezier strokes
on a Sphere face to `split_sphere_face_by_open_seam`, and either reject or
re-interpret a straight rim-to-rim `DrawLine`. (Engine capability is complete +
tested; the tool UX is a distinct decision.) S9 (closed shapes, all 4 surfaces, 4
tools) remains complete and unaffected.

## Lock-ins (α)

- **L-284-1** Engine split is shape-agnostic (de-risk proven) — closed shapes
  reuse `split_{surface}_face_by_circle`; no new closed-split surgery.
- **L-284-2** Closed vs open split are DIFFERENT mechanisms: closed = cap-loop
  reparent (exists); open (S3) = boundary-split + insert (Phase 2, new).
- **L-284-3** Projection generalizes `circle_on_{surface}` (unroll→sample→map).
- **L-284-4** Surface inheritance (ADR-089 A-χ) + manifold preserved by the
  existing split; new projection must keep samples on-surface + non-wrapping.
- **L-284-5** Tool dispatch mirrors DrawCircleTool `surfaceKind` (additive; no
  planar-draw change, ADR-046 P31 #4).
- **L-284-6** α is spec + sim only (no production change). Multi-week β needs
  explicit 결재.
- **L-284-7** 절대 #[ignore] 금지.

## Wiring + menu/toolbar re-review (2026-07-08, post-β-3)

Full re-verification of the curved-sketch closed-shape path:

**Layers — all present + consistent:**
- Engine: `polyline_on_{cylinder,cone,torus,sphere}` (project) + split
  (`split_{cyl,cone,torus}_face_by_circle` reused, `split_sphere_face_by_polyline`
  new).
- Scene: `draw_polyline_on_{cylinder,cone,torus,sphere}` (4) + shared
  `finish_polyline_split`.
- WASM: `drawPolylineOn{Cylinder,Cone,Torus,Sphere}` (4);
  `wasm_export_baseline_unchanged` passes (additive).
- Bridge: `drawPolylineOnCurved(kind,…)` dispatcher + 4 interface decls.

**Tool dispatch — consistent across all draw tools:**
- Circle → `drawCircleOn{surface}` (analytic Circle, ADR-202/257/263).
- Rect / Polygon / Freehand / Bezier → `drawPolylineOnCurved` (polyline,
  ADR-284) — each has the `surfaceKind 2/3/4/5` detect + host pick + closed-loop
  dispatch (freehand/bezier gate on closure). grep-confirmed 4/4.

**Menu/toolbar — UNCHANGED (additive-only, ADR-046 P31 #4):** the curved branch
is INTERNAL to the existing rect/polygon/freehand/bezier tools — no new command,
export-on-menu, `MenuBar` entry, `index.html` toolbar entry, or ActionCatalog
/CommandCatalog change (grep-confirmed empty). ActionCatalog ⊇ CommandCatalog
intact (CatalogConsistency 3/3).

**Verified:** full workspace cargo (axia-geo 2206 / core / wasm / transaction)
0 failed; vitest 2508 / 1 skipped; tsc 0; ADR-catalog check pass; browser
(on-surface AND tangent-plane corners) → `{cap, annulus}` + verifyInvariants
valid.

## Cross-link

- ADR-202 (sphere circle, S3/S6 deferred — this ADR takes them up).
- ADR-257 / ADR-263 (cylinder / cone / torus circle — the split template).
- ADR-089 A-χ (split surface inheritance).
- ADR-046 P31 #4 (additive tool dispatch).
- 메타-원칙 #6 (measure-first) / #14 (면은 닫힌 경계로부터).
