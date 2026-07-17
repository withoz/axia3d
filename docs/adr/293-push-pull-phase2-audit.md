# ADR-293 — Push/Pull Phase 2 Audit (α): Phase 2 already ships; the roadmap was stale

- **Status**: Proposed
- **Date**: 2026-07-15 (**revised the same day — the first revision was wrong, see §7**)
- **Amends**: ADR-190 §4 Phase 2 (scope correction, measured)
- **Cross-link**: ADR-190 (roadmap, LOCKED #78) · ADR-252 (source-face carve routing, "옵션 A
  스마트 자동 전환") · ADR-264 (embedded boss fuse, LOCKED #93) · ADR-196 (MoveOnly + inward
  clamp, LOCKED #82) · ADR-267 (through-vs-blind for circles) · ADR-269 (through robustness /
  cross-drill reject) · 메타-원칙 #6 (measure-first) · #16 (heuristic-automation boundary)

---

## 1. Verdict

**ADR-190 §4 Phase 2 is already implemented.** P2.1 (자동 carve/recess) and P2.2 (관통 push →
구멍) both work today, through `carve_pocket_from_source_face` (ADR-252). There is no β to build.
The roadmap text is stale — same doc-lag as ADR-259 (β already shipped) and ADR-264 (code already
there).

**One genuine gap remains, and it is not what the roadmap describes:** a **whole-face** inward push
past the solid's thickness is silently clamped — the solid collapses to a sliver with no signal.

## 2. Method

Measured through the **production bridge** (`window.__axia.get('bridge')` → WASM → Scene), **one
scenario per fresh page reload**. `wallThicknessFromSourceFace` is exported to WASM (returns `-1`
for `None`), so the routing gate itself is directly observable rather than inferred.

## 3. The routing gate (measured, not read)

`exec_create_solid` (scene.rs ~8305):

```rust
if !self.suppress_source_carve {
    if let CreateSolidMode::Extrude { distance } = mode {
        if distance < 0.0 && self.mesh.wall_thickness_from_source_face(face_id).is_some() {
            let r = self.carve_pocket_from_source_face(face_id, -distance);  // auto pocket ↔ through
            if !matches!(r, CommandResult::Error(_)) { return r; }
        }
    }
}
```

`wall_thickness_from_source_face` (carve.rs:1601) is `None` when: no larger coplanar container
face · outline < 3 pts · no opposite wall on the inward ray.

Measured gate values:

| profile | gate | routes? |
|---|---|---|
| box top, **whole face** | **−1 (None)** | ✗ → plain extrude + MoveOnly clamp |
| **imprinted rect** on the top | **1000** (= thickness) | ✓ auto pocket↔through |
| **imprinted circle** on the top | **1000** | ✓ (ADR-267 follow-up works) |

**This is exactly the Q1 policy** (사용자 결재 2026-07-15: "route only when the profile is an
imprinted inner loop — an unambiguous 'cut this shape through' intent; keep the clamp for a
whole-face push"). It is already the shipped behaviour; the 결재 needs no code.

## 4. Measured behaviour (clean scenes; box 2000×1000×1000, z∈[0,1000])

| scenario | result | verdict |
|---|---|---|
| `drawRectAsShape` 800×600 on the top | 7 faces, closedSolid ✓ | rect **imprints** (ADR-264 fuse) |
| push rect **−300** (shallow) | 11 faces, closedSolid ✓ | **pocket** ✓ |
| push rect **−1500** (> thickness) | **10 faces**, closedSolid ✓, nm=0, **+Z-normal faces = 1**, volume 2e9 → **1.68e9** | **THROUGH-HOLE** ✓ (P2.2 works) |
| push **whole top −1500** | ret=true, 6 faces, closedSolid ✓, top z 1000 → **0.001**, **volume 2e9 → 2000** | **silent clamp** ← the real gap |
| push A's wall **+1200** into box B | ret=false, wall unmoved, mesh byte-unchanged | **fail-closed** (intended, LOCKED #82) |
| control: A alone, wall **+1200** | ret=true, wall 500 → 1700 | attributes the reject to **penetration**, not distance |

The through-hole is proven structurally, not by face count alone: **exactly one +Z-normal face**
(the top ring) — a floor-deep pocket would have two (top ring **+ pocket floor**) — plus a −Z
bottom ring and 4 hole walls = 10 faces, watertight.

## 5. The one real gap

**Whole-face inward push past the thickness is silently clamped.** Gate = `-1` → no routing → the
ADR-196 clamp (`move_only_max_inward`) pins the face `MIN_SOLID_THICKNESS` above the far side.
Measured: a 2000×1000×1000 box (volume 2e9) pushed −1500 becomes **volume 2000** — a sliver — and
`createSolidExtrude` returns **`true`**. The result is watertight and valid, so no gate objects;
the user simply gets a degenerate solid with **no message**.

Keeping the clamp is correct (Q1, 메타-원칙 #16 — a whole-face push is ambiguous, and
auto-cutting would be exactly the heuristic-automation trap). **The defect is the silence, not the
clamp.** β scope, if taken: report the clamp (Toast: "두께에서 멈춤 — 관통하려면 면에 형상을
그린 뒤 미세요"), and/or refuse a push that collapses the solid below a usable thickness.

## 6. Corrected Phase 2 scope

| roadmap item | measured | action |
|---|---|---|
| P2.1 자동 carve/recess | **SHIPPED** | mark ADR-190 §4 P2.1 closed |
| P2.2 관통 push → 구멍 | **SHIPPED** (rect + circle) | mark closed |
| push into another solid | fail-closed (Q2 결재 = 유지) | no action |
| — | **whole-face push clamps silently** | the only β candidate (Q3 결재 = P2.2만 → out of scope for now) |

ADR-190's "+40~50 회귀, 위험 중상" assumed new geometry; the geometry was already there.

## 7. Honest record — this audit was wrong twice before it was right

Written down so the next reader does not repeat it (메타-원칙 #6):

1. **Polluted scene.** The first run accumulated solids in one scene and mixed
   `meshManifoldInfo()` (whole-mesh) with `verifyOutwardNormals()`. It reported "the rect never
   imprints (`closed=false, bnd=4`) and the push is rejected (`ret=false`)" — **the exact
   opposite** of the truth. Isolation (one scenario per reload) flipped it.
2. **Reading a centroid without checking the topology.** The −1500 push put `facesCentroid` of the
   profile at **z=0**, which I declared a "silent clamp at the bottom" and wrote up as P2.2's
   defect. It was a **through-hole** — the profile had become the bottom ring. Counting +Z-normal
   faces (1, not 2) settled it in one probe. The first revision of this ADR asserted the wrong
   conclusion and was committed (`6e67fa2`) before this correction.

**L-293-5 (rewritten): measure through the bridge, one fresh page per scenario, and confirm a
topological claim topologically (normals / loops), never by a single centroid.**

## 8. Lock-ins

- **L-293-1** P2.1 and **P2.2 are both CLOSED** — do not re-implement. ADR-190 §4 Phase 2 is stale.
- **L-293-2** The routing gate is `wall_thickness_from_source_face(face).is_some()` and already
  encodes the Q1 policy (imprinted inner loop → route; whole face → clamp). Measured: −1 / 1000 /
  1000.
- **L-293-3** Cross-solid push is fail-closed (measured, controls attribute it to penetration).
  Q2 결재 = keep.
- **L-293-4** The only measured defect is the **silent** whole-face clamp (2e9 → 2000 volume,
  returns `true`). Q3 결재 = P2.2만 → out of scope here; recorded for a future ADR.
  → ✅ **CLOSED 2026-07-15** under ADR-190 Phase 3. The clamp stays (§5 was right that it
  should); only the silence went. New read-only export `moveOnlyMaxInward(face)` (`-1` =
  unclamped) lets the Push/Pull tool read the limit **before** committing — afterwards the
  face's thickness *is* the clamped value and the over-push is unmeasurable — and Toast
  "두께 N mm 에서 멈췄습니다 — 관통하려면 면에 형상을 그린 뒤 미세요". Regressions: vitest
  +5 (mutation-verified) + E2E +3 (real DOM Toast). See ADR-190 §4 Phase 3.
- **L-293-5** See §7 — bridge + fresh page + topological confirmation.
- **L-293-6** α is measurement only; no code. No β is needed for Phase 2.
