/**
 * ADR-292 — plane-consistent object snap (OSNAP re-introduction).
 *
 * OSNAP was disabled 2026-05-18 (LOCKED #63) because the old auto-magnet
 * returned a snapped vertex's RAW 3D position (incl. its z) as the committed
 * point → off-plane RECT corners → star-shaped self-intersecting solid.
 *
 * ADR-292 re-introduces it plane-consistently: a snap can only move the
 * IN-PLANE position; the cardinal-axis / face-plane force is re-applied as the
 * TERMINAL transform, so a committed point NEVER leaves the active draw plane.
 *
 * This spec proves, through the real WASM engine + real canvas layout, that:
 *  (1) a click NEAR an existing vertex SNAPS to it (in-plane), and
 *  (2) the committed point's cardinal axis stays exactly 0 (on-plane), and
 *  (3) a click FAR from any geometry is NOT snapped (raw), still on z=0.
 *
 * NOTE: Playwright uses `npm run preview` (production build). Re-build
 * (`npm run build`) after any change. Property/method names survive terser
 * (only locals mangle), so `toolManager.get3DPoint` is callable here.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-292 — plane-consistent object snap', () => {
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

  test('click near a corner snaps to it in-plane; z stays exactly 0; far click is raw', async ({ page }) => {
    // set up: a 100×100 rect on the z=0 ground → 4 corners as snap targets.
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      bridge.drawRectAsShape(50, 50, 0, 0, 0, 1, 1, 0, 0, 100, 100); // corners (0,0,0)..(100,100,0)
      tm.syncMesh();
      vp.setCameraState({ radius: 280, phi: 0.9, theta: 0.75, targetX: 50, targetY: 50, targetZ: 0, orthoZoom: 4, viewMode: '3d' });
      tm.setTool('line');
    });
    // let the snap cache idle-refresh (updateFromMesh via requestIdleCallback).
    await page.waitForTimeout(400);

    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      const cam = vp.activeCamera; const canvas = vp.renderer.domElement;
      const rect = canvas.getBoundingClientRect();
      const V3 = cam.position.constructor;
      const project = (p: number[]) => {
        const v = new V3(p[0], p[1], p[2]).project(cam);
        return { x: rect.left + (v.x * 0.5 + 0.5) * rect.width, y: rect.top + (-v.y * 0.5 + 0.5) * rect.height };
      };
      const canvasOk = rect.width > 10 && rect.height > 10;
      const snapVerts = tm.snap?.vertices?.length ?? 0;
      const corner = [100, 100, 0];
      const s = project(corner);
      // cursor ~7px from the exact corner → inside the 15px snap threshold.
      const near = tm.get3DPoint({ clientX: s.x + 7, clientY: s.y - 6 });
      // cursor well away from any vertex → no snap.
      const far = tm.get3DPoint({ clientX: s.x - 200, clientY: s.y + 150 });
      return {
        canvasOk, snapVerts,
        near: near ? [near.x, near.y, near.z] : null,
        far: far ? [far.x, far.y, far.z] : null,
      };
    });

    expect(r.canvasOk).toBe(true);           // real canvas layout (Playwright)
    expect(r.snapVerts).toBeGreaterThanOrEqual(4); // rect corners cached
    // (1) near-corner click SNAPPED to the exact corner (7px offset erased)
    expect(r.near).not.toBeNull();
    expect(Math.hypot(r.near![0] - 100, r.near![1] - 100)).toBeLessThan(0.5);
    // (2) cardinal axis stays exactly 0 — the LOCKED #63 safety invariant
    expect(Math.abs(r.near![2])).toBeLessThan(1e-6);
    // (3) far click is NOT snapped (raw ground point), still on z=0
    expect(r.far).not.toBeNull();
    expect(Math.hypot(r.far![0] - 100, r.far![1] - 100)).toBeGreaterThan(5);
    expect(Math.abs(r.far![2])).toBeLessThan(1e-6);
  });
});
