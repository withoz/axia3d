/**
 * Command indicator (#tool-label) lives in the status bar, not floating over
 * the viewport.
 *
 * #tool-label used to be a pill floating over the 3D view (position:absolute
 * top:80px/left:12px). It also showed the raw lowercase tool id for tools
 * missing from the ad-hoc name maps (e.g. "plane" instead of "Work Plane").
 * This spec proves, on the real production DOM, that:
 *  (1) #tool-label is a child of #statusbar (moved into the status bar), and
 *  (2) switching tools updates it there, and
 *  (3) the plane tool now shows the friendly "Work Plane" (SSOT names), and
 *  (4) it is laid out inside the status bar (static, not an absolute overlay).
 */
import { test, expect } from '@playwright/test';

test.describe('Command indicator in status bar', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        return w.__axia && typeof w.__axia.get === 'function' && w.__axia.get('toolManager');
      },
      { timeout: 30000 },
    );
  });

  test('#tool-label is inside #statusbar and shows friendly names', async ({ page }) => {
    const layout = await page.evaluate(() => {
      const tl = document.getElementById('tool-label')!;
      const sb = document.getElementById('statusbar')!;
      const cs = getComputedStyle(tl);
      const r = tl.getBoundingClientRect();
      const sbr = sb.getBoundingClientRect();
      const kids = Array.from(sb.children);
      const idx = (id: string) => kids.indexOf(document.getElementById(id) as Element);
      return {
        insideStatusbar: sb.contains(tl),
        position: cs.position,
        within:
          r.x >= sbr.x - 1 && r.right <= sbr.right + 1 &&
          r.y >= sbr.y - 1 && r.bottom <= sbr.bottom + 1,
        text: tl.textContent,
        // Positioned after the coordinates and immediately before the unit
        // setting (sb-meta "· mm · 4").
        afterCoords: idx('sb-coords') < idx('tool-label'),
        beforeUnit: idx('tool-label') < idx('sb-meta'),
      };
    });
    expect(layout.insideStatusbar).toBe(true);
    expect(layout.position).toBe('static'); // not an absolute floating overlay
    expect(layout.within).toBe(true);
    expect(layout.text).toBe('Select');
    expect(layout.afterCoords).toBe(true);
    expect(layout.beforeUnit).toBe(true);

    const clickTool = async (id: string) => {
      await page.evaluate((toolId) => {
        const b = document.querySelector(`.tool-btn[data-tool="${toolId}"]`) as HTMLElement | null;
        b?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      }, id);
      return page.evaluate(() => document.getElementById('tool-label')!.textContent);
    };

    expect(await clickTool('rect')).toBe('Rectangle');
    expect(await clickTool('plane')).toBe('Work Plane'); // was raw "plane"
    expect(await clickTool('circle')).toBe('Circle');
    expect(await clickTool('sphere')).toBe('Sphere');
  });
});
