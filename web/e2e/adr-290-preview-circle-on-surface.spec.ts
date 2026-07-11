/**
 * ADR-290 곡면 편집 마무리 — on-surface DrawCircle preview E2E.
 *
 * The DrawCircle tool's live preview used to draw a FLAT tangent-plane circle
 * even when the user was sketching on a curved host (Sphere/Cylinder/Cone/
 * Torus) — the preview floated off the surface and mismatched the committed
 * result (drawCircleOnSphere etc.). This track adds `preview_circle_on_surface`
 * (read-only) so the preview FOLLOWS the surface.
 *
 * Layering of the coverage:
 *   - Rust unit test locks the ENGINE (preview_circle_on_surface returns an
 *     on-surface polyline for a Sphere, None for a Plane).
 *   - vitest locks the bridge wrapper + the DrawCircleTool preview branch.
 *   - THIS spec is the end-to-end link: the WASM-compiled engine + WasmBridge
 *     produce an on-surface polyline in a real browser (every point lies ON the
 *     surface, NOT on the flat tangent plane).
 *
 * Only mangling-safe bridge methods are used (object property names survive the
 * production minify; local vars don't).
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any
 * WASM rebuild for this spec to pick up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-290 — on-surface circle preview', () => {
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

  test('sphere preview polyline follows the surface (every point on the sphere, not flat)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 5);
      // find a Sphere face by scanning candidate ids (kind 3 = Sphere).
      const findFace = (kind: number) => {
        for (let f = 0; f < 60; f++) {
          try { if (bridge.faceSurfaceKind(f) === kind) return f; } catch { /* inactive */ }
        }
        return -1;
      };
      const sph = findFace(3);
      // north-pole latitude circle: center on the sphere, radius reference on it.
      const poly: Float32Array | null = bridge.previewCircleOnSurface(
        sph, [0, 0, 5], [3, 0, 4],
      );
      if (!poly) return { ok: false };
      let allOnSphere = true;
      let allAtPole = true; // a FLAT tangent circle would sit at z=5
      let n = 0;
      for (let i = 0; i + 2 < poly.length; i += 3) {
        const x = poly[i], y = poly[i + 1], z = poly[i + 2];
        const rad = Math.sqrt(x * x + y * y + z * z);
        if (Math.abs(rad - 5) > 0.05) allOnSphere = false;
        if (Math.abs(z - 5) > 0.05) allAtPole = false;
        n++;
      }
      return { ok: true, len: poly.length, n, allOnSphere, allAtPole };
    });
    expect(r.ok).toBe(true);
    expect(r.len! % 3).toBe(0);
    expect(r.n!).toBeGreaterThanOrEqual(3);
    expect(r.allOnSphere).toBe(true);   // FOLLOWS the sphere
    expect(r.allAtPole).toBe(false);    // NOT the flat tangent-plane circle
  });

  test('cylinder preview polyline lies on the wall (dist-to-axis ≈ radius)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setCylinderPathBDefault?.(true);
      // Z-up cylinder, radius 5.
      bridge.create_cylinder(0, 0, 0, 5, 10, 24);
      let cyl = -1;
      for (let f = 0; f < 60; f++) {
        try { if (bridge.faceSurfaceKind(f) === 2) { cyl = f; break; } } catch { /* inactive */ }
      }
      if (cyl < 0) return { ok: false, reason: 'no cylinder face' };
      // derive a real wall point from the mesh buffer (robust vs axis/z range).
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let wall: [number, number, number] | null = null;
      for (let i = 0; i + 2 < pos.length; i += 3) {
        const x = pos[i], y = pos[i + 1], z = pos[i + 2];
        if (Math.abs(Math.sqrt(x * x + y * y) - 5) < 0.2) { wall = [x, y, z]; break; }
      }
      if (!wall) return { ok: false, reason: 'no wall vertex' };
      // center = the wall point; radius reference = a nearby wall point.
      const ang = Math.atan2(wall[1], wall[0]) + 0.25;
      const rref: [number, number, number] = [5 * Math.cos(ang), 5 * Math.sin(ang), wall[2]];
      const poly: Float32Array | null = bridge.previewCircleOnSurface(cyl, wall, rref);
      if (!poly) return { ok: false, reason: 'null poly' };
      let allOnWall = true;
      let n = 0;
      for (let i = 0; i + 2 < poly.length; i += 3) {
        const x = poly[i], y = poly[i + 1];
        if (Math.abs(Math.sqrt(x * x + y * y) - 5) > 0.1) allOnWall = false;
        n++;
      }
      return { ok: true, n, allOnWall };
    });
    expect(r.ok).toBe(true);
    expect(r.n!).toBeGreaterThanOrEqual(3);
    expect(r.allOnWall).toBe(true); // FOLLOWS the cylinder wall
  });
});
