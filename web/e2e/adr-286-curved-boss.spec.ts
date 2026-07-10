/**
 * ADR-286 β-4 — Curved BOSS (outward protrusion) on a Cylinder wall: E2E.
 *
 * End-to-end regression (real Chromium + production build + compiled WASM) for
 * raising a boss from a sketched Cylinder cap — the mirror of the ADR-271 curved
 * pocket. Flow: create_cylinder → find the Cylinder side face → drawCircleOnCylinder
 * (ADR-257) splits it into cap + remainder → carveCurvedBoss(cap, height) pushes
 * the cap radially OUTWARD → a raised curved boss.
 *
 * Asserts: the boss succeeds (walls > 0), the mesh stays a watertight manifold
 * (verifyInvariants), the topology grows (face count up), and a new Cylinder-kind
 * roof face exists at radius + height (surface inheritance, ADR-263 A-χ). Also
 * asserts a non-positive height is rejected and leaves the mesh untouched.
 *
 * Only mangling-safe bridge methods are used (object property names survive the
 * production minify). Playwright uses `npm run preview` (production build) per
 * playwright.config.ts — re-build the prod bundle (`npm run build`) AFTER any WASM
 * rebuild so this spec picks up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-286 — curved boss (outward) on a cylinder wall', () => {
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

  test('sketch circle → push out → curved boss (roof at r+height, manifold, +faces)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.setCylinderPathBDefault?.(true);
      // radius 10, height 20 → z ∈ [0, 20]; 16 segments.
      bridge.create_cylinder(0, 0, 0, 10, 20, 16);
      // Cylinder side face (kind 2); base/top are Plane (kind 1).
      let side = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 2) { side = i; break; }
      }
      // Draw a porthole circle on the wall at z-mid (10), angle 0 → offset 0.4.
      const res = JSON.parse(bridge.drawCircleOnCylinder(side, [10, 0, 10], [10 * Math.cos(0.4), 10 * Math.sin(0.4), 10]));
      const facesBefore = bridge.getStats().faces;
      const height = 5;
      const walls = bridge.carveCurvedBoss(res.cap, height);
      const facesAfter = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      // Find a Cylinder-kind face whose radius ≈ 15 (the raised roof, r+height).
      let roofRadius = -1;
      for (let i = 0; i < facesAfter + 4; i++) {
        if (bridge.faceSurfaceKind(i) === 2) {
          const s = bridge.getFaceSurfaceJson(i);
          if (s) {
            const rad = JSON.parse(s).radius;
            if (Math.abs(rad - 15) < 1e-3) { roofRadius = rad; break; }
          }
        }
      }
      return { walls, facesBefore, facesAfter, valid: inv.valid, viol: inv.violationCount, roofRadius };
    });
    expect(r.walls).toBeGreaterThan(0);          // N side walls raised
    expect(r.facesAfter).toBeGreaterThan(r.facesBefore); // walls + roof added
    expect(r.valid).toBe(true);                  // watertight manifold
    expect(r.viol).toBe(0);
    expect(r.roofRadius).toBeCloseTo(15, 3);     // roof at r + height (10 + 5), Cylinder-inherited
  });

  test('non-positive height rejected → mesh untouched', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.setCylinderPathBDefault?.(true);
      bridge.create_cylinder(0, 0, 0, 10, 20, 16);
      let side = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 2) { side = i; break; }
      }
      const res = JSON.parse(bridge.drawCircleOnCylinder(side, [10, 0, 10], [10 * Math.cos(0.4), 10 * Math.sin(0.4), 10]));
      const facesBefore = bridge.getStats().faces;
      const walls = bridge.carveCurvedBoss(res.cap, -3);
      const facesAfter = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { walls, facesBefore, facesAfter, valid: inv.valid };
    });
    expect(r.walls).toBe(-1);                     // rejected
    expect(r.facesAfter).toBe(r.facesBefore);     // mesh untouched (snapshot rollback)
    expect(r.valid).toBe(true);
  });
});
