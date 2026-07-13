/**
 * Panel resize — Inspector + Style floating panels are resizable via the SE
 * grip (and persist across reload).
 *
 * The resize handles were CSS-scaffolded in DraggablePanels.css
 * ([data-panel-resize="se"|"e"|"s"]) but never wired: no handle elements were
 * injected into the DOM and no drag-to-resize logic existed. This spec proves,
 * through a REAL mouse drag on the real (production-build) DOM, that:
 *  (1) dragging the SE grip grows the panel toward bottom-right, and
 *  (2) the new size is clamped to the panel's SizeConstraints + safe viewport, and
 *  (3) the size persists across a page reload (localStorage 'axia-panel-layout').
 *
 * The panels are plain HTML DOM (not the WebGL canvas), so their layout is
 * fully available to Playwright's real mouse.
 */
import { test, expect } from '@playwright/test';

test.describe('Floating panel resize', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        return w.__axia && typeof w.__axia.get === 'function' && w.__axia.get('panelManager');
      },
      { timeout: 30000 },
    );
  });

  test('SE grip drag resizes the Inspector panel and persists across reload', async ({ page }) => {
    // Open the panel and move it somewhere with room to grow toward bottom-right.
    const start = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const el = document.getElementById('xia-inspector')!;
      el.classList.add('open');
      const pm = w.__axia.get('panelManager');
      const p = pm.panels.get('xia-inspector');
      p.floatingRect.x = 140;
      p.floatingRect.y = 120;
      el.style.left = '140px';
      el.style.top = '120px';
      const r = el.getBoundingClientRect();
      return { w: r.width, h: r.height };
    });

    const handle = page.locator('#xia-inspector [data-panel-resize="se"]');
    await expect(handle).toBeVisible();
    const box = await handle.boundingBox();
    if (!box) throw new Error('no SE handle bounding box');

    // Real mouse drag: grab the SE grip and pull it +90 / +70.
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 90, box.y + box.height / 2 + 70, { steps: 10 });
    await page.mouse.up();

    const after = await page.evaluate(() => {
      const r = document.getElementById('xia-inspector')!.getBoundingClientRect();
      return { w: r.width, h: r.height };
    });

    // Grew toward bottom-right (allow clamping slack: at least +50 of the +90/+70).
    expect(after.w).toBeGreaterThan(start.w + 50);
    expect(after.h).toBeGreaterThan(start.h + 50);
    // Clamped to the Inspector's maxWidth (600) / maxHeight (1000).
    expect(after.w).toBeLessThanOrEqual(600);
    expect(after.h).toBeLessThanOrEqual(1000);

    // Persist check: reload and confirm the resized dimensions were restored.
    await page.reload();
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        return w.__axia && typeof w.__axia.get === 'function' && w.__axia.get('panelManager');
      },
      { timeout: 30000 },
    );
    const restored = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const p = w.__axia.get('panelManager').panels.get('xia-inspector');
      return { w: p.floatingRect.width, h: p.floatingRect.height };
    });
    expect(Math.abs(restored.w - after.w)).toBeLessThan(6);
    expect(Math.abs(restored.h - after.h)).toBeLessThan(6);
  });

  test('SE grip drag resizes the Style panel', async ({ page }) => {
    const start = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const el = document.getElementById('style-panel')!;
      el.classList.add('open');
      const pm = w.__axia.get('panelManager');
      const p = pm.panels.get('style-panel');
      p.floatingRect.x = 140;
      p.floatingRect.y = 120;
      el.style.left = '140px';
      el.style.top = '120px';
      const r = el.getBoundingClientRect();
      return { w: r.width, h: r.height };
    });

    const handle = page.locator('#style-panel [data-panel-resize="se"]');
    await expect(handle).toBeVisible();
    const box = await handle.boundingBox();
    if (!box) throw new Error('no SE handle bounding box');

    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 90, box.y + box.height / 2 + 60, { steps: 10 });
    await page.mouse.up();

    const after = await page.evaluate(() => {
      const r = document.getElementById('style-panel')!.getBoundingClientRect();
      return { w: r.width, h: r.height };
    });
    expect(after.w).toBeGreaterThan(start.w + 50);
    expect(after.h).toBeGreaterThan(start.h + 40);
    expect(after.w).toBeLessThanOrEqual(600);
    expect(after.h).toBeLessThanOrEqual(800);
  });
});
