# ADR-297 — Punching a hole in a kernel-native closed-curve cap

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Kernel / Carve

## 1. Problem

ADR-295 §5 left this open: the carve APIs could not act on a Path B cap. Measured on
a `create_cylinder_kernel_native_clean` (2 planar caps + 1 cylindrical side, each a
closed-curve face — one anchor vert + one self-loop edge, ADR-089):

```
punch_circular_hole  → Err("no coplanar face contains the hole center …")
drill_circular_through_hole → Err("drill: no opposite wall along -normal …")
```

One assumption going in was wrong and worth recording: the face normal is **not**
degenerate on these faces (`Face::normal()` returns the stored normal, length 1). The
only blocker was the **polygon gate** — every candidate loop was rejected by
`v.len() >= 3`, and a closed-curve face has exactly one vertex.

## 2. Decision

Reuse what already exists rather than teach the carve layer a new boundary type.
`Mesh::face_outline_points` (ADR-105 / ADR-267 follow-up) already returns loop vertex
positions for a polygon face and a **read-only tessellation of the boundary curve** for
a closed-curve face. Three changes:

1. **`carve_face_plane`** takes its polygon from `face_outline_points`, so a Path B cap
   is visible to every carve raycast. It also now **rejects curved surfaces
   explicitly** — that was implicit while only polygons arrived (a Path A cylinder's
   side quads really are planar), but a Path B *side* is a one-vertex closed-curve face
   too, and modelling it as a plane would have silently corrupted every ray hit.
2. **Host search** in `punch_circular_hole` / `punch_rect_hole` / `punch_polygon_hole`
   uses the same outline, so the cap is found.
3. **After host selection**, a closed-curve host is polygonized via the existing
   `polygonize_closed_curve_face` (which carries the analytic surface over) so the
   rebuild has a real outer loop. A polygon host returns `None` and is untouched.

Polygonizing the host is proportionate here: `punch_*` produces a faceted opening by
construction (it takes a `segments` count), so the result is a polygon-with-hole either
way. The read-only outline in (1) and (2) loses nothing — the mesh keeps its curve.

## 3. What is still refused, and why

Through-drilling a Path B solid still fails, one step later than before. The raycast now
finds the far cap (depth measured correctly as 1000 for a 1000-tall cylinder), and both
the entry and exit punches succeed in isolation — but `bridge_through_loops` requires
the exit wall's normal to be anti-parallel to the drill axis, and **every** face of a
Path B cylinder reports a cached normal of `+Z`, the bottom cap included. The guard can
therefore never be satisfied.

That is a property of closed-curve faces' cached normals, not of the drill, and fixing
it means giving the bridge a trustworthy orientation source (the analytic surface normal
with outward sense) — a separate change. `drill_through_a_path_b_solid_is_refused_cleanly`
pins the current behaviour so that work has a baseline.

## 4. Lock-ins

- **L-297-1** `face_outline_points` is the one way carve code obtains a face's outer
  boundary; do not reintroduce a `collect_loop_verts` + `len() >= 3` gate.
- **L-297-2** `carve_face_plane` must reject curved analytic surfaces — it models a
  plane, and closed-curve faces now reach it.
- **L-297-3** A closed-curve punch host is polygonized through
  `polygonize_closed_curve_face` (surface preserved), never by hand.
- **L-297-4** Polygon hosts keep byte-identical behaviour, asserted by a positive test.
- **L-297-5** Through-drill on a Path B solid stays refused with the straight-through
  message until the orientation source is fixed; do not relax the anti-parallel guard.
- **L-297-6** 절대 #[ignore] 금지.

## 5. Verification

Regressions (+4) in `axia-geo` `closed_curve_cap_carve`:
`punch_finds_a_closed_curve_cap_as_its_host` (host found, hole present, invariants
valid, face count unchanged), `punch_rect_and_polygon_reach_a_closed_curve_cap_too`
(all three variants shared the gate), `a_polygon_host_is_untouched_by_the_closed_curve_path`
(box regression guard), `drill_through_a_path_b_solid_is_refused_cleanly` (documents §3).

**Mutation-checked**: restoring the raw-loop-vert host gate fails three of the four.

Workspace **3247 pass / 0 fail**. **Live** (real Chromium, rebuilt WASM): a Path B
cylinder (3 faces — Plane / Plane / Cylinder) accepts `punchHole` on its cap, returning
a face with invariants valid, where it previously errored.

## Cross-link

ADR-295 §5 (this was its listed remainder), ADR-089 (closed-curve faces), ADR-105 /
ADR-267 (`face_outline_points`, `polygonize_closed_curve_face`), ADR-194 (carve query),
ADR-269 / ADR-273 (drill guards), 메타-원칙 #4 / #5 / #6.
