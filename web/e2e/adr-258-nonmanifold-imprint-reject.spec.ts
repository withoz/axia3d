/**
 * ADR-258 γ — Coplanar imprint on a solid face, E2E (real Chromium).
 *
 * 2026-08-03 — the first case changed meaning. A rectangle that overlaps a
 * solid's face and runs past its edge used to be rolled back: the part inside
 * the face ended up covered twice, and the engine treated that as damage. It is
 * repairable, and repairing it is the point of the draw
 * (`subtract_double_covered_faces`), so the draw is now resolved instead of
 * refused. The user asked for exactly this:
 *
 *   "입체면 밖으로 확장해서 그리는 기능도 구현해야 합니다"
 *
 * What the guard still refuses is genuine corruption, and the second case —
 * a contained imprint — is unchanged.
 *
 * End-to-end regression (real Chromium + production build + compiled WASM)
 * for the fail-closed guard (Scene::guard_imprint, β-1): a coplanar draw that
 * would introduce a non-manifold edge (crossing/touching a solid face
 * boundary — the exact condition the ADR-047 R1 orange overlay flags) is
 * rolled back + rejected; a fully-contained draw passes through.
 *
 * Layering:
 *   - Rust unit tests lock the engine guard (adr258_* in scene.rs).
 *   - vitest locks the bridge Toast surfacing (β-2 surfaceDrawReject).
 *   - THIS spec is the end-to-end link: the WASM-compiled engine + WasmBridge
 *     reject a cross-boundary imprint and leave the solid manifold, in a real
 *     browser.
 *
 * Geometry: create_box spans z ∈ [cz-h/2, cz+h/2] (ADR-103 Z-up, centered).
 * create_box(0,0,0,100,100,100) → box [-50,50]³, top face at z=50.
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any
 * WASM rebuild for this spec to pick up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-258 — reject non-manifold coplanar imprint', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge');
      },
      { timeout: 30000 },
    );
  });

  test('an imprint that runs past the face edge now resolves into separate regions', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.create_box(0, 0, 0, 100, 100, 100); // top at z=50
      const beforeFaces = bridge.getStats().faces;
      // partial-overlap rect on the top: center (20,20,50), 100×100 → spans
      // [-30,70]² → crosses the box-top boundary (x=50, y=50).
      const ret = bridge.drawRectAsShape(20, 20, 50, 0, 0, 1, 0, 1, 0, 100, 100);
      w.__axia.get('toolManager')?.syncMesh?.();
      const mi = bridge.meshManifoldInfo();
      const inv = bridge.verifyInvariants();
      return {
        ret, beforeFaces, afterFaces: bridge.getStats().faces,
        nm: mi.nonManifoldEdgeCount, closed: mi.isClosedSolid,
        valid: inv.valid, viol: inv.violationCount,
        si: bridge.detectSelfIntersections?.()?.count ?? 0,
      };
    });
    expect(r.beforeFaces).toBe(6);            // clean box
    expect(r.ret).toBeGreaterThanOrEqual(0);  // accepted
    // Three regions where there were two overlapping ones: the part only the
    // top face had, the part they shared, and the part that reached past the
    // edge.
    expect(r.afterFaces).toBeGreaterThan(6);
    expect(r.si, 'nothing may end up covered twice').toBe(0);
    // The solid is open along the new boundary — that is what drawing past an
    // edge means — but every violation must be one of those shared edges and
    // nothing else.
    expect(r.viol).toBe(r.nm);
  });

  test('contained imprint on a solid face is accepted (manifold split)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.create_box(0, 0, 0, 100, 100, 100);
      const beforeFaces = bridge.getStats().faces;
      // fully-contained rect (30×30 well inside the 100×100 top).
      const ret = bridge.drawRectAsShape(0, 0, 50, 0, 0, 1, 0, 1, 0, 30, 30);
      w.__axia.get('toolManager')?.syncMesh?.();
      const mi = bridge.meshManifoldInfo();
      const inv = bridge.verifyInvariants();
      return {
        ret, beforeFaces, afterFaces: bridge.getStats().faces,
        nm: mi.nonManifoldEdgeCount, valid: inv.valid, viol: inv.violationCount,
      };
    });
    expect(r.ret).toBeGreaterThanOrEqual(0); // accepted (shape id)
    expect(r.afterFaces).toBeGreaterThan(r.beforeFaces); // split added faces
    expect(r.nm).toBe(0);                    // contained imprint stays manifold
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });
});
