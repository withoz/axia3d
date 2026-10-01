/**
 * Clicking a spline back onto its first point makes a face.
 *
 * `DrawSplineTool` always called `drawBSplineWithCurve` — the OPEN curve —
 * however the points sat, so `drawClosedBSplineAsCurve` (engine op, WASM export
 * and bridge wrapper all present since ADR-089 A-Α) had zero callers. Its
 * sibling `DrawBezierTool` has routed closure since A-ψ.
 *
 * The engine half is measured in `a_closed_spline_closes.rs` and the tool half
 * in `aClosedSplineClosesTheLoop` (vitest, against a mock bridge). Neither can
 * answer the question the user asks — does clicking it make a face — so this
 * drives real mouse clicks on the real canvas against the real engine.
 *
 * The discriminator is the face count, and it is not arbitrary: an OPEN spline
 * makes edges and no face. So "faces >= 1" can only come from the closed path,
 * and the second test is the control that shows it.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 */
import { test, expect } from '@playwright/test';

async function setup(page: import('@playwright/test').Page) {
  // Curve mode is default-ON (ADR-089 A-π) but an explicit OFF preference is
  // honoured, so pin it rather than inherit whatever the profile holds.
  await page.addInitScript(() => {
    try { localStorage.setItem('axia:draw-curve-mode', 'true'); } catch { /* private mode */ }
  });
  await page.goto('/');
  await page.waitForFunction(
    () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = (window as any).__axia;
      return w && typeof w.get === 'function' && w.get('bridge') && w.get('toolManager');
    },
    { timeout: 30000 },
  );
  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const c = (window as any).__axia;
    c.get('viewport').setViewMode('top');
    c.get('toolManager').setTool('spline');
  });
  await page.waitForTimeout(120);
}

async function stats(page: import('@playwright/test').Page) {
  return page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    const s = bridge.getStats();
    const inv = bridge.verifyInvariants();
    return {
      faces: s.faces,
      edges: s.edges,
      violations: (inv?.violations ?? []).length,
      lastError: bridge.lastError?.() ?? '',
    };
  });
}

test.describe('a spline clicked back onto its start', () => {
  test('makes a face', async ({ page }) => {
    test.setTimeout(60_000);
    await setup(page);

    const box = await page.locator('canvas').first().boundingBox();
    if (!box) throw new Error('no canvas');
    const cx = box.x + box.width / 2;
    const cy = box.y + box.height / 2;

    // Four corners, then the SAME screen pixel as the first: one ray, one
    // plane, so the world point is identical and the gap is 0.
    const corners: [number, number][] = [
      [cx - 140, cy - 100], [cx + 140, cy - 100],
      [cx + 140, cy + 100], [cx - 140, cy + 100],
    ];
    for (const [x, y] of corners) {
      await page.mouse.click(x, y);
      await page.waitForTimeout(70);
    }
    await page.mouse.click(corners[0][0], corners[0][1]);
    await page.waitForTimeout(70);
    await page.keyboard.press('Enter');
    await page.waitForTimeout(250);

    const s = await stats(page);
    console.log('  closed spline:', JSON.stringify(s));
    expect(s.faces, `a closed loop must synthesise a face — lastError: ${s.lastError}`)
      .toBeGreaterThanOrEqual(1);
    expect(s.violations, 'and the mesh stays sound').toBe(0);
  });

  /** The control: without the closing click there is no face to find. */
  test('and an open one does not', async ({ page }) => {
    test.setTimeout(60_000);
    await setup(page);

    const box = await page.locator('canvas').first().boundingBox();
    if (!box) throw new Error('no canvas');
    const cx = box.x + box.width / 2;
    const cy = box.y + box.height / 2;

    for (const [x, y] of [
      [cx - 140, cy - 100], [cx + 140, cy - 100],
      [cx + 140, cy + 100], [cx - 140, cy + 100],
    ] as [number, number][]) {
      await page.mouse.click(x, y);
      await page.waitForTimeout(70);
    }
    await page.keyboard.press('Enter');
    await page.waitForTimeout(250);

    const s = await stats(page);
    console.log('  open spline:', JSON.stringify(s));
    // PREMISE: the tool ran at all — an open spline still lays down edges.
    expect(s.edges, 'the open spline must have been drawn').toBeGreaterThanOrEqual(3);
    expect(s.faces, 'but an open curve is not a face').toBe(0);
    expect(s.violations).toBe(0);
  });
});
