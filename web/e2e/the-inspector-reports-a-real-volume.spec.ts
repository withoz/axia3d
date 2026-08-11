/**
 * The number the Inspector shows for a solid someone just drew.
 *
 * `getXiaInfo` — what the XIA Inspector reads — used to fan each face's OUTER
 * loop in the WASM layer, a third copy of a sum the engine already had and the
 * only one that never learned what the other two did. A fan needs three
 * vertices and a Path B face has ONE anchor (ADR-089), so every kernel-native
 * primitive contributed nothing. Path B is the production default
 * (LOCKED #47-#49), so drawing a sphere and opening the Inspector showed
 * **volume 0**. The same loop never took a hole out either.
 *
 * A source guard in `crates/axia-wasm/tests/step6_additive_only.rs` holds the
 * wiring; it cannot hold the number. This does — real Chromium, production
 * build, compiled WASM, through the same bridge call the panel makes.
 *
 * Playwright serves `npm run preview` (the production bundle), so re-run
 * `npm run build` after any WASM rebuild or this spec reads a stale engine.
 */
import { test, expect } from '@playwright/test';

const R = 50;
const TRUTH = (4 / 3) * Math.PI * R ** 3;

test.describe('the Inspector reports a real volume', () => {
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

  test('a Path B sphere is not reported as empty', async ({ page }) => {
    const got = await page.evaluate(
      ({ r }) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const bridge = (window as any).__axia.get('bridge');
        bridge.setSpherePathBDefault(true);
        const eng = bridge.engine;
        eng.create_sphere(0, 0, 0, r, 32, 32);

        // The faces of everything on screen — which is the sphere.
        const buf = bridge.getMeshBuffers();
        const faces = Array.from(new Set(Array.from(buf.faceMap as Uint32Array)));
        const info = bridge.getXiaInfo(faces);
        return {
          faces: faces.length,
          volume: info ? info.volume : null,
          area: info ? info.surfaceArea : null,
          meshVolume: eng.meshVolume(),
        };
      },
      { r: R },
    );

    expect(got.faces).toBeGreaterThan(0);
    // The reason this spec exists: this used to be exactly 0.
    expect(got.volume).toBeGreaterThan(0);
    expect(Math.abs(got.volume! / TRUTH - 1)).toBeLessThan(0.01);
    // And it must be the same reader `meshVolume` uses, not a second opinion.
    expect(Math.abs(got.volume! - Math.abs(got.meshVolume))).toBeLessThan(1e-6);
    expect(got.area!).toBeGreaterThan(0);
  });

  test('a drilled hole is taken out of the reported volume', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const eng = bridge.engine;
      eng.create_box(0, 0, 0, 200, 200, 200);
      const solid = bridge.getXiaInfo(
        Array.from(new Set(Array.from(bridge.getMeshBuffers().faceMap as Uint32Array))),
      );
      bridge.drillThroughHole([0, 0, 100], [0, 0, 1], 40, 32);
      const bored = bridge.getXiaInfo(
        Array.from(new Set(Array.from(bridge.getMeshBuffers().faceMap as Uint32Array))),
      );
      return { solid: solid?.volume ?? null, bored: bored?.volume ?? null };
    });

    expect(got.solid).not.toBeNull();
    expect(Math.abs(got.solid! - 8.0e6)).toBeLessThan(1);
    // A Ø80 bore through 200 mm removes ~1.0e6; the old reader removed ~0.67e6
    // of it and left the rest, reading 7,334,091 where the engine said 6,996,839.
    expect(got.bored!).toBeLessThan(7.05e6);
    expect(got.bored!).toBeGreaterThan(6.9e6);
  });
});
