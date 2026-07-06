# ADR-275 — Planar/Solid Box Boolean: Scope + Honest No-Op Guard

- **Status**: Accepted
- **Date**: 2026-07-06
- **Context**: Engine stabilization session (branch `claude/hopeful-sinoussi-e2d17a`)
- **Supersedes / amends**: none (documents an existing capability gap; does not change ADR-064/066/074/075/076/197 decisions)

## Problem

Direct runtime testing surfaced that a **box − box subtract via the UI does
nothing** — the two solids stay intact, no cut. The failure is silent-ish: the
UI showed a low-severity `Toast.info` ("모든 N개 pair 가 교차하지 않거나 면 분할
미생성") which reads like normal completion and, for two clearly-overlapping
boxes, is misleading ("교차하지 않거나" = "not intersecting" when they plainly
do overlap).

## Investigation (measurement-first)

Two engine probes were added (diagnostic only, no engine change):

- `crates/axia-geo/tests/boolean_planar_probe.rs` — vertex-position dump.
- `crates/axia-geo/tests/boolean_scoping.rs` — 6 configs × 3 ops × both paths.

**Scoping matrix (6 configs: corner-poke / top-center notch / through-slot /
enclosed cavity / stacked-shares-z100-plane / lateral-half-overlap; ops
SUB/UNI/INT; paths classic `Mesh::boolean` + DCEL `boolean_dispatch_dcel_multi`):
every single cell is a NO-OP** — box A's 8 original corners are always intact.

Decisive findings:

1. **Classic `Mesh::boolean` never SPLITS faces.** Active vert count stays 16
   (or 12), never grows. Stage 1 ("교차선 수집", `boolean.rs` ~1608) only aliases
   `coplanar_intersections`; there is **no general (non-coplanar)
   triangle-triangle intersection collector** (grep confirms only
   `detect_coplanar_faces` exists). A normal box poke crosses at NON-coplanar
   faces → 0 intersection segments → no split → classify keeps/removes WHOLE
   faces only → never a partial cut. (corner-poke INT → 0 faces; stacked SUB →
   12→5 with **4 non-manifold violations** = invalid output.) This path is wired
   only to `demo_*` functions, never the UI.

2. **DCEL `boolean_dispatch_dcel_multi` (the UI path) is surface-pair SSI, not
   solid CSG.** For box operands it always reports `path=Nurbs`, all N×M pairs
   "ok", but `new=0 / removed=0` — every Plane×Plane pair resolves disjoint and
   is preserved. It never cuts a box solid.

3. **Route-switching does NOT fix it.** Routing the UI's non-NURBS pairs to
   classic (a previously-hypothesized wiring fix) would route to something that
   also no-ops. This is a **missing capability, not a wiring gap.**

4. **What DOES work**: the curved analytic Boolean dispatch (ADR-197) — an
   analytic primitive (sphere / cylinder / cone / torus) ∩ an axis-aligned box
   that only cuts it in Z. That is the Boolean that is actually implemented.

## Decision

**Route (c): honest no-op guard + document scope now; defer a real solid-CSG
kernel to a future, separately-approved initiative.**

- **Guard** (`web/src/ui/BooleanHandler.ts`, `handleMultiDcelResult`
  all-disjoint branch): replace the misleading `Toast.info` with a
  `Toast.warning` that names the real limitation and what is supported:

  > `{op}: 변경 없음 — 두 solid 가 실제로 떨어져 있거나, 평면(box) solid
  > boolean 이 아직 미지원입니다. 현재 곡면 analytic surface(구·원기둥·원뿔·
  > 원환) ∩ 축정렬 box 절단만 지원됩니다 (ADR-275).`

  The wording is correct for BOTH genuinely-disjoint solids and the
  overlapping-box gap, so no overlap-detection is needed.

- **Scope documented**: box/planar solid boolean is unsupported end-to-end;
  curved analytic ∩ box (ADR-197) is supported. The two engine probes are kept
  as measurement assets.

### Rejected alternatives

- **(a) route non-NURBS pairs to classic `Mesh::boolean`** — rejected by the
  scoping matrix: classic also no-ops (never splits faces).
- **(a′) implement a real triangle-mesh CSG** (general tri-tri intersection +
  face split + robust inside/outside classification + re-stitch) — this is the
  correct long-term answer but is a from-scratch solid-boolean kernel: weeks of
  work, high risk, and it touches the LOCKED Boolean ADR lineage
  (064/066/074/075/076). Deferred to a future initiative with its own design ADR
  and user approval.
- **(b) extend DCEL to solid CSG** — the DCEL path is fundamentally NURBS
  surface SSI; making it do solid box CSG is equally major. Deferred.

## Consequences

- Users who try box-box boolean now get a clear, actionable warning instead of a
  silent/misleading no-op — aligns with the "healthy engine: silent failure →
  clear feedback" stabilization goal.
- No engine behavior change; no regression to the working curved-analytic path.
- A real solid-CSG kernel remains a known, documented future initiative (a′).

## Regression

- `web/src/ui/BooleanHandler.test.ts` — the all-disjoint case now asserts the
  ADR-275 warning (변경 없음 / 미지원 / ADR-275), skips syncMesh, no fall-through.
- `crates/axia-geo/tests/boolean_scoping.rs` — scoping matrix (measurement).
- `crates/axia-geo/tests/boolean_planar_probe.rs` — vertex-dump probe.

## Cross-link

- ADR-064 / 066 (NURBS Boolean → DCEL, Path Z / Path Y) — the DCEL path.
- ADR-197 (curved analytic Boolean dispatch) — the Boolean that works.
- ADR-074 / 075 / 076 (group selection UX / E2E / legacy sunset).
- 메타-원칙 #5 (사용자 편의 — 명확한 피드백) · #6 (Preventive) · #16 (자동화 antipattern).
- Memory: `project-boolean-runtime-finding` (full matrix + route options).
