/**
 * The cursor's mini work-plane grid, end to end.
 *
 * A draw tool waiting for its first point puts a small red disc of grid lines
 * under the cursor, on the plane the next click would land in. Ported from
 * Kayac's `MiniGridSettings` / `upload_workplane_grid`, whose defaults these are
 * (radius 48 px, 4 cells to the rim, 0.5 px line half-width).
 *
 * The unit tests cover the geometry and the wiring; what only a browser can show
 * is that a real pointer move over the real canvas actually produces the thing,
 * and that it survives the switch to an axis view.
 *
 * ⚠ The axis-view case is the branch Kayac does not have. Its comment says
 * "still perspective projection — true ortho is a separate polish", so its
 * radius formula is `fov_y`-only; our top/front/right views run
 * `OrthographicCamera`, where magnification does not depend on depth at all.
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Only mangling-safe object property names are used —
 * local variable names do not survive the production minify.
 */
import { test, expect } from '@playwright/test';

/** The mini grid is the LineSegments2 the viewport parks at renderOrder 1001. */
const READ_GRID = `(() => {
  const w = window;
  const vp = w.__axia.get('viewport');
  let found = null;
  vp.scene.traverse((o) => {
    if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) found = o;
  });
  if (!found) return { present: false };
  const pos = found.geometry.attributes.instanceStart;
  // Extent of the drawn points, so the apparent size can be compared.
  const box = found.geometry.boundingBox
    ? null
    : (found.geometry.computeBoundingBox(), null);
  const bb = found.geometry.boundingBox;
  return {
    present: true,
    segments: pos ? pos.count : 0,
    linewidth: found.material.linewidth,
    size: bb ? Math.max(bb.max.x - bb.min.x, bb.max.y - bb.min.y, bb.max.z - bb.min.z) : 0,
    ignored: box,
  };
})()`;

test.describe('the cursor mini grid', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('viewport');
      },
      { timeout: 30000 },
    );
  });

  test('a draw tool waiting for its first point puts a disc under the cursor', async ({ page }) => {
    // Nothing before a draw tool is picked.
    const before = await page.evaluate(READ_GRID);
    expect((before as { present: boolean }).present).toBe(false);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });

    const canvas = page.locator('canvas').first();
    const box = await canvas.boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    // The indicator is RAF-throttled, so give it frames rather than a guess.
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const vp = (window as any).__axia.get('viewport');
        let hit = false;
        vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
          if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) hit = true;
        });
        return hit;
      },
      { timeout: 5000 },
    );

    const g = (await page.evaluate(READ_GRID)) as {
      present: boolean; segments: number; linewidth: number; size: number;
    };
    expect(g.present).toBe(true);
    // 4 cells to the rim: offsets -4..4, the two at |off| == R have no chord, so
    // 7 offsets x 2 directions = 14 segments.
    expect(g.segments).toBe(14);
    // 0.5 is a HALF-width; LineMaterial.linewidth is the full one.
    expect(g.linewidth).toBeCloseTo(1.0, 6);
    expect(g.size).toBeGreaterThan(0);
  });

  test('it is still drawn in an axis view, where the camera is orthographic', async ({ page }) => {
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      w.__axia.get('viewport').setViewMode('top');
      w.__axia.get('toolManager').setTool('line');
    });
    // The camera really is the orthographic one now.
    const ortho = await page.evaluate(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => !!((window as any).__axia.get('viewport').activeCamera as {
        isOrthographicCamera?: boolean;
      }).isOrthographicCamera,
    );
    expect(ortho).toBe(true);

    const canvas = page.locator('canvas').first();
    const box = await canvas.boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const vp = (window as any).__axia.get('viewport');
        let hit = false;
        vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
          if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) hit = true;
        });
        return hit;
      },
      { timeout: 5000 },
    );

    const g = (await page.evaluate(READ_GRID)) as { present: boolean; size: number };
    expect(g.present).toBe(true);
    // ⚠ A finite, non-zero disc. Under the perspective formula an ortho camera's
    // depth would give a wrong size here, and a NaN would give none at all.
    expect(Number.isFinite(g.size)).toBe(true);
    expect(g.size).toBeGreaterThan(0);
  });

  test('turning it off in the settings clears it', async ({ page }) => {
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });
    const canvas = page.locator('canvas').first();
    const box = await canvas.boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const vp = (window as any).__axia.get('viewport');
        let hit = false;
        vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
          if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) hit = true;
        });
        return hit;
      },
      { timeout: 5000 },
    );

    // localStorage is the setting's home; a reload is what reads it.
    await page.evaluate(() => localStorage.setItem('axia:mini-grid-visible', 'false'));
    await page.reload();
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('viewport');
      },
      { timeout: 30000 },
    );
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });
    const box2 = await page.locator('canvas').first().boundingBox();
    if (!box2) throw new Error('no canvas');
    await page.mouse.move(box2.x + box2.width / 2 + 20, box2.y + box2.height / 2 + 20);
    await page.waitForTimeout(500);

    const g = (await page.evaluate(READ_GRID)) as { present: boolean };
    expect(g.present).toBe(false);
  });
});
