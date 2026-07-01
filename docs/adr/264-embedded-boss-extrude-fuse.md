# ADR-264 — Embedded Boss Extrude: Fuse instead of Cleave/Preserve

**Status**: Proposed (α — spec only)
**Date**: 2026-06-27
**Amends**: ADR-102 (γ Detach-on-Arrangement cleave), ADR-196 (MoveOnly dispatch), ADR-087 K-ε (kernel-native command suite)
**Author**: WYKO (engine), diagnosed + simulated this session
**Meta-principles**: #4 (SSOT), #6 (preventive), #9 (no regression), #10 (LOCKED change = new ADR), #14 (face from closed boundary), #15 (headless ≡ tool path)

---

## 1. Problem (engine-grounded, this session)

Extruding an **embedded sub-face** (a rectangle drawn on a box top → "boss")
produces a **non-manifold / cracked** result, NOT a fused solid.

Simulated on adr-186 (kind-chatterjee), `create_box` + `DrawRectAsShape` on top
+ `CreateSolid Extrude`:

```
after-extrude: boundary_edge_count=4, geometric-coincident-3-face=4, closed_solid=FALSE
```

Four bottom-rim locations have **3 faces meeting** (ring + preserved profile +
side wall). The engine's own overlay detector `collect_non_manifold_edges`
(radial, per-EdgeId) reports **0** because the ADR-102 cleave split each rim
into **2 coincident EdgeIds (a crack)** — so the radial walk under-counts
(1 + 2), never ≥3. The orange overlay therefore never fired even though the
geometry is broken.

### Root cause
`create_solid.rs::extrude_planar_box` unconditionally **preserves the profile
face** (`all_solid_faces.push(profile_face)`) and reuses boundary verts. For an
embedded profile the rim edge is already shared by `profile + ring`, so the new
side wall cannot twin with the ring (no free half-edge) → 3-share. The ADR-102
cleave "fixes" the 3-share by **duplicating the profile boundary** (separating
it from the ring) — but that opens the ring (4 boundary edges) and leaves the
boss as a floating solid in a crack. The cleave traded *non-manifold* for
*cracked-open*; neither is a fused solid.

Circle bosses are accidentally clean only because a kernel-native circle is a
**self-loop** (`boundary_verts==1`) → `siblings==0` (no cleave) → a different
cleanup-aware path (`extrude_closed_curve_face_via_tessellation`).

## 2. Decision (D1/D2/D3, 2026-06-27 user sign-off)

**An embedded sub-face that sits on a CLOSED SOLID extrudes by FUSING into that
solid: remove the profile face, build the side walls on the existing shared rim
edges (re-twinning with the surrounding ring), no cleave.**

- **D1 — scope of fuse**: fuse **only when the profile's connected component is
  a closed 2-manifold solid** (boss-on-solid). A flat coplanar arrangement
  (open component, e.g. ADR-101 §B-3b auto-intersect sub-faces) keeps the
  ADR-102 cleave. Discriminator = `face_set_manifold_info(component).is_closed_solid`.
- **D2 — direction**: both `dist>0` (outward boss) and `dist<0` (inward
  pocket). Side-wall winding flips by sign (mirrors current `extrude_planar_box`).
- **D3 — detection hardening**: add **coincident-position** non-manifold
  counting (catches cracks the radial check misses) + a **solid-scope
  post-op assert** (extrude/push-pull result must be every-edge-exactly-2-faces
  + boundary 0). `collect_non_manifold_edges` (radial, ADR-021 P7 form overlay)
  is preserved unchanged.

### Simulation evidence (prototype, all PASS)
| case | boundary | coincident-3-face | verify | closed |
|---|---|---|---|---|
| embedded boss dist>0 (remove-profile+fuse) | 0 | 0 | valid | true |
| embedded pocket dist<0 (remove-profile+cap+fuse) | 0 | 0 | valid | true |
| discriminator boss-on-box | — | — | — | is_closed_solid=true |
| discriminator flat-arrangement | — | — | — | is_closed_solid=false |
| free sheet (siblings=0) | preserve-profile current path unchanged |
| circle boss (self-loop path) | already clean, unchanged |

## 3. Lock-ins

- **L1** — Fuse ⇔ profile's connected component `is_closed_solid` AND
  `siblings` non-empty. Else: siblings empty → preserve (free); siblings
  non-empty + open → cleave (ADR-102, unchanged).
- **L2** — Fuse path: `remove_face(profile)` (frees rim HEs) → `add_face(top)`
  → side quads reuse freed rim HEs (`find_halfedge` Pass 1) → ring↔wall twin.
  No bottom cap (profile is interior → removed, not flipped). ADR-183 flip
  applies only to the free/preserve path.
- **L3** — `CreateSolidResult.profile_removed: bool`. `exec_create_solid`
  unregisters the removed profile from Shape/Xia ownership; registers top +
  sides only.
- **L4** — ADR-102 cleave preserved verbatim for the open-component case.
  ADR-196 MoveOnly dispatch (full-top) unchanged — orthogonal.
- **L5** — Detection: new `collect_non_manifold_edges_geometric` (coincident
  position) + `assert_closed_solid_after` guard on extrude/push-pull. Radial
  `collect_non_manifold_edges` (form-layer overlay) untouched (LOCKED #1).
- **L6** — Headless ≡ tool path (#15): `create_solid_extrude` WASM entry and
  the live Push/Pull path (`begin/commit_live_extrude`) inherit the fix (same
  `exec_create_solid` dispatch). No new entry point.

## 4. Path Z atomic plan

- **α** (this commit) — spec.
- **β** — engine: discriminator + fuse path in `create_solid.rs`;
  `CreateSolidResult.profile_removed`. axia-geo regression.
- **γ** — scene: `exec_create_solid` ownership for removed profile. axia-core.
- **δ** — detection hardening (D3): geometric non-manifold count + solid assert.
- **ε** — vitest: confirm same WASM/bridge entry, no surface drift.
- **ζ** — real Chromium: boss → orange overlay gone + closed solid.

### Regression (절대 #[ignore] 금지)
- `embedded_rect_boss_extrude_is_closed_manifold` (boundary 0, coincident-3 0)
- `embedded_rect_pocket_dist_neg_is_closed_manifold`
- `free_sheet_extrude_still_preserves_profile` (siblings=0 guard)
- `flat_arrangement_extrude_still_cleaves` (ADR-102 guard)
- `full_top_push_still_moveonly` (ADR-196 guard)
- `circle_boss_still_clean` (self-loop path guard)

## 5. Cross-link
ADR-102 (cleave — amended scope), ADR-196 (MoveOnly — orthogonal), ADR-087 K-ε
(kernel-native commands), ADR-021 P7 / LOCKED #1 (form-layer ≥3 overlay
preserved), ADR-183 (outward base cap — free path only), ADR-101 §B-3b (flat
arrangement = cleave retained), 메타-원칙 #14/#15.
