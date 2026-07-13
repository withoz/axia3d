# ADR-292 — OSNAP Re-introduction (plane-consistent object snap)

- **Status**: Accepted
- **Date**: 2026-07-13
- **Category**: Input / UX / Snap
- **Refines**: ADR-137 (Guidance-only Snap, Draft) — supersedes its "commit never
  moves" stance with a measured plane-consistent model where the commit DOES snap
  but only in-plane
- **Amends**: LOCKED #63 (2026-05-18 snap disable, PR #101)
- **Related**: ADR-047 P32 (snap-chain self-touch exclusion), ADR-166/188 (plane
  lock), ADR-167/168 (EPS_PLANE / drift snap), ADR-170 (normalizeDrawInput SSOT),
  ADR-175/178 (face-aware drawing plane), ADR-026 P12 (cardinal SSOT), ADR-037 P22
  (Pick→Promote), ADR-146 (findSnap telemetry), LOCKED #5 (spatial-hash dedup)

## 1. Context

Object snapping (OSNAP) was **fully disabled** on 2026-05-18 (LOCKED #63, PR #101):
`getSnappedPoint()` was gutted to `return rawGroundPoint`, and mousedown/mousemove
were re-routed to call `get3DPoint()` directly, bypassing snap entirely. The
reason: the old auto-magnet returned a snap candidate's **raw 3D world position —
including its own out-of-plane z — as the committed point**. When a snapped
box-vertex at z=50 replaced the cardinal-forced z=0 (or the active draw-plane
offset), the RECT corners left the plane → **star-shaped self-intersecting solid**.
Secondarily, the snap path issued synchronous WASM calls on the mousemove hot path
→ "recursive use of an object" Rust borrow crashes.

The user asked to re-introduce OSNAP with an explicit constraint: **"when
implementing OSNAP, prioritize consistency with other features at click time."**

A measure-first audit (4-agent) of the full click-time pipeline established that
the entire snap engine (`SnapManager.findSnap` with 15 candidate types, spatial
hash, exclusion, `SnapVisual`, `OsnapPanel`, Alt+X filters) is **intact and still
live-fed** (`updateFromMesh` runs every `syncMesh` — `findAlignedDistance` for
Push/Pull kept it warm). Only the *consumption* was short-circuited. The real work
is not rebuilding snap — it is reconciling snap output with the NEWER click-time
pipeline (cardinal force / face-plane / plane-lock) that did not exist when the old
snap was written.

## 2. Decision

**Re-introduce OSNAP with a plane-consistent commit model** (user-approved:
"클릭 시 자동 스냅 + 안전 재투영" + "보수적 face-creation preset").

The single governing rule — the draw plane is resolved FIRST, and the committed
point ALWAYS lies exactly on it:

```
1. resolve the draw plane (getDrawPlane: lock > sketch > view > sticky > face)
2. find a snap candidate on the raw ray / screen-space (TS-only over the cached
   DCEL geometry — NO WASM in the hot path)
3. PROJECT the candidate back onto the active plane (drop the normal component)
4. re-apply the cardinal-axis / face-plane force as the TERMINAL transform
5. respect plane-lock, ADR-047 chain exclusion, engine-dedup tolerance floor
```

Snap can only move the **in-plane** position; it can **never** supply the
plane-normal coordinate. A snapped off-plane vertex is committed as its **coplanar
shadow**, never its raw z — which is exactly the off-plane commit that produced the
2026-05-18 defect. `DrawCircleTool.getPointOnDrawPlane` already implemented this
template (snap → projectOntoPlane → re-force); ADR-292 generalizes it to the SSOT.

**Architectural home**: `ToolManager.applyObjectSnap(raw, plane, e)` — a private
helper called inside `get3DPoint()` (the committed-point SSOT for ~15 tools) on all
three branches (sketch / face / ground), with the cardinal force kept terminal;
and exposed as `ToolContext.snapToPlane` so `DrawRectTool.projectClickToCardinalPlane`
(the one tool that re-derives its own point) gets the identical treatment.

**Initial mode set**: `applyFaceCreationPreset()` — endpoint / midpoint /
intersection / nearest / onFace / perpendicular / parallel / axisX·Y·Z. Excludes
extension / apparent / grid / center / quadrant / tangent (they snap into empty
space and create dangling vertices — exactly the face-creation fragility the
disable was protecting).

## 3. Lock-ins (L-292-1 ~ L-292-10)

- **L-292-1** Snap is applied inside `get3DPoint` (SSOT) + `DrawRect`'s cardinal
  projection — NEVER as a terminal transform; the cardinal/face force always runs
  AFTER snap.
- **L-292-2** A snap result is ALWAYS projected onto the active draw plane
  (`plane.projectPoint`) before it can become the committed point — a snap moves
  the in-plane (x,y) position only, never the plane-normal coordinate. This is the
  invariant that provably prevents the 2026-05-18 off-plane RECT defect.
- **L-292-3** Snap candidate generation is **TS-only** over the cached DCEL
  geometry (`SnapManager.findSnap`) — NO WASM call in the mousemove/mousedown hot
  path (prevents the "recursive use of an object" borrow crash). `try/catch` →
  fall back to raw.
- **L-292-4** ADR-047 P32 chain self-touch exclusion re-wired:
  `snap.setExcludePositions(activeTool.getExcludedSnapPoints())` before every
  `findSnap` — the pending chain's mid-vertices are excluded (chainStart stays
  snappable for loop-close), so snap can never pull a corner onto its own
  not-yet-committed vertex.
- **L-292-5** Conservative `applyFaceCreationPreset()` mode set at init (excludes
  extension/apparent/grid/center/quadrant/tangent).
- **L-292-6** plane-lock (ADR-166/188) respected — snap projects onto the plane
  `getDrawPlane` returns (which already applies the lock); `applyObjectSnap` never
  mutates/unlocks `_planeLock`.
- **L-292-7** `snap.enabled` gate (OsnapPanel master toggle + Alt+X filters,
  already wired) controls it; disabled → raw passthrough (identical to LOCKED #63
  behavior).
- **L-292-8** `SnapVisual.update(snap, camera)` renders the marker on a hit,
  cleared on a miss — the guidance layer.
- **L-292-9** Backward compat: `getSnappedPoint` stays a pass-through (its callers
  pass an already-snapped `get3DPoint` output — no double-snap); `snapToPlane` on
  ToolContext is optional (`?.`), so test mocks / tools without it fall back to raw.
- **L-292-10** 절대 #[ignore] 금지.

## 4. Deferred (follow-up)

- ~~K (inference lock) / Tab (tentative cycle) key handlers~~ — **LANDED as a
  follow-up (2026-07-13, §D below).** The audit refuted the premise: the handlers
  already existed in `KeyboardShortcuts.ts` (:217-243) and went live again when
  this ADR re-enabled `findSnap`. Three fixes made them correct: (1) K
  `stopImmediatePropagation` (K also jumped to Back view), (2) `getActiveTentative`
  so a Tab-cycled pick is honored at commit (mousedown re-runs `findSnap` which
  resets the index), (3) `resetTentative` on mousemove + `clearLockedInference` +
  `resetTentative` on Esc / view-change / tool-switch (the lock/tentative leaked
  past intent boundaries, unlike ADR-166 plane-lock).
- Mid-draw Tab (2nd+ point of a multi-click draw) is owned by the capture-phase
  handler for VCB dimension-field cycling — Tab-tentative is idle-only; routing
  cycling-while-drawing through `tool.onKeyDown` is a separate larger change.
- Screen-space snap for OffsetTool's `getGroundPoint` path (lacks the cardinal
  force) — route through get3DPoint or add the force if OSNAP is wanted there.
- Persisted enable/preset via localStorage (currently OsnapPanel session state).
- Guide dashed lines (axis/parallel/extension) rendering polish.

## D. Acceptance Log

- **α (measure-first audit)** — 4-agent click-time consistency audit
  (`osnap-clicktime-consistency-audit`): pipeline order + insertion point, preserved
  snap infra + SnapResult contract, invariants + exact old defect. Converged on the
  single plane-consistent architecture above. User 결재: click-time auto-snap +
  safe re-projection + conservative preset.
- **β-1 (SSOT wiring)** — `ToolManager.applyObjectSnap` + wired into `get3DPoint`
  (sketch/face/ground branches, cardinal force terminal) + `applyFaceCreationPreset`
  at init + ADR-047 exclusion re-wired. Covers direct-point tools + DrawCircle. 908
  tool tests pass (no regression), tsc 0.
- **β-2 (DrawRect + units)** — `ToolContext.snapToPlane` exposed;
  `DrawRectTool.projectClickToCardinalPlane` snaps (both coplanar-pick + ray∩plane
  paths) before `forceCardinalAxis`. 3 unit tests: snap moves in-plane / cannot
  override the cardinal axis even if it returns off-plane (LOCKED #63 safety) /
  fallback when `snapToPlane` absent.
- **β-3 (E2E + closure)** — `web/e2e/adr-292-osnap-plane-consistent.spec.ts` (real
  Chromium + WASM + real canvas): a click 7px from a rect corner SNAPS to the exact
  corner, z stays exactly 0; a far click is raw, still z=0. **1/1 pass.** ADR
  closure + LOCKED #97 + LOCKED #63 amend note + README + memory.

**Regression totals**: vitest +3 (DrawRect ADR-292), Playwright +1. tsc 0. 908
tool tests green (no regression). Engine unchanged (TS-only). 절대 #[ignore] 금지.

- **follow-up (K/Tab wiring, 2026-07-13)** — 2-scout measure-first audit refuted
  the "handlers missing" premise (they exist + went live with this ADR). Three
  fixes: (1) `KeyboardShortcuts.ts` K handler `+ e.stopImmediatePropagation()`
  (K no longer also switches to Back view — same-window listener collision);
  (2) `SnapManager.getActiveTentative()` + `applyObjectSnap` prefers it before
  `findSnap` (guarded `!hasLockedInference`) so a Tab-cycled candidate is honored
  at commit; (3) `resetTentative()` at the top of the mousemove listener +
  `clearLockedInference()` / `resetTentative()` at `cancelCurrentTool` /
  `notifyViewModeChange` / `setTool` (the lock/tentative leaked past intent
  boundaries — a deliberate divergence from ADR-166 plane-lock, which persists
  across setTool). vitest +5 (lock cleared on Esc/view/tool + tentative no-ops),
  Playwright +1 (K-lock constrains the commit through applyObjectSnap; tool switch
  clears it). 1023 tool/snap/keyboard tests green, tsc 0.

## §Lessons

- **L1 measure-first found the ONE safe architecture.** All three successful audit
  agents converged: snap must sit after plane resolution and before the terminal
  cardinal/face force. The old defect was precisely that snap was the *terminal*
  transform returning a raw off-plane vertex. Ordering — not the snap engine — was
  the whole problem.
- **L2 the fix is re-wiring, not rebuilding.** The snap engine survived the disable
  fully intact and live-fed (a sibling consumer, `findAlignedDistance` for
  Push/Pull, kept `updateFromMesh` warm). Re-introduction = call the existing
  `findSnap` + project onto the plane.
- **L3 project-onto-plane is the safety invariant, provable by a unit test.** The
  "snap cannot override the cardinal axis even if it returns off-plane" test locks
  the property that makes OSNAP consistent with LOCKED #63 — a snap is an in-plane
  hint, structurally unable to reproduce the star-shaped RECT.
- **L4 dev-preview canvas is 0×0 in the Browser pane** (same root cause as the
  screenshot timeout) → screen-coordinate snap can't be exercised there; Playwright
  (real Chromium layout) is the reliable 시연 surface. Property/method names survive
  terser, so `get3DPoint` is callable in the production build too.

## Cross-link

- ADR-137 (Guidance-only Snap — refined by this ADR) / LOCKED #63 (the disable —
  amended)
- ADR-047 P32 (exclusion) / ADR-166/188 (plane lock) / ADR-167/168 (EPS_PLANE) /
  ADR-170 (normalizeDrawInput SSOT) / ADR-175/178 (face-aware plane) / ADR-026 P12
  (cardinal SSOT) / ADR-146 (findSnap telemetry) / LOCKED #5 (dedup floor)
- ADR-046 P31 #4 (additive — no new action/menu; OsnapPanel already existed) /
  ADR-087 K-ζ (사용자 시연 게이트 — the E2E) / 메타-원칙 #4 (SSOT) / #5 (UX) /
  #6 (measure-first) / LOCKED #44 (Complete Meaning per Merge) / #66 (STATUS-POLICY)
