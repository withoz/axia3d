/**
 * ADR-190 Phase 3 — repeat last distance (double-click), SketchUp parity.
 *
 * `PushPullTool.lastPPDist` was written by all four commit paths and read by
 * NOTHING — a dead cache. A double-click's 2nd mousedown already lands in
 * Phase 2 with dist ≈ 0 (the cursor did not move), where MIN_COMMIT_DIST
 * swallows it; that dead slot is the hook. The unit tests drive synthetic
 * MouseEvents, so only this spec can prove the part that belongs to the
 * BROWSER: that a real double-click actually raises `detail` to 2 and that the
 * engine commits the remembered distance.
 *
 * NOTE: Playwright uses `npm run preview` (production build). Re-build
 * (`npm run build`) after any change. Property/method names survive terser
 * (only locals mangle), so `toolManager` / `bridge` are callable here.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-190 Phase 3 — repeat last (double-click)', () => {
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

  test('a real double-click on a fresh face repeats the last committed distance', async ({ page }) => {
    // Two identical rects, far apart on the z=0 ground. Push the first with an
    // explicit VCB distance, then DOUBLE-CLICK the second and assert it rose by
    // the same amount — with no distance typed or dragged the second time.
    const setup = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 200, 200);     // A around origin
      bridge.drawRectAsShape(600, 0, 0, 0, 0, 1, 1, 0, 0, 200, 200);   // B, far away
      tm.syncMesh();
      vp.setCameraState({
        radius: 1600, phi: 0.85, theta: 0.6,
        targetX: 300, targetY: 0, targetZ: 100, orthoZoom: 4, viewMode: '3d',
      });
      const r = vp.renderer.domElement.getBoundingClientRect();
      return { canvasW: Math.round(r.width), canvasH: Math.round(r.height) };
    });
    expect(setup.canvasW, 'real canvas layout is required for mouse input').toBeGreaterThan(10);

    // Project the two face centres through the live camera (Three is reachable
    // via the viewport's own objects, so we avoid importing it here).
    const pts = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const vp = ax.get('viewport');
      const cam = vp.activeCamera;
      const canvas = vp.renderer.domElement;
      const r = canvas.getBoundingClientRect();
      // clone a vector off an existing object to get a THREE.Vector3 instance
      const mk = (x: number, y: number, z: number) => {
        const v = cam.position.clone(); v.set(x, y, z); return v;
      };
      const proj = (x: number, y: number, z: number) => {
        const v = mk(x, y, z).project(cam);
        return {
          x: Math.round(r.left + ((v.x + 1) / 2) * r.width),
          y: Math.round(r.top + ((1 - v.y) / 2) * r.height),
        };
      };
      return { a: proj(0, 0, 0), b: proj(600, 0, 0) };
    });

    // ── 1) push A by an explicit 150 via the VCB → seeds lastPPDist ──
    const afterA = await page.evaluate(({ a }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const tm = ax.get('toolManager'); const bridge = ax.get('bridge');
      tm.setTool('pushpull');
      const canvas = ax.get('viewport').renderer.domElement;
      const md = new MouseEvent('mousedown', { clientX: a.x, clientY: a.y, bubbles: true, detail: 1 });
      canvas.dispatchEvent(md);
      // typed distance, through the same entry the VCB panel uses → commits and
      // records lastPPDist
      tm.applyVCBValue(150);
      tm.syncMesh();
      return { faces: bridge.getStats().faces };
    }, pts);
    expect(afterA.faces, 'A must have extruded into a solid').toBeGreaterThan(2);

    const zBefore = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      return w.__axia.get('bridge').getStats().faces;
    });

    // ── 2) REAL double-click on B — no typing, no dragging ──
    await page.mouse.dblclick(pts.b.x, pts.b.y);
    await page.waitForTimeout(300);

    const res = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge');
      ax.get('toolManager').syncMesh();
      const inv = bridge.verifyInvariants();
      // Height of B ALONE (x > 400). A was already pushed to 150, so a
      // whole-mesh max would read 150 even if B never moved — measure B's own
      // region or the assertion proves nothing.
      const buf = bridge.getMeshBuffers();
      let maxZB = 0;
      if (buf && buf.positions) {
        const p = buf.positions;
        for (let i = 0; i < p.length; i += 3) {
          if (p[i] > 400) maxZB = Math.max(maxZB, p[i + 2]);
        }
      }
      return {
        faces: bridge.getStats().faces, maxZB,
        valid: inv.valid, violations: inv.violationCount,
      };
    });

    // B rose to the SAME height A was pushed to → the remembered 150 was reused
    expect(res.maxZB, 'the double-clicked face (B) must rise by the repeated distance').toBeCloseTo(150, 0);
    expect(res.faces, 'B must have extruded too').toBeGreaterThan(afterA.faces);
    expect(res.valid, 'the result must stay valid').toBe(true);
    expect(res.violations).toBe(0);
    expect(zBefore).toBeGreaterThan(0); // sanity: the probe ran
  });
});
