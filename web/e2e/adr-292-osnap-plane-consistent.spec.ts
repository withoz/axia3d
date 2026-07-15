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

  test('K inference-lock constrains the commit through applyObjectSnap; tool switch clears it', async ({ page }) => {
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      bridge.drawRectAsShape(50, 50, 0, 0, 0, 1, 1, 0, 0, 100, 100);
      tm.syncMesh();
      vp.setCameraState({ radius: 280, phi: 0.9, theta: 0.75, targetX: 50, targetY: 50, targetZ: 0, orthoZoom: 4, viewMode: '3d' });
      tm.setTool('line');
    });

    // Wait for the hover to ACTUALLY resolve to a snap instead of sleeping a
    // fixed 400 ms and hoping.
    //
    // Why (2026-07-15, first CI run on withoz/axia3d): this test went flaky on
    // ubuntu — `expect(r.locked).toBe(true)` got `false`, then passed on retry.
    // Root cause is the step below: it locks `tm.snap.lastSnap` BLINDLY, and
    // `setLockedInference(null)` is a silent no-op, so a hover that resolves to
    // nothing surfaces two lines later as `locked === false`. The hover can
    // resolve to nothing because `ToolManager.scheduleSnapRefresh` defers
    // `snap.updateFromMesh` to a `requestIdleCallback` — the snap spatial hash
    // is NOT warm the moment `syncMesh()` returns. A fixed sleep is a bet on
    // machine speed; polling the real precondition is not. (Not reproducible on
    // a fast Windows host: 20/20 passed even at --repeat-each=20 --workers=4.)
    //
    // Production is unaffected — the real K handler guards
    // (`else if (toolManager.snap.lastSnap)`, KeyboardShortcuts.ts), so pressing
    // K with no snap is a no-op there. Only this test bypasses that guard by
    // calling setLockedInference directly.
    // The sibling test above asserts these preconditions explicitly
    // (`canvasOk`, `snapVerts >= 4`) and passed on the same CI run; this one
    // asserted nothing, which is why its failure was a mystery. The catch below
    // makes any recurrence self-diagnosing instead.
    // NOTE the predicate must return a BOOLEAN — waitForFunction resolves on any
    // truthy value, so returning a diagnostic object would make it a no-op that
    // never actually waits. Diagnostics are stashed on window instead and read
    // back only if it times out.
    await page
      .waitForFunction(
        () => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const w = window as any; const ax = w.__axia;
          const tm = ax.get('toolManager'); const vp = ax.get('viewport');
          const canvas = vp.renderer.domElement;
          const rect = canvas.getBoundingClientRect();
          const cam = vp.activeCamera;
          const V3 = cam.position.constructor;
          const v = new V3(100, 100, 0).project(cam);
          const projFinite = Number.isFinite(v.x) && Number.isFinite(v.y);
          const layoutOk = rect.width > 10 && rect.height > 10;
          if (layoutOk && projFinite) {
            const x = rect.left + (v.x * 0.5 + 0.5) * rect.width;
            const y = rect.top + (-v.y * 0.5 + 0.5) * rect.height;
            tm.get3DPoint({ clientX: x + 6, clientY: y - 5 });
          }
          w.__snapProbe = {
            canvas: [Math.round(rect.width), Math.round(rect.height)],
            snapVerts: tm.snap?.vertices?.length ?? 0, // 0 ⇒ idle refresh hasn't landed
            projFinite,
          };
          return layoutOk && projFinite && tm.snap.lastSnap !== null;
        },
        undefined,
        { timeout: 10_000 },
      )
      .catch(async () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const d = await page.evaluate(() => (window as any).__snapProbe);
        throw new Error(
          `hover never resolved to a snap in 10s — ${JSON.stringify(d)}. ` +
            `snapVerts 0 ⇒ the deferred updateFromMesh never ran; ` +
            `projFinite false / canvas ~0 ⇒ camera or layout, not the snap cache.`,
        );
      });

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
      const cs = project([100, 100, 0]);
      // 1. hover near the corner → snaps to it (populates lastSnap)
      tm.get3DPoint({ clientX: cs.x + 6, clientY: cs.y - 5 });
      // 2. K = lock the current snap (the corner)
      tm.snap.setLockedInference(tm.snap.lastSnap);
      const locked = tm.snap.hasLockedInference();
      // 3. commit at a FAR cursor → still constrained to the locked corner, z=0
      const fs = project([0, 0, 0]);
      const whileLocked = tm.get3DPoint({ clientX: fs.x, clientY: fs.y });
      // 4. tool switch clears the lock (intent boundary)
      tm.setTool('circle');
      const clearedAfterSwitch = !tm.snap.hasLockedInference();
      // 5. same far cursor now returns the raw far point (not the corner)
      tm.setTool('line');
      const afterUnlock = tm.get3DPoint({ clientX: fs.x, clientY: fs.y });
      return {
        locked, clearedAfterSwitch,
        whileLocked: whileLocked ? [whileLocked.x, whileLocked.y, whileLocked.z] : null,
        afterUnlock: afterUnlock ? [afterUnlock.x, afterUnlock.y, afterUnlock.z] : null,
      };
    });

    expect(r.locked).toBe(true);
    expect(r.clearedAfterSwitch).toBe(true);      // reset on tool switch
    // while locked, the commit stays at the locked corner (100,100,0) despite a far cursor
    expect(r.whileLocked).not.toBeNull();
    expect(Math.hypot(r.whileLocked![0] - 100, r.whileLocked![1] - 100)).toBeLessThan(0.5);
    expect(Math.abs(r.whileLocked![2])).toBeLessThan(1e-6);
    // after unlock, the same far cursor is no longer pulled to the corner
    expect(r.afterUnlock).not.toBeNull();
    expect(Math.hypot(r.afterUnlock![0] - 100, r.afterUnlock![1] - 100)).toBeGreaterThan(5);
  });
});
