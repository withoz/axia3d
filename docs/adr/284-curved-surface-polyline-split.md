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
- **β-3 dispatch/bridge (browser demo), β-4 open line (S3)**: next.

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

## Cross-link

- ADR-202 (sphere circle, S3/S6 deferred — this ADR takes them up).
- ADR-257 / ADR-263 (cylinder / cone / torus circle — the split template).
- ADR-089 A-χ (split surface inheritance).
- ADR-046 P31 #4 (additive tool dispatch).
- 메타-원칙 #6 (measure-first) / #14 (면은 닫힌 경계로부터).
