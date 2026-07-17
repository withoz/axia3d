/**
 * Command indicator (#tool-label) placement + single unit setting.
 *
 * The bottom bar had the unit shown twice: a passive readout (sb-meta
 * "· mm · 4") in #statusbar AND the interactive unit button (cb-unit-btn,
 * "단위 / 정밀도") in #commandbar. Per request the redundant left readout was
 * removed so only the right interactive unit setting remains, and the command
 * indicator (#tool-label — shows the active tool 「사각형」 / view mode
 * 「3D 뷰」) was moved to sit immediately before that unit setting.
 *
 * This spec proves, on the real production DOM, that:
 *  (1) the left sb-meta readout is gone (single unit setting), and
 *  (2) #tool-label lives in .cb-tools immediately before cb-unit-btn, and
 *  (3) switching tools updates it there (plane → friendly 「작업 평면」), and
 *  (4) the right unit button remains present + interactive.
 *
 * The names are Korean since ADR-294 batch 13 (they were hard-coded English,
 * which a Korean user saw in the status bar). playwright.config pins ko-KR,
 * so this runs the app the user runs rather than the runner's en-US default.
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
    // Korean since ADR-294 batch 13: the tool names were hard-coded English,
    // so a Korean user clicked 「사각형」 and the status bar said "Rectangle".
    // This spec asserted that English and passed — the runner's locale was
    // en-US, so it never saw what the user saw. playwright.config now pins
    // ko-KR, which is why these read as they do.
    expect(layout.text).toBe('선택');

    const clickTool = async (id: string) => {
      await page.evaluate((toolId) => {
        const b = document.querySelector(`.tool-btn[data-tool="${toolId}"]`) as HTMLElement | null;
        b?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      }, id);
      return page.evaluate(() => document.getElementById('tool-label')!.textContent);
    };

    expect(await clickTool('rect')).toBe('사각형');
    expect(await clickTool('plane')).toBe('작업 평면'); // was raw "plane"
    expect(await clickTool('circle')).toBe('원');
  });
});
