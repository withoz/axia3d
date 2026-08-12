/**
 * What the user gets when a hole would cross a hole that is already there.
 *
 * Before this, the drill refused the crossing (a straight tube cannot bridge the
 * void the first bore left) and `DrawHoleTool` fell through to punching a 2D
 * face hole. Measured in this very harness: the fallback SUCCEEDED, the solid
 * went from closed to OPEN, and the user was told "면 구멍을 뚫었습니다".
 *
 * Now the engine says both things the tool needs — whether anything lies across
 * the axis at all, and whether the kernel can do this particular crossing — and
 * the tool asks instead of guessing (메타-원칙 #16).
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

const R = 40;

test.describe('a crossing hole is asked about, not guessed at', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge');
      },
      { timeout: 30_000 },
    );
  });

  test('the engine reports the crossing, and the kernel closes it', async ({ page }) => {
    const got = await page.evaluate(({ r }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const eng = bridge.engine;
      eng.create_box(0, 0, 0, 200, 200, 200);
      bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
      const closedAfterFirst = bridge.verifyOutwardNormals().isClosedSolid;

      // The ordinary drill still refuses.
      const refused = bridge.drillThroughHole([100, 0, 0], [1, 0, 0], r, 32);
      // And now it can be told apart from a sheet.
      const plan = bridge.canDrillCrossingBore([100, 0, 0], [1, 0, 0], r, 32);
      const kept = bridge.drillCrossingBore([100, 0, 0], [1, 0, 0], r, 32);
      return {
        closedAfterFirst,
        refused,
        plan,
        kept,
        closed: bridge.verifyOutwardNormals().isClosedSolid,
        valid: bridge.verifyInvariants().valid,
        faces: bridge.getStats().faces,
      };
    }, { r: R });

    expect(got.closedAfterFirst).toBe(true);
    expect(got.refused).toBe(-1);
    // The two answers the tool needs, separately.
    expect(got.plan.crossing).toBe(true);
    expect(got.plan.ok).toBe(true);
    // And the crossing itself.
    expect(got.kept).toBeGreaterThan(0);
    expect(got.closed).toBe(true);
    expect(got.valid).toBe(true);
    expect(got.faces).toBe(134);
  });

  test('a crossing the kernel cannot do is reported, not silently punched', async ({ page }) => {
    const got = await page.evaluate(({ r }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const eng = bridge.engine;
      eng.create_box(0, 0, 0, 200, 200, 200);
      bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
      const before = bridge.getStats().faces;
      // A smaller radius: a genuine quartic, which the kernel path refuses.
      const plan = bridge.canDrillCrossingBore([100, 0, 0], [1, 0, 0], 25, 32);
      const kept = bridge.drillCrossingBore([100, 0, 0], [1, 0, 0], 25, 32);
      return {
        plan,
        kept,
        before,
        after: bridge.getStats().faces,
        closed: bridge.verifyOutwardNormals().isClosedSolid,
      };
    }, { r: R });

    // It IS a crossing — so the face-hole fallback must not run — but not one
    // the kernel can do, so only the reason is offered.
    expect(got.plan.crossing).toBe(true);
    expect(got.plan.ok).toBe(false);
    expect(String(got.plan.reason)).toContain('crossing bore');
    // Refused with the solid intact.
    expect(got.kept).toBe(-1);
    expect(got.after).toBe(got.before);
    expect(got.closed).toBe(true);
  });

  test('an ordinary sheet still reports no crossing, so the face hole still works', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
      const plan = bridge.canDrillCrossingBore([0, 0, 0], [0, 0, 1], 40, 32);
      return { plan, faces: bridge.getStats().faces };
    });
    expect(got.plan.crossing).toBe(false);
    expect(got.plan.ok).toBe(false);
  });
});
