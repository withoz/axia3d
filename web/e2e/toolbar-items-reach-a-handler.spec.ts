/**
 * EVERY TOOLBAR ITEM DOES SOMETHING — checked by running it.
 *
 * `ActionWiring.test.ts` reads source text: it can see that a branch exists and
 * cannot see that the branch runs. That distinction is not academic here — the
 * first version of its toolbar guard matched `const bare = action.slice(5)`, so
 * wrapping the branch in `if (false)` left it green. This file runs the ids.
 *
 * Measured in real Chromium on 2026-08-10, before the fix:
 *
 *   toolbar data-action  31   reaching no handler: ['tool-plane']
 *   toolbar data-tool    33   failing to activate: []
 *
 * `tool-plane` is the SKETCH PLANE dropdown's "📐 작업 평면 · 세 점". The same
 * id in the MENU worked the whole time, because MenuBar's switch carries
 * `case 'tool-plane'` — PR #97 repointed both entries and taught only one path.
 *
 * ⚠ The tool reader is `toolManager.currentTool` and the setter is `setTool`.
 * An earlier probe used `setActiveTool` (which does not exist) and reported all
 * 33 tools dead; the premise assertion below is what stops that from being
 * mistaken for a finding.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('toolbar wiring, at runtime', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('toolManager');
      },
      { timeout: 30000 },
    );
  });

  test('every data-action item reaches a handler', async ({ page }) => {
    const out = await page.evaluate(() => {
      const tb = document.getElementById('toolbar')!;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tm = (window as any).__axia.get('toolManager');
      const ids = [
        ...new Set(
          [...tb.querySelectorAll('[data-action]')].map((e) => e.getAttribute('data-action')!),
        ),
      ];
      // Routed by main.ts before executeAction ever sees them.
      const routedInMain = ['bool-union', 'bool-subtract', 'bool-intersect'];
      const dead: string[] = [];
      const bypassed: string[] = [];
      for (const id of ids) {
        if (routedInMain.includes(id)) continue;
        if (id.startsWith('tool-')) {
          // ★CLICK THE ELEMENT. Calling `tm.setTool(id.slice(5))` here would
          // bypass main.ts entirely — which is where the gap was — so an
          // earlier version of this test passed with the dispatch removed.
          // `el.click()` fires a real click event, so the toolbar's own
          // listener runs even for a dropdown item that is not visible.
          if (id === 'tool-explode') {
            // An alias for ungroup, not a tool: nothing to read afterwards.
            if (tm.executeAction('ungroup') === false) dead.push(id);
            continue;
          }
          const bare = id.slice(5);
          const el = tb.querySelector(`[data-action="${id}"]`) as HTMLElement | null;
          if (!el) {
            bypassed.push(id);
            continue;
          }
          tm.setTool('select');
          el.click();
          if (tm.currentTool !== bare) dead.push(id);
          tm.setTool('select');
          continue;
        }
        let ok = false;
        try {
          ok = tm.executeAction(id) !== false;
        } catch {
          ok = false;
        }
        if (!ok) dead.push(id);
      }
      return { count: ids.length, dead, bypassed };
    });

    expect(out.count, 'the toolbar was read at all').toBeGreaterThan(20);
    // PREMISE: the tool-* ids were exercised THROUGH the DOM. If an element
    // could not be found the test would silently stop checking the very path
    // this file exists for.
    expect(out.bypassed, 'tool-* ids with no element to click').toEqual([]);
    expect(
      out.dead,
      'Toolbar items that do nothing — the user gets "알 수 없는 명령입니다".',
    ).toEqual([]);
  });

  test('every data-tool item activates its tool', async ({ page }) => {
    const out = await page.evaluate(() => {
      const tb = document.getElementById('toolbar')!;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tm = (window as any).__axia.get('toolManager');
      // PREMISE: the reader round-trips. Without this a wrong method name makes
      // every tool look dead, which is exactly what happened once.
      tm.setTool('select');
      const readerWorks = tm.currentTool === 'select';
      const tools = [
        ...new Set([...tb.querySelectorAll('[data-tool]')].map((e) => e.getAttribute('data-tool')!)),
      ].filter((t) => t !== 'undo' && t !== 'redo'); // wired directly in main.ts
      const dead: string[] = [];
      for (const t of tools) {
        try {
          tm.setTool(t);
          if (tm.currentTool !== t) dead.push(t);
        } catch {
          dead.push(`${t}(threw)`);
        }
        tm.setTool('select');
      }
      return { readerWorks, count: tools.length, dead };
    });

    expect(out.readerWorks, 'currentTool must round-trip "select"').toBe(true);
    expect(out.count).toBeGreaterThan(25);
    expect(out.dead, 'Toolbar buttons that do not activate their tool').toEqual([]);
  });
});
