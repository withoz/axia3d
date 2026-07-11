# ADR-264 — Embedded Boss Extrude: Fuse instead of Cleave/Preserve

**Status**: Accepted (β~ζ 구현 완료 — §D Acceptance Log, 2026-07-11 measure-first closure)
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

## D. Acceptance Log (2026-07-11, measure-first closure)

**사용자 "진행" (2026-07-11)** → 로드맵 #2 후속으로 ADR-264 진입. measure-first
로 doc-lag 확정: Status 는 "Proposed (α — spec only)" 였으나 실측 = **β~δ 이미
shipped** (fuse gate + fuse impl `create_solid.rs:240~797` + 8 회귀 green, gap
0). ADR-259 와 동일 doc-lag 패턴.

- **β~δ (prior, doc-lag)** — `create_solid` Extrude arm 의 fuse gate (`fuse_embedded`:
  siblings 존재 + Plane + AllLinear + profile 의 connected component 가 closed
  solid) → cleave/preserve 대신 remove profile + shared-rim 측벽 (twin re-link).
  `face_connected_components` + `face_set_manifold_info` discriminator. AllCircular
  boss 는 closed-curve extrude 경로가 clean 처리 (`adr264_circle_boss_still_clean`).
  회귀 8 green: axia-core `adr264_embedded_rect_boss_extrude_is_closed_manifold` /
  `adr264_embedded_rect_pocket_is_closed_manifold` / `adr264_free_sheet_extrude_
  still_preserves_profile` (siblings=0 guard) / `adr264_flat_arrangement_component_
  is_open` (ADR-101 §B-3b cleave 유지 guard) / `adr264_circle_boss_still_clean` /
  `adr264_geometric_detector_catches_cleave_crack` / `adr264_extrude_on_stale_face_
  id_errors_not_panics`; axia-geo `adr264_geometric_detector_clean_on_box`.
- **ζ (2026-07-11) — real Chromium 시연 게이트** — `web/e2e/adr-264-embedded-boss.
  spec.ts` ×2: box top 에 embedded rect (drawRectAsShape) → createSolidExtrude
  (up dist>0 boss / down dist<0 pocket) → `verifyOutwardNormals().isClosedSolid`
  true + verifyInvariants valid 0 viol. 사용자 tool-path (createSolidExtrude →
  SolidCreated) 로 fuse 도달 확인. **코드 변경 0** (E2E + docs only — β 는 이미
  production dist).
- **full stack 확인**: embedded boss 는 is_move_only 아님(flat sub-face) → Scene
  MoveOnly dispatch skip → mesh `create_solid` Extrude arm fuse gate. `SolidCreated`
  → 기존 WASM `create_solid_extrude` → true (arm 수정 불필요, ADR-259 draft 와 달리).
- workspace 3016 passed / 0 failed / 1 ignored (변경 0), E2E +2 (2/2). catalog ✓.

### E. Lessons
- **L1** measure-first 가 doc-lag 재노출 (ADR-259 에 이어 2연속) — ADR-264~274
  systemic doc-lag ([[project-engine-state-and-doc-lag]]). 판정 전 코드/테스트 대조.
- **L2** gap 없는 doc-lag = 코드 신설 금지 — 이미 shipped+tested β 를 Status/docs
  만 정합. 시연 게이트(E2E)는 additive 로 tool-path 도달 확인 (engine 테스트가
  이미 authority `is_closed_solid` — E2E 는 user-path 확인용).
- **L3** SolidCreated arm 은 기존 WASM 이 이미 true (ADR-259 draft 의 PushPullDone
  arm 수정과 대비 — fuse 는 create_solid 경로라 arm 무변경).
