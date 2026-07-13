/**
 * Command indicator (#tool-label) placement + single unit setting.
 *
 * The bottom bar had the unit shown twice: a passive readout (sb-meta
 * "· mm · 4") in #statusbar AND the interactive unit button (cb-unit-btn,
 * "단위 / 정밀도") in #commandbar. Per request the redundant left readout was
 * removed so only the right interactive unit setting remains, and the command
 * indicator (#tool-label — shows the active tool "Rectangle" / view mode
 * "3D Perspective") was moved to sit immediately before that unit setting.
 *
 * This spec proves, on the real production DOM, that:
 *  (1) the left sb-meta readout is gone (single unit setting), and
 *  (2) #tool-label lives in .cb-tools immediately before cb-unit-btn, and
 *  (3) switching tools updates it there (plane → friendly "Work Plane"), and
 *  (4) the right unit button remains present + interactive.
 */
import { test, expect } from '@playwright/test';

test.describe('Command indicator before the unit setting', () => {
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

  test('single unit setting; #tool-label sits before cb-unit-btn and shows friendly names', async ({ page }) => {
    const layout = await page.evaluate(() => {
      const tl = document.getElementById('tool-label')!;
      const cbTools = document.querySelector('.cb-tools')!;
      const unitBtn = document.getElementById('cb-unit-btn')!;
      const cs = getComputedStyle(tl);
      const kids = Array.from(cbTools.children);
      const tlR = tl.getBoundingClientRect();
      const ubR = unitBtn.getBoundingClientRect();
      return {
        sbMetaRemoved: document.getElementById('sb-meta') === null,
        inCbTools: cbTools.contains(tl),
        position: cs.position,
        beforeUnitBtn: kids.indexOf(tl) >= 0 && kids.indexOf(tl) < kids.indexOf(unitBtn),
        toLeftOfUnitBtn: tlR.x < ubR.x,
        unitBtnInteractive: getComputedStyle(unitBtn).cursor === 'pointer',
        text: tl.textContent,
      };
    });
    expect(layout.sbMetaRemoved).toBe(true);      // redundant left readout gone
    expect(layout.inCbTools).toBe(true);
    expect(layout.position).toBe('static');       // not an absolute floating overlay
    expect(layout.beforeUnitBtn).toBe(true);      // before the unit setting (DOM order)
    expect(layout.toLeftOfUnitBtn).toBe(true);    // before it visually too
    expect(layout.unitBtnInteractive).toBe(true); // right unit setting still usable
    expect(layout.text).toBe('Select');

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
  });
});
