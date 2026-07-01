/**
 * ADR-202 ζ — Curved-surface sketching closure: drawCircleOnSphere E2E.
 *
 * End-to-end regression coverage (real Chromium + production build +
 * compiled WASM engine) for drawing closed circles ON a sphere face
 * (cap / annulus split, the sphere MVP shipped this track).
 *
 * Layering of the coverage:
 *   - Rust unit tests lock the ENGINE geometry — `tessellate_sphere_clipped`
 *     smooth marching boundary (single + multi circle) and the co-spherical
 *     clip gate that keeps ADR-197/198 Boolean caps out.
 *   - vitest locks the DrawCircleTool sphere DISPATCH (mocked bridge).
 *   - THIS spec is the missing end-to-end link: the WASM-compiled engine +
 *     WasmBridge produce a correct cap/annulus split in a real browser, with
 *     a smooth on-circle boundary, and 2+/overlapping circles never collapse
 *     the sphere to a flat disk.
 *
 * Only mangling-safe bridge methods are used (object property names survive
 * the production minify; local vars don't — same contract every other E2E
 * spec relies on).
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any
 * WASM rebuild for this spec to pick up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-202 — circle sketching on a sphere', () => {
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

  test('single circle → cap + annulus split (both Sphere faces, manifold, smooth boundary)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 5);
      const before = bridge.getStats().faces;
      // north-pole latitude circle → plane z=4, radius 3 in xy (simple to check).
      const res = JSON.parse(bridge.drawCircleOnSphere(0, [0, 0, 5], [3, 0, 4]));
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      const capKind = bridge.faceSurfaceKind(res.cap);
      const annKind = bridge.faceSurfaceKind(res.annulus);
      // Smooth boundary: the marching clip snaps boundary verts EXACTLY onto
      // the circle. Count render verts lying on z=4 / r=3 (a jagged centroid
      // clip would leave them at grid positions, off the circle).
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let onCircle = 0;
      for (let i = 0; i < pos.length; i += 3) {
        const x = pos[i], y = pos[i + 1], z = pos[i + 2];
        if (Math.abs(z - 4) < 1e-3 && Math.abs(Math.sqrt(x * x + y * y) - 3) < 1e-3) onCircle++;
      }
      return {
        before, after, cap: res.cap, annulus: res.annulus,
        valid: inv.valid, viol: inv.violationCount, capKind, annKind, onCircle,
      };
    });
    expect(r.before).toBe(2); // Path B sphere = 2 hemispheres
    expect(r.after).toBe(3); // cap + annulus(host) + other hemisphere
    expect(r.capKind).toBe(3); // Sphere
    expect(r.annKind).toBe(3); // Sphere (host keeps its sphere surface)
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    expect(r.onCircle).toBeGreaterThanOrEqual(10); // smooth boundary on the circle
  });

  test('two separated circles on one hemisphere → host carries 2 holes, manifold, full 3D', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 5);
      // +Y and -Y small circles on the upper hemisphere (separated).
      const r1 = JSON.parse(bridge.drawCircleOnSphere(0, [0, 3, 4], [0, 4, 3]));
      const host = r1.annulus;
      const r2 = JSON.parse(bridge.drawCircleOnSphere(host, [0, -3, 4], [0, -4, 3]));
      const stats = bridge.getStats();
      const inv = bridge.verifyInvariants();
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let zmin = 1e9, zmax = -1e9;
      for (let i = 0; i < pos.length; i += 3) {
        zmin = Math.min(zmin, pos[i + 2]);
        zmax = Math.max(zmax, pos[i + 2]);
      }
      return { faces: stats.faces, valid: inv.valid, viol: inv.violationCount, host, cap2: r2.cap, zmin, zmax };
    });
    expect(r.faces).toBe(4); // 2 hemispheres + 2 caps
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // host renders the FULL 3D sphere (multi-clip did not empty/flatten it).
    expect(r.zmin).toBeLessThan(-4.9);
    expect(r.zmax).toBeGreaterThan(4.9);
  });

  test('overlapping circles → still a full 3D solid (no flat-disk regression)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 5);
      // three overlapping circles near the north pole (the user's stress case).
      bridge.drawCircleOnSphere(0, [0, 0, 5], [2, 0, 4.58]);
      bridge.drawCircleOnSphere(0, [1, 0, 4.9], [3, 0, 4]);
      bridge.drawCircleOnSphere(0, [0, 1, 4.9], [2, 2, 4]);
      const inv = bridge.verifyInvariants();
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let zmin = 1e9, zmax = -1e9;
      for (let i = 0; i < pos.length; i += 3) {
        zmin = Math.min(zmin, pos[i + 2]);
        zmax = Math.max(zmax, pos[i + 2]);
      }
      return { valid: inv.valid, viol: inv.violationCount, zmin, zmax };
    });
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // the sphere stays a full 3D solid (z spans ~[-5, 5]), NOT a flat disk.
    expect(r.zmin).toBeLessThan(-4.9);
    expect(r.zmax).toBeGreaterThan(4.9);
  });
});
