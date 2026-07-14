/**
 * Snap indicator sits AFTER the coordinates in a fixed, always-reserved slot,
 * so it appearing / disappearing / changing never shifts the status bar.
 *
 * #sb-snap ("●center" etc.) used to sit BEFORE the coords and collapse to 0px
 * when empty, expanding to 110px on a snap → a 116px shove of the coords,
 * F-keys, plane badge, and 치수 every time a snap appeared. It is now placed
 * after the coords with a permanent 110px reservation.
 *
 * Proven on the real production DOM: snap comes after coords in the flex row,
 * the slot width is a constant 110px whether empty or full, and the downstream
 * F-keys / commandbar positions are byte-stable.
 */
import { test, expect } from '@playwright/test';

test.describe('Status-bar snap after coords (fixed slot)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        return w.__axia && typeof w.__axia.get === 'function' && w.__axia.get('bridge');
      },
      { timeout: 30000 },
    );
  });

  test('snap slot follows coords, is always 110px, and never reflows downstream', async ({ page }) => {
    const order = await page.evaluate(() =>
      Array.from(document.getElementById('statusbar')!.children)
        .map((c) => c.id)
        .filter(Boolean),
    );
    expect(order.indexOf('sb-coords')).toBeGreaterThanOrEqual(0);
    expect(order.indexOf('sb-snap')).toBeGreaterThan(order.indexOf('sb-coords')); // after coords

    const measure = (snapText: string) =>
      page.evaluate((t) => {
        const snap = document.getElementById('sb-snap')!;
        snap.textContent = t;
        const f1 = document.querySelector('[data-fkey="F1"]');
        const cmdbar = document.getElementById('commandbar');
        return {
          snapW: Math.round(snap.getBoundingClientRect().width),
          f1X: f1 ? Math.round(f1.getBoundingClientRect().x) : null,
          cmdbarX: cmdbar ? Math.round(cmdbar.getBoundingClientRect().x) : null,
        };
      }, snapText);

    const empty = await measure('');
    const shortSnap = await measure('●center');
    const widest = await measure('●perpendicular');

    // Always-reserved 110px slot, regardless of content.
    expect(empty.snapW).toBe(110);
    expect(shortSnap.snapW).toBe(110);
    expect(widest.snapW).toBe(110);

    // Downstream never moves when a snap appears/changes.
    expect(shortSnap.f1X).toBe(empty.f1X);
    expect(widest.f1X).toBe(empty.f1X);
    expect(shortSnap.cmdbarX).toBe(empty.cmdbarX);
    expect(widest.cmdbarX).toBe(empty.cmdbarX);
  });
});
