/**
 * Exporting IFC when one element has nothing left to draw.
 *
 * `export_ifc_model` skips an element with no faces — but it tested the RAW list,
 * which still names faces the mesh no longer has. So an owner whose faces had all
 * died looked non-empty, was pushed as an element, emitted no geometry, and that
 * error propagated: `.unwrap_or_default()` turned the whole export into an empty
 * string and the bridge returned null.
 *
 * Measured 2026-08-11 — split a box's top face, merge it back, and you could no
 * longer export IFC at all:
 *
 * ```text
 *   after the split   Xia 6/6 live, Shape 2/2 live   → 2 elements
 *   after the merge   Xia 6/5 live, Shape 2/0 live   → null, no file
 * ```
 *
 * Every geometry emitter downstream already re-derives from `mesh.faces` filtered
 * by `is_active`, so dropping the dead here changes nothing about what is drawn —
 * only which elements are worth writing.
 *
 * Playwright serves the production bundle, so re-run `npm run build` after any
 * WASM rebuild or this reads a stale engine.
 */
import { test, expect } from '@playwright/test';

const ELEMENT_RE =
  /#\d+=IFC(WALL|SLAB|COLUMN|BEAM|BUILDINGELEMENTPROXY)\('[^']*',#\d+,'([^']*)'/g;

test.describe('one dead element does not take the file down', () => {
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

  test('a merge that empties an owner still exports a file', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const live = (f: number) => {
        try {
          return (bridge.getFaceVertices(f)?.length ?? 0) > 0;
        } catch {
          return false;
        }
      };
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      // Split the top face, which makes a Shape of the two halves.
      bridge.drawLineAsShape(-100, 0, 100, 100, 0, 100);
      const beforeIfc = bridge.exportIfcModel('probe') as string | null;

      // Merge them back. Both operands die and one new face is born.
      const merged = bridge.engine.mergeFacesByEdge(16);

      // Find an owner the merge emptied — that is the condition under test.
      const shapes = Array.from(bridge.getShapeIds?.() ?? []) as number[];
      const emptied = shapes
        .map((s) => {
          const ids = Array.from(bridge.getShapeFaceIds(s) ?? []) as number[];
          return { listed: ids.length, live: ids.filter(live).length };
        })
        .filter((r) => r.listed > 0 && r.live === 0);

      return {
        merged,
        beforeIfc,
        emptied,
        afterIfc: bridge.exportIfcModel('probe') as string | null,
      };
    });

    expect(got.merged).toBeGreaterThanOrEqual(0);
    expect(got.beforeIfc).not.toBeNull();
    // The condition this exists for: an owner that lists faces and has none live.
    expect(got.emptied.length, 'the merge should have emptied an owner').toBeGreaterThan(0);
    // And the file still comes out.
    expect(got.afterIfc, 'IFC export returned null after the merge').not.toBeNull();
    const names = [...(got.afterIfc ?? '').matchAll(ELEMENT_RE)].map((m) => `${m[1]}:${m[2]}`);
    expect(names).toContain('WALL:Box');
  });

  test('a clean model is unchanged', async ({ page }) => {
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      return bridge.exportIfcModel('probe') as string | null;
    });

    expect(got).not.toBeNull();
    const names = [...(got ?? '').matchAll(ELEMENT_RE)].map((m) => `${m[1]}:${m[2]}`);
    expect(names).toEqual(['WALL:Box']);
  });

  test('a drilled solid still exports as one element', async ({ page }) => {
    // The #115/#117 result must survive this filter: live faces are still claimed.
    const got = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      bridge.engine.create_box(0, 0, 0, 200, 200, 200);
      bridge.drillThroughHole([100, 0, 0], [1, 0, 0], 40, 32);
      return bridge.exportIfcModel('probe') as string | null;
    });

    expect(got).not.toBeNull();
    const names = [...(got ?? '').matchAll(ELEMENT_RE)].map((m) => `${m[1]}:${m[2]}`);
    expect(names).toEqual(['WALL:Box']);
  });
});
