/**
 * A material pick is one undo step (ADR-313 D5).
 *
 * The engine's material command recorded nothing, so the undo right after a
 * pick went back to the step before it. Measured here before the change,
 * through the Inspector:
 *
 * ```text
 *   an extruded box               6 faces
 *   pick 콘크리트                   6 faces, engine material 1
 *   undo once                     1 face,  engine material 0   ← the extrude went
 * ```
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 * ⚠ Mutation-checked: route the pick back through the plain command
 * (`assignMaterial`) and the first test loses the extrude again; route the
 * removal through `removeMaterial` and the second test's undo lands on the
 * pick before it, leaving no brick.
 */
import { test, expect, type Page } from '@playwright/test';

async function boot(page: Page) {
  await page.goto('/');
  await page.waitForFunction(
    () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = (window as any).__axia;
      return w && typeof w.get === 'function' && w.get('bridge');
    },
    { timeout: 30000 },
  );
}

/** An extruded box, drawn and synced; returns its faces. */
async function aBox(page: Page): Promise<number[]> {
  return page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ax = (window as any).__axia;
    const bridge = ax.get('bridge');
    const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
    bridge.createSolidExtrude(bridge.getShapeFaceIds(shape)[0], 500);
    ax.get('syncMesh')();
    return bridge.getShapeFaceIds(shape);
  });
}

/** Pick a material (or '' for none) in the Inspector, the way a user does. */
async function pick(page: Page, faces: number[], materialId: string) {
  await page.evaluate(
    ({ faces, materialId }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('selection').selectFaces(faces);
      const sel = document.getElementById('xi-material') as HTMLSelectElement;
      sel.value = materialId;
      sel.dispatchEvent(new Event('change'));
    },
    { faces, materialId },
  );
}

async function state(page: Page, face: number) {
  return page.evaluate((f) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    return { faces: bridge.getStats().faces as number, material: bridge.getFaceMaterial(f) as number };
  }, face);
}

async function action(page: Page, name: string) {
  await page.evaluate(
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (n) => (window as any).__axia.get('toolManager').dispatchAction(n),
    name,
  );
}

test.describe('a material pick is one undo step', () => {
  test('one undo takes back the pick, not the extrude before it', async ({ page }) => {
    await boot(page);
    const faces = await aBox(page);
    expect(faces.length, 'a box to paint').toBe(6);

    await pick(page, faces, 'concrete');
    expect(await state(page, faces[0])).toEqual({ faces: 6, material: 1 });

    await action(page, 'undo');
    expect(await state(page, faces[0]), 'the pick is gone and the box stays').toEqual({ faces: 6, material: 0 });

    await action(page, 'redo');
    expect(await state(page, faces[0]), 'and redo puts the pick back').toEqual({ faces: 6, material: 1 });
  });

  test('one undo brings back a material that was taken off', async ({ page }) => {
    await boot(page);
    const faces = await aBox(page);
    await pick(page, faces, 'brick');
    await pick(page, faces, '');
    expect(await state(page, faces[0])).toEqual({ faces: 6, material: 0 });

    await action(page, 'undo');
    expect(await state(page, faces[0]), 'brick is back on the box').toEqual({ faces: 6, material: 5 });
  });
});
