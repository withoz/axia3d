/**
 * ADR-284 follow-up — drawing closed shapes on a curved surface, for real.
 *
 * ADR-284's closure says "Rect / Polygon / Freehand / Bezier →
 * drawPolylineOnCurved … grep-confirmed 4/4". The wiring is indeed there. It
 * did not work. Measured in real Chromium before this fix, on a Path B
 * cylinder:
 *
 *   Circle   → faces 3→4   ✓
 *   Polygon  → faces 3→3   ✗  ("wraps"; only a console.warn — no toast)
 *   Rect     → faces 3→3   ✗  (never even called the engine)
 *   Ellipse  → faces 3→3   ✗  (wired here; same failure)
 *
 * Root cause, one line up the stack: `get3DPoint` treated a curved face as a
 * plane. A Path B cylinder's side is ONE face wrapping 360°, so its averaged
 * DCEL normal points along the axis and the "face plane" passes through the
 * axis — a click on the surface at (200,0,200) returned (0,0,200). Every tool
 * that centres on that point built its shape around the axis, and the engine
 * correctly refused it as encircling. Circle was the sole survivor because it
 * reads `plane.origin` (the tangent point) instead of `point`.
 *
 * So this spec is the runtime check ADR-284 never had: grep proves a call site
 * exists, not that a user gets a face.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-284 follow-up — closed shapes on a curved surface', () => {
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

  // Per-tool gestures. RECT takes two OPPOSITE CORNERS, so its points must span
  // both surface directions — (200,0,200)→(200,0,250) shares y and is a
  // zero-width rect the tool rightly ignores. The others are centre-out.
  const GESTURES = [
    { tool: 'circle', clicks: 2, a: [200, 0, 200], b: [200, 0, 250], c: [200, 50, 200] },
    { tool: 'rect', clicks: 2, a: [200, -40, 170], b: [200, 40, 230], c: [200, 50, 200] },
    { tool: 'polygon', clicks: 2, a: [200, 0, 200], b: [200, 0, 250], c: [200, 50, 200] },
    { tool: 'ellipse', clicks: 3, a: [200, 0, 200], b: [200, 0, 250], c: [200, 50, 200] },
  ] as const;

  for (const g of GESTURES) {
    const { tool, clicks } = g;
    test(`${tool} splits the cylinder's side face`, async ({ page }) => {
      const s = await page.evaluate(async (cfg: typeof GESTURES[number]) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const ax = (window as any).__axia;
        const bridge = ax.get('bridge'); const tm = ax.get('toolManager'); const vp = ax.get('viewport');
        bridge.create_cylinder(0, 0, 0, 200, 400, 24); tm.syncMesh();
        vp.setCameraState({
          radius: 700, phi: 1.5708, theta: 0,
          targetX: 0, targetY: 0, targetZ: 200, orthoZoom: 4, viewMode: '3d',
        });
        // setCameraState lands on the NEXT frame — projecting immediately uses
        // the previous camera and silently yields off-screen coordinates.
        await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
        const cam = vp.activeCamera;
        const rect = vp.renderer.domElement.getBoundingClientRect();
        const proj = (x: number, y: number, z: number) => {
          const v = cam.position.clone(); v.set(x, y, z); v.project(cam);
          return {
            x: Math.round(rect.left + ((v.x + 1) / 2) * rect.width),
            y: Math.round(rect.top + ((1 - v.y) / 2) * rect.height),
          };
        };
        // the click point must resolve to the SURFACE, not the axis
        const surfaceProbePt = proj(200, 0, 200);
        const probe = tm.get3DPoint({ clientX: surfaceProbePt.x, clientY: surfaceProbePt.y });
        tm.setTool(cfg.tool);
        return {
          faces: bridge.getStats().faces,
          surfacePt: probe ? [+probe.x.toFixed(1), +probe.y.toFixed(1), +probe.z.toFixed(1)] : null,
          a: proj(cfg.a[0], cfg.a[1], cfg.a[2]),
          b: proj(cfg.b[0], cfg.b[1], cfg.b[2]),
          c: proj(cfg.c[0], cfg.c[1], cfg.c[2]),
        };
      }, g);

      // the fix, at its source: the click resolves to the surface (200,0,200),
      // not the axis (0,0,200)
      expect(s.surfacePt, 'get3DPoint must return the surface point on a curved face')
        .toEqual([200, 0, 200]);
      expect(s.faces, 'a Path B cylinder is 3 faces').toBe(3);

      await page.mouse.click(s.a.x, s.a.y);
      await page.mouse.click(s.b.x, s.b.y);
      if (clicks > 2) await page.mouse.click(s.c.x, s.c.y);
      await page.waitForTimeout(250);

      const after = await page.evaluate(() => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const ax = (window as any).__axia; const bridge = ax.get('bridge');
        ax.get('toolManager').syncMesh();
        const inv = bridge.verifyInvariants();
        return { faces: bridge.getStats().faces, valid: inv.valid, viol: inv.violationCount };
      });
      expect(after.faces, 'the side face must actually split (cap + remainder)').toBe(4);
      expect(after.valid).toBe(true);
      expect(after.viol).toBe(0);
    });
  }
});
