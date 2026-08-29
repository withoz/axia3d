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
   * ⚠ The first rewrite then asserted the disc changed "without moving the
   * mouse", which is a scenario no user is in. The second walked to the panel and
   * back, which was real but could not see the control take effect.
   *
   * The disc now STAYS while the pointer is over the settings panel — that is
   * where its own size controls are — so the effect is visible where the user is
   * looking, and that is what these assert.
   */
  test('the disc stays while the pointer is on the settings panel', async ({ page }) => {
    await armAtCentre(page);
    await openSettings(page);
    await expect(page.locator('#sp-mini-grid')).toBeVisible({ timeout: 5000 });

    // ⚠ Before this change the disc was gone by now: leaving the canvas cleared
    // it, so the radius slider had nothing to preview.
    expect(
      await page.evaluate(gridIsUp),
      'the disc vanished on the way to its own controls',
    ).toBe(true);
  });

  test('unticking the box clears it while the panel is still open', async ({ page }) => {
    await armAtCentre(page);
    await openSettings(page);
    const box = page.locator('#sp-mini-grid');
    await expect(box).toBeVisible({ timeout: 5000 });
    await expect(box).toBeChecked();

    await box.uncheck();
    await page.waitForFunction(gridIsDown, undefined, { timeout: 5000 });

    // ⚠ Do not call `openSettings` again — the status-bar button toggles, so a
    // second click closes the panel and takes the checkbox with it.
    await box.check();
    await page.waitForFunction(gridIsUp, undefined, { timeout: 5000 });
  });

  test('the radius slider resizes it live, with the pointer still on the panel', async ({ page }) => {
    await armAtCentre(page);
    const small = (await page.evaluate(READ_GRID)) as { size: number };
    expect(small.size).toBeGreaterThan(0);

    await openSettings(page);
    const slider = page.locator('#sp-mini-grid-radius');
    await expect(slider).toBeVisible({ timeout: 5000 });
    await slider.fill('200');
    await slider.dispatchEvent('input');

    // No trip back to the canvas: the disc is still up at its last cursor spot
    // and redraws from the new radius. 48 px -> 200 px is a bit over 4x.
    await page.waitForFunction(
      (was: number) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const vp = (window as any).__axia.get('viewport');
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        let g: any = null;
        vp.scene.traverse((o: { renderOrder: number; type: string; visible: boolean }) => {
          if (o.renderOrder === 1001 && o.type === 'LineSegments2' && o.visible) g = o;
        });
        if (!g) return false;
        g.geometry.computeBoundingBox();
        const bb = g.geometry.boundingBox;
        return bb.max.x - bb.min.x > was * 2;
      },
      small.size,
      { timeout: 5000 },
    );
  });

  /**
   * The other half of the rule. The panel is an exception, not a licence to
   * leave a stale disc on screen.
   *
   * ⚠ The first attempt keyed on `mouseleave`'s `relatedTarget` being inside the
   * panel, which cannot work: the button that opens the panel lives in the
   * toolbar, so at the moment the pointer leaves the canvas the panel does not
   * exist on screen yet. Open and close are the events that carry the intent.
   */
  test('closing the panel forgets it', async ({ page }) => {
    await armAtCentre(page);
    await openSettings(page);
    await expect(page.locator('#sp-mini-grid')).toBeVisible({ timeout: 5000 });
    expect(await page.evaluate(gridIsUp)).toBe(true);

    await openSettings(page); // the status-bar button toggles
    await page.waitForFunction(gridIsDown, undefined, { timeout: 5000 });
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
