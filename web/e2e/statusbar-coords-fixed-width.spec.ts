/**
 * Status-bar coordinate field has a fixed reserved width, so the F-keys,
 * plane badge, and 치수 (commandbar) do NOT shift as the cursor moves.
 *
 * The coords readout (#sb-coords) previously used min-width:220px, so any
 * value past ~±100 m/axis grew the field and pushed everything downstream
 * right. It now reserves a fixed 300px (≈ ±1 km/axis) with overflow clip.
 *
 * Proven on the real production DOM by writing short / wide / out-of-range
 * coordinate strings and asserting the coords box width and the downstream
 * element positions are byte-stable.
 */
import { test, expect } from '@playwright/test';

test.describe('Status-bar coords fixed width (no downstream reflow)', () => {
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

  test('coords width + F-key / 치수 positions stay fixed across coord lengths', async ({ page }) => {
    const measure = (text: string) =>
      page.evaluate((t) => {
        const coords = document.getElementById('sb-coords')!;
        coords.textContent = t;
        const f1 = document.querySelector('[data-fkey="F1"]');
        const cmdbar = document.getElementById('commandbar');
        return {
          coordsW: Math.round(coords.getBoundingClientRect().width),
          f1X: f1 ? Math.round(f1.getBoundingClientRect().x) : null,
          cmdbarX: cmdbar ? Math.round(cmdbar.getBoundingClientRect().x) : null,
        };
      }, text);

    const short = await measure('0.0000, 0.0000, 0.0000');
    const wide = await measure('-999,999.9999, -999,999.9999, -999,999.9999'); // ~1 km/axis
    const extreme = await measure('4,999,999.9999, 4,999,999.9999, 4,999,999.9999'); // clipped

    // Fixed reserved width (300px), unchanged by content.
    expect(short.coordsW).toBe(300);
    expect(wide.coordsW).toBe(300);
    expect(extreme.coordsW).toBe(300);

    // Downstream elements never move.
    expect(wide.f1X).toBe(short.f1X);
    expect(extreme.f1X).toBe(short.f1X);
    expect(wide.cmdbarX).toBe(short.cmdbarX);
    expect(extreme.cmdbarX).toBe(short.cmdbarX);
  });
});
