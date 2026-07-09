# ADR-285 — Parametric Direct Edit of Analytic Curved Faces (α spec)

**Status:** Proposed (α spec + de-risk sim only — β implementation needs 결재)
**Date:** 2026-07-09
**Track:** "진짜 analytic 곡면 커널 편집" — 사용자 결재 방향 = **파라메트릭 직접 편집**

## 1. Premise correction (measure-first, this session)

The track was framed on the premise that a cylinder side is "~22 planar facets"
and that a planar `fold_across_edge` is why true-analytic editing is missing. A
full audit **disproved that for this repo (E:/AXiA3D)**:

- Path B cylinder side = **ONE `AnalyticSurface::Cylinder` face** (self-loop rim,
  `inners=1`); sphere = 2 hemispheres; cone = 2 faces; torus = 1 face — all single
  analytic faces (`mesh.rs:7464~7826`). Path A cylinder = 1 analytic face per quad
  (u_range-parameterized, `primitives.rs:94`), still not a facet pool.
- `fold_across_edge` **does not exist** in the repo (0 matches).
- The "22 facets" are **render tessellation** (`tessellate_face_surface`,
  chord_tol 0.02 mm, `mesh_export.rs:42`) — surface is truth, polygons are a
  view-time artifact.

So surfaces are **already analytic**. The real gap is that there is **no way to
edit an analytic face's defining PARAMETERS in place** — the only path today is
full surface replacement + manual boundary rebuild.

## 2. Goal

Select a curved analytic face (Cylinder / Sphere / Cone / Torus) and change its
**defining parameters** (radius / height / etc.) such that the analytic surface
AND its boundary geometry update **in place** — topology (FaceId / edges / anchor)
preserved, so owner tracking, selection, and manifold validity survive.

Non-goal (this ADR): NURBS/Bezier/BSpline control-point or knot editing (that
depends on Path X tensor-UV-inversion — separate track). Non-uniform → generalized
surface promotion (ADR-053 Phase J, deferred).

## 3. De-risk (landed sim, `adr285_sim_sphere_radius_parametric_edit`)

Sphere radius 10 → 15 in place, using APIs that **already exist**:

1. `set_curve_radius(equator_edge, 15)` — updates the equator `Circle` curve radius
   **and** moves the anchor vertex to `center + basis_u·15` (one call, mesh.rs:10056).
2. `set_face_surface(hemisphere, Sphere { radius: 15, .. })` × 2 — swaps the surface
   + bumps `surface_version` so the render cache re-tessellates.

**Measured:** topology identical (2 faces), north surface radius = 15, anchor at
(15,0,0), tessellation now lies on r=15 (max dev < 0.15 mm), `verify_face_invariants`
valid. Topology is unchanged by construction → manifold preserved for free.

This proves the **mechanism**: parametric edit = `set_curve_radius` on each rim +
`set_face_surface` on each surface-bearing face. No DCEL surgery, no rebuild.

## 4. Mechanism per primitive (design)

| Primitive | Faces | Rims (self-loop Circle) | Radius edit | Height edit |
|---|---|---|---|---|
| **Sphere** | 2 hemispheres | 1 equator (shared) | `set_curve_radius(eq)` + 2× `set_face_surface` | n/a |
| **Cylinder** | base + top + side | 2 (bottom + top, each shared with a cap) | `set_curve_radius`×2 + side `set_face_surface` | move top rim anchor +z + side `v_range` |
| **Cone** | base + side | 1 base (shared with disk) | `set_curve_radius(base)` + side `set_face_surface` (half_angle recompute) | move apex + side surface |
| **Torus** | 1 | self-loop seam | major/minor via `set_face_surface` + seam curve | n/a |

Caps (Plane) have no radius param — their boundary IS the rim edge, so they follow
automatically when the rim's `set_curve_radius` moves the shared anchor. **This is
the same shared-rim insight as ADR-284 β-4** (the rim edge is shared; updating it
updates both incident faces).

## 5. Scope decisions (Q1~Q5 — 결재 필요)

- **Q1 (MVP order)** — recommend **Sphere → Cylinder → Cone → Torus** (simplest →
  most complex; sphere is 1 param + de-risked). Each its own atomic sub-step
  (LOCKED #44). Alt: Cylinder first (most common). **추천: Sphere first.**
- **Q2 (editable params)** — Sphere: `radius`. Cylinder: `radius` + `height`.
  Cone: `radius` + `height`. Torus: `major_radius` + `minor_radius`. (center/axis =
  already Move/Rotate via `transform`.) **추천: 위 표.**
- **Q3 (in-place vs rebuild)** — **in-place** (de-risked; preserves FaceId / owner /
  selection / Undo granularity). **추천: in-place.**
- **Q4 (UI trigger)** — MVP: select a curved face → **Inspector shows editable
  numeric params** → type a value → engine call. Drag-handles = future. Engine API
  `set_cylinder_radius(face)` etc. + WASM + bridge + Inspector wiring. **추천:
  numeric Inspector.**
- **Q5 (scope boundary)** — **whole primitive faces only** for MVP (a face whose
  surface is a single Cylinder/Sphere/Cone/Torus + self-loop rim). Split/trimmed
  curved faces (partial u/v range from ADR-284) + NURBS-family = **out of scope**
  (deferred; NURBS needs Path X). **추천: primitive faces only.**

## 6. Lock-ins (proposed, β 확정 시)

- **L-285-1** In-place edit — topology (FaceId/edges/anchor) preserved; no rebuild.
- **L-285-2** Reuse existing primitives (`set_curve_radius` + `set_face_surface`);
  no new DCEL surgery.
- **L-285-3** Shared-rim consistency — editing a rim's `set_curve_radius` updates
  BOTH incident faces (surface-bearing + cap) via the shared anchor (ADR-284 β-4
  insight).
- **L-285-4** `surface_version` bump → render cache re-tessellates (no stale mesh).
- **L-285-5** Manifold preserved by construction (topology unchanged) — regression
  asserts `verify_face_invariants` valid after each edit.
- **L-285-6** Transaction-wrapped (single Undo per edit) + owner tracking intact.
- **L-285-7** Primitive faces only (Q5); NURBS-family + split/trimmed faces deferred.
- **L-285-8** Additive (ADR-046 P31 #4) — new APIs, existing ops UNCHANGED.
- **L-285-9** 절대 #[ignore] 금지.

## 7. β sub-step roadmap (각 atomic PR)

- **β-1 Sphere** radius — ✅ **LANDED (2026-07-09)**.
- **β-2 Cylinder** radius + height — ✅ **LANDED (2026-07-09)**.
- **β-3 Cone** radius + height (half_angle recompute; apex move).
- **β-4 Torus** major + minor.
- **β-5** Inspector UX polish + real-Chromium demo sweep.

### β-1 Sphere radius — LANDED (2026-07-09)

Full stack, all 5/5 Q recommendations:
- **Engine** `Mesh::set_sphere_radius(face, r)` — given any one hemisphere, finds
  the twin (radial twin of the equator HE) + does `set_curve_radius(equator)` (rim
  Circle + anchor) + `set_face_surface` on both hemispheres. Rejects non-Sphere /
  r≤0. Topology unchanged → manifold by construction.
- **Scene** `Scene::set_sphere_radius` — transaction-wrapped (single Undo). No
  owner reconcile needed (faces unchanged).
- **WASM** `setSphereRadius(faceId, radius) -> bool` (additive).
- **Bridge** `WasmBridge.setSphereRadius(faceId, radius)` (guards r>0).
- **UI** XiaInspector — a single Sphere-face selection injects a "곡면 반지름 (mm)"
  numeric field (`#xi-curved-edit`) into `#xi-content`; change/Enter →
  `bridge.setSphereRadius` + `toolManager.syncMesh()`. Hidden for non-Sphere /
  empty / edge selections.
- **Regression**: axia-geo `adr285_beta1_set_sphere_radius` (one-face API →
  both hemispheres + equator + tessellation update, topology unchanged, manifold;
  reject non-Sphere/r≤0) + `adr285_sim_sphere_radius_parametric_edit` (de-risk) +
  vitest WasmBridge ×2 (forwards / rejects r≤0).
- **Real-WASM browser**: `create_sphere` → bridge `setSphereRadius(0, 18)` → both
  hemispheres r=18, faces 2, nm=0, valid; **Undo → r=10**. Inspector UI: select
  hemisphere → radius field shows "10" → type "20" + change → both hemispheres
  r=20, faces 2, nm=0, valid, viewport synced.

### β-2 Cylinder radius + height — LANDED (2026-07-09)

Path B cylinder (measured): base cap (Plane z=0) + top cap (Plane z=h) + side
(Cylinder, `v_range=(0,h)`) + 2 rims (bottom/top self-loop Circles, each shared
with a cap).

- **Engine** `Mesh::set_cylinder_radius(side, r)` — `set_curve_radius` on BOTH
  rims (moves anchors) + side surface radius; caps follow via shared anchors.
  `Mesh::set_cylinder_height(side, h)` — keeps base fixed, moves the TOP rim
  (anchor + Circle center by `axis_dir·Δh`) + side `v_range → (v_lo, v_lo+h)` +
  top cap Plane origin `+Δh`. Top rim/cap found by axial coord. Reject non-Cylinder
  annulus / non-positive. Topology unchanged → manifold.
- **Scene** `set_cylinder_radius` / `set_cylinder_height` — transaction-wrapped.
- **WASM** `setCylinderRadius` / `setCylinderHeight` (additive).
- **Bridge** `WasmBridge.setCylinderRadius` / `setCylinderHeight` (guard >0).
- **UI** XiaInspector — the curved editor is now surface-kind-aware: a Cylinder
  side (kind 2) selection shows **radius + height** fields; Sphere (kind 3) shows
  radius. Change/Enter → the matching bridge call + `syncMesh`.
- **Regression**: axia-geo `adr285_beta2_set_cylinder_radius_and_height`
  (radius+height update, topology unchanged, manifold; reject non-Cylinder/≤0) +
  de-risk sims (`adr285_beta2_sim_cylinder_structure`,
  `..._radius_height_edit`) + vitest WasmBridge ×2.
- **Real-WASM browser**: `create_cylinder(r10,h20)` → `setCylinderRadius(side,6)`
  + `setCylinderHeight(side,30)` → radius 6 / height 30, faces 3, `verifyInvariants`
  valid (0 viol). Inspector: select side → 2 fields ("반지름"/"높이") showing
  6/30. (Note: `meshManifoldInfo` reports nm=1 on a Path B cylinder — the known
  self-loop-rim artifact, pre-existing + independent of this edit;
  `verify_face_invariants` is authoritative + valid.)

## 8. Cross-link

- Audit (this session) — surfaces already analytic; `fold_across_edge` absent.
- ADR-031 Phase D (AnalyticSurface storage), ADR-033 (NURBS surfaces).
- ADR-094/104/113/114/115 (Path B primitives — the edit targets).
- ADR-284 β-4 (shared-rim insight — rim edge shared by two faces).
- ADR-089 A-χ (surface inheritance).
- ADR-053 Phase J (non-uniform → NURBS promote, deferred).
- Path X (tensor UV inversion — NURBS edit prerequisite, separate track).
- 메타-원칙 #4 (SSOT) / #5 (사용자 편의) / #6 (Preventive/measure-first) / #13
  (surface=truth, mesh=view).
- LOCKED #44 (Complete Meaning per Merge — β sub-steps atomic).
