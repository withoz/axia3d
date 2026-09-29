/**
 * A Quick Colour is kept (ADR-313 D4).
 *
 * `assign-quick-color` made a material only the app knew, with an invented id
 * ≥ 10001 — its comment believed the engine stored ids as opaque numbers. The
 * engine refuses an id its library does not hold, so it recorded nothing.
 * Measured before the change, in this browser against this engine:
 *
 * ```text
 *   pick #ff0000             drawn #ff0000   engine 0
 *   draw something, undo     drawn #e8e8e8   (the colour was gone)
 * ```
 *
 * Now the engine creates the colour as a Project material and the app uses the
 * id it returns, so the engine holds the colour on the face, and an undo that
 * reads the faces back leaves it there.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 * ⚠ Mutation-checked: restore the invented id and the engine records 0; drop
 * the reuse of an existing colour and the second test counts two materials.
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

async function drawnColour(page: Page, faceId: number): Promise<string | null> {
  return page.evaluate((f) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const vp = (window as any).__axia.get('viewport');
    const fm: Uint32Array = vp.faceMap;
    const idx: Uint32Array = vp.indexBuffer;
    const col: Float32Array | undefined = vp.colorAttribute?.array;
    if (!fm || !idx || !col) return null;
    for (let tri = 0; tri < fm.length; tri++) {
      if (fm[tri] !== f) continue;
      const v = idx[tri * 3];
      return '#' + [col[v * 3], col[v * 3 + 1], col[v * 3 + 2]]
        .map((x) => Math.round(x * 255).toString(16).padStart(2, '0'))
        .join('');
    }
    return null;
  }, faceId);
}

/** Apply a Quick Colour to `faces` the way a user does: the action, then the picker. */
async function quickColour(page: Page, faces: number[], hex: string) {
  await page.evaluate(
    ({ faces, hex }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      ax.get('selection').selectFaces(faces);
      ax.get('toolManager').dispatchAction('assign-quick-color');
      // The action appends a hidden colour input; other colour inputs already
      // exist in the page (the style panel), so take the one it just added.
      const inputs = document.querySelectorAll('input[type=color]');
      const input = inputs[inputs.length - 1] as HTMLInputElement;
      input.value = hex;
      input.dispatchEvent(new Event('change'));
    },
    { faces, hex },
  );
}

test('a quick colour is recorded by the engine and survives an unrelated undo', async ({ page }) => {
  await boot(page);
  const f = await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ax = (window as any).__axia;
    const bridge = ax.get('bridge');
    const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
    const f = bridge.getShapeFaceIds(shape)[0];
    ax.get('syncMesh')();
    return f as number;
  });
  await quickColour(page, [f], '#ff0000');
  const r = await page.evaluate((f) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    const id = bridge.getFaceMaterial(f);
    const listed = JSON.parse(bridge.getAllMaterials()).find((m: { id: number }) => m.id === id);
    return { f, id, colour: listed?.color ?? null };
  }, f);
  expect(r.id, 'the engine records a real material on the face').toBeGreaterThan(0);
  expect(r.colour, 'and it is the colour that was picked').toBe('#ff0000');
  expect(await drawnColour(page, r.f)).toBe('#ff0000');

  await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ax = (window as any).__axia;
    ax.get('bridge').drawRectAsShape(2000, 0, 0, 0, 0, 1, 1, 0, 0, 100, 100); // unrelated
    ax.get('syncMesh')();
    ax.get('toolManager').dispatchAction('undo');
  });
  expect(await drawnColour(page, r.f), 'an unrelated undo must not take the colour').toBe('#ff0000');
});

test('the same colour twice is one material, not two', async ({ page }) => {
  await boot(page);
  const [a, b] = await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ax = (window as any).__axia;
    const bridge = ax.get('bridge');
    const one = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
    const two = bridge.drawRectAsShape(2000, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
    ax.get('syncMesh')();
    return [bridge.getShapeFaceIds(one)[0], bridge.getShapeFaceIds(two)[0]] as number[];
  });
  await quickColour(page, [a], '#00aa55');
  await quickColour(page, [b], '#00aa55');
  const r = await page.evaluate(({ a, b }) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    return {
      ida: bridge.getFaceMaterial(a),
      idb: bridge.getFaceMaterial(b),
      project: bridge.listMaterialsByTier('Project').length,
    };
  }, { a, b });
  // PREMISE: both faces got a real material, or "one" proves nothing.
  expect(r.ida, 'the first face has the colour').toBeGreaterThan(0);
  expect(r.idb, 'both faces carry the same material').toBe(r.ida);
  expect(r.project, 'the file gains one material, not a copy per click').toBe(1);
});
