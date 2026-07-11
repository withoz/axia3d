# ADR-278 — Curved-primitive Boolean: generality audit + tessellate-then-v2 direction

- **Status**: Accepted (β implemented + demo-verified 2026-07-07)

## Context

ADR-277 delivered general polyhedral CSG (transversal + coplanar + MIXED +
rotation + non-box, all watertight via `boolean_solid` v2-first). The one
remaining "major" was curved-primitive boolean (cylinder / sphere / cone /
torus). This ADR audits what curved boolean actually does today — measurement
first, mirroring the ADR-276 box generality audit that de-risked ADR-277.

## Audit (`adr278_curved_boolean_audit`)

Curved primitives have TWO representations:
- **Path A (polygonal / tessellated):** `create_cylinder` etc. at the engine
  default emit a many-faced polyhedron (24-seg cylinder = 26 faces, sphere = 192,
  cone = 25).
- **Path B (analytic / kernel-native):** production default (localStorage) — a
  2–3 face solid carrying the `AnalyticSurface`, boundary = a self-loop edge.

Measured `boolean_solid` (v2-first → v1/ADR-197 fallback) on each vs a box:

| operand | rep | op | result |
|---|---|---|---|
| cylinder | Path A | SUB / UNI / INT | ✅ watertight (8 / 31 / 26 faces) |
| sphere | Path A | SUB | ✅ watertight (192→78 faces) |
| cone | Path A | SUB | ✅ watertight (31 faces) |
| cylinder | **Path B** | SUB | ❌ **NO-OP** (box returned unchanged, 6 faces) |
| cylinder | **Path B** | INT | ❌ empty (0 faces) |
| sphere / cone | **Path B** | SUB | ❌ **NO-OP** (box unchanged) |

## Finding

- **Polygonal (Path A) curved boolean is SOLVED** — a tessellated cylinder /
  sphere / cone is just a many-faced polyhedron, and ADR-277's v2 (imprint +
  arrange + strict-interior classify) is general, so it cuts them watertight for
  all three ops with ZERO curved-specific code (γ generalization).
- **The real gap is the ANALYTIC (Path B) representation, which NO-OPs.**
  `boolean_solid` → v2 can't process the `< 3`-vert self-loop analytic faces
  (`prepare_solid` skips them), and the ADR-197 analytic dispatch only covers a
  narrow set (curved ∩ axis-box that Z-cuts), NOT box−cylinder subtract → the box
  is returned unchanged. **No corruption, but no cut.**
- **This is user-facing:** production defaults Path B ON, so a user who draws a
  cylinder and subtracts it from a box currently gets NOTHING carved.

## Decision (proposed — β needs 결재)

**Recommended (Path A′): tessellate Path B curved faces before the v2 imprint.**
Mirror the existing ADR-110 π-β pass (`polygonize_closed_curve_face`) — when a
`boolean_solid_v2` operand carries an analytic self-loop face, polygonize it to a
chord-tolerant polygon first, then run the proven polygonal v2 path. The result
is polygonal (the analytic surface is lost on the cut faces, like every other
tessellation-based CAD boolean), but it CUTS correctly for all ops + arbitrary
angles. Low-risk, reuses proven machinery.

**Alternative (analytic-preserving): extend ADR-197 analytic SSI** to general
curved boolean (cylinder/sphere/cone/torus × box × any op), preserving the
`AnalyticSurface` on the result. Much larger (surface-surface intersection
curves, trim loops), higher risk. Defer unless analytic-surface preservation on
boolean results is a hard requirement.

## Lock-ins (audit)

- **L-278-1** Polygonal curved boolean watertight (asserted) — do not regress.
- **L-278-2** Path B curved boolean is fail-closed no-op today (no corruption) —
  guarded; the β fix flips it to a real cut.
- **L-278-3** β fix = tessellate-then-v2 (ADR-110 π-β pattern reuse), not a new
  analytic kernel, unless 결재 chooses the analytic-preserving alternative.
- **L-278-4** 절대 #[ignore] 금지.

## Acceptance Log

- **2026-07-07 β (`polygonalize_curved_operand`)** — the recommended Path A′
  landed. `boolean_solid` now runs a one-time pre-pass that regenerates a Path B
  (analytic self-loop) cylinder as an equivalent POLYGONAL cylinder (reuses the
  proven Path A `create_cylinder` builder, 32 segments) before dispatching. The
  whole (v2-first, v1-fallback) machinery then cuts it watertight — polygonal
  curved boolean was already SOLVED (L-278-1).
  - **Placement (critical):** the polygonalize runs at the `boolean_solid` entry,
    NOT inside `boolean_solid_v2`. v2 is fail-closed and rolls the mesh back on
    Err; a polygonalization trapped inside v2 would be discarded before the v1
    fallback ran, leaving v1 with the original analytic faces → no-op. Doing it
    once at the top means BOTH v2 and the v1 fallback see the polygonal
    polyhedron.
  - **`create_cylinder` semantics:** `center` is the BASE (spans z ∈ [center.z,
    center.z + height]), so the regenerated base = the operand's zmin, not the
    mid-plane. (Initial attempt used the mid-plane → double-shifted the cylinder
    off the box → 0 intersection segments → no-op; caught by a bbox probe.)
  - **MVP scope:** axis-aligned ±Z Path B cylinders. Rotated cylinders and Path B
    sphere / cone / torus still return the operand unchanged (documented no-op, no
    regression — follow-up increments).
  - **Verification (3 layers):**
    - Engine — `adr278_curved_boolean_audit` asserts Path B cyl−box now CUTS
      (res_faces > 6) AND is watertight (closed + manifold). Full axia-geo lib
      2187 pass / 0 fail / 0 ignored; boolean suite 286 pass.
    - Browser (real Chromium, production build) — the actual UI path: primary
      `booleanDispatchDcelMulti` no-ops on the Path B cylinder
      (`UnsupportedSurfaceKind` — "face_b surface conversion failed"), the
      `booleanSolid` rescue fires → mesh cuts 9→8 faces (verts 24→73),
      `verifyInvariants` valid, 0 violations. Before the β fix this rescue itself
      no-op'd.

- **2026-07-11 β follow-up — SPHERE / CONE / TORUS (memory follow-up closed)** —
  `polygonalize_curved_operand` extended from cylinder-only to sphere/cone/torus.
  Path B sphere/cone/torus − box subtract was a **silent no-op** (the box returned
  unchanged; a real user-facing gap since production defaults Path B ON). Now each
  is polygonalized at the `boolean_solid` entry → the v2 imprint CUTS watertight.
  - **Sphere** — extract `center + radius` from the Sphere surface → `create_sphere`
    (Path A, 24×16). Axis-agnostic (a full sphere is fully defined by center+radius).
  - **Cone** — extract `apex + half_angle` from the Cone surface; the Path B cone's
    APEX is a DEGENERATE (non-DCEL) point, so the operand's verts are all on the
    base ring → take `base_z` from the verts, `apex_z` from the surface, `height =
    apex_z − base_z`, `base_radius = height·tan(half_angle)` → `create_cone` (Path A,
    32). Axis-aligned ±Z, apex-above MVP.
  - **Torus** — no Path A builder (torus is kernel-native from day 1, ADR-115), so a
    new `build_polygonal_torus` (u×v quad grid, watertight, axis-agnostic) rebuilds
    it. Verified standalone SI-free + closed (`adr278_polygonal_torus_builder_is_watertight`).
  - **Grazing/tangential limitation (fail-closed, correct):** a curved operand
    *tangent* to a box face (e.g. a torus straddling the top face, z=110) produces a
    genuinely self-intersecting subtract (measured 128 self-intersections) that the
    ADR-276 validity gate correctly REJECTS → rolls back (WASM `boolean_solid_op`) →
    safe no-op. Clean *through* overlaps (torus at z=50, sphere/cone piercing a face)
    cut watertight. This is a real geometric hardness of grazing curved CSG, not a
    builder bug.
  - **Lesson (regression correctness):** the initial regression used `let _ =
    boolean_solid(...)` + `after > before` and was FOOLED — the direct engine call
    does NOT roll back on a gate `Err` (leaves the polygonalized-but-uncommitted
    faces, so `after > before` even on failure), while the WASM `boolean_solid_op`
    DOES roll back. Fixed to assert `boolean_solid(...).is_ok()` explicitly + use
    clean-overlap configs. Browser (real Chromium) is the ground truth here.
  - **Verification (3 layers):** engine `adr278_pathb_sphere_cone_torus_subtract_cuts`
    (all three Ok + cut + watertight) + `adr278_polygonal_torus_builder_is_watertight`;
    workspace 3018 pass / 0 fail / 1 ignored; browser E2E `adr-278-pathb-curved-subtract.spec.ts`
    ×3 (sphere/cone/torus − box via `booleanSolid` → cut + isClosedSolid + valid).
  - **Still deferred:** rotated (non-±Z) cylinder/cone, inverted (apex-below) cone,
    grazing/tangential curved subtract (needs robust tangent CSG). No new WASM/bridge/
    tool wiring — the fix lives in `boolean_solid`, so all callers (BooleanHandler →
    `booleanSolid`) benefit automatically.

## Cross-link

- ADR-277 (general mesh CSG — the v2 path this reuses).
- ADR-276 (box generality audit — the measure-first pattern this mirrors).
- ADR-197 (curved analytic Boolean dispatch — the narrow analytic path today).
- ADR-110 π-β (`polygonize_closed_curve_face` — the tessellate pattern for β).
- ADR-104 family (Path B primitives — the analytic representation).
- Memory: `project-boolean-runtime-finding`.
