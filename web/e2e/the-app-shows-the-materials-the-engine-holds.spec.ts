/**
 * The app shows the materials the engine holds (ADR-313 D3).
 *
 * The viewport colours a face from the app's material table, and until ADR-313
 * nothing refilled that table from the engine. Measured before the change, in
 * this same browser against the same engine:
 *
 * ```text
 *   brick box, saved, reloaded, opened   viewport #e8e8e8   engine 벽돌   Inspector ""
 *   brick member imported from IFC       viewport #e8e8e8   engine 벽돌   Inspector ""
 * ```
 *
 * The engine kept every material; the app just never asked. Each test here
 * reads the colour the renderer actually draws with — the colour attribute of
 * the mesh — not the app's table, which is what was wrong.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 * ⚠ Mutation-checked: remove the `syncFromEngine()` call from `syncMesh` and
 * every test fails; stop the Inspector adding options for materials it learns
 * of later and "a material only the engine knows" fails on the name.
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

/** The colour the renderer draws a face with, as '#rrggbb'. */
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

/** Pick a material in the Inspector for the given faces, the way a user does. */
async function pickInInspector(page: Page, faces: number[], materialId: string) {
  await page.evaluate(
    ({ faces, materialId }) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      ax.get('selection').selectFaces(faces);
      const sel = document.getElementById('xi-material') as HTMLSelectElement;
      sel.value = materialId;
      sel.dispatchEvent(new Event('change'));
    },
    { faces, materialId },
  );
}

async function inspectorShows(page: Page, faces: number[]): Promise<string> {
  return page.evaluate((faces) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__axia.get('selection').selectFaces(faces);
    return (document.getElementById('xi-material') as HTMLSelectElement).value;
  }, faces);
}

test.describe('the app shows the materials the engine holds', () => {
  test('a reopened project keeps its materials on screen', async ({ page }, info) => {
    await boot(page);
    const faces: number[] = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
      bridge.createSolidExtrude(bridge.getShapeFaceIds(shape)[0], 500);
      ax.get('syncMesh')();
      return bridge.getShapeFaceIds(shape);
    });
    expect(faces.length, 'a box to paint').toBe(6);
    await pickInInspector(page, faces, 'brick');
    const brick = await drawnColour(page, faces[0]);
    // PREMISE: the paint took — otherwise "it came back" proves nothing.
    expect(brick, 'the box must be drawn brick before it is saved').not.toBe('#e8e8e8');

    const download = page.waitForEvent('download');
    await page.evaluate(() =>
      document
        .querySelector('[data-action="file-save"]')
        ?.dispatchEvent(new MouseEvent('click', { bubbles: true })),
    );
    const path = info.outputPath('brick.xia');
    await (await download).saveAs(path);

    await boot(page); // a reload is a fresh app
    const chooser = page.waitForEvent('filechooser');
    await page.evaluate(() =>
      document
        .querySelector('[data-action="file-open"]')
        ?.dispatchEvent(new MouseEvent('click', { bubbles: true })),
    );
    await (await chooser).setFiles(path);
    await page.waitForFunction(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => ((window as any).__axia.get('viewport').faceMap?.length ?? 0) > 0,
      { timeout: 15000 },
    );

    const engineId = await page.evaluate(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (f) => (window as any).__axia.get('bridge').getFaceMaterial(f),
      faces[0],
    );
    expect(engineId, 'the engine kept it (it always did)').toBe(5);
    expect(await drawnColour(page, faces[0]), 'and now the screen does too').toBe(brick);
    expect(await inspectorShows(page, faces), 'and the Inspector names it').toBe('brick');
  });

  test('an imported IFC member shows its material', async ({ page }) => {
    await boot(page);
    // A brick member, the colour the app draws brick with, and the file.
    const made = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const brickId = JSON.parse(bridge.getAllMaterials()).find(
        (m: { name: string }) => m.name === '벽돌',
      ).id;
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
      bridge.createSolidExtrude(bridge.getShapeFaceIds(shape)[0], 500);
      const faces: number[] = bridge.getShapeFaceIds(shape);
      bridge.assignMaterial(new Uint32Array(faces), brickId);
      bridge.promoteShapeToXia(shape, brickId);
      return { brickId, ifc: bridge.exportIfcModel('m') as string };
    });
    expect(made.ifc, 'the file must name the material').toContain('IFCMATERIAL(');

    await boot(page);
    const faces: number[] = await page.evaluate((text) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      ax.get('bridge').importIfc(text);
      ax.get('syncMesh')();
      return Array.from(new Set(Array.from(ax.get('viewport').faceMap as Uint32Array)));
    }, made.ifc);
    expect(faces.length, 'the member came in').toBeGreaterThan(0);

    const engineIds = await page.evaluate(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (fs) => fs.map((f) => (window as any).__axia.get('bridge').getFaceMaterial(f)),
      faces,
    );
    expect(new Set(engineIds), 'the engine holds 벽돌 on every face').toEqual(new Set([made.brickId]));
    // 0xc45a3a is the app's 벽돌 (web/src/materials/MaterialLibrary.ts).
    for (const f of faces) {
      expect(await drawnColour(page, f), `face ${f} must be drawn brick`).toBe('#c45a3a');
    }
    expect(await inspectorShows(page, faces), 'and the Inspector names it').toBe('brick');
  });

  test('a material only the engine knows is drawn and named', async ({ page }) => {
    // A name that is not one of the app's twelve, so the import creates it in
    // the engine (ADR-311) and the app has no entry for it until it mirrors one.
    await boot(page);
    const ifc: string = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');
      const red = bridge.addProjectMaterial('적벽돌', 'Red brick', 0xaa3322);
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
      bridge.createSolidExtrude(bridge.getShapeFaceIds(shape)[0], 500);
      bridge.assignMaterial(new Uint32Array(bridge.getShapeFaceIds(shape)), red);
      bridge.promoteShapeToXia(shape, red);
      return bridge.exportIfcModel('m');
    });

    await boot(page);
    const r = await page.evaluate((text) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      bridge.importIfc(text);
      ax.get('syncMesh')();
      const faces = Array.from(new Set(Array.from(ax.get('viewport').faceMap as Uint32Array)));
      const id = bridge.getFaceMaterial(faces[0]);
      const listed = JSON.parse(bridge.getAllMaterials()).find((m: { id: number }) => m.id === id);
      return { faces, id, name: listed?.name ?? null, colour: listed?.color ?? null };
    }, ifc);
    // PREMISE: the engine holds a material the app has no built-in for.
    expect(r.name, 'the import kept the name').toBe('적벽돌');
    expect(r.id, 'and it is not one of the twelve').toBeGreaterThan(12);
    expect(r.colour, 'with a colour of its own').not.toBeNull();

    for (const f of r.faces) {
      expect(await drawnColour(page, f), `face ${f} is drawn in the engine's colour`).toBe(r.colour);
    }
    expect(await inspectorShows(page, r.faces), 'and the Inspector names it').toBe(`engine-${r.id}`);
  });

  test('both halves of a split face keep its material', async ({ page }) => {
    await boot(page);
    const sheet: number[] = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
      ax.get('syncMesh')();
      return bridge.getShapeFaceIds(shape);
    });
    await pickInInspector(page, sheet, 'concrete');
    const concrete = await drawnColour(page, sheet[0]);
    expect(concrete, 'the sheet must be drawn concrete before it is split').not.toBe('#e8e8e8');

    const halves: number[] = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      ax.get('bridge').drawLineAsShape(0, -600, 0, 0, 600, 0);
      ax.get('syncMesh')();
      return Array.from(new Set(Array.from(ax.get('viewport').faceMap as Uint32Array)));
    });
    // PREMISE: the line divided the sheet, or there is nothing to test.
    expect(halves.length, 'the line must split the sheet in two').toBe(2);
    for (const f of halves) {
      expect(await drawnColour(page, f), `half ${f} must still be concrete`).toBe(concrete);
    }
  });
});
