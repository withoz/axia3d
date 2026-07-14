/**
 * Core modeling-flow coverage — the two gaps the E2E suite was missing
 * (verified by the 2026-07-14 coverage audit): (1) drawing a DISJOINT coplanar
 * shape after an extruded box, and (2) Boolean UNION of two solids. Overlap
 * auto-split and closed-loop synthesis are already covered elsewhere
 * (adr-101-b6-*, z0-closed-loop-face-synthesis) and are NOT duplicated here.
 *
 * GAP #1 — mirrors the Rust regression
 * `disjoint_coplanar_ground_rect_after_box_succeeds` (crates/axia-core scene
 * tests) at the real-Chromium tool-path level. Before the kernel fix, the
 * boundary re-derive (ADR-281 β-1 `solid_top_boundary`, built unscoped) fed a
 * far-away solid's coplanar bottom into the arrange → 3-face non-manifold edges
 * → `guard_imprint` rolled the whole draw back, so "draw a box, then draw
 * another shape on the ground" was blocked. The fix scopes `solid_top_boundary`
 * to the drawn region (connected-component AABB overlap).
 *
 * GAP #2 — Boolean union of two overlapping solids → merged watertight solid.
 * Engine authority: axia-geo `adr197_beta2_union_box_box_watertight` (boundary
 * HEs == 0 ≡ closed solid, invariants valid). The browser had solid-vs-solid
 * Boolean E2E only for SUBTRACT (adr-278); this seals UNION. Operand pattern
 * mirrors adr-278: fresh scene → face ids are 0-based contiguous, split by
 * `getStats().faces` count, then the general `booleanSolid` (the same entry
 * BooleanHandler uses).
 *
 * Playwright uses the production build (`npm run preview`) — rebuild
 * (`npm run build`) after any WASM rebuild.
 */
import { test, expect } from '@playwright/test';
import { waitForBridgeReady } from './helpers/boolean-fixtures';

interface AxiaWindow {
  __axia?: {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    get<T = any>(key: string): T;
  };
}

test.describe('core modeling flow — disjoint-after-box + solid union', () => {
  test.beforeEach(async ({ page }) => {
    // Match the browser production defaults (ADR-176/186): auto behaviors ON.
    // Set explicitly (before app init) so the test is self-documenting and
    // robust to any future default change. Harmless for the union test, which
    // is a direct mesh op independent of these flags.
    await page.addInitScript(() => {
      localStorage.setItem('axia:auto-intersect-on-draw', 'true');
      localStorage.setItem('axia:auto-face-synthesis-on-draw', 'true');
      localStorage.setItem('axia:face-rederive-on-draw', 'true');
    });
    await page.goto('/');
    await waitForBridgeReady(page);
  });

  test('drawing a disjoint coplanar rect after an extruded box succeeds (+1 face, nm=0, valid)', async ({ page }) => {
    const r = await page.evaluate(() => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');

      // box: 1000x800 rect at z=0 → extrude 500 (bottom face is coplanar z=0).
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 0, 1, 0, 1000, 800);
      const profile = bridge.getShapeFaceIds(shape)[0];
      bridge.createSolidExtrude(profile, 500);
      const boxMi = bridge.meshManifoldInfo();
      const boxInv = bridge.verifyInvariants();
      const before = bridge.faceCount();

      // disjoint rect FAR AWAY on the SAME z=0 plane (coplanar with box bottom).
      const drawRet = bridge.drawRectAsShape(2000, 0, 0, 0, 0, 1, 0, 1, 0, 600, 600);
      const after = bridge.faceCount();
      const mi = bridge.meshManifoldInfo();
      const inv = bridge.verifyInvariants();

      return {
        box: {
          faces: boxMi.faceCount,
          closed: boxMi.isClosedSolid,
          nm: boxMi.nonManifoldEdgeCount,
          valid: boxInv.valid,
        },
        drawRet,
        before,
        after,
        nm: mi.nonManifoldEdgeCount,
        valid: inv.valid,
        viol: inv.violationCount,
      };
    });

    // box precondition — a clean 6-face closed watertight solid.
    expect(r.box.faces).toBe(6);
    expect(r.box.closed).toBe(true);
    expect(r.box.nm).toBe(0);
    expect(r.box.valid).toBe(true);

    // the disjoint draw was NOT rejected (returns a valid ShapeId, not -1).
    expect(r.drawRet).toBeGreaterThanOrEqual(0);
    // exactly +1 free-standing face, no non-manifold edge introduced, valid.
    expect(r.after).toBe(r.before + 1);
    expect(r.nm).toBe(0);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  test('overlapping coplanar rects still auto-split into 3 sub-faces (fix did not over-skip)', async ({ page }) => {
    // Control for GAP #1: the disjoint-scoping fix must NOT suppress the
    // coplanar auto-split path. Mirrors Rust `adr176_two_rects_as_shape_
    // partial_overlap_auto_split` (1 → 3 active faces).
    const r = await page.evaluate(() => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');
      bridge.drawRectAsShape(100, 100, 0, 0, 0, 1, 0, 1, 0, 200, 200);
      const afterA = bridge.faceCount();
      bridge.drawRectAsShape(200, 200, 0, 0, 0, 1, 0, 1, 0, 200, 200);
      const afterB = bridge.faceCount();
      const inv = bridge.verifyInvariants();
      return { afterA, afterB, valid: inv.valid };
    });
    expect(r.afterA).toBe(1);
    expect(r.afterB).toBe(3); // rect_a_only / lens / rect_b_only (ADR-101 P7)
    expect(r.valid).toBe(true);
  });

  test('Boolean union of two overlapping boxes yields a merged closed watertight solid', async ({ page }) => {
    const r = await page.evaluate(() => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');

      // box A: 120^3 centered (0,0,60). Face ids 0..5 in a fresh scene.
      bridge.create_box(0, 0, 60, 120, 120, 120);
      const nA = bridge.getStats().faces;
      const facesA = Array.from({ length: nA }, (_, i) => i);

      // box B: 120^3 centered (80,80,60) — OVERLAPS A (offset 80 < 120).
      bridge.create_box(80, 80, 60, 120, 120, 120);
      const nAll = bridge.getStats().faces;
      const facesB = Array.from({ length: nAll - nA }, (_, i) => nA + i);

      const before = nAll;
      const res = bridge.booleanSolid(new Uint32Array(facesA), new Uint32Array(facesB), 'union');
      const after = bridge.getStats().faces;
      const outward = bridge.verifyOutwardNormals();
      const inv = bridge.verifyInvariants();

      return {
        nA,
        facesBCount: facesB.length,
        ok: !!(res && res.ok),
        before,
        after,
        merged: after !== before,
        isClosedSolid: outward.isClosedSolid,
        valid: inv.valid,
        viol: inv.violationCount,
      };
    });

    // two 6-face boxes built as separate DCEL solids.
    expect(r.nA).toBe(6);
    expect(r.facesBCount).toBe(6);
    // union succeeded and remeshed the two solids into one.
    expect(r.ok).toBe(true);
    expect(r.merged).toBe(true);
    // canonical merged criterion (Rust adr197_beta2_union_box_box_watertight):
    // closed solid (0 boundary HEs) + valid invariants.
    expect(r.isClosedSolid).toBe(true);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });
});
