/**
 * The size the Inspector quotes for a curved thing.
 *
 * Area and volume already read a curved face from its SURFACE — the bounding
 * box, the third reader, still read the boundary vertices, and a Path B
 * primitive barely has any. Measured in this harness, with area and volume
 * exact right beside them:
 *
 * ```text
 *   sphere   r=10        20 x 0 x 0     should be 20 x 20 x 20
 *   cylinder r=10 h=20   20 x 0 x 0     should be 20 x 20 x 20
 *   cone, torus           0 x 0 x 0
 * ```
 *
 * Path B is the production default, so this was every sphere, cylinder, cone
 * and torus a user makes.
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

test.describe('the Inspector reports a real size', () => {
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

  test('every Path B primitive quotes its true extent', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const info = (faceId: number) => {
        const xia = bridge.getXiaForFace(faceId);
        const ids = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
        const j = JSON.parse(bridge.engine.get_xia_info(new Uint32Array(ids)));
        return {
          l: j.length as number,
          w: j.width as number,
          h: j.height as number,
          area: j.surfaceArea as number,
          vol: j.volume as number,
        };
      };
      return {
        sphere: info(bridge.create_sphere(0, 0, 0, 10, 24, 12)),
        cylinder: info(bridge.create_cylinder(100, 0, 0, 10, 20, 32)),
        cone: info(bridge.create_cone(200, 0, 0, 10, 20, 32)),
        torus: info(bridge.create_torus(300, 0, 0, 20, 5)),
      };
    });

    // length/width/height come back sorted largest-first.
    for (const [name, want] of [
      ['sphere', [20, 20, 20]],
      ['cylinder', [20, 20, 20]],
      ['cone', [20, 20, 20]],
      ['torus', [50, 50, 10]],
    ] as const) {
      const d = got[name];
      expect.soft(d.l, `${name} length`).toBeCloseTo(want[0], 2);
      expect.soft(d.w, `${name} width`).toBeCloseTo(want[1], 2);
      expect.soft(d.h, `${name} height`).toBeCloseTo(want[2], 2);
      // Not one of the dimensions may be zero — the collapse this closes.
      expect.soft(Math.min(d.l, d.w, d.h), `${name} smallest`).toBeGreaterThan(0.1);
    }

    // The two readers that were already right must stay right.
    expect(got.sphere.area).toBeCloseTo(4 * Math.PI * 100, 2);
    expect(got.sphere.vol).toBeCloseTo((4 / 3) * Math.PI * 1000, 2);
    expect(got.cylinder.area).toBeCloseTo(2 * Math.PI * 10 * 20 + 2 * Math.PI * 100, 2);
    expect(got.cylinder.vol).toBeCloseTo(Math.PI * 100 * 20, 2);
  });

  test('a box is unchanged — a planar face is still read from its boundary', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.create_box(0, 0, 0, 20, 30, 40);
      // `create_box` hands back a face id that is NOT one of the owned ones —
      // a fresh scene's first box is FaceId 1..6 — so find the owner by asking.
      let xia = -1;
      for (let f = 0; f < 20 && xia < 0; f++) xia = bridge.getXiaForFace(f);
      const ids = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
      const j = JSON.parse(bridge.engine.get_xia_info(new Uint32Array(ids)));
      return { l: j.length as number, w: j.width as number, h: j.height as number, vol: j.volume as number };
    });

    // Sorted largest-first; create_box maps w→X, h→Z, d→Y.
    expect(got.l).toBeCloseTo(40, 6);
    expect(got.w).toBeCloseTo(30, 6);
    expect(got.h).toBeCloseTo(20, 6);
    expect(got.vol).toBeCloseTo(20 * 30 * 40, 3);
  });
});
