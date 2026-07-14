/**
 * Cmd-K palette commands actually dispatch (menu-backed commands work).
 *
 * The palette's `action()` commands routed through executeAction, which is
 * silent for ids it doesn't handle — so menu-only commands (panels, imports,
 * view modes) were no-ops from the palette. They now route through a
 * #menubar-scoped `dispatchMenuAction`, so they open their panels; and the
 * bottom-bar-audit batch added the panel/import ids to the palette.
 *
 * Proven on the real production DOM by opening the palette, searching, and
 * pressing Enter — the target panel opens.
 */
import { test, expect } from '@playwright/test';

test.describe('Cmd-K palette dispatch', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        return w.__axia && typeof w.__axia.get === 'function' && w.__axia.get('bridge');
      },
      { timeout: 30000 },
    );
  });

  const runCommand = async (page: import('@playwright/test').Page, query: string) => {
    await page.keyboard.press('Control+k');
    const input = page.locator('.cmd-palette-input');
    await expect(input).toBeVisible();
    await input.fill(query);
    await page.keyboard.press('Enter');
    await expect(page.locator('#cmd-palette-root')).toHaveCount(0); // palette closed
  };

  test('a previously-broken menu command (장면 패널) opens its panel via the palette', async ({ page }) => {
    const scenes = page.locator('#scenes-panel');
    await expect(scenes).toBeHidden();
    await runCommand(page, '장면');
    await expect(scenes).toBeVisible();
  });

  test('a newly-cataloged command (Capability Explorer) opens its panel via the palette', async ({ page }) => {
    const cap = page.locator('#capability-explorer');
    await expect(cap).toBeHidden();
    await runCommand(page, 'Capability');
    await expect(cap).toBeVisible();
  });
});
