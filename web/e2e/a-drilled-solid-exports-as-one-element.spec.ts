/**
 * A drilled solid is still one thing.
 *
 * ADR-203 §36 named missing face ownership the real blocker — the new faces
 * "orphan into a separate 'Model' element at IFC export" — and fixed it for the
 * door, the rect drill and the rect punch. The circular, polygon and crossing
 * drills were never promoted to the Scene layer, so the same gesture gave a
 * different answer. Measured in this harness on a 200³ box with a Ø80 bore:
 *
 * ```text
 *   rect drill      → 1 element:  WALL:Box
 *   circular drill  → 2 elements: WALL:Box + WALL:Model      ← the orphan
 * ```
 *
 * The Inspector saw the same gap from the other side: the Xia's own volume read
 * 5,333,333 for a solid whose true volume is 6,994,690, because the face set it
 * summed over was an open surface.
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

const R = 40;
const TRUE_VOLUME = 200 ** 3 - Math.PI * R * R * 200;

/** Element names an IFC file declares, e.g. ["WALL:Box"]. */
const ELEMENT_RE =
  /#\d+=IFC(WALL|SLAB|COLUMN|BEAM|BUILDINGELEMENTPROXY)\('[^']*',#\d+,'([^']*)'/g;

test.describe('a drilled solid exports as one element', () => {
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

  test('a circular bore does not split the wall into an orphan "Model"', async ({ page }) => {
    const got = await page.evaluate(({ r }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      const xia = bridge.getXiaForFace(1);
      const tube = bridge.drillThroughHole([100, 0, 0], [1, 0, 0], r, 32);

      let active = 0;
      let unowned = 0;
      for (let f = 0; f < 400; f++) {
        try {
          if (bridge.getFaceVertices(f)?.length) {
            active++;
            if (bridge.getXiaForFace(f) === -1) unowned++;
          }
        } catch { /* inactive id */ }
      }
      const owned = Array.from(bridge.getXiaFaceIds(xia) ?? []) as number[];
      const info = JSON.parse(bridge.engine.get_xia_info(new Uint32Array(owned)));
      return {
        tube,
        active,
        unowned,
        volume: info.volume as number,
        ifc: bridge.exportIfcModel('probe') as string | null,
        closed: bridge.verifyOutwardNormals().isClosedSolid,
        valid: bridge.verifyInvariants().valid,
      };
    }, { r: R });

    expect(got.tube).toBeGreaterThan(0);
    expect(got.unowned).toBe(0); // every wall of the bore has an owner
    expect(got.closed).toBe(true);
    expect(got.valid).toBe(true);

    // The number the Inspector shows, over the owner's own face set.
    expect(got.volume).toBeCloseTo(TRUE_VOLUME, 0);

    const names = [...(got.ifc ?? '').matchAll(ELEMENT_RE)].map((m) => `${m[1]}:${m[2]}`);
    expect(names).toEqual(['WALL:Box']);
  });

  test('a crossing bore — 128 more walls — is owned too', async ({ page }) => {
    const got = await page.evaluate(({ r }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      bridge.drillThroughHole([0, 0, 100], [0, 0, 1], r, 32);
      const walls = bridge.drillCrossingBore([100, 0, 0], [1, 0, 0], r, 32);

      let active = 0;
      let unowned = 0;
      for (let f = 0; f < 600; f++) {
        try {
          if (bridge.getFaceVertices(f)?.length) {
            active++;
            if (bridge.getXiaForFace(f) === -1) unowned++;
          }
        } catch { /* inactive id */ }
      }
      return {
        walls,
        active,
        unowned,
        ifc: bridge.exportIfcModel('probe') as string | null,
        closed: bridge.verifyOutwardNormals().isClosedSolid,
      };
    }, { r: R });

    expect(got.walls).toBeGreaterThan(0);
    expect(got.active).toBeGreaterThan(100); // the crossing really is the big one
    expect(got.unowned).toBe(0);
    expect(got.closed).toBe(true);

    const names = [...(got.ifc ?? '').matchAll(ELEMENT_RE)].map((m) => `${m[1]}:${m[2]}`);
    expect(names).toEqual(['WALL:Box']);
  });

  test('a polygon bore is owned, and the rect sibling is unchanged', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const census = () => {
        let active = 0;
        let unowned = 0;
        for (let f = 0; f < 400; f++) {
          try {
            if (bridge.getFaceVertices(f)?.length) {
              active++;
              if (bridge.getXiaForFace(f) === -1) unowned++;
            }
          } catch { /* inactive id */ }
        }
        return { active, unowned };
      };
      const loop: [number, number, number][] = [
        [100, -40, -40], [100, 40, -40], [100, 40, 40], [100, -40, 40],
      ];

      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      const poly = bridge.drillPolygonThroughHole(loop, [1, 0, 0]);
      const afterPoly = census();

      // The rect drill already did this before today — it must still.
      bridge.engine.create_box(600, 0, 0, 200, 200, 200);
      const rect = bridge.drillRectThroughHole([700, -40, -40], [700, 40, 40], [1, 0, 0]);
      const afterBoth = census();

      return { poly, afterPoly, rect, afterBoth };
    });

    expect(got.poly).toBeGreaterThan(0);
    expect(got.afterPoly.unowned).toBe(0);
    expect(got.rect).toBeGreaterThan(0);
    expect(got.afterBoth.unowned).toBe(0);
  });
});
