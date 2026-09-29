/**
 * A material pick moves its owner — when it leaves no doubt (ADR-313 D5).
 *
 * The Inspector promised *"재질을 부여하면 이 객체는 XIA (특성)로 승격됩니다"*.
 * Measured before the change, in this browser against the real engine:
 *
 * ```text
 *   all six faces of a box on 콘크리트   badge "XIA (특성)"   getXiaForFace -1 on all six
 *   a XIA made by hand, material "없음"  toast "재질 제거 시 1건 강등 실패"   the XIA stayed
 * ```
 *
 * Nothing in the app promoted, and the badge read the app's own material
 * state; ADR-091's demotion was refused because the removal cleared only the
 * faces. The engine now moves the owner in the pick's own undo step, and the
 * badge asks the engine.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 * ⚠ Mutation-checked: let the badge read the app's material state again and
 * the sheet and the one-face tests fail on the badge; stop reporting a refusal
 * and the sheet test finds no reason; stop reporting a demotion and there is
 * no 되돌리기 to press.
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

/** A 1000×1000 sheet, optionally extruded into a box; returns its faces. */
async function draw(page: Page, extrude: boolean): Promise<number[]> {
  return page.evaluate((extrude) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const ax = (window as any).__axia;
    const bridge = ax.get('bridge');
    const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 1000, 1000);
    if (extrude) bridge.createSolidExtrude(bridge.getShapeFaceIds(shape)[0], 500);
    ax.get('syncMesh')();
    return bridge.getShapeFaceIds(shape);
  }, extrude);
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

async function look(page: Page, faces: number[]) {
  return page.evaluate((faces) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const bridge = (window as any).__axia.get('bridge');
    return {
      xias: faces.map((f) => bridge.getXiaForFace(f) as number),
      materials: faces.map((f) => bridge.getFaceMaterial(f) as number),
      badge: document.getElementById('xi-phys-badge')?.textContent ?? null,
      toast: document.getElementById('axia-toast-container')?.innerText ?? '',
    };
  }, faces);
}

test.describe('a material pick moves its owner', () => {
  test('a box painted whole becomes a XIA, and the badge says so', async ({ page }) => {
    await boot(page);
    const faces = await draw(page, true);
    expect(faces.length, 'a box').toBe(6);

    await pick(page, faces, 'concrete');
    const s = await look(page, faces);
    expect(s.xias.every((x) => x >= 0), `every face belongs to a XIA: ${s.xias}`).toBe(true);
    expect(new Set(s.xias).size, 'one XIA').toBe(1);
    expect(s.badge).toBe('XIA (특성)');
  });

  test('a sheet keeps its material, stays a Shape, and says why', async ({ page }) => {
    await boot(page);
    const faces = await draw(page, false);

    await pick(page, faces, 'concrete');
    const s = await look(page, faces);
    expect(s.materials, 'the face keeps the material').toEqual([1]);
    expect(s.xias, 'no XIA — a sheet encloses nothing').toEqual([-1]);
    expect(s.badge, 'and the badge does not claim one').toBe('형태 (Shape)');
    expect(s.toast, 'the reason is shown').toContain('닫힌 입체가 아니라');
  });

  test('one face painted leaves the box a Shape', async ({ page }) => {
    await boot(page);
    const faces = await draw(page, true);

    await pick(page, [faces[0]], 'brick');
    const s = await look(page, [faces[0]]);
    expect(s.materials).toEqual([5]);
    expect(s.xias).toEqual([-1]);
    expect(s.badge).toBe('형태 (Shape)');
  });

  test('taking the material off a XIA demotes it, and 되돌리기 gives both back', async ({ page }) => {
    await boot(page);
    const faces = await draw(page, true);
    await pick(page, faces, 'concrete');
    const before = await look(page, faces);
    // PREMISE: there is a XIA to take the material off.
    expect(before.xias.every((x) => x >= 0), 'the box is a XIA first').toBe(true);

    await pick(page, faces, '');
    const off = await look(page, faces);
    expect(off.xias.every((x) => x === -1), `no XIA after the removal: ${off.xias}`).toBe(true);
    expect(off.materials.every((m) => m === 0)).toBe(true);
    expect(off.toast).toContain('재질 제거됨 — 형태로 강등');
    expect(off.badge).toBe('형태 (Shape)');

    const pressed = await page.evaluate(() => {
      const container = document.getElementById('axia-toast-container');
      const button = Array.from(container?.querySelectorAll('button') ?? []).find(
        (b) => b.textContent?.includes('되돌리기'),
      );
      button?.click();
      return button !== undefined;
    });
    expect(pressed, 'the toast offers 되돌리기').toBe(true);

    const back = await look(page, faces);
    expect(back.xias, 'the XIA is back').toEqual(before.xias);
    expect(back.materials.every((m) => m === 1), 'with its material').toBe(true);
    expect(back.badge).toBe('XIA (특성)');
  });
});
