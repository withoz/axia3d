/**
 * ADR-190 Phase 3 — the engine's vocabulary must not reach the user.
 *
 * `lastError()` carries the kernel's error chain verbatim, and the tools Toast
 * it. Measured through the production bridge before this change, pushing a
 * cylinder's curved side face showed:
 *
 *     돌출/잘라내기 실패: Face needs at least 3 verts
 *
 * — true (a closed-curve face is 1 anchor + 1 self-loop edge, ADR-089) and
 * useless. Other paths leaked ADR numbers, Rust type names and "Q3 fallback to
 * legacy push_pull".
 *
 * The unit tests pin `humanizeEngineError` as a function; only this spec proves
 * the wiring reaches a real Toast in a real browser.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-190 Phase 3 — engine errors are humanized', () => {
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

  test('pushing a curved side face says what to do, not "3 verts"', async ({ page }) => {
    const armed = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      bridge.create_cylinder(0, 0, 0, 200, 400, 24);
      tm.syncMesh();
      let side = -1;
      for (let f = 0; f < 8; f++) if (bridge.faceSurfaceKind(f) === 2) { side = f; break; } // 2 = Cylinder
      tm.setTool('pushpull');
      ax.get('selection').selectFaces([side]);
      return { side, kind: bridge.faceSurfaceKind(side) };
    });
    expect(armed.side, 'the cylinder side face must be found').toBeGreaterThanOrEqual(0);
    expect(armed.kind, 'sanity: it really is a Cylinder-surface face').toBe(2);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').applyVCBValue(100);
    });

    const toast = await page.locator('#axia-toast-container').innerText();
    // the engine still rejects — that part is unchanged and correct
    expect(toast).toContain('실패');
    // ...but in the user's language, pointing at the path that works
    expect(toast, 'engine vocabulary must not reach the user').not.toContain('verts');
    expect(toast).toContain('곡면 위에 원을 그린 뒤');
  });

  test('no ADR number, Rust type name or fallback note ever reaches a Toast', async ({ page }) => {
    // a curved profile cannot be tapered; the raw message named ADR-259, the
    // (Plane, AllLinear) enum pair and the Q3 push_pull fallback
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      bridge.create_cylinder(0, 0, 0, 200, 400, 24);
      tm.syncMesh();
      let side = -1;
      for (let f = 0; f < 8; f++) if (bridge.faceSurfaceKind(f) === 2) { side = f; break; }
      tm.setTool('pushpull');
      ax.get('selection').selectFaces([side]);
      tm.applyVCBValue(100, 30); // distance + taper angle
    });

    const toast = await page.locator('#axia-toast-container').innerText();
    expect(toast.length, 'something must be said').toBeGreaterThan(0);
    for (const leak of ['ADR-', 'AllLinear', 'push_pull', 'FaceId(', 'top_scale', 'Q3 fallback']) {
      expect(toast, `"${leak}" leaked into the UI`).not.toContain(leak);
    }
  });
});
