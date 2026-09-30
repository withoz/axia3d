/**
 * The memory budget (ADR-013) measures what its areas are named after.
 *
 * Two of its samplers read things that were not there, so they reported 0 for
 * the life of the app. Measured before the change, in this browser:
 *
 * ```text
 *   'rust'     0        WASM linear memory 1,966,080 bytes
 *              read `bridge.engine.memory`; the memory is returned by init()
 *              and held on the bridge — AxiaEngine has no `memory` member
 *   'history'  0        History panel: 1 entry
 *              read container key 'operationLog', never registered; the log
 *              is the getOperationLog() singleton. The 'history' evict handler
 *              read the same key, so a forced evict left the entry in place.
 * ```
 *
 * Both were string-keyed or `any`-typed, so tsc saw neither.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 * ⚠ Mutation-checked: drop the 'rust' registration from main.ts and the first
 * test fails; point the 'history' sampler/handler back at
 * `container.tryGet('operationLog')` and the second fails.
 */
import { test, expect, type Page } from '@playwright/test';

async function boot(page: Page) {
  await page.goto('/');
  await page.waitForFunction(
    () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      return w.__axia?.get?.('bridge')?.isReady?.() && typeof w.__AXIA_EVICT === 'function';
    },
    { timeout: 30000 },
  );
}

test('the rust area reports the WASM linear memory', async ({ page }) => {
  await boot(page);
  const r = await page.evaluate(() => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    const bridge = w.__axia.get('bridge');
    return {
      sampled: w.__AXIA_MEMORY.bytes.rust as number,
      // The truth: the WebAssembly.Memory init() returned, held on the bridge.
      real: bridge.wasmMemory.buffer.byteLength as number,
    };
  });
  expect(r.real).toBeGreaterThan(0);
  expect(r.sampled).toBe(r.real);
  // WASM memory grows in 64 KiB pages; a sampler reading anything else (a
  // buffer length, a count) would almost never land on a page boundary.
  expect(r.sampled % 65536).toBe(0);
});

test('the history area counts the operation log, and evicting it clears the log', async ({ page }) => {
  await boot(page);
  const before = await page.evaluate(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    const bridge = w.__axia.get('bridge');
    const tm = w.__axia.get('toolManager');
    bridge.create_box(0, 0, 0, 200, 200, 200);
    tm.syncMesh();
    // Subdivide records one entry in the operation log, with no prompt.
    tm.executeAction('subdivide');
    w.__axia_historyPanel.toggle();
    await new Promise((r) => setTimeout(r, 200));
    return {
      rows: document.querySelectorAll('.history-panel .hp-list > *').length,
      sampled: w.__AXIA_MEMORY.bytes.history as number,
    };
  });
  // Premise: the log really has an entry, seen where a user sees it.
  expect(before.rows).toBe(1);
  expect(before.sampled).toBe(200); // one entry × the sampler's 200 bytes

  const after = await page.evaluate(async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as any;
    const ev = w.__AXIA_EVICT();
    await new Promise((r) => setTimeout(r, 200));
    return {
      evicted: ev.areasEvicted as string[],
      rows: document.querySelectorAll('.history-panel .hp-list > *').length,
      sampled: w.__AXIA_MEMORY.bytes.history as number,
    };
  });
  expect(after.evicted).toContain('history');
  expect(after.rows).toBe(0);
  expect(after.sampled).toBe(0);
});
