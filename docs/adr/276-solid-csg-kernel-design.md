# ADR-276 — Solid CSG Kernel: Design + Phased Plan (α spec)

- **Status**: Accepted (design accepted; Phase 1 β landed 2026-07-06; Phases 2–5 gated)
- **Date**: 2026-07-06
- **Context**: Follows ADR-275 (planar/solid box boolean is unimplemented; honest
  no-op guard shipped). User approved route **(a′) — implement the real solid-CSG
  kernel, starting from a design ADR**.
- **This ADR is α (spec + audit + phased plan) only.** No production engine change.
  Each β phase is a separate atomic PR gated by user 결재 (Path Z / LOCKED #44).

## Problem

Box/planar solid boolean (subtract/union/intersect) does not cut (ADR-275). A
real triangle-mesh CSG is needed: collect surface-surface intersections, split
faces along them, classify sub-faces inside/outside, assemble the kept shell,
merge coplanar, and guarantee a watertight manifold result.

## Audit finding (the reframe — measurement-first)

**The classic `Mesh::boolean` already has a full 6-stage pipeline, and the
general intersection collector already exists and works — it is simply NOT
WIRED into `boolean()`.**

| Stage | Function (`boolean.rs`) | Status |
|-------|-------------------------|--------|
| 0 prepare solid (fan-tri) | `prepare_solid` | exists |
| 1 **general** tri-tri crossing | `find_intersections` → `boolean_geo::triangle_triangle_intersection` | **exists, works, but wired only to "Intersect with Model" — NOT to `boolean()`** |
| 1′ coplanar overlap | `detect_coplanar_faces` | exists; **the ONLY thing `boolean()` Stage 1 uses** |
| 2 split faces by segments | `split_faces_by_intersections` | exists (2D project + insert crossings + sub-polygon split) |
| 3 classify in/out | `classify_split_faces` + `point_in_solid` | exists (centroid ray test, per-op logic) |
| 5 assemble + flip | inline + `flip_face` | exists |
| 6 merge coplanar | `merge_coplanar_result_faces` | exists |

`boolean()` line ~1613: `let intersections = coplanar_intersections;` — it ignores
`find_intersections` entirely. (Git history is squashed at baseline `155e127`, so
the reason for the disconnection is not recoverable; treat the general path as
**never validated for `boolean()`** — hence the safety gates below.)

### Phase 0 de-risk simulation (measured, `adr276_phase0_sim_general_intersection_and_split`)

Wiring `find_intersections` into Stage 1, box A [0,0,50] 100³ − box B:

| config | `find_intersections` segs | split faces | invariants valid |
|---|---|---|---|
| corner-poke | **6** (coplanar 0) | 12→18 | **valid** ✅ |
| top-center notch | **8** | 12→16 | **valid** ✅ |
| through-slot | **16** | 12→16 | **INVALID** ⚠ |
| enclosed cavity | **0** (no surface crossing — correct) | 12→12 | valid |

**The collector and split stages work** for surface-crossing configs (segs>0,
faces grow, topology valid for corner-poke/notch). This is NOT a from-scratch
kernel. The remaining work is bounded:

1. **Wire Stage 1** (`find_intersections` ∪ coplanar) into `boolean()`.
2. **Harden split robustness** — through-slot yields an invalid result (fan-tri
   convexity assumption and/or multi-segment-per-face split ordering).
3. **Enclosure/void case** — 0-seg subtract (B ⊂ A) must produce an internal
   shell (hollow), not a no-op; disjoint UNI; enclosed INT.
4. **Coplanar coincidence** — shared-plane faces (fold in `detect_coplanar_faces`
   + merge/dedup so results stay manifold).
5. **Safety + verify end-to-end** — classify/assemble produce a correct CUT
   solid, guarded by the existing gates.

## Decision (proposed — needs 결재 per phase)

Implement the kernel by **completing the existing pipeline**, not rewriting it,
in fail-closed atomic phases. Each phase wires more of the pipeline and is gated
by the existing safety infrastructure so an incorrect result rolls back instead
of corrupting the mesh:

- **ADR-267** `verify_volume_integrity` / watertight gate,
- **ADR-272** `closure_preserving_gate_passed` (closed→open reject),
- **ADR-273** `detect_self_intersections` gate.

### Proposed phased plan (each = separate atomic PR + 결재)

- **Phase 1 — Wire + fail-closed** (S–M): call `find_intersections` and union
  with coplanar in Stage 1; run the full pipeline; wrap the whole `boolean()` in
  a snapshot + the three gates with byte-identical rollback on any
  invalid/opened/self-intersecting result. Success criterion: corner-poke +
  notch cut end-to-end (browser-verified, manifold valid); through-slot &
  enclosed **safely roll back** (clear message) rather than corrupt. This alone
  makes the common convex-overlap cut work.
- **Phase 2 — Split robustness** (M): fix the through-slot invalid result
  (evaluate: non-convex face triangulation, multi-segment split ordering,
  chained crossings). Removes a rollback case.
- **Phase 3 — Enclosure / void** (M) ✅ DONE 2026-07-07: 0-seg subtract
  (B ⊂ A) → internal cavity. Engine already correct; fix was broadening the UI
  rescue to also fire on DCEL gate rejection. (disjoint/enclosed UNI/INT
  semantics still deferred.)
- **Phase 4 — Coplanar coincidence** (M) — ✅ DONE 2026-07-07 for all 3 ops
  (new `coplanar_grid_cells` + side-occupancy `resolve_coplanar_planes`; box
  union/subtract/intersect cut watertight, browser-verified). Non-rect + mixed
  coplanar+transversal still fail-closed (deferred). (touching/coincident-plane =
  degenerate input, separate.)
- **Phase 5 — Routing + default + demo** (S): decide UI routing (see Q2), set
  default on/off, browser demo across the config matrix, full regression + a
  proper regression suite replacing the print-only sim.

### Decision points needing user 결재

- **Q1 — Phase 1 gate policy**: fail-closed rollback on any invalid result
  (recommended — no corruption, honest "couldn't cut this config yet") vs
  best-effort commit. Recommend fail-closed.
- **Q2 — UI routing**: keep classic planar CSG as a separate path and dispatch
  by surface kind (all-Plane operands → classic CSG; curved → existing ADR-197
  DCEL), vs unify. Recommend surface-kind dispatch (Plane→classic, curved→DCEL),
  reusing `classify_dispatch_eligibility`.
- **Q3 — Default**: engine default off + production localStorage opt-in during
  hardening (ADR-049 P-5e-α pattern), flip on after Phase 2–4 land. Recommend.
- **Q4 — Triangulation**: keep fan-tri (convex assumption) and reject non-convex,
  vs earcut (`boolean_geo::project_to_2d` + earcut already used by ADR-273).
  Decide in Phase 2 with data.

## Consequences

- Reframes (a′) from "weeks, from-scratch CSG kernel" to "complete + harden an
  existing, mostly-working pipeline in gated phases." Lower risk than feared.
- Fail-closed gates mean each phase is safe to ship: unsupported configs roll
  back cleanly (never corrupt), matching the ADR-275 honesty principle.
- LOCKED Boolean lineage (064/066/074/075/076) untouched until Q2 routing is
  decided; the curved-analytic path (ADR-197) stays the path for curved operands.

## Regression

- `crates/axia-geo/src/operations/boolean.rs` — `adr276_phase0_sim_general_intersection_and_split`
  (measurement + regression guard: the general collector must find box-box
  crossings and split must grow faces for surface-crossing configs). Kept as the
  Phase 0 evidence; Phase 5 replaces the print-only parts with assertion suites.
- Existing scoping assets (ADR-275): `boolean_scoping.rs`, `boolean_planar_probe.rs`.

## Acceptance Log

### Phase 1 β — landed 2026-07-06 (user-approved: proceed, fail-closed / Q1)

- **Separate entry, zero regression** — rather than change `Mesh::boolean`
  universally (which broke 24 existing tests: `boolean_union_with_face_split`
  + 13 `boolean_dispatch` routing tests that use `boolean()` as an oracle /
  mesh-fallback and only assert routing, not geometry), the solid-CSG path is a
  new entry. `boolean_impl(…, use_general: bool)`; `boolean()` → `false`
  (byte-identical pre-ADR-276); **`boolean_solid()` → `true`** (general tri-tri
  Stage 1 + fail-closed gate). All existing callers untouched.
- **Stage 1 wiring** — `boolean_solid` unions `find_intersections` (general
  non-coplanar) with the coplanar overlaps.
- **Fail-closed gate** — snapshot (`self.clone()`) before mutation; after Stage 6,
  if `verify_face_invariants().is_valid()` is false OR
  `detect_self_intersections().is_clean()` is false → `*self = backup` +
  `bail!` (byte-identical rollback). Closed-solid NOT required (2D/sheet operands
  legitimately open).
- **Measured (Rust `adr276_phase1_box_box_subtract_cuts_and_never_corrupts` +
  browser via `demoBooleanSolidTwoBoxes`)**: corner-poke SUB → **cuts** (faces
  12→9, verts 16→22, invariants valid, non-manifold 0); notch → cuts, valid;
  through-slot → **fail-closed rollback** (Err, byte-identical, valid); enclosed
  → no-op (0 segs). Browser end-to-end (Rust→WASM→bridge) confirms corner-poke =
  9 faces, invariants valid, 0 non-manifold.
- **HONEST limitation** — the cut result is **valid + non-corrupting but NOT yet
  watertight** (corner-poke: `is_closed_solid=false`, 6 boundary edges — the
  notch walls are not fully stitched). Phase 1 delivers a valid, non-corrupting
  cut and proves the pipeline; **watertight sealing is Phase 2/3.** The gate
  guarantees no corruption, not completeness.
- **Regression**: axia-geo 2158 / axia-core / axia-transaction all green (0
  regression). New: `boolean_solid` / `boolean_impl`, `demoBooleanSolidTwoBoxes`
  (verification harness, not UI-wired), `adr276_phase0_sim_*` + `adr276_phase1_*`.
- **Not done in Phase 1** (per plan): watertight sealing (Phase 2), through-slot
  robustness (Phase 2), enclosure/void (Phase 3), coplanar coincidence (Phase 4),
  UI routing + default (Phase 5, Q2/Q3). `boolean_solid` is not reachable from
  the UI yet — the UI still shows the ADR-275 honest no-op warning.

### Phase 2 (partial) — audit + fail-closed-correct gate (2026-07-06, user-approved "bounded 해결 시도")

- **Root cause of the open cut (Q4 confirmed).** Audit (`adr276_phase2_audit_
  open_seam_duplicate_verts`) on corner-poke: verts ARE shared (0 coincident
  duplicates — LOCKED #5 dedup works), yet 6 boundary edges remain. Dumping the
  face loops + boundary-edge owners showed the box-box cut is produced as a
  **diagonal / tetrahedral notch, not the true rectangular notch**: A's three
  bitten faces (top / +x / +y) are each cut with a single DIAGONAL across the
  corner (e.g. +y face `…(20,50,100)→(50,50,70)…`, missing the real inner corner
  `(20,50,70)`), and the notch walls are TRIANGLES (half-quads). The 6 open edges
  are those diagonals. **`prepare_solid` fan-triangulates every face (convex-
  assumption MVP), so `find_intersections` computes tri-tri segments along
  triangle diagonals → the box-box intersection curve is topologically wrong.**
- **Not a bounded fix.** Correcting this requires reworking the intersection to
  preserve polygon loops (face-face intersection, not tri-tri on fan-tri'd
  faces) — core CSG, deferred to Phase 2 proper.
- **Safe bounded action taken — closed→closed gate.** The prior gate
  (invariants + SI) did NOT catch "valid-but-open", so `boolean_solid` was
  COMMITTING a geometrically-wrong open cut. Added: when BOTH operands are
  watertight solids, the result must be watertight too (`face_set_manifold_info
  (&merged_faces).is_closed_solid`), else roll back byte-identically.
  `boolean_solid` is now **fail-closed-correct**: for the current box configs it
  cleanly declines (rolls back) instead of shipping a wrong cut, and will only
  commit once the intersection rework produces watertight results.
- **Behavior change:** the Phase 1 "corner-poke cuts" result (9 faces, open) now
  rolls back — that cut was geometrically wrong, so declining it is more correct.
  `boolean()` (use_general=false) is unaffected (gate only runs for
  `boolean_solid`). Curved-analytic path (ADR-197) unaffected (early return).
- **Regression:** axia-geo 2159 pass / 0 fail. Test renamed
  `adr276_phase12_box_box_never_commits_open_or_invalid` — asserts every config
  is valid + (committed⇒watertight) OR (Err⇒byte-identical rollback). Audit test
  kept as the Phase 2 characterization asset.
- **Phase 2 core — intersection-curve rework DONE + verified (2026-07-07).**
  Added `find_intersections_polygonal` (+ `face_polygon_plane`,
  `clip_line_to_convex_poly`): the true face-to-face intersection = the line
  `plane_a ∩ plane_b` clipped to BOTH face polygons (no fan-triangulation).
  For corner-poke it produces the EXACT 6-segment rectangular notch loop
  (verified + asserted in `adr276_phase2_audit`: 6 segments, all endpoints on
  the notch box {20,50}×{20,50}×{70,100}) — versus the tri-tri collector's
  wrong diagonals. This is the geometric core of the fix.
- **NEXT gap (why it is NOT yet wired live): downstream split-by-chain.**
  `split_polygon_2d` cuts a polygon by a STRAIGHT chord only — it pairs the two
  boundary crossings and connects them straight, IGNORING the interior corner
  vertex a box-box notch needs (A's +y face must be cut along the L-chain
  `(50,50,70)→(20,50,70)→(20,50,100)`). So with the correct segments it still
  fails to split → A+B stay intact → the closed-solid gate would WRONGLY admit
  two disjoint boxes (boundary==0). Therefore `boolean_impl` Stage 1 keeps the
  tri-tri collector for now (open result → gate rolls back → fail-closed, no
  wrong output); `find_intersections_polygonal` is retained as the verified,
  `#[allow(dead_code)]` building block (exercised by the audit test).
- **Phase 2 split-by-chain WIRED (2026-07-07) — 3 of 4 sub-problems solved,
  seam-welding is the last gap.** Added `split_faces_by_chains` (+
  `assemble_chains`, `apply_chain_split`, `ensure_boundary_vertex`) and wired
  `boolean_impl` (use_general) to Stage 1 = `find_intersections_polygonal`,
  Stage 2 = `split_faces_by_chains`. Verified:
  - ✓ intersection curve correct (6-segment notch loop)
  - ✓ `split_face_by_chain` cuts the L-corner (sim)
  - ✓ `split_faces_by_chains` splits all 6 crossed faces into 2 each, mesh valid
    (`adr276_phase2_probe_split_faces_by_chains_corner_poke`)
  - ✗ **seam welding**: after classify drops the corner-rects, the notch seam
    has 12 OPEN boundary edges. Root cause: A's 3 faces share seam verts via
    shared-edge `split_edge`, but B is a SEPARATE solid — B's `split_edge`
    creates a DUPLICATE vertex at a seam endpoint that is A-interior but
    B-boundary (e.g. (20,50,70): interior to A's +y face, on B's −x wall edge),
    so A's seam edge (V3–V2) and B's (V7–V2) don't share → open. No weld/stitch
    utility exists.
  Gate still protects: box-box → open → `closed_solid=false` → byte-identical
  rollback (fail-closed, no wrong output). Wiring kept (exercises the verified
  code on the live path; safe).
- **Phase 2 SEAM WELDING COMPLETE (2026-07-07) — box-box corner subtract cuts
  WATERTIGHT.** `weld_result_seam` + `boolean_impl` Stage 5.5 (use_general).
  The weld does NOT do manual HE surgery: it buckets result-face verts by
  position, remaps each coincident group to a survivor, and REBUILDS the faces
  via `add_face` (two-phase collect→remove→re-add), which auto-shares edges by
  `find_edge` → the seam closes. Simulation-first validated closure (assembled
  boundary 12 → welded boundary 0) before wiring.
  - **corner-poke SUB → ok, 12→9 faces, is_closed_solid=true, boundary=0,
    non-manifold=0** (committed watertight cut; was a rollback). Verified
    end-to-end in the browser (`demoBooleanSolidTwoBoxes` → 9, closed).
  - notch / through-slot → still roll back (closed-loop notch / full-span slot
    need multi-chain / different handling; safe fail-closed).
  - enclosed cavity → no-op (Phase 3 void; valid closed A).
  So box-box CONVEX-CORNER subtract works: find_intersections_polygonal → chain
  split → classify → seam weld → watertight, gate-admitted.
- **Phase 5 UI ROUTING DONE (2026-07-07) — box-box cuts from the UI.** Real
  WASM export `booleanSolid` + `WasmBridge.booleanSolid` + BooleanHandler rescue:
  after `booleanDispatchDcelMulti`, if the DCEL result is the planar NO-OP
  (pathUsed=Nurbs, nothing new/removed), try `booleanSolid`; ok → syncMesh +
  "완료 (solid CSG)"; declines (fail-closed) → fall through to the ADR-275
  warning. Curved/DCEL path untouched (only the box no-op is rescued).
  Browser-verified end-to-end: 2 overlapping box solids → select all →
  executeAction('bool-subtract') → watertight cut (12→9 faces, closed).
  export_baseline += booleanSolid; BooleanHandler.test 27 (+2 rescue).
- **Phase 2 notch + slot DONE (2026-07-07) — closed-loop hole + multi-chain.**
  - CLOSED-LOOP (`assemble_closed_loops` + `apply_closed_loop_split`): a
    face crossed by a closed intersection loop (notch mouth / slot exit) →
    annulus (`add_face_with_holes`) + inner disk; classify keeps the annulus
    (hole-aware `face_classify_point`), drops the disk. `weld_result_seam`
    extended to rebuild HOLED faces (bucket inner-loop verts + re-add via
    add_face_with_holes) so the annulus welds to B's walls.
  - MULTI-CHAIN (`point_on_face_boundary` / `chain_fits_face`): a face with
    ≥2 open chains → split_face_by_chain applied SEQUENTIALLY, each chain
    routed to the sub-face its endpoints lie on.
  - Result (adr276_phase12 + browser): top-center notch → 12→11 watertight;
    through-slot → 12→10 watertight. Both cut from the UI (rescue routing):
    "차집합 완료 (solid CSG)", is_closed_solid=true. **box-box subtract now
    cuts watertight for corner / blind-notch / through-slot.**
- **Phase 3 enclosure/void DONE (2026-07-07) — enclosed subtract makes a
  cavity, wiring fix (user-approved: "상세한 시뮬로 진행하고 배선도 같이").**
  - **Simulation finding (measurement-first):** the ENGINE was never the
    problem — `boolean_solid` on B⊂A already produces a correct CAVITY (0
    intersections → no split; assemble keeps all of A outward + all of B
    FLIPPED inward via the existing Subtract `flip_face`). `adr276_phase3_
    enclosed_subtract_makes_cavity` measures the stored normals: **6 outward
    (A shell) + 6 inward (B void shell)**, closed, valid, 0 violations. The
    "no-op" was a WIRING gap, not an engine gap.
  - **Root cause (the wiring):** `boolean_dispatch_dcel_multi_json` (the DCEL
    path the UI hits first) does NOT cleanly no-op on enclosed boxes — the
    NURBS dispatch yields invariant-violating geometry (`inv=12`) that its
    `closure_preserving_gate_passed` rolls back byte-identically → returns
    `{kind:'error'}`. The ADR-276 Phase 5 rescue only fired on a CLEAN no-op
    (`kind='ok' && pathUsed='Nurbs' && allNew/allRemoved empty`), so the
    error slipped past → `boolean_solid` was never tried → user saw nothing.
  - **Fix (BooleanHandler.startBooleanOp):** broaden the rescue to also fire
    on a DCEL gate REJECTION (`dcelRejected = multiResult.kind === 'error'`),
    not just the clean no-op. In both cases the DCEL path already restored the
    mesh to pre-op state, so `boolean_solid` runs on clean input, and its OWN
    fail-closed gate (closed→closed + invariants + SI) is the arbiter — if it
    can't handle a config it declines and the honest DCEL message follows.
    Strictly additive: working DCEL cuts (`kind='ok'` WITH new faces) never
    reach the rescue.
  - **Verified end-to-end (browser, real UI path):** enclosed subtract →
    "차집합 완료 (solid CSG)", 12→12 faces, `verifyInvariants().valid=true`
    (0 violations), `meshManifoldInfo.isClosedSolid=true`; render buffer =
    24 tris (12 outward A shell + 12 inward void shell = confirmed cavity);
    single Undo restores the 2 separate boxes. **Regression clear (same
    session, direct `booleanSolid`):** corner-poke 12→9, blind-notch 12→11,
    through-slot 12→10 — all closed, valid, 0 violations.
  - **Not done (deferred):** the DCEL path still WASTES work computing an
    invalid result for enclosed boxes before rolling back (a Plane-only
    early-route to `boolean_solid` would skip it — Phase 5 optimization, not
    correctness). The `face_cached_normals_or_compute` / attached-surface
    normal is not flipped by `flip_face` (harmless for box cavities — Plane
    faces render via the planar `face.normal()` path; curved cavities use
    ADR-198 `face_surface_reversed`) — noted as a latent inconsistency for a
    future `flip_face` surface-orientation ADR.
- **Phase 4 coplanar CHARACTERIZED (2026-07-07) — fail-closed locked; the fix
  is a real 2D-face-boolean feature, NOT a wiring fix (user: "상세한 시뮬로
  진행합니다").** Sim (`adr276_phase4_*`) over 3 coplanar configs × 3 ops:
  - **stacked/touching** (A z[0,100] + B z[100,200], same XY): the shared z=100
    plane makes B's bottom verts COINCIDE with A's top verts → spatial-hash
    dedup FUSES them → the INPUT is already non-manifold (each z=100 rim edge
    shared by 4 faces). This is degenerate INPUT (touching solids fused on
    creation), not a boolean gap — out of scope (input hygiene / a separate
    "coincident-plane operands" concern). boolean_solid correctly rolls back
    (invalid input → gate fails).
  - **lateral half-overlap** (A x[-50,50] + B x[0,100], both y[-50,50]z[0,100])
    and **flush pocket**: VALID closed inputs, but boolean_solid currently
    rolls back (fail-closed no-op). Root cause (traced): the y=±50 & z=0/100
    face pairs are COPLANAR and OVERLAPPING (double-covered in x[0,50]); the
    intersection segments lie ON face boundaries so `split_faces_by_chains`
    splits nothing (A 0/6, B 0/6); classify keeps the overlapping coplanar
    faces from both operands → `merge_coplanar_result_faces` can't unify them
    (it merges edge-adjacent coplanar faces, not OVERLAPPING ones) → OPEN
    result (boundary=12) → closed→closed gate rolls back byte-identically.
  - **The real fix (Phase 4 proper):** a coplanar-face-pair resolver (below).
    Note the reusable-utils assumption was WRONG (investigated): `polygon_
    difference_walking` needs exactly-2 transversal crossings; `greiner_hormann`
    skips coincident/collinear edges + needs transversal crossings; the box-flush
    coplanar topology has only collinear edges → NONE of them fit. A new robust
    primitive was needed.
- **Phase 4 UNION DONE (2026-07-07) — coplanar side-occupancy resolver, box
  UNION cuts watertight (user: "권장으로 진행").** Two boxes flush on shared
  planes and overlapping now merge into ONE watertight solid.
  - **New primitive `coplanar_grid_cells`** (unit-tested `adr276_phase4_
    coplanar_grid_cells_primitive`): axis-aligned rectangle 2D-boolean via grid
    decomposition — the cartesian product of all rects' x/y edges; each cell is
    wholly in/out each operand, so one center test classifies it. Robust for
    overlap / flush / containment / disjoint + any operand count (the topology
    the reusable utils couldn't handle).
  - **`resolve_coplanar_planes` (side-occupancy classification)** — the KEY
    insight: a coplanar face STRADDLES the other solid's boundary, so the
    centroid `point_in_solid` classify is a coin-flip (measured: A's z=0 & y=-50
    faces DROPPED while z=100 & y=50 KEPT — same x=0 centroid, opposite verdict).
    Instead, for each plane shared by A and B, grid-decompose all faces on it and
    classify each cell by SIDE-OCCUPANCY: `point_in_solid` at cell-center ± εN
    for the op's solid predicate (Union = A∪B). A cell is boundary iff the two
    sides differ; the outward normal points to the empty side. Correct for
    same-normal flush overlap AND opposite-normal interior coincidence.
  - **Integration:** coplanar-shared-plane faces are EXCLUDED from the centroid
    classify + removed; `resolve_coplanar_planes` (read-only) returns outward-
    wound quads, added after removal (`add_vertex` dedup shares corners with the
    cut faces). The coplanar-Union path SKIPS weld + `merge_coplanar_result_
    faces` — the cells are already watertight, and `merge_faces_by_edge`
    corrupts collinear-vertex coplanar rects (drops a neighbour's area → re-opens
    the solid). The extra coplanar cell edges are harmless (valid 2-manifold
    interior edges, hidden by the coplanar render policy, LOCKED #16).
  - **Verified:** engine `adr276_phase4_lateral_overlap_union_watertight` (union
    = box x[-50,100], closed, valid, 0 violations) + browser end-to-end (create
    two overlapping boxes → bool-union → "solid-CSG cut: union, totalFaces=14",
    AABB x[-50,100] y[-50,50] z[0,100], is_closed_solid=true, 0 violations).
    Regression clear: corner/notch/slot subtract untouched (Union path only).
  - **Scope (UNION increment):** axis-aligned coplanar rects.
- **Phase 4 SUBTRACT + INTERSECT DONE (2026-07-07) — coplanar box CSG complete
  for all three ops (user: "진행").** The side-occupancy resolver was already
  op-aware (its `in_result` predicate encodes Union = A∪B / Subtract = A∖B /
  Intersect = A∩B); the only gate was `op == Union` in Stage 4.9. Opening it to
  all `use_general` ops wired subtract + intersect with ZERO new algorithm.
  - **Verified (engine):** `adr276_phase4_lateral_overlap_subtract_watertight`
    (A−B = box x[-50,0]) + `..._intersect_watertight` (A∩B = box x[0,50]) — both
    closed, valid, 0 violations. `..._union_watertight` (x[-50,100]) unchanged.
  - **Verified (browser, direct booleanSolid):** subtract → 6 faces, closed,
    nm=0, x[-50,0]; intersect → 6 faces, closed, nm=0, x[0,50]; union → 14
    faces, closed, x[-50,100]. (The executeAction rescue path's getStats read is
    stale after booleanSolid — a known Phase-3 cosmetic; the engine result +
    console "solid-CSG cut" are authoritative.)
  - **Regression clear:** corner/notch/slot subtract untouched (they have NO
    coplanar-shared planes → coplanar_keys empty → resolver never fires). Full
    axia-geo 2170 pass.
  - **Still deferred (fail-closed):** see the non-rect / mixed characterization
    below.
- **Phase 4 non-rect / MIXED characterized (2026-07-07) — the coplanar resolver
  handles non-rect footprints; the real gap is MIXED (coplanar + transversal),
  which is the general-CSG stitch (user: "상세한 시뮬레이션으로 배선관계와 모든
  문제점 검토").** Detailed sim of the 2-axis-offset union (A x[-50,50]y[-50,50]
  + B x[0,100]y[0,100], both z[0,100] — z=0/z=100 footprint is an L-shape and the
  x/y side faces cross transversally):
  - **Finding 1 — the coplanar resolver ALREADY handles the non-rect (L-shape)
    footprint:** `resolve_coplanar_planes` grid-decomposes the two rects on each
    shared z-plane and side-occupancy-classifies the cells → 7 cells per plane =
    the L-shape (14 quads total), correct. So "non-rect coplanar" per se is not
    the blocker — the grid + side-occupancy produces rectilinear (L/staircase)
    footprints from rect inputs for free.
  - **Finding 2 — the real gap is MIXED (coplanar + transversal):** the 2-axis
    union's x/y SIDE faces cross transversally (10 intersection segs); those
    partial side faces (e.g. A's x=50 kept only for y[-50,0]) must be split AND
    stitched to the coplanar z-cells. The op rolls back (measured: 20 faces, 4
    non-manifold edges each shared by 3 faces = T-junctions where a transversal
    side face meets a coplanar cell edge without a shared vertex set). Welding
    alone does not reconcile them — this is the general-CSG imprint+merge (a
    consistent shared vertex set across coplanar cells AND transversal splits),
    the genuinely hard remaining piece, NOT a wiring tweak.
  - **Safe improvement landed:** the weld/merge heuristic is refined — weld now
    runs for EVERY `use_general` op (a mixed config's transversal seam needs it;
    the coplanar cells are already vert-shared so weld is a no-op for them), and
    only `merge_coplanar_result_faces` is skipped when coplanar cells are present
    (it corrupts collinear-vertex coplanar rects). Pure-coplanar (1-axis) union/
    subtract/intersect still watertight (engine + browser re-verified); the mixed
    2-axis config remains fail-closed.
  - **Locked:** `adr276_phase4_mixed_config_coplanar_resolver_works_op_fails_
    closed` asserts BOTH — the resolver produces the 14-quad L-shape AND the full
    mixed op is no-corruption (rolls back to the valid 2-box input).
- **Generality AUDIT (2026-07-07) — the current solid-CSG is AXIS-ALIGNED-BOX
  scoped; the next investment is a GENERAL mesh CSG, not a box-specific mixed
  resolver (user: "Generality 감사 (측정)").** Measured boolean_solid on rotated
  boxes (`adr276_generality_audit_axis_only_rotations_fail_closed`):
  | config | coplanar | transversal | result |
  |---|---|---|---|
  | axis corner (baseline) | 0 | 6 | ✅ cut, 9 faces, watertight |
  | rot Z 45° (SUB/UNI) | 2 | 8 | fail-closed (mixed: z-planes stay coplanar) |
  | rot X 30° | 2 | 16 | fail-closed (mixed: x-planes stay) |
  | **rot (1,1,1) 30°** | **0** | **10** | **fail-closed (PURE transversal!)** |
  | rot Z 5° (near-coplanar) | 2 | 10 | fail-closed |
  - **Decisive finding:** `rot (1,1,1)` has coplanar=0 (pure transversal, no
    coplanar complication) yet STILL rolls back. So `find_intersections_polygonal`
    + `split_by_chain` + `weld` do NOT handle arbitrary-angle DIAGONAL cuts — the
    transversal machinery is effectively axis-aligned-box-specific (axis cuts →
    axis-aligned segments + L-corner chains split_by_chain handles; a rotated box
    → diagonal segments/chains → fail). Every rotation is fail-closed (no
    corruption).
  - **Decision:** the next boolean investment is a GENERAL mesh CSG (robust
    mesh-mesh intersection + classify + retriangulate/stitch for arbitrary-angle
    faces), which SUBSUMES both rotation AND the MIXED box case. A box-specific
    mixed resolver would be wasted effort (general CSG covers it). Large kernel
    effort (BSP or robust boolean), high regression risk on LOCKED boolean —
    warrants its own ADR + 결재.
  - **Curved primitives** (cylinder/sphere/cone/torus ∩ axis box) are a SEPARATE
    partially-working path (ADR-197 curved dispatch, tried before the general
    path) — not covered by this audit.
- **Remaining (deferred):** GENERAL mesh CSG (arbitrary-angle / rotated / non-box
  solids — the audit's finding; subsumes MIXED coplanar+transversal + truly
  non-rect INPUT faces), coincident-plane operand hygiene (stacked/touching =
  degenerate non-manifold input), curved-primitive generalization (ADR-197).

## Lock-ins (for the β phases)

- **L-276-1** Complete the existing pipeline; do NOT rewrite `find_intersections`
  / `split_faces_by_intersections` / `classify_split_faces` unless a phase proves
  a specific one is unfixable.
- **L-276-2** Every phase is fail-closed: snapshot + ADR-267/272/273 gates +
  byte-identical rollback on invalid/opened/self-intersecting results.
- **L-276-3** Each phase = separate atomic PR + user 결재 (Path Z / LOCKED #44).
- **L-276-4** Curved operands keep routing to ADR-197 DCEL; classic CSG is for
  all-Plane operands (pending Q2).
- **L-276-5** 절대 #[ignore] 금지.

## Cross-link

- ADR-275 (planar boolean scope + no-op guard) — direct predecessor.
- ADR-064 / 066 / 074 / 075 / 076 (NURBS Boolean → DCEL lineage) — untouched.
- ADR-197 (curved analytic Boolean dispatch) — the curved path.
- ADR-267 (watertight gate) · ADR-272 (closure-preserving gate) · ADR-273
  (self-intersection checker) — the fail-closed safety infrastructure.
- ADR-049 P-5e-α (engine-off + production opt-in) — Q3 default pattern.
- 메타-원칙 #4 (SSOT) · #5 (사용자 편의) · #6 (Preventive) · #9 (회귀 없음) · #16.
- Memory: `project-boolean-runtime-finding`.
