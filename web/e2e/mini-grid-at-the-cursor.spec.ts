/**
 * The cursor's mini work-plane grid, end to end.
 *
 * A draw tool waiting for its first point puts a small red disc of grid lines
 * under the cursor, on the plane the next click would land in. Ported from
 * Kayac's `MiniGridSettings` / `upload_workplane_grid`, whose defaults these are
 * (radius 48 px, 4 cells to the rim, 0.5 px line half-width). It is the only
 * thing drawn at the cursor plane — the fixed-size axis/quad gizmo it replaced
 * is gone.
 *
 * The unit tests cover the geometry and the wiring; what only a browser can show
 * is that a real pointer move over the real canvas produces the thing, that it
 * survives the switch to an axis view, and that the settings panel and a tool
 * switch actually reach it.
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
import type { Page } from '@playwright/test';

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
  found.geometry.computeBoundingBox();
  const bb = found.geometry.boundingBox;
  return {
    present: true,
    segments: pos ? pos.count : 0,
    linewidth: found.material.linewidth,
    depthWrite: found.material.depthWrite,
    size: bb ? Math.max(bb.max.x - bb.min.x, bb.max.y - bb.min.y, bb.max.z - bb.min.z) : 0,
  };
})()`;

/** Is a visible grid in the scene right now? Used to wait out the RAF throttle. */
function gridIsUp(): boolean {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const vp = (window as any).__axia.get('viewport');
  let hit = false;
  vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
    if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) hit = true;
  });
  return hit;
}

function gridIsDown(): boolean {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const vp = (window as any).__axia.get('viewport');
  let hit = false;
  vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
    if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) hit = true;
  });
  return !hit;
}

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

  /** Pick a draw tool and put the pointer on the canvas until the disc appears. */
  async function armAtCentre(page: Page): Promise<void> {
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });
    const box = await page.locator('canvas').first().boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForFunction(gridIsUp, undefined, { timeout: 5000 });
  }

  /** Open Settings the way a user does — the button in the status bar. */
  async function openSettings(page: Page): Promise<void> {
    await page.locator('#settings-btn').click();
  }

  test('a draw tool waiting for its first point puts a disc under the cursor', async ({ page }) => {
    // Nothing before a draw tool is picked.
    const before = await page.evaluate(READ_GRID);
    expect((before as { present: boolean }).present).toBe(false);

    await armAtCentre(page);

    const g = (await page.evaluate(READ_GRID)) as {
      present: boolean; segments: number; linewidth: number; size: number; depthWrite: boolean;
    };
    expect(g.present).toBe(true);
    // 4 cells to the rim: offsets -4..4, the two at |off| == R have no chord, so
    // 7 offsets x 2 directions = 14 segments.
    expect(g.segments).toBe(14);
    // 0.5 is a HALF-width; LineMaterial.linewidth is the full one.
    expect(g.linewidth).toBeCloseTo(1.0, 6);
    expect(g.size).toBeGreaterThan(0);
    // ⚠ An overlay that skips the depth test but still writes depth makes lines
    // behind it vanish. The unit test reads this from source; this reads it off
    // the material the running app actually built.
    expect(g.depthWrite).toBe(false);
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

    const box = await page.locator('canvas').first().boundingBox();
    if (!box) throw new Error('no canvas');
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.waitForFunction(gridIsUp, undefined, { timeout: 5000 });

    const g = (await page.evaluate(READ_GRID)) as { present: boolean; size: number };
    expect(g.present).toBe(true);
    // ⚠ A finite, non-zero disc. Under the perspective formula an ortho camera's
    // depth would give a wrong size here, and a NaN would give none at all.
    expect(Number.isFinite(g.size)).toBe(true);
    expect(g.size).toBeGreaterThan(0);
  });

  /**
   * ⚠ This used to set localStorage and reload, which tested that the preference
   * PERSISTS — not that the checkbox does anything.
   *
   * ⚠ And the first rewrite asserted the disc changed "without moving the mouse",
   * which is a scenario no user is in: measured, clicking the settings button
   * takes the pointer off the canvas, `mouseleave` fires, and the grid is already
   * down before the checkbox is touched. The journey below is the real one —
   * leave the canvas, change the setting, come back.
   */
  test('unticking the settings box stops the disc coming back', async ({ page }) => {
    await armAtCentre(page);

    await openSettings(page);
    const box = page.locator('#sp-mini-grid');
    await expect(box).toBeVisible({ timeout: 5000 });
    await expect(box).toBeChecked();
    await box.uncheck();

    // Back to the canvas, where the disc would otherwise return.
    const canvas = await page.locator('canvas').first().boundingBox();
    if (!canvas) throw new Error('no canvas');
    await page.mouse.move(canvas.x + canvas.width / 2 + 15, canvas.y + canvas.height / 2 + 15);
    await page.waitForTimeout(300);
    expect(await page.evaluate(gridIsUp), 'the disc came back with the setting off').toBe(false);

    // And ticking it brings it back. ⚠ Do not call `openSettings` again — the
    // status-bar button toggles, so a second click closes the panel and the
    // checkbox goes with it. The panel is still open.
    await page.locator('#sp-mini-grid').check();
    await page.mouse.move(canvas.x + canvas.width / 2 - 15, canvas.y + canvas.height / 2 - 15);
    await page.waitForFunction(gridIsUp, undefined, { timeout: 5000 });
  });

  /** The size controls have to reach the disc on the next pass over the canvas. */
  test('the radius slider resizes it', async ({ page }) => {
    await armAtCentre(page);
    const small = (await page.evaluate(READ_GRID)) as { size: number };
    expect(small.size).toBeGreaterThan(0);

    await openSettings(page);
    const slider = page.locator('#sp-mini-grid-radius');
    await expect(slider).toBeVisible({ timeout: 5000 });
    await slider.fill('200');
    await slider.dispatchEvent('input');

    const canvas = await page.locator('canvas').first().boundingBox();
    if (!canvas) throw new Error('no canvas');
    await page.mouse.move(canvas.x + canvas.width / 2 + 10, canvas.y + canvas.height / 2 + 10);
    await page.waitForFunction(gridIsUp, undefined, { timeout: 5000 });

    const big = (await page.evaluate(READ_GRID)) as { size: number };
    // 48 px -> 200 px is a bit over 4x; well clear of the depth difference the
    // 10 px cursor shift makes.
    expect(big.size).toBeGreaterThan(small.size * 2);
  });

  /** A tool switch is not a pointer event, and the disc must still go. */
  test('switching to a non-drawing tool clears it, with the mouse still', async ({ page }) => {
    await armAtCentre(page);
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('select');
    });
    await page.waitForFunction(gridIsDown, undefined, { timeout: 5000 });
  });
});
