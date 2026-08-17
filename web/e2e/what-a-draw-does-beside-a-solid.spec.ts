/**
 * What a draw does beside a solid — this session's changes, in the running app.
 *
 * Four things moved in the engine and none had been seen outside a Rust test.
 * Each is something somebody does: draw across a hole's rim, draw on the plane a
 * box's bottom is on, draw through a box at a height, draw beside a box. Real
 * Chromium, real WASM, the production build.
 *
 * Every check reads the bridge's own report — face counts, `verifyInvariants`,
 * `verifyOutwardNormals` — rather than pixels, because what changed is topology
 * and the render would not tell them apart.
 *
 * ⚠ The app's scenes are not the Rust fixtures. `bridge.create_box` goes through
 * the scene layer, so a box dropped onto existing coplanar faces is auto-split
 * against them and starts life with more than six faces. One of the four reads
 * differently here because of that, and it says so rather than being tuned until
 * it agrees.
 *
 * Rebuild (`npm run build:wasm && npm run build`) before running: Playwright
 * serves the production build.
 */
import { test, expect } from '@playwright/test';

const boot = async (page: import('@playwright/test').Page) => {
  await page.goto('/');
  await page.waitForFunction(
    () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = (window as any).__axia;
      return w && typeof w.get === 'function' && w.get('bridge');
    },
    { timeout: 30000 },
  );
};

test.describe('a draw beside a solid', () => {
  /**
   * A rectangle across the rim of a hole in a solid's top.
   *
   * The re-tile used to decline this plane — it reaches two perimeters, the
   * top's outer rim and the hole's — because it counted components rather than
   * owners. One solid with a hole is ONE owner, so it is carried now and the rim
   * divides the drawn rect.
   */
  test('a rect across a hole rim is divided by the rim', async ({ page }) => {
    await boot(page);
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const b = (window as any).__axia.get('bridge');
      b.create_box(0, 0, 60, 200, 120, 200);
      const drilled = b.drillRectThroughHole([-30, -30, 120], [30, 30, 120], [0, 0, 1]);
      const before = b.getStats().faces;
      // x ∈ [0,60] — the hole's rim is at x = 30, so this straddles it.
      b.drawRectAsShape(30, 0, 120, 0, 0, 1, 0, 1, 0, 60, 30);
      const inv = b.verifyInvariants();
      return {
        drilled,
        before,
        after: b.getStats().faces,
        valid: inv.valid,
        violations: inv.violations.length,
      };
    });
    expect(r.drilled).toBeGreaterThan(0); // the fixture needs its hole
    expect(r.after).toBeGreaterThan(r.before + 1); // divided, not added whole
    expect(r.valid).toBe(true);
    expect(r.violations).toBe(0);
  });

  /**
   * A circle drawn on the plane a box's BOTTOM is on, overlapping it.
   *
   * The post-draw repair used to replace that bottom with a free sheet, which
   * left the box's walls without a neighbour — the engine then read them as
   * sheets, so pushing one extruded a second box onto the first. The repair
   * declines any change that leaves a solid open now.
   */
  test('a draw on the plane a box sits on leaves the box closed', async ({ page }) => {
    await boot(page);
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const b = (window as any).__axia.get('bridge');
      b.create_box(0, 0, 100, 200, 200, 200);
      const before = b.getStats().faces;
      b.drawCircleAsCurve(-70, -70, 0, 0, 0, 1, 60);
      const inv = b.verifyInvariants();
      return {
        before,
        after: b.getStats().faces,
        closed: b.verifyOutwardNormals().isClosedSolid,
        valid: inv.valid,
        violations: inv.violations.length,
      };
    });
    expect(r.after).toBeGreaterThan(r.before); // the draw lands
    expect(r.closed).toBe(true); // and the box survives it
    expect(r.valid).toBe(true);
    expect(r.violations).toBe(0);
  });

  /**
   * A square drawn at a height INSIDE a box — one side out, then two.
   *
   * The second one used to duplicate the middle piece: the split took every
   * crossing segment at once and paired the intersection points along the
   * direction of the first, so with two walls it paired across them and handed
   * back the whole square twice. It cuts by one connected path at a time now,
   * and both read clean.
   */
  test('a square through a box: one side clean', async ({ page }) => {
    await boot(page);
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const b = (window as any).__axia.get('bridge');
      b.create_box(150, 50, 160, 180, 120, 180);
      b.drawPolygonAsShape(150, 100, 200, 0, 0, 1, 70, 4);
      const inv = b.verifyInvariants();
      return { faces: b.getStats().faces, violations: inv.violations.length };
    });
    expect(r.violations).toBe(0);
  });

  test('a square through a box: two sides clean too', async ({ page }) => {
    await boot(page);
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const b = (window as any).__axia.get('bridge');
      b.create_box(150, 50, 160, 180, 120, 180);
      b.drawPolygonAsShape(100, 100, 200, 0, 0, 1, 70, 4);
      const inv = b.verifyInvariants();
      return { faces: b.getStats().faces, violations: inv.violations.length };
    });
    expect(r.violations).toBe(0);
  });

  /**
   * A circle drawn beside a box, on a plane two rectangles already share.
   *
   * ⚠ THE ONE THAT READS DIFFERENTLY HERE. In the Rust fixture the box is built
   * straight into the mesh and the same draw comes out sound; through the bridge
   * the box is auto-split against the rectangles it lands on and starts with ten
   * faces instead of six, and the draw still leaves two stacked.
   *
   * So the fix holds for the scene it was reduced from and not for the app's own
   * richer one. That is worth having in front of us rather than tuned away: the
   * circle IS divided — ten faces to twenty — and what is left over is the same
   * kind of stack, in a scene one step more tangled than the repro.
   */
  test('a circle beside a box divides, and still leaves a stack here', async ({ page }) => {
    await boot(page);
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const b = (window as any).__axia.get('bridge');
      b.drawRectAsShape(0, 100, 100, 0, 0, 1, 1, 0, 0, 100, 75);
      b.drawRectAsShape(0, 100, 100, 0, 0, 1, 1, 0, 0, 180, 135);
      b.create_box(-50, 50, 160, 180, 120, 180);
      const before = b.getStats().faces;
      b.drawCircleAsCurve(0, 100, 100, 0, 0, 1, 110);
      const inv = b.verifyInvariants();
      return {
        before,
        after: b.getStats().faces,
        violations: inv.violations.length,
        first: inv.violations[0] ?? null,
      };
    });
    // The division happens.
    expect(r.after).toBeGreaterThan(r.before + 1);
    // And this scene still stacks. When that stops, say what fixed it.
    expect(r.violations).toBeGreaterThan(0);
    expect(String(r.first)).toContain('cover the same ground');
  });
});
