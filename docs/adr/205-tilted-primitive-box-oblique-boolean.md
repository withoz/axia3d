# ADR-205 — Tilted Primitive ∩ World-Box: Oblique-Section Boolean (B track)

- **Status**: Proposed
- **Date**: 2026-06-18
- **Author**: WYKO + Claude
- **Track**: ADR-197/204 Z-axis lift — (B) world-box general
- **Depends on**: ADR-204 (oriented sphere) / ADR-158 (Ellipse = NURBS-only) /
  ADR-089 (closed Bezier/BSpline/NURBS self-loop faces) / ADR-027 (NURBS kernel)

## 1. 사용자 결재 (canonical)

> ADR-204 closure 후 재개 우선순위 #1: "(B) world-box general — 기운
> primitive ∩ world-axis box". "다음 진행" (2026-06-18).

## 2. Problem — local-frame / oriented 패턴이 닿지 않는 경계

ADR-197 (cylinder/cone/torus local-frame) + ADR-204 (oriented sphere)는 cut이
primitive **자기 축 ⊥ 평면**일 때만 동작한다. **기운 primitive ∩ world-axis
box**는 box 평면이 그 조건을 위반한다:

probe `probe_b_dispatch_tilted_cylinder_world_box` (커밋 `85eb8e3`): 기운
cylinder(axis=(0,0.6,0.8)) ∩ world box → **clean Err** (#Track2 guard).

## 3. Truth-first audit (4-agent, 2026-06-18) — 진짜 gap

### 3.1 단면 taxonomy (geom)

cylinder 축 **n**, box 평면 normal **m**, θ = angle(n, m):

| θ | 평면 vs 축 | 무한 cylinder 단면 |
|---|---|---|
| 0° (m ∥ n) | ⊥ 축 | **원** (r) |
| 0<θ<90° | oblique | **타원** (semi-minor=r, semi-major=r/cos θ) |
| 90° (m ⊥ n) | ∥ 축 | **2 직선** (semi-tilt edge case) |

기운 축은 어떤 cardinal axis 와도 ∥ 아님 → **box 6 평면 모두 oblique →
전부 elliptic arc**. cylinder 자기 cap만 원. **원 = cap(표현 가능) / 타원 +
직선 = box-cut = gap.**

현 dispatch 가 `axis ∥ Z` **AND** `XY-containment` 두 precondition을 요구하는
이유 = 모든 cutting plane 의 θ=0 (원-only) 보장. 축을 기울이면 θ≠0 → 타원 →
표현 set 밖.

### 3.2 인프라 현황 (SSI 존재, 표현/trim 부재)

- **SSI 수학 존재**: `ssi/analytic.rs::plane_cylinder` (L183-232) oblique →
  ellipse (semi-major/minor 계산), `plane_cone` Dandelin, 회귀
  `plane_cylinder_45deg_yields_ellipse`. **단 `SurfaceIntersection`은 sampled
  points(polyline) 반환** — analytic ellipse curve 아님.
- **Ellipse 곡선 표현 부재**: `AnalyticCurve` = Line/Circle/Arc/Bezier/BSpline/
  NURBS. **Ellipse variant 없음** (ADR-158: Ellipse = NURBS-only).
- **sew = circle 기반**: `add_self_loop_circle` / `sew_curved_band` /
  `sew_closed_curve_pair`는 Circle 경계만. **analytic elliptic face trim 부재** —
  cylinder side 가 ellipse 로 trim 되면 uv 도메인이 비-사각형(sinusoid) 경계.
- **classify hard-reject**: `classify_curved_primitive` (L73/99/116) 비-Z 축 →
  None. `CurvedPrim`의 `center_z`가 Z 가정.

### 3.3 난이도 ranking

**sphere (해결, 항상 원) < cylinder (타원) < cone (ellipse/parabola/hyperbola
Dandelin) < torus (quartic spiric).**

## 4. Decision — 점진 atomic, cylinder 단일 oblique 평면부터

(B) general 은 multi-week 트랙. **첫 atomic = 기운 cylinder ∩ 단일 oblique
평면 → 1 elliptic cap** (halfspace 경로의 직접 analogue). 격리: 정확히 하나의
신규 primitive (elliptic cap). multi-plane corner / line-pair / Dandelin /
subtract 미접촉.

elliptic cap 은 closed-form: semi-minor=r, semi-major=r/cos θ, major-axis =
n 의 평면 투영 정규화, center = 축선이 평면 뚫는 점 — **SSI/Newton 불요**.

### 4.1 첫 atomic 이 강제하는 인프라 (whole (B) family 공유)

1. **Ellipse 경계 곡선** — ADR-158 NURBS-only ellipse OR Bezier 근사 self-loop
   (ADR-089 closed NURBS/Bezier self-loop face 활용). 정확 ellipse = rational
   quadratic NURBS (4-arc OR 1-period).
2. **Analytic surface uv-trim** — cylinder side 가 ellipse 로 trim → 측면 face
   의 uv 경계가 비-사각형. NURBS surface 의 `trim_loops` (ADR-033) 가 선례,
   analytic Cylinder 엔 부재 → 신규 (또는 tessellation-time boundary clip).

## 5. Lock-ins (canonical for ADR-205)

- **L-205-1** 첫 atomic = cylinder 단일 oblique 평면 intersect-only, 1 elliptic
  cap (multi-plane/subtract/cone/torus 미접촉)
- **L-205-2** classify_curved_primitive 비-Z reject 해제 (CurvedPrim 에 axis_dir
  추가, center_z Z-가정 제거)
- **L-205-3** Ellipse 경계 = ADR-158 NURBS-only (신규 AnalyticCurve variant 0,
  ADR-089 closed-curve self-loop face 재사용)
- **L-205-4** elliptic cap = closed-form (semi-minor=r / semi-major=r/cosθ),
  SSI/Newton 불요
- **L-205-5** cylinder side uv-trim 은 trim_loops (ADR-033) 선례 OR
  tessellation boundary-clip
- **L-205-6** 기존 Z축 local-frame/oriented op (ADR-197/204) 불변 (additive) —
  oblique dispatch 는 별도 분기
- **L-205-7** sphere 는 (B) 밖 (항상 원, ADR-204 corner/rounded-box 이미 처리)
- **L-205-8** 절대 #[ignore] 금지

## 6. Atomic plan (각 별도 ADR/atomic, multi-week)

- **205-α** (본 spec) — 결정 + audit
- **205-β-1** — Ellipse 경계 인프라: `ellipse_as_nurbs(center, major, minor,
  axes)` helper + closed NURBS self-loop face (ADR-089 재사용) + 회귀
- **205-β-2** — cylinder 단일 oblique 평면 → elliptic cap: `boolean_cylinder_
  oblique_halfspace(cyl, plane_origin, plane_normal)` + cylinder side uv-trim +
  sew + render + 시연 (ADR-087 K-ζ)
- **205-β-3** — 2 oblique 평면 → elliptic slab (2 cap)
- **205-β-4** — ∥-axis line-pair (semi-tilt) + multi-plane corner
- **205-γ** — dispatch 연결: classify 비-Z 해제 + oblique 분기 → boolean()
  에서 기운 cylinder ∩ box 활성 (user-facing)
- **deferred 별도 ADR**: cone Dandelin / torus spiric

## 7. Out of scope (deferred)

- tilted cone (Dandelin ellipse/parabola/hyperbola 분기) — 별도 ADR
- tilted torus (quartic spiric) — 별도 ADR
- co-oriented box subset (box가 primitive frame 정렬) — dispatch local-frame,
  별도 작은 트랙 (재개 우선순위 #2)

## 8. Cross-link

- ADR-197 (cylinder/cone/torus local-frame, Z축 lift)
- ADR-204 (oriented sphere — (B) 밖, 항상 원)
- ADR-158 (Ellipse = NURBS-only) / ADR-089 (closed NURBS/Bezier self-loop face)
- ADR-033 (NURBS surface trim_loops — uv-trim 선례) / ADR-027 (NURBS kernel)
- ADR-034 (SSI — plane_cylinder/cone oblique 존재) / ADR-064/066 (NURBS Boolean
  DCEL)
- 메타-원칙 #4 (SSOT) / #6 (Preventive) / #10 (ADR 불변) / #16 (휴리스틱 antipattern)
- LOCKED #44 (Complete Meaning per Merge) / probe `85eb8e3`

## 9. Acceptance log

### β-1 (closed, `f1ef093`)
`nurbs::ellipse(center, semi_major, semi_minor, u_axis, v_axis)` — ellipse = affine
image of the standard rational 9-pt quadratic circle (axis pts w=1, corners
w=√2/2, clamped knots), mapped via `center + a·x·u + b·y·v`. + `add_face_closed_
curve` accepts the closed NURBS (ADR-089) → 1 anchor / 1 self-loop NURBS edge /
1 Plane face. 회귀 axia-geo +2.

### β-2 (cylinder single-oblique halfspace)

**섬세한 시뮬레이션 먼저** (truth-first, 3 probes):
1. **ellipse 기하** — closed-form (center = axis∩plane; semi_minor=r,
   semi_major=r/cosθ; minor=(m×n)̂, major=(n−(n·m)m)̂) built as `nurbs::ellipse`
   lies EXACTLY on the cylinder (radial=r) AND the cut plane (1e-7), 5 oblique
   configs. ✅
2. **DCEL sew** — `sew_curved_band` (curve-generic `AnalyticCurve` top/bot)
   accepts an ELLIPSE top boundary → watertight (open HE 0) + manifold + a
   Cylinder band. ✅
3. **render** — the default `surface.tessellate()` over the rectangular `v_range`
   over-draws PAST the elliptic cut (69/276 band verts above the plane). ❌ →
   motivates a boundary-aware clip.

**구현**:
- `Mesh::tessellate_cylinder_clipped(face, chord_tol)` — a Cylinder band with one
  OBLIQUE planar boundary is tessellated **boundary-aware**: per circumferential
  `u`, the axial extent is `v ∈ [v_lo(u), v_hi(u)]` where each bound is where the
  generator at `u` pierces a boundary plane (exact, no chord-snap; generalises to
  the β-3 slab). Returns `None` for a perpendicular band (both boundaries ⊥ axis →
  `v_range` already exact) so existing cylinders render unchanged. Wired into
  `export_buffers_inner` after `tessellate_sphere_clipped`. Over-draw 69→0.
- `Mesh::boolean_cylinder_oblique_halfspace(cyl, plane_origin, plane_normal, mat)` —
  reads the Cylinder, computes the elliptic section, keeps the +`plane_normal`
  side, removes the original, and sews `[trimmed band, elliptic cap, kept disk]`.
  MVP gate: a clean cut THROUGH the side (each end cap clear by `r·sinθ`) — this
  clearance also GUARANTEES `|v_keep−t| > z_span = r·tanθ`, so the band's min/max
  strip never flips. ⟂ / ∥ / cap-clipping cuts `bail!()`.

**적대적 검증** (workflow rate-limited → 직접 sweep) — found & fixed **1 real bug**:
on a kept-**LOW** end (`kept_outward = −n_a`, e.g. a flipped plane normal) the kept
disk rendered INWARD, because the Circle render fast-path orients the disk fan by
the **circle's `normal` field**, not the face hint. Fix: `kept_circle.normal =
kept_outward` (the kept-HIGH path was `+n_a` either way, masking the bug). The
elliptic cap (NURBS self-loop) is unaffected — it correctly uses the face hint −m.
Adversarial sweep now covers kept-high / kept-low / off-origin / tilted-axis valid
cuts + 5 degenerate rejections + the perpendicular-band gate.

**회귀**: axia-geo +3 (sim sweep + sew/render sim + production) + 1 adversarial
sweep = **+4** (axia-geo 1901→1905, 0 회귀, #[ignore] 0). axia-core 390 /
axia-transaction 5 green. Engine-internal only — no WASM/UI surface (γ wires
dispatch).

### β-3 (cylinder two-parallel-oblique-plane slab)

`Mesh::boolean_cylinder_oblique_slab(cyl, plane_normal m, d_lo, d_hi, mat)` — two
PARALLEL oblique planes (shared `m`, offsets `d_lo < d_hi` along `m` from
`axis_origin`) → keeps the band `d_lo < (p−axis_origin)·m < d_hi`: a trimmed
Cylinder band with TWO elliptic boundaries + two planar ELLIPTIC caps (no
circular end disk). A close mirror of β-2 reusing `nurbs::ellipse`,
`tessellate_cylinder_clipped`, and `sew_curved_band` — the boundary-aware clip's
min/max strip already generalises to BOTH boundaries oblique, so no new render
work. Both caps are NURBS self-loops rendered by the face hint (cap_lo −m, cap_hi
+m), so the β-2 circle-normal subtlety does NOT arise. Each ellipse centre is at
axial `t = d / (n_a·m)`; the gate requires each ellipse wholly on the side
(`v0 < t ± z_span < v1`). ⟂ / ∥ / `d_lo ≥ d_hi` / out-of-range `bail!()`.

**적대적 검증** (직접 sweep) — **0 bug** (예상대로, circular disk 부재로 β-2 버그
클래스 없음). production test (Z-axis, band stays between planes + front-facing)
first-try PASS. Adversarial sweep: tilted-axis / off-origin / assorted normals
(incl. negative-component) valid slabs (d derived from chosen axial centres,
`n_a·m`-sign-robust) + 4 degenerate rejections.

**회귀**: axia-geo +2 (production + adversarial sweep) — **1905→1907**, 0 회귀,
#[ignore] 0. axia-core 390 / axia-transaction 5 green. Engine-internal only.

### β-4 (∥-axis line-pair → cylinder flat cut / D-shaft)

A plane PARALLEL to the axis (`plane_normal ⟂ axis`) sections the cylinder in a
LINE PAIR, not an ellipse — a different family. `Mesh::boolean_cylinder_axial_
halfspace(cyl, plane_origin, plane_normal, mat)` keeps the +`plane_normal` side as
a flat-on-cylinder (D-shaft): a PARTIAL Cylinder band (kept arc `u ∈ (α−ψ, α+ψ)`,
`α = atan2(m·p̂, m·r̂)`, `ψ = acos(−d_axis/r)`) + a flat rectangle (the cut) + two
D-shaped end caps (arc + chord). Returns `[band, flat, cap_v_hi, cap_v_lo]`.

**섬세한 시뮬레이션 먼저** — the novel render piece is the partial band; a
`Cylinder` restricted to a `u_range` already tessellates wholly on the kept side
(probe: max x = 0.9999 ≤ 1, no clip needed). So the band reuses the existing
`u_range`-honouring tessellation — no new render path.

**구현** (octant pattern, NOT a self-loop sew): remove the original → 6 verts
(each arc SPLIT at its midpoint so a D-cap has 3 boundary verts, clearing the
render polygon path's `< 3` guard + reaching `he_arc_fill_points`) → 4
`add_face_with_holes` calls, each `oriented` so its Newell normal points outward
(consistent orientation → shared edges twin) → `attach_arc` the 4+2+2 arc edges +
`set_surface`. The band keeps an analytic Cylinder (u_range restricted); the
caps/flat are Plane (face hint −n_a / +n_a / −m). ⟂ / oblique / missing-cut
`bail!()`.

**적대적 검증** (직접 sweep) — **0 bug**, production first-try PASS (incl. the
critical "all 4 faces render" check — the D-cap arc survives the ≥3-vert guard).
Sweep: keep-major / keep-minor / diametral (half-cylinder) / off-origin / tilted-
axis valid cuts + 4 degenerate rejections. The `oriented` helper derives winding
from geometry, so there is no β-2-style kept-side asymmetry.

**회귀**: axia-geo +3 (chord/partial-band sim + production + adversarial sweep) —
**1907→1910**, 0 회귀, #[ignore] 0. axia-core 390 / axia-transaction 5 green.
Engine-internal only.

### β-5 α (multi-plane corner — de-risk + ellipse_arc infra)

The hardest atomic: a cylinder clipped by ≥2 NON-parallel oblique planes meeting
at a corner (the box-corner case). The caps become PARTIAL ellipses (clipped by
neighbouring planes) and the band's top boundary is PIECEWISE. α de-risks the
geometry / render with three detailed simulations + builds the partial-ellipse
infrastructure, BEFORE the (large) DCEL build (β-5 β).

**Detailed simulations (all pass)** — minimal case = a "tent" cut (two oblique
planes meeting at a ridge on a Z-cylinder):
1. **Corner geometry** — the plane∩plane line ∩ cylinder gives the CORNER points
   (on the cylinder AND both planes; general two-plane-line formula). The band's
   top is PIECEWISE `min_i v_plane_i(u)`, the active plane switching exactly at
   the corner angles (the e1/e2 arcs meet at the corners).
2. **Band render generalisation** — `tessellate_cylinder_clipped` (β-2/3) clips to
   2 planes (min/max); a corner needs N planes: `v ∈ [max(lower bounds),
   min(upper bounds)]`. The simulated N-plane strip keeps every band sample in the
   kept region (0/723 outside).
3. **Partial elliptic cap** — a cap arc (a sub-arc of the cut ellipse) is
   representable as a rational-quadratic NURBS (the affine image of a circle arc);
   `he_arc_fill_points` renders NURBS, so the partial cap renders. A sampled
   sub-arc lies on the cylinder AND the plane to 1e-15.

**Infra**: `curves::nurbs::ellipse_arc(center, a, b, û, v̂, φ0, φ1)` — a partial
ellipse as a rational-quadratic B-spline, split into ≤ 90° segments (`C0` joins),
exact on the ellipse + interpolating both angular endpoints. The cap-arc builder
for β-5 β.

**β-5 β DCEL scope (discovered)** — bigger than β-2/3/4 (the "multi-week atomic"):
(1) the band mixes a SELF-LOOP inner (bottom circle) with a MULTI-EDGE outer (the
piecewise top) — not expressible via `add_face_with_holes` (a cylinder band isn't
2D-nested), so a new `mesh.rs` sew helper + manual HE wiring; (2) generalise the
clip to N planes, deriving each plane + its keep side from the twin CAP face
(`outer = min-upper`, `inner = max-lower`); (3) the partial caps share the band's
arc edges + a ridge edge. Built carefully: sew helper → watertight test → render →
caps → adversarial.

**회귀**: nurbs +1 (ellipse_arc) + axia-geo sim +3 = **+4**, 0 회귀, #[ignore] 0.
Engine-internal, no production-path change.

### β-5 β-1 (corner DCEL + render infra, proven by a full simulation)

The hard infra, built in the endorsed order (sew → watertight → render → cap) and
proven end-to-end by a complete tent-cut DCEL simulation:

- `Mesh::sew_corner_band(top_verts, top_curves, bottom_anchor, bottom_circle, …)`
  — wires the CORNER band: a Cylinder side whose top is a MULTI-EDGE loop (the
  piecewise elliptic arcs) and whose bottom is a self-loop circle (the band's
  inner). Unlike `sew_curved_band` (two self-loops), the top is `n` regular edges
  (`add_edge` + directed-HE + `set_curve` + manual next/prev loop). The cached
  band normal is derived from the top-loop winding (ADR-007 I2). Also creates the
  bottom disk. Returns `(band, bottom_disk, top_vids)` so the caller builds the
  partial caps with `add_face_with_holes` reusing the band arc edges + a ridge.
- `Mesh::tessellate_cylinder_corner_clipped(face, chord_tol)` — N-plane band
  render: `v ∈ [max(inner-loop lower bounds), min(outer-loop upper bounds)]` per
  `u`. Gated to a corner (≥2 DISTINCT oblique planes in the outer loop), so β-2/β-3
  (single outer self-loop) and β-4 (⟂ circle arcs) are untouched. Wired into
  `export_buffers_inner` after `tessellate_cylinder_clipped`.

**DCEL simulation** (`sim_adr205_beta5_corner_dcel_watertight`) — actually builds
the tent-cut solid (`sew_corner_band` + 2 `add_face_with_holes` caps + ridge) and
verifies it is WATERTIGHT (open HEs 0) + invariant-valid + MANIFOLD + all 4 faces
render + the band stays in the kept region (0 verts outside, the N-plane clip).
This proves the band wiring + the corner clip before the production op.

**회귀**: axia-geo +1 (DCEL sim) — 1914→1915, 0 회귀, #[ignore] 0. No production-
path change (the corner clip's gate excludes every existing face).

### β-5 β-2 (production `boolean_cylinder_corner` — the user-callable MVP)

`Mesh::boolean_cylinder_corner(cyl, p1_origin, p1_normal, p2_origin, p2_normal,
mat)` — generalises the tent-cut sim to ARBITRARY two-plane corners. Keeps the
`+plane_normal` side of BOTH (a tent): bottom disk + corner band (piecewise
elliptic top) + two PARTIAL elliptic caps. Returns `[band, bottom_disk, cap, cap]`.

Pipeline: corner-find (ridge = plane1∩plane2 line, `disc>0` ∩ cylinder → 2
corners) → corner `u` + per-arc active plane (min upper bound at the arc midpoint)
→ arc-midpoint verts → `ellipse_arc` curves on the active plane's ellipse → remove
the original → `sew_corner_band` + 2 `add_face_with_holes` caps. MVP gate: both
planes oblique with `n_a·m < 0` (cut from the top), non-parallel, the ridge
crossing the side (2 corners on it), and the bottom circle wholly kept.

**적대적 검증** (직접 sweep) — found & fixed **2 real orientation bugs**:
1. `sew_corner_band`'s band cached normal — a "flip toward `band_normal`" step
   was wrong: for a Cylinder band the caller's radial `band_normal` is ⟂ the axial
   winding normal, so `dot ≈ 0` and float noise (−1e-16) flipped the sign,
   tripping ADR-007 I2. Fix: use `compute_normal` (Newell = the winding) directly.
2. global outward orientation — the natural top-loop winding left the partial caps
   wound INWARD (Newell = +m_i) and the bottom disk's Circle render fan faced +n_a
   (the β-2 circle-normal bug class again). Fix: build the top loop in DECREASING u
   (so the caps' Newell = −m_i) + set the bottom circle's `normal = −n_a`.

Production first-try after the fixes. Sweep: symmetric / asymmetric / tilted-ridge
/ off-origin valid corners (all watertight + manifold + 4 faces render + front-
facing + band in the kept region) + 4 degenerate rejections (parallel / ⟂ / ridge-
misses / cut-from-below).

**회귀**: axia-geo +2 (production + adversarial sweep) — 1915→1917, 0 회귀,
#[ignore] 0. axia-core 390 / axia-transaction 5 green. The β-5 multi-plane corner
MVP (a cylinder cut by a 2-plane corner) is now user-callable engine-internal.

### γ-engine (single-plane trim dispatch — engine + simulation)

The user-facing closure starts with a tilted cylinder cut by ONE arbitrary plane.
A detailed simulation (`sim_adr205_gamma_tilted_cylinder_plane_dispatch`) first
audited the existing dispatch (`try_curved_intersect_dispatch` rejects non-Z
cylinders + uses the Z-locked slab) and verified the γ routing geometry, then the
engine dispatch was added:

`Mesh::boolean_cylinder_trim_plane(cyl, plane_origin, plane_normal, mat)` — the
single entry the SliceTool / `boolean()` calls. Keeps the +`plane_normal` side,
dispatching on `cosθ = |n_a·m|`: `≈1` (⟂ axis) → local-frame slab to the far end
(a ⟂ halfspace, tilted axis preserved); `≈0` (∥ axis) → β-4 axial flat; otherwise
→ β-2 elliptic halfspace.

The simulation verified all three single-plane branches (oblique / axial / ⟂) on a
TILTED cylinder are watertight + manifold + invariant-valid, plus the box ∩
cylinder slab routing (a Z-only box cut → β-3 with the box Z-range, band within
the range). **Routing constraint discovered**: a box face must cut the cylinder's
SIDE (not clip an end cap), else β-3 bails — the box-∩-cylinder auto-routing (γ-2)
must branch on this.

**회귀**: axia-geo +1 (dispatch sim) — 1917→1918, 0 회귀, #[ignore] 0. Engine-only
(no WASM/UI yet).

### 다음

γ-wire (SliceTool single-plane MVP): Scene `trim_curved_volume_by_plane` → WASM
`trimCurvedByPlane` → TS bridge → SliceTool arbitrary-plane + browser demo.

### γ-wire-core (engine + Scene trim, plane-general)

The core (non-UI) layers of the SliceTool single-plane MVP:

- `Mesh::trim_curved_by_plane(faces, plane_origin, plane_normal, mat)` — the
  curved-trim dispatcher (mirrors `cut_curved_by_z_plane`, but plane-general +
  TRIM-only). `Some(Ok)` on a cylinder it handles, `None` → polygonal fallback.
  Unlike `classify_curved_primitive` (Z-axis only) it detects a Cylinder of ANY
  axis directly, so tilted cylinders route. MVP scope: cylinder (sphere / cone /
  torus arbitrary-plane = γ-2).
- `Scene::trim_curved_volume_by_plane(faces, plane_origin, plane_normal)` —
  transaction-wrapped (single undo), keeps the `+plane_normal` side as one volume
  on the source XIA; `routed: false` → polygonal fallback. Mirrors the TRIM branch
  of `cut_curved_volume_by_z`.

A Scene test (`adr205_gamma_trim_curved_volume_by_plane`) routes a cylinder XIA cut
by an oblique plane (→ β-2) — `routed`, the source XIA owns the trimmed result,
mesh valid — and confirms a box input yields `routed: false` (fallback), mesh
intact.

**회귀**: axia-core +1 (Scene trim test) — 390→391, 0 회귀, #[ignore] 0. axia-geo
+26 lines (dispatcher, tested via Scene). The WASM / TS bridge / SliceTool layers
remain (γ-wire-ui).

### γ-wire-ui (user-facing last layer — WASM + TS bridge + SliceTool + browser demo)

The user-facing closure of the single-plane trim:

- **WASM** `trimCurvedByPlane(face_ids, ox,oy,oz, nx,ny,nz)` (lib.rs) — thin
  wrapper over `Scene::trim_curved_volume_by_plane`, mirroring `cutCurvedByZPlane`.
  Returns `{ok, routed, resultFaces?, totalFaces?, error?}`. `routed:false` →
  the input had no curved primitive, caller falls through to the polygonal slice.
- **TS bridge** (WasmBridge.ts) — `trimCurvedByPlane?(faceIds, ox,oy,oz, nx,ny,nz)`
  added to `AxiaEngineExtended`.
- **SliceTool** (SliceTool.ts) — arbitrary-plane TRIM branch BEFORE the polygonal
  slice fallback (after the horizontal `cutCurvedByZPlane` branch). For
  `cutMode !== 'slice'`, `keepN = above ? normal : -normal`; on `routed:true`,
  syncs + Toast; on `routed:false`, falls through to polygonal.

**Browser demo (real WASM, localhost:3002)**: Path B cylinder (r=5 h=12, kind=2
Cylinder side) cut by the oblique plane `x+z=6` (cosθ=0.707 → β-2 dispatch). Result
`ok:true, routed:true, resultFaces:[3,4,5]` = Cylinder band (kind=2, **surface
preserved**) + base disk + elliptic cap (both Plane). Watertight: ADR-007
`valid:true, violationCount:0`, non-manifold 0 (the `boundary_edge_count:1` is a
Path B self-loop counting quirk — identical before/after the trim, not a trim bug;
the engine's real watertight criterion null-face-HE=0 from the committed β-2 test
holds). Three.js render: `front-mesh`/`back-mesh-sheet`, **Z∈[0,11]** slanted
elliptic top (un-trimmed would be flat z=12), 2537 tris = analytic chord-tolerant
tessellation. (Canvas screenshot times out — known issue; verified via geometry-
level `preview_eval`.)

**회귀**: WASM/TS only (no Rust test — WASM exports are browser-verified, mirroring
`cutCurvedByZPlane`). SliceTool + WasmBridge vitest 261 passed, 0 regression. tsc
clean.

### γ-2a (box ∩ tilted-cylinder auto-routing — SLAB config)

The first user-facing `box ∩ cylinder` auto-route: a `boolean(tilted-cylinder,
axis-box, Intersect)` whose box is a **slab in one cardinal direction** is now
detected + routed to β-3 oblique-slab through the curved-intersect dispatch (no
manual plane). Before γ-2a this Erred (the #Track2 guard — "circular-section slab
machinery cannot represent oblique sections"); β-2..β-5 built exactly that oblique
SSI, so γ-2a flips the bail into a route.

- **`try_tilted_cylinder_box_slab`** (boolean.rs) — runs inside
  `try_curved_intersect_dispatch` BEFORE `classify_curved_primitive` (which rejects
  tilted cylinders). For each box face it compares the face's cardinal coordinate
  against the cylinder's lateral-surface extent along that axis
  (`ao·e + [min,max] v·(â·e) ± r·amp`, amp = radial sweep onto e). Classifies a
  PURE single-axis slab (one parallel pair cuts both faces, the other two contain
  the cylinder), checks the slab is OBLIQUE (cosθ = |â·e| ∈ (ε,1−ε)), and routes
  `boolean_cylinder_oblique_slab` with that pair's cardinal normal + axis-relative
  offsets. Box consumed via `remove_box_solid`.
- **Decline semantics** (메타-원칙 #16): a non-cylinder operand / Z-axis cylinder
  (handled by the circular-section path) / non-slab config (halfspace / corner /
  multi-plane) → `None` (fall through → #Track2 Err); a recognised slab that β-3
  cannot represent (an ellipse would extend past an end cap) → `Some(Err)`
  surfaced, not silently faceted.
- **No new user-facing surface** — the existing Boolean tool's `boolean(…,
  Intersect)` gets smarter; no WASM/TS/UI change. The dispatch reorder
  (`classify_axis_box` before `classify_curved_primitive`) is behavior-neutral for
  every pre-existing case (proved by the full suite + the adversarial sweep's
  Z-axis case).

**회귀**: axia-geo +3 — `sim_…detection` (which-faces-cut geometry: X/Y/Z extents,
cut-count classification, β-3 result + negative multi-axis probe), `…autoroutes_to
_beta3` (public `boolean()` wiring, commutative, Cylinder band + tilted axis
preserved), `…adversarial_sweep` (general tilt + off-origin route; corner +
end-cap-clip graceful Err with mesh intact; Z-axis no-regression). axia-geo lib
1918→1921, full workspace green (axia-core 391), 0 regression, #[ignore] 0.

### γ-2b (box ∩ tilted-cylinder — HALFSPACE + no-op containment)

Reusing the which-faces-cut geometry, `try_tilted_cylinder_box_halfspace`
classifies each of the box's 6 faces against the cylinder's cardinal extents —
**Cuts** (coord strictly inside the cylinder's e-extent), **NonBinding** (cylinder
entirely on the inside of the face), or **Excluding** (cylinder entirely outside).
It then routes by the binding-face arrangement (run after the slab route in the
dispatch):

- **EXACTLY ONE Cuts** (rest NonBinding) → β-2 oblique halfspace, keeping the
  INSIDE of the box (plane normal = −outward; origin = e·coord), provided the
  cutting axis is oblique (cosθ = |â·e| ∈ (ε,1−ε)).
- **ZERO Cuts + ZERO Excluding** (box ⊇ cylinder) → no-op `A ∩ B = A`: return the
  cylinder unchanged, consume the box.
- **≥1 Excluding** (disjoint → empty intersect) / a parallel-axis clip (cosθ≈0,
  β-4 territory) / ≥2 Cuts (slab — caught earlier — / corner — γ-2c) → `None`
  (fall through). A recognised halfspace β-2 cannot cut cleanly (the plane clips an
  end cap) → `Some(Err)` surfaced.

**회귀**: axia-geo +3 — `sim_…halfspace_noop_detection` (6-face Cuts/NonBinding/
Excluding classification + β-2 keep-inside), `…halfspace_and_containment_autoroute`
(public `boolean()` halfspace + containment no-op returns the cylinder faces
unchanged + commutative), `…halfspace_adversarial_sweep` (−Z / +Y / off-origin
routes; parallel-axis + disjoint + end-cap-clip graceful Err mesh-intact). axia-geo
lib 1921→1924, full workspace green (axia-core 391), 0 regression, #[ignore] 0.

### γ-2c (box ∩ tilted-cylinder — CORNER → β-5 tent)

The third box-config route completes the slab/halfspace/corner/no-op set.
`try_tilted_cylinder_box_corner` (run after the halfspace route) reuses the face
classification: it collects the cutting faces, and routes **exactly two Cuts on
DIFFERENT axes** (rest NonBinding, none Excluding) to β-5 `boolean_cylinder_corner`
— but only the subset β-5 can represent:

- Each cutter maps to its (origin on the plane, **inward** normal m = −outward).
- β-5 keeps the LOWER tent (+m side of each plane, base cap kept), so it requires
  both planes to be **"upper bounds"**: n_a·m < 0 (the cylinder kept below each).
  A corner with a non-upper-bound face (e.g. +Z + −Y, where −Y's inward +Y has
  n_a·m > 0) is a corner orientation β-5 cannot represent → `None` (deferred, with
  general N-plane corners).
- Both must be oblique (|n_a·e| ∈ (ε,1−ε)). β-5's own preconditions (ridge crosses
  the side; corners within the caps; **the base cap is wholly kept** — both planes
  clear of the v0 cap's extent) surface as `Some(Err)` if violated.

**회귀**: axia-geo +3 — `sim_…corner_detection` (6-face classification → two-Cuts
collection + (origin, inward-normal) mapping + upper-bound/oblique checks + β-5
tent watertight + band-in-kept-region; non-upper-bound +Z+−Y deferred), `…corner_
autoroutes_to_beta5` (public `boolean()` +Z+Y corner → 4-face tent, commutative,
Cylinder band + tilted axis preserved), `…corner_adversarial_sweep` (+Z+X different
axis pair + off-origin route; non-upper-bound + ridge-miss graceful Err mesh-
intact). The adversarial sweep caught β-5's base-cap-clear precondition (the
cutting planes must clear the v0 cap). axia-geo lib 1924→1927, full workspace green
(axia-core 391), 0 regression, #[ignore] 0.

**box ∩ tilted-cylinder auto-routing complete** (slab → β-3 / halfspace → β-2 /
corner → β-5 / no-op containment), all surface-preserving through the existing
Boolean tool. Deferred: ≥3-face / non-tent corners (general N-plane), cone Dandelin
(elliptic-section cone cuts), torus spiric sections.

### cone Dandelin α (de-risk + ellipse helper infra)

The next primitive: box ∩ tilted CONE. An oblique plane cutting a cone is a conic
(Dandelin): an ELLIPSE when the plane is steeper than the cone's slant, else a
parabola/hyperbola. This α step de-risks + locks the **ellipse-section closed form**
(the β-2-cone prerequisite, analogous to the cylinder β-1 ellipse infra).

`cone_oblique_ellipse(apex, axis_dir, half_angle, plane_origin, plane_normal) ->
Option<(center, semi_major, semi_minor, major_dir, minor_dir)>` — returns the
planar ellipse, or `None` when the section is not a bounded ellipse (parabola/
hyperbola |n_a·m| ≤ p·tanα, plane ⟂ the axis = a circle, or plane through the
apex). Geometry (apex A, unit axis n_a apex→base, half-angle α, plane (O, m)):

```
D = n_a·m,  p = |m − D·n_a|,  q = (m − D·n_a)/p,  r2 = n_a × q,
a = cosα·D,  b = sinα·p,  k = (O−A)·m,  denom = a²−b²  (>0 for the ellipse),
center     = A + (k/denom)(a·cosα·n_a − b·sinα·q),
semi_major = |k|·√(b²cos²α + a²sin²α) / denom,   (axis in the n_a–q plane)
semi_minor = |k|·sinα / √denom.                   (axis along r2)
```

**회귀**: axia-geo +1 — `sim_adr205_cone_oblique_ellipse_geometry`: validates the
helper for (A) a Z-axis cone + oblique plane and (B) a TILTED cone + a cardinal +Z
box face (the γ target) — every sampled ellipse point lies on the cone (angle to
axis = α, 1e-6) ∩ the plane, within the finite axial range; the parabola/hyperbola
+ apex-plane cases return `None`; `nurbs::ellipse` (β-1) accepts the params.
axia-geo lib 1927→1928, 0 regression, #[ignore] 0.

### β-2-cone α (DCEL surgery de-risk)

The kept BASE side of a cone cut by an oblique plane is a frustum-with-an-elliptic-
top: base disk (Plane) + cone-side band (Cone surface, base circle + ellipse
boundaries) + elliptic cap (Plane). This α step proves that the **existing reuse
primitive `sew_curved_band`** (the same one cylinder β-2 uses) sews a Cone band
watertight + manifold — the core DCEL risk — BEFORE the production op + the
boundary-aware render.

`sim_adr205_cone_oblique_halfspace_dcel` builds the frustum directly: top = the
elliptic section (NURBS self-loop from `cone_oblique_ellipse` + `nurbs::ellipse`),
bottom = the base circle, band = the Cone surface, caps = elliptic Plane + base
disk Plane. The result is watertight (no null-face HE), manifold (0 non-manifold
edges), invariant-valid, and the band keeps its Cone surface. axia-geo lib
1928→1929, 0 regression, #[ignore] 0.

### β-2-cone β (production op + boundary-aware render)

`boolean_cone_oblique_halfspace(cone_faces, plane_origin, plane_normal, material)`
— a kernel-native cone cut by an oblique plane, keeping the base FRUSTUM:
`[cone_side_band, elliptic_cap, base_disk]`.

- **Op**: reads the Cone surface (`cone_full_of`), requires the plane to SEPARATE
  the apex (−m) from the base (+m) — keeping the base on +m — computes the section
  via `cone_oblique_ellipse` (cone Dandelin α), guards the ellipse strictly within
  the axial range (clear of apex + base), removes the original cone, and sews the
  frustum with the existing `sew_curved_band` (β-2-cone α). Deferred (Err): keeping
  the apex tip (base on −m), a parabola/hyperbola section, a ⟂/∥ plane, or a
  non-separating plane.
- **Render** `tessellate_cone_clipped` (mesh.rs) — boundary-aware, dispatched after
  `tessellate_cylinder_corner_clipped`. Per angle u the band spans `v ∈ [v_lo,
  v_hi]` where the cone generator pierces each boundary plane; the vertex is
  `apex + axis·v + radial·(v·tanα)` (exactly `cone::evaluate`, so the per-(u,v)
  analytic normal is exact). Mirrors `tessellate_cylinder_clipped`; without it the
  band would render the full cone and over-draw past the oblique ellipse.

**회귀**: axia-geo +2 — `adr205_beta2cone_oblique_halfspace_frustum` (3-face frustum,
watertight + manifold + invariant, Cone band preserved + rendered boundary-aware
[every band vertex on the kept +m side — no over-draw] + front-facing; apex-tip
keep deferred), `adr205_beta2cone_oblique_adversarial_sweep` (steeper / off-axis /
TILTED cone + cardinal −Z [the γ target via rotate_verts] route; hyperbola / ⟂ /
non-separating graceful Err mesh-intact). axia-geo lib 1929→1931, full workspace
green (axia-core 391, axia-wasm builds), 0 regression, #[ignore] 0.

### β-3-cone (oblique elliptic slab)

`boolean_cone_oblique_slab(cone_faces, plane_normal, d_lo, d_hi, material)` — a cone
cut by TWO PARALLEL oblique planes (shared normal m, offsets d_lo < d_hi along m
from the apex) keeps the elliptic SLAB: `[cone_side_band, cap_hi, cap_lo]` — a
trimmed Cone band with TWO ellipse boundaries + two elliptic caps (no base disk,
no apex). Reuses ALL of β-2-cone's infra: `cone_oblique_ellipse` for both sections,
`sew_curved_band` for the band, and `tessellate_cone_clipped` for the render (which
already handles two planar boundaries — both ellipses here). cap_lo faces −m, cap_hi
faces +m (away from the kept band). MVP scope: the slab lies strictly between the
apex (d=0) and the base, both sections are bounded ellipses wholly on the side.

**회귀**: axia-geo +2 — `adr205_beta3cone_oblique_slab` (3-face slab, watertight +
manifold + invariant, Cone band preserved + rendered boundary-aware [every band
vertex within d∈[d_lo,d_hi]] + front-facing), `adr205_beta3cone_slab_adversarial_
sweep` (TILTED cone + two cardinal ±Z planes [the γ slab target via rotate_verts]
route; slab-containing-apex + ⟂ plane graceful Err mesh-intact). axia-geo lib
1931→1933, 0 regression, #[ignore] 0 (boolean.rs only — the render already supports
two-ellipse bands).

### γ-cone-slab (box ∩ tilted-cone → β-3-cone auto-routing)

The first user-facing `box ∩ cone` auto-route: `boolean(tilted-cone, axis-box,
Intersect)` whose box is a SLAB in one cardinal direction is detected + routed to
β-3-cone through the curved-intersect dispatch (no manual plane).

`try_tilted_cone_box_slab` (run after the cylinder routes) adapts the cylinder γ-2a
which-faces-cut detection to the cone: the cone narrows to the apex, so its cardinal
extent spans the apex POINT + the base RIM
(`min(apex·e, base_center·e − r·amp_e)` … `max(apex·e, base_center·e + r·amp_e)`).
A clean single-axis slab needs the apex on one side of BOTH faces AND the WHOLE base
disk on the other (apex below the slab + base above, or vice-versa), with the other
two directions containing the cone — then it routes
`boolean_cone_oblique_slab(e, box.min[e]−apex·e, box.max[e]−apex·e)`. Returns `None`
for a Z-axis cone (the circular classify path), a non-cone operand, or any non-slab
config (halfspace / base-clip / corner — deferred). No new user-facing surface (the
existing Boolean tool gets smarter).

**회귀**: axia-geo +2 — `sim_adr205_gamma_cone_box_slab_detection` (cone apex+base-
rim cardinal extent + clean-slab-axis classification + β-3-cone route + negative
X-thin probe), `adr205_gamma_cone_box_slab_autoroutes` (public `boolean()` wiring,
commutative, Cone band preserved + watertight + manifold). axia-geo lib 1933→1935,
full workspace green (axia-core 391), 0 regression, #[ignore] 0.

### γ-cone-halfspace (box ∩ tilted-cone → β-2-cone + no-op containment)

`try_tilted_cone_box_halfspace` (run after the cone slab route) classifies each of
the box's 6 faces against the cone (apex point + base disk):

- A face **cleanly cuts** (apex on one side, the WHOLE base disk on the other) with
  the BASE on the INWARD side → a β-2-cone frustum cut. EXACTLY ONE such face (rest
  containing the cone) → `boolean_cone_oblique_halfspace(e·coord, inward)`.
- ZERO cuts + all containing (box ⊇ cone) → no-op `A∩B=A` (return the cone, consume
  the box).
- Any face EXCLUDING the cone (disjoint), CLIPPING the base disk, or keeping the
  APEX TIP (apex inside) → `None` (deferred — the apex-tip keep is a separate
  construction; the apex is a degenerate point, not a `sew_curved_band` boundary).

**회귀**: axia-geo +1 — `adr205_gamma_cone_box_halfspace_autoroutes` (public
`boolean()`: apex-clipping box → β-2-cone frustum; box ⊇ cone → containment returns
the cone faces unchanged; commutative; apex-tip keep deferred Err). axia-geo lib
1935→1936, full workspace green (axia-core 391), 0 regression, #[ignore] 0.

**box ∩ tilted-cone auto-routing**: slab → β-3-cone / halfspace → β-2-cone (frustum)
/ no-op containment, all surface-preserving through the existing Boolean tool.
Deferred: apex-tip keep, base-clipping cuts, cone corner (two perpendicular faces).

### cone-corner α (tent geometry de-risk)

The cone analog of cylinder β-5: a cone cut by TWO oblique planes forming a base-
keeping TENT (base disk + corner band + two partial elliptic caps). This α step
de-risks the tent GEOMETRY before the (β-5-level) DCEL/render.

`sim_adr205_cone_corner_tent_geometry` — each plane gives a cone-section ellipse
(`cone_oblique_ellipse`); the two planes' ridge crosses the cone at two CORNER
points (where the ellipses meet, found by bisecting `v_e1(u) − v_e2(u)`); per
generator u the kept base frustum's top follows `max(v_e1(u), v_e2(u))` (the
binding plane closer to the base), the active plane switching at the corners. The
probe verifies the corners lie on BOTH planes + the cone (within the finite axial
range) and the band top sits on the active plane + the cone. (A test-sampling note:
the corner-angle detector samples a count NOT divisible by 4, so the symmetric
corners at π/2, 3π/2 fall strictly between samples — an exact-grid crossing would
be missed by a strict sign change.) axia-geo lib 1936→1937, 0 regression,
#[ignore] 0.

### cone-corner β-1 (DCEL de-risk)

`sim_adr205_cone_corner_dcel_watertight` builds the cone TENT solid — base disk +
corner band (Cone surface, base circle inner + a 4-edge tent top of two active
ellipse ARCS) + two partial elliptic caps — with the EXISTING reuse primitive
`sew_corner_band` (the same one cylinder β-5 uses) + two `add_face_with_holes` caps,
and verifies it is watertight (0 open HEs) + manifold + invariant-valid. The cone
mirrors cylinder β-5 with two flips: the kept base frustum's tent top follows
`max(v_e1, v_e2)` (not `min`), and `n_a` (apex→base) IS the base-outward normal. The
corners (ridge ∩ cone) come from bisecting `v_e1(u) − v_e2(u)`; the active ellipse
arcs use `nurbs::ellipse_arc` + `cone_oblique_ellipse`. axia-geo lib 1937→1938, 0
regression, #[ignore] 0 (DCEL sim, no render change).

### cone-corner β-2 (production op + boundary-aware render)

`boolean_cone_corner(cone_faces, p1_origin, p1_normal, p2_origin, p2_normal,
material)` — a cone cut by a base-keeping TENT (two oblique planes) → a 4-face
corner solid `[band, base_disk, cap_a, cap_b]`.

- **Op**: validates both planes (bounded ellipse via `cone_oblique_ellipse`, apex
  on −m, base on +m), computes the ridge ∩ cone CORNERS by a closed-form quadratic
  on the nappe, picks the active plane per arc (base frustum → argMAX v_e), builds
  the 4-edge tent top of two active ellipse arcs (`nurbs::ellipse_arc`), removes the
  original cone, and sews with `sew_corner_band` + two `add_face_with_holes` caps.
  Rejected: parallel planes, a ridge that misses the cone, a corner past an end, a
  clipped base disk.
- **Render** `tessellate_cone_corner_clipped` (mesh.rs, dispatched after
  `tessellate_cone_clipped`) — per generator the band spans `v ∈ [max(OUTER tent
  planes), min(INNER base plane)]`, the MIRROR of `tessellate_cylinder_corner_
  clipped` (for the cone the tent OUTER loop is the LOWER bound, the base circle
  INNER loop the UPPER bound). Vertex = `cone::evaluate`.
- **Orientation** (debugging note): the cone mirrors cylinder β-5, so the top loop
  uses the OPPOSITE winding (`[c1, mid_a, c2, mid_b]`, not `[…, mid_b, …, mid_a]`)
  — this makes the partial caps' Newell normals point OUTWARD (−m_i) AND satisfy
  ADR-007 I2 (the cached normal must equal the winding's Newell — a `set_normal`
  flip would violate it; the winding itself must carry the orientation).

**회귀**: axia-geo +1 — `adr205_cone_corner_tent` (4-face tent, watertight + manifold
+ invariant, Cone band preserved + rendered boundary-aware [every band vertex on
the kept +m side of BOTH planes] + all faces front-facing; parallel + ridge-miss
rejected). axia-geo lib 1938→1939, full workspace green (axia-core 391, axia-wasm
builds), 0 regression, #[ignore] 0.

### cone-corner γ (box ∩ tilted-cone → cone corner auto-routing)

`try_tilted_cone_box_corner` (run after the cone halfspace route) reuses the cone
apex+base-rim classifier: for a sufficiently TILTED cone (apex + base spread across
two cardinal directions) two PERPENDICULAR box faces can each cleanly cut the cone
keeping the BASE on the inward side (apex on −m, the whole base disk on +m) — a
base-keeping tent → route `boolean_cone_corner(o1, m1_inward, o2, m2_inward)`.
EXACTLY two such base-keeping cuts on different axes (rest containing) routes;
anything else (a face excluding / clipping the base / keeping the apex tip, or ≠2
perpendicular cuts) → `None` (the apex-tip corner is deferred). No new user-facing
surface.

**회귀**: axia-geo +1 — `adr205_gamma_cone_box_corner_autoroutes` (public `boolean()`:
a cone tilted ~34° about Y + a corner box [+X at x=3 AND +Z at z=4, both keeping the
base] → `boolean_cone_corner` tent, watertight + manifold + Cone band, commutative).
axia-geo lib 1939→1940, full workspace green (axia-core 391), 0 regression,
#[ignore] 0.

**box ∩ tilted-cone auto-routing complete** (slab → β-3-cone / halfspace → β-2-cone
frustum / corner → cone-corner tent / no-op containment), all surface-preserving
through the existing Boolean tool — full cylinder parity except the apex-tip keep.

### torus spiric α (geometry + DCEL de-risk — two simulations, no production yet)

A new primitive family. Unlike cylinder/cone (oblique section = a conic ELLIPSE, exactly
a rational-quadratic NURBS self-loop), an oblique plane cutting a TORUS gives a
**spiric section** — a degree-4 (quartic) curve of the Cassini-oval family. There is
no exact NURBS self-loop, so the cap boundary must be a **sampled polyline**. Two probes
de-risk this before any production op:

**§1 geometry** (`sim_adr205_torus_spiric_section_geometry`) — the tractable handle is the
**minor circle** at major angle u: `M_u(v) = c_u + r·cos v·radial_u + r·sin v·n_a`
(`c_u = C + R·radial_u`). The plane `(X−O)·m=0` pierces it where `A_u + B_u·cos v + D·sin v = 0`
with `A_u=(c_u−O)·m`, `B_u=r(radial_u·m)`, `D=r(n_a·m)` (D u-independent) — 0/1/2 solutions
per u, EXACTLY the cylinder/cone boundary-aware pattern. Proven: (a) every section point lies
on the torus ∩ plane to ~1e-15 (boundary-aware is exact → `tessellate_torus_clipped` feasible);
(b) the ⟂-axis plane degenerates to the known z-cut — 2 concentric circles, loops=2,
perim=4πR=50.27 (validates the union-find topology classifier + z-cut consistency);
(c) topology range — a plane within ~22° of the axis (`|m_∥|/|m_⊥| ≤ r/√(R²−r²)`) pierces
every minor circle twice → an ANNULAR cap (2 spiric ovals, perim ≠ 4πR ⇒ genuinely a spiric);
a steeper/grazing cut misses some minor circles (hist[0]>0) → the annulus pinches to one oval.

**§2 DCEL surgery** (`sim_adr205_torus_oblique_halfspace_dcel`) — the key claim: **no new sew
primitive is needed**. The kept (annular) side is a **Torus band** (kept half-tube, an annulus)
+ an **annular Plane cap** (planar region between the two ovals), sharing the SAME two sampled
spiric loops. Because `add_edge` reuses an existing edge and `make_loop` grabs the free twin
half-edge, two `add_face_with_holes` calls with REVERSED windings make the band & cap share
every rim edge's twin → watertight + manifold (verified: nm_edges=0, invariants valid). This is
the production recipe for `boolean_torus_oblique_halfspace`. (Outer vs inner oval is split per-u
by `cos v` — radial-from-axis = R + r·cos v.)

**회귀**: axia-geo +2 (`sim_adr205_torus_spiric_section_geometry` + `sim_adr205_torus_oblique_halfspace_dcel`).
axia-geo lib 1940→1942, 0 regression, #[ignore] 0. No production op / WASM / render / γ yet
(next steps below).

### β-2-torus (production op + boundary-aware render)

`boolean_torus_oblique_halfspace(torus_faces, plane_origin, plane_normal, material)` — a
kernel-native torus cut by an OBLIQUE plane in the ANNULAR regime, keeping the `+plane_normal`
halfspace → `[Torus band, annular Plane cap]`. Follows the §2 de-risk recipe exactly:
`torus_full_of` reads the surface; the two spiric ovals are sampled (validating annular — every
minor circle pierced twice, else `bail!` for the deferred pinched regime); the band & cap are
sewn by two `add_face_with_holes` with reversed windings (sharing each rim edge's twin). Newell
of the u-ordered outer oval orients the faces so **cap.normal() = −m (outward)** and
**band.normal() = +m (the kept side)**. Validation runs BEFORE any face removal, so a bail leaves
the mesh INTACT.

Render **`tessellate_torus_clipped`** (mesh.rs, after `tessellate_cone_corner_clipped` in the
dispatch chain): unlike cylinder/cone (an analytic self-loop → `plane_of`), the band's spiric
boundary is a SAMPLED polyline, so the cut plane is recovered from `face.normal()` (= +m, the
kept side) through the outer-loop centroid. Per `u` the minor circle is pierced twice; the KEPT
arc (midpoint on +kept_n) is sampled into an `n_u × n_v` grid — `n_v ≥ 1` rows because the minor
circle is CURVED (≠ the cone's straight generator). The grid winding `[a,c,d],[a,d,b]` is
front-facing by construction (`∂T/∂u × ∂T/∂v = (R + r·cos v)·r·(outward normal)`).

**회귀**: axia-geo +3 — `adr205_beta2torus_oblique_halfspace_annular` (tilt ~11.5° + cardinal +Z
through centre → 2 faces, watertight + manifold, oriented, render on the kept side) +
`adr205_beta2torus_oblique_adversarial_sweep` (annular configs route / outside-tube + steep-tilt
pinched → graceful Err, mesh intact) + `adr205_beta2torus_orientation_scale_sweep` (deterministic
ground truth over the five adversarial dimensions: kept-side m=−Z & negative tilt / **every render
triangle front-facing** / threshold 20° routes vs 29° bails vs ⟂-axis z-cut routes / off-centre
asymmetric ovals / scale R=1000 & thin r=0.3 with the threshold scaling as `atan(r/√(R²−r²))` /
non-cardinal tilt axis). axia-geo lib 1942→1945, core 391, 0 regression, #[ignore] 0. First-try
pass (geometry + DCEL de-risk paid off). The adversarial workflow hit a server rate-limit, so the
sweep was authored as a deterministic test instead — stronger (real ground truth, not speculation).

### β-3-torus slab α (topology + DCEL de-risk — two simulations, no production yet)

A torus cut by TWO parallel oblique planes (kept band `d_lo < (X−C)·m < d_hi`). Far more complex
than the cylinder/cone slab (one ellipse per plane → a 2-ellipse band): the torus annular slab is
4-oval-bounded with TWO band components. Two probes fix the MVP scope before any production.

**§3 topology** (`sim_adr205_torus_slab_section_geometry`) — per minor circle the kept set is
`{v : c_lo < cos(v−φ) < c_hi}`, `c_lo=(d_lo−A_u)/amp`, `c_hi=(d_hi−A_u)/amp` → 0/1/2 arcs (or the
WHOLE minor circle when the slab swallows it). Measured arc-count histograms (tilted torus R=4 r=1.5,
⟂-Z planes): the **STRADDLING** slab (d_lo<0<d_hi within the tube, e.g. ±0.4) is uniformly **2-arc**
(1440/1440) — every minor circle cut by both planes; a **ONE-SIDED** slab (0.4..0.9) is MIXED
(343 one-arc + 1097 two-arc); a **THICK** slab (±2.0) swallows part of the tube (646 minor circles
wholly inside). Finding: only the straddling regime is clean.

**§4 DCEL surgery** (`sim_adr205_torus_slab_dcel`) — the straddling slab is exactly **TWO Torus belts**
(outer + inner, split per-u by cos v) + **TWO annular Plane caps**, bounded by FOUR sampled spiric
ovals {outer,inner}×{d_lo,d_hi}. Each oval is shared by exactly two faces, so giving those two faces
OPPOSITE windings of the shared oval (the §2 recipe scaled to four `add_face_with_holes` calls —
cap_lo: outer_lo+inner_lo / outer_belt: outer_lo↔outer_hi / cap_hi: outer_hi+inner_hi / inner_belt:
inner_lo↔inner_hi) makes them share every rim edge's twin → watertight + manifold (verified: nm_edges=0,
no open half-edges, invariants valid).

**회귀**: axia-geo +2 (`sim_adr205_torus_slab_section_geometry` + `sim_adr205_torus_slab_dcel`).
axia-geo lib 1945→1947, 0 regression, #[ignore] 0. No production op / render / γ yet (next steps below).

### β-3-torus slab (production op + 2-belt boundary-aware render)

`boolean_torus_oblique_slab(torus_faces, plane_normal, d_lo, d_hi, material)` — a kernel-native
torus cut by TWO parallel oblique planes (offsets `d_lo < d_hi` from the centre along m), keeping
the STRADDLING annular slab → `[outer_belt, inner_belt, cap_lo, cap_hi]`. Follows the §4 recipe:
four spiric ovals are sampled (validating that BOTH planes cut every minor circle twice, else
`bail!` for the one-sided/tube-swallowing regimes); the four faces are sewn by four
`add_face_with_holes` calls with each oval shared by two faces at opposite windings. The global
winding is chosen from `Newell(outer_lo)·m` so cap_lo.normal()=−m and cap_hi.normal()=+m (both
lids outward). Validation precedes face removal → a bail leaves the mesh intact.

Render **`tessellate_torus_slab_clipped`** (mesh.rs, after `tessellate_torus_clipped`): a belt's two
boundary loops live on the TWO planes, so the cut planes are recovered from the loops' Newell normal
+ centroids and the belt side (outer/inner) from the outer loop's mean radius vs R. Per u the kept
strip's two ends are the two planes' pierces ON this side; `n_v` rows sample the arc between them, the
v-ordering normalised so v increases with the row → front-facing. `tessellate_torus_clipped`
(halfspace) gains a guard returning `None` for a belt whose two loops are non-coplanar (a slab belt),
so the dispatch routes each face to the right render.

**회귀**: axia-geo +3 — `adr205_beta3torus_oblique_slab_straddling` (tilt ~11.5° + ±0.4 +Z planes →
4 faces, watertight + manifold, caps outward ±m, both belts within the slab + front-facing) +
`_slab_adversarial` (straddling variants route / d_lo≥d_hi + one-sided-mixed + clear-of-tube →
graceful Err, mesh intact) + `_orientation_scale_sweep` (the cap-flip + 2-belt render generalise:
m=−Z, asymmetric slab, non-cardinal tilt axis, large R=1000 r=300). axia-geo lib 1947→1950, core 391,
0 regression, #[ignore] 0. First-try pass (the §3+§4 de-risk paid off).

### γ-torus (box ∩ tilted-torus auto-routing)

`try_tilted_torus_box` (run after the cone routes, before `classify_curved_primitive`). A torus is
"thin" along its axis (±r) and "wide" across it (±(R+r)), so the only clean cuts are by the box
faces along the cardinal `e*` most aligned with the axis — their normal ≈ the axis → annular
sections. The torus cardinal extent is closed-form: `C·e ± (√(1−(axis·e)²)·R + r)` (since
axis·e, p1·e, p2·e are an orthonormal triple). The classifier requires `|axis·e*| > √(R²−r²)/R`
(the §1 annular threshold) and that the two OTHER cardinals CONTAIN the torus (a ⊥-axis side cut is
the deferred pinched regime). Along `e*`: 0 cuts → no-op A∩B=A; 1 cut → β-2-torus halfspace
(inward normal ±e*); 2 cuts → β-3-torus slab (offsets from the centre). Declines Z-axis tori (the
existing circular classify path) and surfaces a clean Err for off-routing inputs.

**회귀**: axia-geo +2 — `adr205_gamma_torus_box_autoroutes` (a torus tilted ~11.5° off +Z routes a
2-cut box → β-3-torus slab (4 faces) / a 1-cut box → β-2-torus halfspace (2 faces) / a box ⊇ torus →
the torus itself, all watertight + manifold + commutative through the public `boolean()`) +
`_adversarial` (e* = X — axis ≈ +X — still routes an X-slab; a side (⊥-axis) cut and a torus tilted
past the threshold decline → graceful Err, mesh intact). axia-geo lib 1950→1952, core 391, 0
regression, #[ignore] 0. First-try pass.

**🎉 box ∩ tilted-torus auto-routing complete** (slab → β-3-torus / halfspace → β-2-torus / no-op
containment), all surface-preserving through the existing Boolean tool. With cylinder + cone + torus
(+ sphere via ADR-204's oriented quadric), the box ∩ tilted-primitive family is complete.

### γ-torus-wire (SliceTool single-plane TRIM → browser, demo-verified)

A single oblique plane trimming a torus is the β-2-torus annular HALFSPACE (keep +plane_normal), so
`Mesh::trim_curved_by_plane` (the SliceTool TRIM dispatcher) gains a Torus branch routing straight to
`boolean_torus_oblique_halfspace` — the op validates annularity itself (Err for the pinched / too-
oblique regime), and its ⟂-axis limit covers the perpendicular cut. Because `Scene::trim_curved_
volume_by_plane` → WASM `trimCurvedByPlane` → `SliceTool` are all surface-agnostic (the SliceTool
passes the selected faces and uses the result iff `routed:true`, else its polygon fallback), this one
engine branch activates the torus trim end-to-end; the SliceTool's cylinder-only comments/debug text
were generalised.

**회귀**: axia-geo +1 — `adr205_gamma_torus_wire_trim_routes` (a tilted torus + oblique plane →
β-2-torus halfspace via the dispatcher / a too-oblique plane → handled Err / a box → `None`, the
polygon fallback). axia-geo lib 1952→1953, 0 regression, #[ignore] 0.

**Demo-verified** (real WASM, localhost:3002): `createTorus(0,0,5,4,1.5)` (Path B) +
`trimCurvedByPlane([fid], 0,0,5, 0.15,0,0.99)` (a slightly-oblique annular cut) →
`{routed:true, resultFaces:[2,1]}` (band + cap), `verifyInvariants` valid (0 violations),
`meshManifoldInfo` `non_manifold_edge_count:0 boundary_edge_count:0` (watertight). The engine routing
reaches the browser; the SliceTool TRIM gesture works on a torus.

### cone apex-tip α (DCEL de-risk — sew_cone_tip helper + simulation)

The deferred companion of β-2-cone: when a box face cuts a cone leaving the APEX on the inward side,
the kept solid is the small APEX cone, not the base frustum. Unlike the frustum (two loops → `sew_
curved_band`), the tip is bounded by ONE elliptic loop + a degenerate APEX pole — exactly the Path B
cone pattern (`create_cone_kernel_native`: one self-loop, apex degenerate). New helper
`Mesh::sew_cone_tip(anchor, boundary, side_surface, side_normal, cap_surface, cap_normal, material)`
wires one self-loop edge (carrying the cut ellipse curve) into a `side` face (the Cone surface, apex
degenerate) on one twin + an elliptic `cap` (Plane) on the other.

**§DCEL** (`sim_adr205_cone_apex_tip_dcel`) — builds the tip for an apex-up cone (apex (0,0,6), base
z=0, r=2) cut by an oblique plane at z≈4 (apex on the kept +m side, ellipse via `cone_oblique_ellipse`
+ `nurbs::ellipse`) and proves it sews watertight + manifold (no open half-edges, invariants valid,
non_manifold_edge_count 0, side keeps the Cone surface, cap the elliptic Plane).

**회귀**: axia-geo +1 (`sim_adr205_cone_apex_tip_dcel`). axia-geo lib 1953→1954, 0 regression,
#[ignore] 0. No production op / render / γ yet — next: `boolean_cone_apex_halfspace` (sew_cone_tip +
cone_oblique_ellipse) + an apex-clipped render (the cone-side fan from the apex v=0 to the elliptic
cut) + γ routing for the apex-inside-box case (extend `try_tilted_cone_box_halfspace`).

### cone apex-tip (production op + apex-clipped render + γ routing)

`boolean_cone_apex_halfspace(cone_faces, plane_origin, plane_normal, material)` — the deferred
companion of β-2-cone, keeping the small APEX cone (the +`plane_normal` side must contain the apex).
Mirrors β-2-cone's validation (separation + bounded ellipse + clean-cut guard) but with the APEX on
+m (not the base), and builds via `sew_cone_tip` (one elliptic self-loop, apex degenerate) → `[side,
cap]`. The elliptic cap faces −m.

Render: `tessellate_cone_clipped` gained a SINGLE-oblique-plane branch — for one boundary plane the
cone side is the apex fan, `v ∈ [0 (apex), v_at(plane)]`. A single ⟂ plane (a Path B cone's base
circle) still fails the `oblique` gate → `None`, so the whole-cone default render is unchanged. The
straight cone generator means the 2-row strip is exact; the apex row collapses to a (harmless)
degenerate fan vertex.

γ routing: `try_tilted_cone_box_halfspace` gained the `cut_apex_in` case (the apex on the inward side
+ the WHOLE base outward) → routes the single binding face to `boolean_cone_apex_halfspace`; the
β-2-cone `cut_base_in` case is unchanged, and a base-clip still defers.

**회귀**: axia-geo +2 — `adr205_cone_apex_tip_halfspace` (apex-up cone + oblique plane → cone-side
fan + elliptic cap, watertight + manifold, apex-clipped render every vertex on the kept +m side +
front-facing; adversarial: base-on-+m and ⟂ plane bail, mesh intact) + `adr205_gamma_cone_box_apex_
tip_autoroutes` (a tilted cone + a box keeping the apex → `boolean_cone_apex_halfspace` via the public
`boolean()`, commutative). The existing `adr205_gamma_cone_box_halfspace_autoroutes` case (D) updated
(apex-tip now routes, was deferred). axia-geo lib 1954→1956, core 391, 0 regression, #[ignore] 0.
First-try pass (op + render + γ; one expected test-behaviour update). The cone halfspace is now
complete for BOTH the base frustum and the apex tip.

### cone apex-tip corner α (DCEL de-risk — sew_corner_tip helper + simulation)

The corner companion of the apex-tip halfspace: a cone cut by TWO oblique planes BOTH keeping the
apex → the small apex cone clipped by a corner. It is the MIRROR of cone-corner (the base-keeping
tent): the kept region is `v ∈ [0 (apex), min(v_e1, v_e2)]`, so the binding plane per arc is
`argMIN v_e` (not argMAX) and the bottom is the degenerate APEX pole (no base disk). New helper
`Mesh::sew_corner_tip(top_verts, top_curves, band, band_normal, material)` = `sew_corner_band` minus
the bottom circle/disk (the band's OUTER loop is the multi-edge top; no inner loop).

**§DCEL** (`sim_adr205_cone_apex_tip_corner_dcel`) — replicates the cone-corner geometry (ridge ∩ cone
quadratic → corners, `argMIN` active plane, elliptic arcs) for a cone (apex (0,0,6), base z=0, r=2)
cut by two planes symmetric about the Y–Z plane (apex on +m), then sews `sew_corner_tip` + two partial
caps (sharing the ridge `(c1,c2)` edge) and proves it watertight + manifold (no open half-edges,
invariants valid, non_manifold_edge_count 0, band keeps the Cone surface).

**회귀**: axia-geo +1 (`sim_adr205_cone_apex_tip_corner_dcel`). axia-geo lib 1956→1957, 0 regression,
#[ignore] 0. No production op / render / γ yet — next: `boolean_cone_apex_corner` (mirror
`boolean_cone_corner` with `argMIN` + `sew_corner_tip`) + an apex-clipped corner render (multi-edge
top to the apex fan) + γ routing for two perpendicular apex-keeping faces.

### cone apex-tip corner (production op + apex-clipped render + γ routing)

`boolean_cone_apex_corner(cone_faces, p1_origin, p1_normal, p2_origin, p2_normal, material)` — the
MIRROR of `boolean_cone_corner`: both planes put the APEX on +m + the base on −m, the binding plane
per arc is `argMIN v_e`, and the result is `[corner_band, cap_a, cap_b]` (no base disk — the apex is
the degenerate pole, sewn by `sew_corner_tip`). The ridge ∩ cone quadratic, the corners, and the
elliptic arcs are identical to cone-corner; only the keep-side (apex vs base) + the bottom (pole vs
disk) flip.

Render: `tessellate_cone_corner_clipped` gained an apex-tip branch — a corner band with NO inner loop
(the base disk's absence signals the apex pole) renders `v ∈ [0 (apex), min over the outer multi-edge
planes]` (the corner tent toward the base), vs the base-keeping `v ∈ [max(outer), min(inner base)]`.

γ routing: `try_tilted_cone_box_corner` now tracks each cut's keep-type (`cut_base_in` vs
`cut_apex_in`) and routes exactly two perpendicular cuts of the SAME type → `boolean_cone_corner`
(both base) or `boolean_cone_apex_corner` (both apex); a mixed corner defers.

**회귀**: axia-geo +2 — `adr205_cone_apex_tip_corner` (two apex-keeping planes → corner band + 2 caps,
watertight + manifold, apex-clipped render every vertex on the kept side of BOTH planes + front-facing;
adversarial: parallel planes and a base-keeping plane bail, mesh intact) + `adr205_gamma_cone_box_apex_
tip_corner_autoroutes` (a tilted cone + a box keeping the apex with two perpendicular faces →
`boolean_cone_apex_corner` via the public `boolean()`, commutative). axia-geo lib 1957→1959, core 391,
0 regression, #[ignore] 0. First-try pass. **The cone is now complete for every box ∩ tilted-cone
configuration** — slab / halfspace (base frustum + apex tip) / corner (base tent + apex tip) / no-op.

### torus PINCHED halfspace α (DCEL de-risk — topology characterization + 1-oval feasibility)

A torus oblique cut is a **spiric** (quartic Cassini family) — unlike the conic sections of the
cylinder/cone, its boundary topology *changes with the cut*: annular (2 ovals, plane within ~22° of
the axis), and — when the plane grazes the donut-hole region — **pinched** with either **1 oval**
(off-centre bulge / oblique grazing) or **2 ovals / lemniscate** (steep cut through the centre). The
de-risk `sim_adr205_torus_pinched_geometry` (a) characterizes the regime via `count_bands(m, o)` —
the number of contiguous pierced-u bands — confirming off-centre bulge = 1, oblique = 1, through-centre
steep = 2; and (b) builds a **1-oval bulge DCEL** (flat torus R=4 r=1.5, plane x=3 keep +X): the oval
walks `tangent(−u_t)` → the `v_a = +acos(cval)` branch → `tangent(u_t)` → the `v_b = −acos(cval)`
branch reversed, where the **exact tangent** is `cos u_t = R_keep_ratio = 3/5.5` (the value where
`cos v = 1`, i.e. the minor circle just kisses the plane — using the rounded `acos(0.545)` mis-places
the oval off the plane by ~2.5mm). The cap (`add_face_with_holes`, Plane −X) + patch
(`add_face_with_holes` reversed, Torus) sew watertight (open=0, invariants valid, non-manifold=0).

**회귀**: axia-geo +1 — `sim_adr205_torus_pinched_geometry`. axia-geo lib 1959→1960, core 391, 0
regression, #[ignore] 0. First-try DCEL pass (after the exact-tangent fix). This is **de-risk only**
(feasibility + topology map); the production wiring (β-2-pinched dispatch + γ + tessellation) for the
1-oval MVP — with the 2-oval lemniscate through-centre sub-case deferred — is a future track, since the
ADR-205 core family is already complete and torus-pinched is a narrow donut-hole-grazing edge case.

### N-plane cylinder corner α (DCEL de-risk — lower envelope + pie-slice caps at the box vertex)

`boolean_cylinder_corner` handles exactly **two** oblique upper-bound planes (a 2-arc tent at a box
EDGE). A box VERTEX clips a tilted cylinder with up to **three** perpendicular faces at once, so the
de-risk `sim_adr205_cyl_corner_n_geometry` characterizes the N-plane generalization (axis into the
+X+Y+Z octant, 3 symmetric box-max faces):

- **Lower envelope** — per generator angle u the kept axial bound is `min_i v_plane_i(u)`; the active
  plane is `argmin`. A symmetric box-vertex clip gives **K=3** active arcs (each plane the min over a
  120° u-band), with K corners = ridge(plane k, plane k+1) ∩ cylinder.
- **Crucial topology finding** — the 3-plane corner is **NOT** a simple generalization of the 2-plane
  tent. The tent's 2 caps share their single ridge `c1↔c2` directly (no interior vertex). At a box
  vertex the three ridges meet at the **box corner V** (the 3-plane intersection), which here sits on
  the axis → **inside** the cylinder. Each cap is then a **pie slice** `[corner_{i-1}, mid_i, corner_i,
  V]` — the elliptic arc (2 band-twin edges) + two ridge edges to V — and the three caps share V (and
  the ridge segments pairwise: `corner_j↔V` is shared by two caps). My first attempt (flat triangles
  between consecutive corners) left 3 open half-edges (each ridge used by one cap only); routing every
  ridge to V closes the shell.

The de-risk builds the generalized `2K`-vertex top loop (`sew_corner_band` already takes ≥3 verts) +
K pie-slice caps to V → watertight (open=0, invariants valid, non-manifold=0), proving the N-plane
caller is feasible for the **V-inside** regime.

**회귀**: axia-geo +1 — `sim_adr205_cyl_corner_n_geometry`. axia-geo lib 1960→1961, 0 regression,
#[ignore] 0. De-risk only; the production `boolean_cylinder_corner_n` (K-arc band + box-vertex pie-slice
caps) + γ 3-cutter routing follow in β/γ. The **V-outside** regime (box corner pokes past the cylinder
surface) is a documented deferred sub-case.

### N-plane cylinder corner β-γ (production op + box-vertex γ routing)

`boolean_cylinder_corner_n(cyl_faces, planes, material)` is the production N-plane corner. It computes
the lower envelope of the planes (3600-sample `argmin v_plane_i(u)`), merges contiguous active runs into
K, and:
- **K < 2** → bail (a halfspace, not a corner);
- **K = 2** → delegate to the validated 2-plane `boolean_cylinder_corner` (a box EDGE tent);
- **K = 3** → the box-vertex pie slice: the three ridge corners + the box vertex `V` (the 3-plane
  intersection, required inside the cylinder), a K-arc band (`sew_corner_band`) + bottom disk + 3
  pie-slice caps `[corners[i-1], mid_i, corners[i], V]`;
- **K > 3** → bail (not a box vertex).

Two production findings (both surfaced by the front-facing render check, not the DCEL):
- **Short-arc φ unwrap** — `ellipse_arc` takes the raw `φ1−φ0` span, so a sub-arc straddling the
  ellipse's ±π seam (e.g. `φ0=π, φ1=−π+δ`) would wrap the LONG way through the ellipse top (rendering
  the cap up to z≈6.4 above the apex). Each ≤180° sub-arc's `φ1` is unwrapped to the short way from `φ0`.
- **DECREASING-u winding** — the band top loop is built in decreasing u (the tent's convention) so the
  twin-reuse-constrained pie-slice cap loop `[corners[i-1], mid_i, corners[i], V]` has its natural
  Newell normal pointing OUTWARD (−m), matching the Plane surface hint with no invariant-violating
  `set_normal` override.

**Render needs no new code** — `tessellate_cylinder_corner_clipped` already collects every distinct
oblique plane from the band's OUTER loop and clips `v_hi = min over those planes` (the lower envelope),
so a 3-arc band on 3 planes renders correctly through the existing path; the disk + pie-slice caps
render via the Plane polygon path (DCEL boundary earcut). The front-facing + kept-region checks confirm
this across the test sweep.

**γ routing** — `try_tilted_cylinder_box_corner` now accepts 2 OR 3 cutters on distinct axes: 2 → the
tent (`boolean_cylinder_corner`, unchanged), 3 → `boolean_cylinder_corner_n` (a box vertex). The public
`boolean(tilted-cylinder, axis-box, Intersect)` auto-routes a box-vertex clip to the 5-face pie-slice
corner, commutatively.

**회귀**: axia-geo +4 — `adr205_cylinder_corner_n_pyramid` (3-plane pyramid → 5 faces, watertight +
manifold + front-facing + kept region), `adr205_cylinder_corner_n_delegation_and_bails` (2-active →
tent; ⟂ / non-upper-bound / <2 bail, mesh intact), `adr205_cylinder_corner_n_adversarial` (asymmetric
tilt/azimuth + off-origin + off-axis apex → 5-face valid; apex-outside bails), `adr205_gamma_box_vertex_
autoroutes_to_corner_n` (public `boolean` 3-face box vertex → 5-face corner, commutative). axia-geo lib
1961→1965, core 391, 0 regression, #[ignore] 0. First-try pass after the two render fixes above.

### N-plane cylinder corner — browser build check + CLOSURE

Browser check (rebuilt WASM, real Chromium): the SIMD-verified build loads cleanly — bridge / engine /
viewport / canvas all ready, zero load errors with the new N-plane code compiled in. The corner GEOMETRY
itself is **not** browser-reachable end-to-end, because the exposed `create_cylinder(cx,cy,cz,r,h,segs)`
is Z-axis only and no low-level mesh builder (`add_face_closed_curve` / `extrude_cylinder_kernel_native`)
is on the bridge — a tilted cylinder would need new bridge API (out of scope). The geometry is instead
exhaustively verified in Rust through the ACTUAL render path (`export_buffers`): front-facing +
kept-region + all-faces-render, across the symmetric pyramid + 4 asymmetric configs + the γ box-vertex
auto-route (commutative). So the N-plane cylinder corner track is **complete** (α de-risk → β op → γ
routing → build-verified), with the deeper end-to-end demo blocked only by the missing tilted-cylinder
browser primitive.

### ADR-205 status (2026-06-22)

The `box ∩ tilted-primitive` oblique-Boolean family is **complete for every common configuration**:
- **cylinder** — halfspace / slab / axial D-shaft / corner tent (box edge) / **N-plane corner (box
  vertex)** + the SliceTool single-plane wire trim;
- **cone** — frustum halfspace / slab / corner tent + apex-tip halfspace + apex-tip corner;
- **torus** — annular halfspace / straddling slab + wire trim;
- **sphere** — the ADR-204 oriented-quadric halfspace / slab / corner family.

Documented **deferred** sub-cases (each a narrow edge case, triggered on demand): the torus PINCHED
1-oval production (α-de-risked) + 2-oval/lemniscate through-centre + one-sided/thick slab; the
cylinder-corner **V-outside** regime; and N-plane (box-vertex) corners for the cone/torus. Per the
user's 2026-06-22 decision the ADR-205 remainder is wrapped up here; the NURBS-foundation Tier 1 tools
(ADR-168) follow in a separate session.
