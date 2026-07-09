/**
 * ADR-285 β-5 — Parametric direct edit of analytic curved faces: E2E.
 *
 * End-to-end regression (real Chromium + production build + compiled WASM) for
 * editing a curved primitive's defining parameters IN PLACE via the bridge:
 *   - Sphere   → radius
 *   - Cylinder → radius + height
 *   - Cone     → base radius + height (half_angle recomputed)
 *   - Torus    → major + minor radius
 *
 * Each asserts: the edit succeeds, the analytic surface param actually changes
 * (getFaceSurfaceJson), the mesh stays manifold (verifyInvariants), and the
 * topology is unchanged (face count constant — parametric edit, not a rebuild).
 *
 * Only mangling-safe bridge methods are used (object property names survive the
 * production minify; local vars don't — same contract as every other E2E spec).
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any WASM
 * rebuild so this spec picks up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-285 — parametric curved-face edit', () => {
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

  test('Sphere radius edit — both hemispheres update, manifold, topology fixed', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 10);
      const facesBefore = bridge.getStats().faces;
      let f = -1;
      for (let i = 0; i < facesBefore + 2; i++) { if (bridge.faceSurfaceKind(i) === 3) { f = i; break; } }
      const rBefore = JSON.parse(bridge.getFaceSurfaceJson(f)).radius;
      const ok = bridge.setSphereRadius(f, 18);
      const rAfter = JSON.parse(bridge.getFaceSurfaceJson(f)).radius;
      const inv = bridge.verifyInvariants();
      return { f, ok, rBefore, rAfter, facesBefore, facesAfter: bridge.getStats().faces, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.f).toBeGreaterThanOrEqual(0);
    expect(r.ok).toBe(true);
    expect(r.rBefore).toBeCloseTo(10, 3);
    expect(r.rAfter).toBeCloseTo(18, 3);
    expect(r.facesAfter).toBe(r.facesBefore);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  test('Cylinder radius + height edit — manifold, topology fixed', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.setCylinderPathBDefault?.(true);
      bridge.create_cylinder(0, 0, 0, 10, 20, 16);
      const facesBefore = bridge.getStats().faces;
      let side = -1;
      for (let i = 0; i < facesBefore + 2; i++) { if (bridge.faceSurfaceKind(i) === 2) { side = i; break; } }
      const okR = bridge.setCylinderRadius(side, 6);
      const okH = bridge.setCylinderHeight(side, 30);
      const s = JSON.parse(bridge.getFaceSurfaceJson(side));
      const h = s.vRange ? s.vRange[1] - s.vRange[0] : -1;
      const inv = bridge.verifyInvariants();
      return { side, okR, okH, radius: s.radius, height: h, facesBefore, facesAfter: bridge.getStats().faces, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.side).toBeGreaterThanOrEqual(0);
    expect(r.okR).toBe(true);
    expect(r.okH).toBe(true);
    expect(r.radius).toBeCloseTo(6, 3);
    expect(r.height).toBeCloseTo(30, 3);
    expect(r.facesAfter).toBe(r.facesBefore);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  test('Cone base radius + height edit — half_angle recomputed, manifold', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_cone(0, 0, 0, 5, 20);
      const facesBefore = bridge.getStats().faces;
      let side = -1;
      for (let i = 0; i < facesBefore + 2; i++) { if (bridge.faceSurfaceKind(i) === 4) { side = i; break; } }
      const okR = bridge.setConeRadius(side, 8);
      const okH = bridge.setConeHeight(side, 12);
      const s = JSON.parse(bridge.getFaceSurfaceJson(side));
      const h = s.vRange ? s.vRange[1] - s.vRange[0] : -1;
      const baseR = typeof s.halfAngle === 'number' ? h * Math.tan(s.halfAngle) : -1;
      const inv = bridge.verifyInvariants();
      return { side, okR, okH, baseR, height: h, facesBefore, facesAfter: bridge.getStats().faces, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.side).toBeGreaterThanOrEqual(0);
    expect(r.okR).toBe(true);
    expect(r.okH).toBe(true);
    expect(r.baseR).toBeCloseTo(8, 2);
    expect(r.height).toBeCloseTo(12, 3);
    expect(r.facesAfter).toBe(r.facesBefore);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  test('Torus major + minor radius edit — manifold, single face', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_torus(0, 0, 0, 10, 3);
      const facesBefore = bridge.getStats().faces;
      let f = -1;
      for (let i = 0; i < facesBefore + 2; i++) { if (bridge.faceSurfaceKind(i) === 5) { f = i; break; } }
      const okMaj = bridge.setTorusMajorRadius(f, 15);
      const okMin = bridge.setTorusMinorRadius(f, 5);
      const s = JSON.parse(bridge.getFaceSurfaceJson(f));
      const inv = bridge.verifyInvariants();
      return { f, okMaj, okMin, major: s.majorRadius, minor: s.minorRadius, facesBefore, facesAfter: bridge.getStats().faces, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.f).toBeGreaterThanOrEqual(0);
    expect(r.okMaj).toBe(true);
    expect(r.okMin).toBe(true);
    expect(r.major).toBeCloseTo(15, 3);
    expect(r.minor).toBeCloseTo(5, 3);
    expect(r.facesAfter).toBe(r.facesBefore);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });
});
