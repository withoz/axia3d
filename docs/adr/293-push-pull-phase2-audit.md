# ADR-293 — Push/Pull Phase 2 Audit (α): what "Hole-through / Boolean" actually needs

- **Status**: Proposed
- **Date**: 2026-07-15
- **Supersedes / amends**: ADR-190 §4 Phase 2 (scope correction, measured)
- **Cross-link**: ADR-190 (roadmap, LOCKED #78) · ADR-191/192 (Phase 1, LOCKED #79/#80) ·
  ADR-196 (MoveOnly dispatch + inward clamp, LOCKED #82) · ADR-264 (embedded boss fuse,
  LOCKED #93) · ADR-194/249/252 (punch/drill/carve assets) · ADR-277 (general mesh CSG) ·
  메타-원칙 #6 (Preventive/measure-first)

---

## 1. Why this ADR exists

ADR-190 §4 planned Phase 2 as:

> **Phase 2 — Hole-through / Boolean (signature CAD, 예정)**
> - P2.1 면을 솔리드 안으로 push → 자동 subtract (carve/recess)
> - P2.2 관통 push → 구멍 (punchHole + ADR-064/066 NURBS Boolean 연동)
> - 예상 회귀 +40~50. 위험 중상. **최고 체감 가치**

That text is from 2026-06-09. **Measurement (2026-07-15) shows P2.1 already ships, and P2.2's
real behaviour is not what the roadmap assumes.** Writing β against the roadmap's wording would
have built something that already exists and missed the thing that doesn't. This is the same
doc-lag pattern that ADR-259 (β already shipped) and ADR-264 (code already there) hit.

## 2. Method

Measured through the **production path** (`window.__axia.get('bridge')` → WASM → Scene), not by
reading source: the bridge is what the UI actually calls. **One scenario per fresh page reload** —
an early polluted run (accumulated solids in one scene, plus `meshManifoldInfo()` whole-mesh
`isClosedSolid` vs `verifyOutwardNormals()`) reported "rect never imprints, push rejected" and
**inverted the conclusion**. Isolation was decisive; the polluted reading is recorded here so it
isn't re-derived as a finding.

Authoritative check: `verifyOutwardNormals().isClosedSolid` + `verifyInvariants()` +
`meshManifoldInfo()` (nm / boundary).

## 3. Measured behaviour (clean scenes)

### A — embedded rect on a box top (box 2000×1000×1000, z∈[0,1000]; rect 800×600 @ z=1000)

| step | faces | closedSolid | valid | verdict |
|---|---|---|---|---|
| `create_box` | 6 | ✓ | ✓ | baseline |
| `drawRectAsShape` on the top | **7** | **✓** | ✓ | **rect IS imprinted** — the top splits into ring + rect, solid stays closed |
| `createSolidExtrude(rect, -300)` (shallow, 300 < 1000) | **11** | **✓** | ✓ | **pocket works** |
| `createSolidExtrude(rect, -1500)` (through, 1500 > 1000) | 10 | ✓ | ✓ | **no hole** — see below |

**⇒ P2.1 (자동 carve/recess) ALREADY WORKS.** ADR-264's fuse imprints the profile; an inward
push carves a pocket. Roadmap says "예정"; reality says shipped.

**⇒ P2.2's actual behaviour is a SILENT CLAMP, not a rejection or a corruption.** The −1500 push
returned `true` and left a *watertight* solid — but `facesCentroid` of the pushed face reads
**z = 0**, i.e. it stopped exactly at the box's own bottom. ADR-196's inward clamp
(`move_only_max_inward`, `dist.max(-(thickness - MIN_SOLID_THICKNESS))`) silently converted a
through-cut into a floor-deep pocket. The user asked for a hole and got a floor, with no signal.

### B — push a wall into ANOTHER solid (A x∈[-500,500], B x∈[1500,2500])

| scenario | dist | result |
|---|---|---|
| A alone, +X wall | +200 | `ret=true`, wall 500 → **700** ✓ |
| A alone, +X wall | **+1200** | `ret=true`, wall 500 → **1700** ✓ |
| **A with B in the path**, +X wall | **+1200** | **`ret=false`, wall unmoved, mesh byte-unchanged** |

The controls attribute this exactly: **the same distance succeeds without B and fails with B**, so
the refusal is caused by *the penetration*, not the distance. Behaviour is **fail-closed** (no
corruption, no silent overlap) — the conservative outcome LOCKED #82 intended when it deferred
"다른-솔리드 carve" to Phase 2.

## 4. Corrected Phase 2 scope

| roadmap item | measured reality | remaining work |
|---|---|---|
| **P2.1** 자동 carve/recess | **DONE** (ADR-264 fuse + inward push) | none — mark closed |
| **P2.2** 관통 push → 구멍 | inward clamp silently makes a floor-deep pocket | **route through-depth pushes to a through-cut instead of clamping** |
| (implied) push into another solid | fail-closed reject | **route to subtract (carve B)** |

**The remaining work is dispatch, not new geometry.** The primitives already exist and are
regression-covered: `drill_rect_through_hole` (ADR-249), `carve_through_from_source_face`
(ADR-252), `punch_rect_hole` (ADR-194), curved `carve_curved_through` (ADR-287/288), and
general mesh CSG `boolean_solid` v2 — which ADR-277 proved watertight for arbitrary-angle,
non-box polyhedra. This is the same Pattern-12 ("mechanism already exists") that ADR-172 and
ADR-259/264 hit.

## 5. Open questions for 결재 (β scope)

- **Q1 — through-cut trigger.** Should `|dist| ≥ thickness` auto-route to a through-cut, or should
  the clamp stay the default with an explicit opt-in? Auto-routing is what a SketchUp/Fusion user
  expects; but LOCKED #82 chose the clamp deliberately for manifold safety, and 메타-원칙 #16
  warns that heuristic automation is the source of cascading side-effects. **Recommend: auto-route
  when the profile is an imprinted inner loop (an unambiguous "cut this shape through" intent),
  keep the clamp for a whole-face MoveOnly push (ambiguous).**
- **Q2 — push into another solid.** Auto-subtract (carve B), or keep the fail-closed reject with a
  Toast telling the user to use Boolean? Auto-subtract silently mutates a solid the user did not
  select — the same intent problem as Q1.
- **Q3 — scope.** P2.2 only, or P2.2 + cross-solid carve together?

## 6. Non-goals

Curved-surface through (already done, ADR-287/288) · Boolean UI (exists) · P2.1 (shipped).

## 7. Lock-ins (α — measurement only, no code)

- **L-293-1** P2.1 is CLOSED — do not re-implement. ADR-190 §4 Phase 2's P2.1 line is stale.
- **L-293-2** P2.2's real defect is the **silent clamp** (measured `z=0` landing on a −1500 push),
  not a rejection/corruption. Any β must fix the *silence*, at minimum.
- **L-293-3** Cross-solid push is **fail-closed today** (measured: `ret=false`, byte-unchanged) —
  it is safe, so β may take it or leave it; it is not a corruption risk.
- **L-293-4** β is a **dispatch/routing** job over existing, regression-covered primitives — budget
  and risk are far below ADR-190's "+40~50, 위험 중상" estimate, which assumed new geometry.
- **L-293-5** Measure through the bridge on a **fresh page per scenario**. A polluted scene
  inverted this audit's first conclusion.
- **L-293-6** No code in α (메타-원칙 #6). β requires 사용자 결재 on Q1–Q3.
