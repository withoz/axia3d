/**
 * Quick unit/precision dropdown on the cb-unit button (bottom-bar UX audit,
 * Category C).
 *
 * The cb-unit button shows "0.0000 mm ▾" — the ▾ chevron implied an inline
 * picker, but the button used to open the full Settings panel, identical to
 * the ⚙ gear right beside it (a false affordance + redundant control). Now
 * the button opens a real inline unit/precision dropdown, and the ⚙ gear
 * remains the full-Settings entry (distinct roles).
 *
 * Proven on the real production DOM:
 *  (1) clicking cb-unit opens #cb-unit-menu with 5 unit options + a precision
 *      select, (2) picking a unit updates cb-unit-lbl and closes the menu,
 *  (3) changing precision reformats cb-unit-val, (4) Esc and outside-click
 *      close it.
 */
import { test, expect } from '@playwright/test';

test.describe('Quick unit/precision dropdown', () => {
  test.beforeEach(async ({ page }) => {
    // Start from the default mm / precision-4 (unit choice persists to localStorage).
    await page.addInitScript(() => {
      try { localStorage.removeItem('axia3d-units'); } catch { /* ignore */ }
    });
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

  test('opens a unit/precision picker; selecting a unit updates the label + closes', async ({ page }) => {
    const lbl = page.locator('#cb-unit-lbl');
    await expect(lbl).toHaveText('mm');

    await page.locator('#cb-unit-btn').click();
    const menu = page.locator('#cb-unit-menu');
    await expect(menu).toBeVisible();
    await expect(menu.locator('[data-unit]')).toHaveCount(5); // mm/cm/m/in/ft
    await expect(menu.locator('#cb-unit-precision')).toBeVisible();

    // Pick cm → label updates, menu closes.
    await menu.locator('[data-unit="cm"]').click();
    await expect(lbl).toHaveText('cm');
    await expect(page.locator('#cb-unit-menu')).toHaveCount(0);

    // Reopen → change precision → cb-unit-val reformats (2 decimals).
    await page.locator('#cb-unit-btn').click();
    await page.locator('#cb-unit-precision').selectOption('2');
    await expect(page.locator('#cb-unit-val')).toHaveText('0.00');

    // Esc closes.
    await page.keyboard.press('Escape');
    await expect(page.locator('#cb-unit-menu')).toHaveCount(0);
  });

  test('outside click closes the dropdown', async ({ page }) => {
    await page.locator('#cb-unit-btn').click();
    await expect(page.locator('#cb-unit-menu')).toBeVisible();
    // Click a neutral bottom-bar spot (coords readout, no handler) → outside
    // mousedown closes the menu.
    await page.locator('#sb-coords').click();
    await expect(page.locator('#cb-unit-menu')).toHaveCount(0);
  });
});
