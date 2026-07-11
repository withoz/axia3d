/**
 * ADR-287 β — Curved cut (pocket) + boss on CONE and TORUS walls: E2E.
 *
 * Extends ADR-271 (cylinder cut) + ADR-286 (cylinder boss) to cone/torus via the
 * shared `curved_carve_core` (per-vertex surface-normal offset; floor/roof = the
 * same analytic surface at the offset parameter — cone = parallel cone, torus =
 * minor∓depth). Cone/torus caps are N-vert polyline geodesics (ADR-263), so they
 * feed the welding core directly (sphere, a self-loop, is deferred — ADR-287 §7).
 *
 * Flow: create_cone/torus → find the curved side face (faceSurfaceKind 4/5) →
 * drawCircleOn{Cone,Torus} splits it into cap + remainder → carveCurvedPocket
 * (push in) / carveCurvedBoss (push out). Asserts: succeeds (walls > 0), stays a
 * manifold (verifyInvariants valid, 0 viol), topology grows (+faces).
 *
 * One primitive + one op per test (fresh page per test → unambiguous face IDs).
 * Only mangling-safe bridge methods. Playwright uses the production build
 * (`npm run preview`) — rebuild (`npm run build`) after any WASM rebuild.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-287 — curved cut/boss on cone + torus', () => {
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

  const coneCarve = (dir: 'pocket' | 'boss') => async ({ page }: { page: import('@playwright/test').Page }) => {
    const r = await page.evaluate((d) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_cone(0, 0, 0, 500, 1000, 32);
      let side = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 4) { side = i; break; }
      }
      const res = JSON.parse(bridge.drawCircleOnCone(side, [250, 0, 500], [250, 0, 600]));
      const before = bridge.getStats().faces;
      const walls = d === 'pocket'
        ? bridge.carveCurvedPocket(res.cap, 60)
        : bridge.carveCurvedBoss(res.cap, 60);
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { walls, before, after, valid: inv.valid, viol: inv.violationCount };
    }, dir);
    expect(r.walls).toBeGreaterThan(0);
    expect(r.after).toBeGreaterThan(r.before);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  };

  const torusCarve = (dir: 'pocket' | 'boss') => async ({ page }: { page: import('@playwright/test').Page }) => {
    const r = await page.evaluate((d) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_torus(0, 0, 0, 500, 100);
      let side = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 5) { side = i; break; }
      }
      const res = JSON.parse(bridge.drawCircleOnTorus(side, [600, 0, 0], [580, 0, 80]));
      const before = bridge.getStats().faces;
      const walls = d === 'pocket'
        ? bridge.carveCurvedPocket(res.cap, 40)
        : bridge.carveCurvedBoss(res.cap, 40);
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { walls, before, after, valid: inv.valid, viol: inv.violationCount };
    }, dir);
    expect(r.walls).toBeGreaterThan(0);
    expect(r.after).toBeGreaterThan(r.before);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  };

  test('cone wall → pocket (push in) manifold', coneCarve('pocket'));
  test('cone wall → boss (push out) manifold', coneCarve('boss'));
  test('torus wall → pocket (push in) manifold', torusCarve('pocket'));
  test('torus wall → boss (push out) manifold', torusCarve('boss'));

  // ADR-287 through-hole ε — a DEEP inward push on a cone cap (depth ≥ the cap's
  // axis-radial distance) auto-routes (Scene) to a diametric THROUGH-drill → a
  // watertight genus-1 tunnel through the cone.
  test('cone wall → deep push = through-drill (watertight tunnel)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_cone(0, 0, 0, 500, 1000, 32);
      let side = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 4) { side = i; break; }
      }
      const res = JSON.parse(bridge.drawCircleOnCone(side, [250, 0, 500], [250, 0, 600]));
      const before = bridge.getStats().faces;
      // cap centre radial ≈ 250; push deeper than that → through-route.
      const walls = bridge.carveCurvedPocket(res.cap, 350);
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { walls, before, after, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.walls).toBeGreaterThan(0);    // tube walls of the through-tunnel
    expect(r.after).toBeGreaterThan(r.before);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  // ε-sphere-2 (ADR-287) — production drawCircleOnSphere now yields an N-vert cap
  // (polyline split + planar-clip render), so a sphere is carveable via the same
  // path: sketch circle → push in (pocket) / out (boss) → watertight manifold.
  const sphereCarve = (dir: 'pocket' | 'boss') => async ({ page }: { page: import('@playwright/test').Page }) => {
    const r = await page.evaluate((d) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.create_sphere(0, 0, 0, 5);
      let host = -1;
      for (let i = 0; i < bridge.getStats().faces + 2; i++) {
        if (bridge.faceSurfaceKind(i) === 3) { host = i; break; }
      }
      const res = JSON.parse(bridge.drawCircleOnSphere(host, [0, 0, 5], [3, 0, 4]));
      const before = bridge.getStats().faces;
      const walls = d === 'pocket'
        ? bridge.carveCurvedPocket(res.cap, 1.5)
        : bridge.carveCurvedBoss(res.cap, 2.0);
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { walls, before, after, valid: inv.valid, viol: inv.violationCount };
    }, dir);
    expect(r.walls).toBeGreaterThan(0);
    expect(r.after).toBeGreaterThan(r.before);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  };

  test('sphere → pocket (push in) manifold [ε-sphere-2]', sphereCarve('pocket'));
  test('sphere → boss (push out) manifold [ε-sphere-2]', sphereCarve('boss'));
});
