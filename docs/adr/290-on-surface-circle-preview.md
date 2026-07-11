# ADR-290 — On-Surface DrawCircle Preview (곡면 편집 마무리)

- **Status**: Accepted
- **Date**: 2026-07-11
- **Category**: Curved-Surface Sketching / UX
- **Supersedes**: —
- **Related**: ADR-202 (sphere circle sketching), ADR-257 (cylinder), ADR-263
  (cone/torus), ADR-287 §H (live curved carve preview — read-only ghost pattern),
  ADR-089 (closed-curve face), ADR-031 Phase D (AnalyticSurface)

## 1. Context

ADR-202 / ADR-257 / ADR-263 shipped **drawing a circle ON a curved host face**
(Sphere/Cylinder/Cone/Torus) — the *commit* path is correct: the tool routes to
`drawCircleOnSphere` / `drawCircleOnCylinder` / `drawCircleOnCone` /
`drawCircleOnTorus`, which build the on-surface circle and split the face into
cap + remainder (both inherit the host surface, ADR-089 A-χ).

But the **live preview** (`DrawCircleTool.updatePreview`) always drew a FLAT
tangent-plane circle — `center + r·cos·right + r·sin·up + n·0.5` — even in the
curved modes. On a sphere the preview floated off the surface (a flat disk at the
tangent plane) and did not match the committed result: the user saw a flat circle
while dragging, then a surface-hugging circle after the click. A measure-first
audit confirmed `updatePreview` had no curved branch; the engine helpers
`circle_on_{sphere,cylinder,cone,torus}` already existed as read-only material.

This is the user-facing finish (사용자 facing 마무리) of the curved-surface
sketching family: the preview should FOLLOW the surface it will be drawn on.

## 2. Decision

Add a **read-only** engine query `Mesh::preview_circle_on_surface(host_face,
center_pt, radius_pt) -> Option<Vec<f32>>` that returns the on-surface circle as
a flat xyz polyline, and wire it through WASM → bridge → `DrawCircleTool`. In the
tool's `onMouseMove`, when the active host is curved (Sphere/Cylinder/Cone/Torus),
draw the on-surface polyline instead of the flat tangent circle; fall back to the
flat preview when the engine returns nothing (non-curved face / degenerate).

The query re-uses the existing `circle_on_{sphere,cylinder,cone,torus}` helpers
(the same ones the commit path uses), so the preview traces exactly what will be
committed. It is `&self` (no mutation) — safe to call every mouse-move, matching
the ADR-287 §H read-only ghost pattern.

## 3. Lock-ins (L-290-1 ~ L-290-9)

- **L-290-1** `preview_circle_on_surface` is READ-ONLY (`&self`) — safe every
  mouse-move (ADR-287 §H pattern).
- **L-290-2** Re-uses the commit-path helpers (`circle_on_sphere` →
  `AnalyticCurve::Circle`, `circle_on_{cylinder,cone,torus}` → `Vec<DVec3>`), so
  the preview matches the committed geometry exactly (메타-원칙 #4 SSOT).
- **L-290-3** Sphere / Cylinder / Cone / Torus only; a non-curved (Plane) face
  → `None`, and the tool falls back to its existing flat preview.
- **L-290-4** Chord tolerance `TOL = 0.05` for the non-analytic surfaces
  (cylinder/cone/torus) — preview granularity, independent of commit.
- **L-290-5** Engine → WASM (`previewCircleOnSurface`) → bridge
  (`previewCircleOnSurface`, graceful `null`) → `DrawCircleTool.updateCurvedPreview`
  full chain; graceful fallback at every layer (legacy build / mock / empty).
- **L-290-6** ADR-046 P31 #4 additive only — no new action / menu / toolbar /
  shortcut. The DrawCircle tool ('tool-circle', shortcut C) is unchanged; the
  preview is purely internal to `onMouseMove`.
- **L-290-7** The radius reference prefers the surface hit (`viewport.pick`),
  falling back to the drawing-plane point — matching the commit path (the engine
  projects onto the surface either way).
- **L-290-8** Fall-back preview keeps the existing flat behavior intact for planar
  faces and for any curved host the engine can't resolve — zero regression to the
  planar DrawCircle path.
- **L-290-9** 절대 #[ignore] 금지.

## 4. Alternatives considered

- **Tessellate the whole cap and draw its boundary** — heavier per-frame; the
  read-only polyline query is the minimal thing that follows the surface.
- **Reuse `preview_curved_carve`** (ADR-287 §H) — that returns carve ghost tris
  for a push/pull, not a sketch circle; different geometry.
- **Leave the flat preview** — rejected: the preview/commit mismatch is exactly
  the 사용자 facing rough edge this track set out to finish.

## D. Acceptance Log

- **β-1 (engine)** — `crates/axia-geo/src/operations/carve.rs`:
  `preview_circle_on_surface` (read-only) + de-risk test
  `adr290_preview_circle_on_sphere_follows_surface` (sphere → all points on the
  sphere within 0.5mm; Plane box face → `None`). axia-geo **2241 pass / 0 fail**.
- **β-2 (WASM)** — `crates/axia-wasm/src/lib.rs`:
  `previewCircleOnSurface(host_face, cx,cy,cz, rx,ry,rz) -> Vec<f32>`.
  `npm run build:wasm` + verify export present in `axia_wasm.js`/`.d.ts`.
- **β-3 (bridge + tool)** — `web/src/bridge/WasmBridge.ts`:
  `previewCircleOnSurface(hostFace, centerPt, radiusPt)` wrapper (graceful null) +
  `AxiaEngineExtended` interface entry. `web/src/tools/DrawCircleTool.ts`:
  `activeCurvedHostFace()` + `updateCurvedPreview()` + `onMouseMove` curved
  branch. `web/src/__mocks__/three.ts`: `CircleGeometry` (enables the flat-preview
  path to be unit-tested). vitest: WasmBridge +4, DrawCircleTool +2.
- **β-4 (E2E + closure)** — `web/e2e/adr-290-preview-circle-on-surface.spec.ts`
  (real Chromium + production WASM): sphere preview polyline all on the sphere and
  NOT on the flat tangent plane; cylinder preview polyline on the wall
  (dist-to-axis ≈ radius). **2/2 pass.** ADR closure + LOCKED #92 + README + memory.

**Regression totals**: axia-geo +1 (2241), vitest +6 (WasmBridge 4 +
DrawCircleTool 2), Playwright +2. tsc 0 errors. Full workspace cargo unchanged
(2241 axia-geo / axia-wasm suites all green, 1 pre-existing slow-channel ignored).
절대 #[ignore] 금지 9/9.

## §Lessons

- **L1 measure-first found the exact gap** — the audit showed `updatePreview` had
  no curved branch and the commit path already had the on-surface helpers; the
  fix was to expose them read-only, not to re-derive geometry.
- **L2 read-only `&self` query is the right shape for previews** — ADR-287 §H
  established it; this track re-applies it. No mutation → no LOD/rebuild races,
  safe every mouse-move.
- **L3 SSOT preview↔commit** — reusing `circle_on_{sphere,cylinder,cone,torus}`
  guarantees the preview traces exactly what commits, eliminating the mismatch
  class of bug rather than approximating it (메타-원칙 #4).
- **L4 additive UX finish** — no new command/menu; the fix lives inside the tool.
  The strongest E2E is the sphere (the exact user scenario): every preview point
  on the sphere, none on the flat tangent plane.

## Cross-link

- ADR-202 / ADR-257 / ADR-263 (curved circle commit path — the preview now
  matches these)
- ADR-287 §H (read-only preview ghost pattern — `preview_curved_carve`)
- ADR-089 A-χ (surface inheritance on split) / ADR-031 Phase D (AnalyticSurface)
- ADR-046 P31 #4 (additive only) / ADR-087 K-ζ (사용자 시연 게이트 — E2E)
- 메타-원칙 #4 (SSOT) / #5 (UX) / #6 (measure-first)
- LOCKED #44 (Complete Meaning per Merge) / #66 (STATUS-POLICY)
