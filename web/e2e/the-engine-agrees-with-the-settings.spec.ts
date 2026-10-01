/**
 * The engine holds the same four draw flags the app thinks it does.
 *
 * `AutoIntersectSettings`, `AutoFaceSynthesisSettings`, `FaceRederiveSettings`
 * and `FreeformOverlapSettings` each keep their own `current` in TypeScript,
 * default ON, and push it to the engine from `main.ts`:
 *
 * ```ts
 *   if (bridge.isReady()) bridge.setXOnDraw(getX());
 *   onXChange((v) => { if (bridge.isReady()) bridge.setXOnDraw(v); });
 * ```
 *
 * ⚠ The engine's own defaults are OFF (ADR-049 P-5e-α: engine OFF, production
 * ON), so the two sides agree ONLY because that push happens. If `isReady()` is
 * false at that moment the push is skipped silently, and nothing reads back —
 * the app would draw as though auto-intersect were on while the engine had it
 * off, with no symptom but wrong geometry.
 *
 * The four `getXOnDraw` exports exist for exactly this question and had **no
 * caller anywhere** until this file (found by walking the wiring map backwards).
 * Measured 2026-10-01: all four read `true` after a default startup, so the
 * divergence is latent, not live. This is what keeps it latent.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 */
import { test, expect } from '@playwright/test';

const FLAGS = [
  ['axia:auto-intersect-on-draw', 'getAutoIntersectOnDraw'],
  ['axia:auto-face-synthesis-on-draw', 'getAutoFaceSynthesisOnDraw'],
  ['axia:face-rederive-on-draw', 'getFaceRederiveOnDraw'],
  ['axia:freeform-overlap-on-draw', 'getFreeformOverlapOnDraw'],
] as const;

async function readFlags(page: import('@playwright/test').Page) {
  await page.waitForFunction(
    () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = (window as any).__axia;
      return w && typeof w.get === 'function' && w.get('bridge');
    },
    { timeout: 30000 },
  );
  return page.evaluate((names: readonly string[]) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge') as Record<string, () => boolean>;
    const out: Record<string, unknown> = {};
    for (const n of names) out[n] = typeof bridge[n] === 'function' ? bridge[n]() : undefined;
    return out;
  }, FLAGS.map(([, g]) => g));
}

test.describe('the four draw flags', () => {
  test('are ON in the engine after a default startup', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto('/');
    const f = await readFlags(page);
    console.log('  default:', JSON.stringify(f));
    for (const [, getter] of FLAGS) {
      // PREMISE: the getter exists. `undefined` would satisfy neither branch
      // below and is the vacuous case this rules out.
      expect(typeof f[getter], `${getter} must be callable`).toBe('boolean');
      expect(f[getter], `${getter} — production default is ON (the engine's own is OFF)`).toBe(true);
    }
  });

  test('follow an explicit OFF preference all the way into the engine', async ({ page }) => {
    test.setTimeout(60_000);
    await page.addInitScript((keys: readonly string[]) => {
      try {
        for (const k of keys) localStorage.setItem(k, 'false');
      } catch { /* private mode */ }
    }, FLAGS.map(([k]) => k));
    await page.goto('/');
    const f = await readFlags(page);
    console.log('  explicit OFF:', JSON.stringify(f));
    for (const [key, getter] of FLAGS) {
      expect(f[getter], `${getter} — ${key} said 'false' and the engine must agree`).toBe(false);
    }
  });
});
