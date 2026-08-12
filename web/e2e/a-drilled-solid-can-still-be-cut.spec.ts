/**
 * Drill a hole, then try to cut the thing.
 *
 * Before this, you could not. `adopt_new_active_faces` added the faces a carve
 * created and never dropped the ones it killed, so each drill left two dead ids
 * (its re-derived host caps) in the owner's list — and Slice sends that list
 * verbatim. `slice.rs` aborts on the first id it cannot find:
 *
 * ```text
 *   clean box     → {"ok":true,"newXia":2}
 *   drilled box   → {"ok":false,"error":"slice: face FaceId(4) not found"}
 * ```
 *
 * They also accumulated — 6/6 → 24/22 → 42/38 → 60/54 over three bores — and
 * rode into every `.axia` file.
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

test.describe('a drilled solid can still be cut', () => {
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

  test('the owner names only faces that exist, and Slice accepts the list', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      const xia = bridge.getXiaForFace(1);
      const tube = bridge.drillThroughHole([100, 0, 0], [1, 0, 0], 40, 32);

      const listed = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
      const live = listed.filter((f) => {
        try {
          return (bridge.getFaceVertices(f)?.length ?? 0) > 0;
        } catch {
          return false;
        }
      });

      // Exactly what SliceTool sends: the owner's list, unfiltered. Cut clear of
      // the bore, which spans z in [-40, 40].
      const slice = JSON.parse(
        bridge.engine.sliceVolumeByPlane(new Uint32Array(listed), 0, 0, 70, 0, 0, 1),
      );
      return {
        tube,
        listed: listed.length,
        live: live.length,
        slice,
        valid: bridge.verifyInvariants().valid,
      };
    });

    expect(got.tube).toBeGreaterThan(0);
    expect(got.listed).toBe(got.live); // no dead ids left behind
    expect(got.slice.ok).toBe(true);
    expect(got.valid).toBe(true);
  });

  test('bore after bore does not accumulate dead ids', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      const xia = bridge.getXiaForFace(1);
      const rows: { listed: number; live: number }[] = [];
      for (const z of [-60, 0, 60]) {
        bridge.drillThroughHole([100, 0, z], [1, 0, 0], 15, 16);
        const listed = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
        const live = listed.filter((f) => {
          try {
            return (bridge.getFaceVertices(f)?.length ?? 0) > 0;
          } catch {
            return false;
          }
        });
        rows.push({ listed: listed.length, live: live.length });
      }
      return rows;
    });

    expect(got).toHaveLength(3);
    for (const [i, row] of got.entries()) {
      expect(row.listed, `after bore ${i + 1}`).toBe(row.live);
      expect(row.live).toBeGreaterThan(6); // the bores really did cut
    }
  });

  test('a clean solid is unaffected', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      const xia = bridge.getXiaForFace(1);
      const listed = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
      const slice = JSON.parse(
        bridge.engine.sliceVolumeByPlane(new Uint32Array(listed), 0, 0, 0, 0, 0, 1),
      );
      return { listed: listed.length, slice };
    });

    expect(got.listed).toBe(6);
    expect(got.slice.ok).toBe(true);
  });
});
