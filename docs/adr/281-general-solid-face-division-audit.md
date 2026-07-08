# ADR-281 — General Solid Face Division: audit + Route B (ADR-277 imprint) unification (α)

- **Status**: Proposed (measure-first audit + design; β needs 결재)

## Context

ADR-280 Level 1 made a crossing shape on a planar solid top a safe *decline*.
The user then clarified the true Level-2 requirement is broader — a GENERAL
capability:

1. **Through-penetration** — a drawn shape / division can create a face that
   penetrates THROUGH the solid (a dividing face, like slicing a solid in two).
2. **Solid-face ↔ solid-face split** — when two solids intersect, their faces
   split each other (mutual imprint), confirmed by the user.
3. **Curved surfaces included** — all of the above on curved faces
   (sphere / cylinder / cone / torus), not just planar.

Measure-first audit (production path, real engine, 2026-07-07/08) before design.

## Audit — current state (measured + cross-referenced)

| Dim | Scenario | State | Evidence |
|---|---|---|---|
| **(a) planar solid face** | **LINE across a box top → surface splits into 2** | ✅ **WORKS** | measured: top 1→2, 7 faces, closed, nm=0 (ADR-172/173) |
| (a) planar solid face | crossing closed shape (rect × circle) → split | ❌ **GAP** | Level 1 declines (ADR-280); measured open pre-fix |
| **(b) curved solid face** | circle sketch on sphere / cylinder / cone | ✅ **WORKS** | ADR-202 / ADR-257 / ADR-263 (LOCKED #83 / #87) |
| (b) curved solid face | LINE / crossing shape on a curved face → split | ❌ **GAP** | ADR-202 S3/S6 deferred; ADR-174 only circle-edge |
| **(c) through / solid division** | draw a line across the top → DIVIDE the solid | ⚠ **PARTIAL/GAP** | the line splits the top SURFACE only; the solid stays ONE component (7 faces closed) — no penetrating dividing face. Through-cut exists via drill/carve (ADR-249/252/269) + push-through (single circle 42 faces) + boolean-subtract a cutting solid |
| **(d) solid ↔ solid mutual split** | two solids intersect → faces split each other | ✅ machinery EXISTS | `imprint_faces` (ADR-277 v2), `intersect_faces_with_model`, boolean (ADR-277 general CSG complete this session). Pure "split-only, keep both" exposure = TBD |

**Summary of gaps (the Level-2 work):**
- G1 — planar solid face split by a crossing CLOSED shape (rect × circle …).
- G2 — curved solid face split by a line / crossing shape.
- G3 — "draw a division → penetrate/divide the solid" (dividing face through the
  interior), as a first-class draw outcome (vs today's boolean/drill/push).
- (d) mutual split machinery exists; may only need a pure-split entry + curved.

## Design — Route B (ADR-277 shared-vertex imprint) unifies all gaps

The ADR-277 boolean-v2 **shared-vertex imprint** is the single unifying kernel:
it subdivides arbitrary faces along an intersection curve with ONE shared vertex
set (watertight by construction, no weld, boundary preserved), classifies
sub-faces, and is fail-closed. It already delivered general polyhedral CSG
(transversal + coplanar + MIXED + rotation + non-box) this session, and ADR-278 β
showed curved (Path B) operands are absorbed by polygonalize-then-imprint.

Route B for Level 2 = **route the gap cases through the imprint instead of the
line-by-line crossing-split + coplanar re-derive** (which fragments the solid-top
boundary before the re-derive — the ADR-280 root cause):

- **G1 (planar crossing):** imprint the crossing shape onto the solid-top face →
  sub-faces share the shape boundary + preserve the outer (wall) boundary.
- **G2 (curved):** polygonalize the curved face (ADR-278 β pattern) or use the
  analytic curved-slice ops (`boolean_{sphere,cylinder,cone,torus}_slice`) →
  imprint → watertight split; re-attach the AnalyticSurface where possible
  (ADR-089 A-χ surface inheritance).
- **G3 (through/divide):** the intersection curve that penetrates the solid
  becomes the dividing boundary; the imprint splits every crossed face, yielding
  the penetrating face set (a real solid division). Reuses drill/carve
  (ADR-249/252/269) where a tool profile is given.
- **(d) mutual split:** expose the imprint stage (without boolean classify) as a
  "split both solids at their intersection, keep both" op.

**Why Route B over Route A** (thread boundary-protection through the crossing-
split): Route A is fragile + invasive (documented wall-dangling panic risk);
Route B reuses the proven watertight imprint. A stays the fallback only.

## Phased β plan (each its own atomic pass + 결재 + full-regression gate; Level 1 fail-closed is the backstop throughout)

- **β-1 (G1)** planar solid-top crossing → imprint dispatch. Smallest, unblocks
  the user's original "rect on box+circle" case with a real split.
- **β-2 ((d))** pure solid↔solid mutual-split entry (imprint without classify).
- **β-3 (G3)** through/divide as a draw outcome (penetrating dividing face).
- **β-4 (G2)** curved solid face split (polygonalize / analytic-slice + imprint +
  surface inheritance).

## Lock-ins (α)

- **L-281-1** Measure-first: the audit table is the ground truth; docs lag, so
  each β step re-measures its dimension before implementing.
- **L-281-2** Route B (ADR-277 imprint) is the unifying kernel; Route A is
  fallback only.
- **L-281-3** Every β step is fail-closed (Level 1 guard backstop) + full
  regression gate + browser demo (ADR-087 K-ζ). Never ship a broken solid.
- **L-281-4** Curved faces keep their AnalyticSurface where possible (ADR-089
  A-χ inheritance); polygonalize only where the imprint requires it (ADR-278 β).
- **L-281-5** 절대 #[ignore] 금지.

## Cross-link

- ADR-280 (solid-top re-tile — Level 1 guard live; this ADR is its generalized
  scope + Route B design).
- ADR-277 (general mesh CSG shared-vertex imprint — the unifying kernel).
- ADR-278 β (polygonalize-then-imprint for curved operands).
- ADR-202 / ADR-257 / ADR-263 (curved sketching — circle on sphere/cyl/cone).
- ADR-172 / ADR-173 (line crossing-split on a solid face — the (a)-line WORKS).
- ADR-174 (curve-edge crossing-split — circle-edge only).
- ADR-249 / ADR-252 / ADR-269 (drill / carve pocket-through — existing through-cut).
- ADR-089 A-χ (split surface inheritance — curved metadata preserved).
- 메타-원칙 #4 (SSOT — one imprint kernel) / #6 (Preventive — measure-first) /
  #9 (회귀 없음) / #14 (면은 닫힌 경계로부터).
- LOCKED #44 (Complete Meaning per Merge — each β step atomic).
