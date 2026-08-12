/**
 * What happens when a WINDOW or a POLYGON hole would cross a hole already there.
 *
 * `DrawHoleTool` was taught to ask instead of guessing; its two siblings were
 * not, and they had the same defect. Measured in this harness on a Ø80-bored
 * 200³ box, for the rect and the polygon alike:
 *
 * ```text
 *   drill → −1 (crossing refused)   punch → 41 (succeeded)   closed → OPEN
 * ```
 *
 * The punch has no reason to refuse: the profile fits its host face perfectly —
 * it is the FAR side that is missing. So the tools now ask the engine first, and
 * this file pins both halves of that: the query agrees with the drill, and the
 * punch they no longer reach really would have opened the solid.
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

const R = 40;
/** A rect and a polygon on the +X wall, both spanning the Ø80 bore on the axis. */
const RECT: [number, number, number][] = [
  [100, -30, -30],
  [100, 30, 30],
];
const POLY: [number, number, number][] = [
  [100, -30, -30],
  [100, 30, -30],
  [100, 30, 30],
  [100, -30, 30],
];
const NX: [number, number, number] = [1, 0, 0];

test.describe('a crossing opening is refused, not quietly punched', () => {
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

  test('the engine reports the crossing for a rect and for a polygon', async ({ page }) => {
    const got = await page.evaluate(
      ({ r, rect, poly, nx }) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const bridge = (window as any).__axia.get('bridge');
        bridge.engine.create_box(0, 0, 0, 200, 200, 200);
        bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
        const bored = {
          closed: bridge.verifyOutwardNormals().isClosedSolid,
          valid: bridge.verifyInvariants().valid,
        };
        return {
          bored,
          rectCrosses: bridge.drillProfileWouldCross(rect, nx),
          polyCrosses: bridge.drillProfileWouldCross(poly, nx),
          // Well clear of the bore, up in the corner of the same wall.
          rectClear: bridge.drillProfileWouldCross(
            [[100, 60, 60], [100, 90, 90]], nx,
          ),
          polyClear: bridge.drillProfileWouldCross(
            [[100, 60, 60], [100, 90, 60], [100, 90, 90], [100, 60, 90]], nx,
          ),
          // Asking must not touch anything.
          after: {
            closed: bridge.verifyOutwardNormals().isClosedSolid,
            valid: bridge.verifyInvariants().valid,
          },
        };
      },
      { r: R, rect: RECT, poly: POLY, nx: NX },
    );

    expect(got.bored.closed).toBe(true);
    expect(got.bored.valid).toBe(true);
    expect(got.rectCrosses).toBe(true);
    expect(got.polyCrosses).toBe(true);
    // The controls matter as much: a query that always says "crossing" would
    // refuse every ordinary window on a bored wall.
    expect(got.rectClear).toBe(false);
    expect(got.polyClear).toBe(false);
    expect(got.after).toEqual(got.bored);
  });

  test('the query agrees with the drill it stands in for', async ({ page }) => {
    const got = await page.evaluate(
      ({ r, rect, poly, nx }) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const bridge = (window as any).__axia.get('bridge');
        const fresh = () => {
          bridge.engine.create_box(0, 0, 0, 200, 200, 200);
          bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
        };
        fresh();
        const rectAsked = bridge.drillProfileWouldCross(rect, nx);
        const rectDrill = bridge.drillRectThroughHole(rect[0], rect[1], nx);
        const polyAsked = bridge.drillProfileWouldCross(poly, nx);
        const polyDrill = bridge.drillPolygonThroughHole(poly, nx);
        return { rectAsked, rectDrill, polyAsked, polyDrill };
      },
      { r: R, rect: RECT, poly: POLY, nx: NX },
    );

    expect(got.rectAsked).toBe(true);
    expect(got.rectDrill).toBe(-1);
    expect(got.polyAsked).toBe(true);
    expect(got.polyDrill).toBe(-1);
  });

  test('the punch the tools no longer reach really would open the solid', async ({ page }) => {
    // This is the reason the guard exists, kept as a measurement rather than a
    // claim. If a future change makes the punch refuse a crossing by itself,
    // this test fails and the guard can be reconsidered — which is the point.
    const got = await page.evaluate(
      ({ r, rect, nx }) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const bridge = (window as any).__axia.get('bridge');
        bridge.engine.create_box(0, 0, 0, 200, 200, 200);
        bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
        const closedBefore = bridge.verifyOutwardNormals().isClosedSolid;
        const drill = bridge.drillRectThroughHole(rect[0], rect[1], nx);
        const punch = bridge.punchRectHole(rect[0], rect[1], nx);
        return {
          closedBefore,
          drill,
          punch,
          closedAfter: bridge.verifyOutwardNormals().isClosedSolid,
        };
      },
      { r: R, rect: RECT, nx: NX },
    );

    expect(got.closedBefore).toBe(true);
    expect(got.drill).toBe(-1); // refused, deliberately
    expect(got.punch).toBeGreaterThan(0); // and the fallback would have succeeded
    expect(got.closedAfter).toBe(false); // leaving the solid open
  });
});
