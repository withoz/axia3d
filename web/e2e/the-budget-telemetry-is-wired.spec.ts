/**
 * The budget telemetry records something, and can be read.
 *
 * The engine has budgets for every latency it cares about (메타-원칙 #11) and a
 * telemetry module that keeps counts and violations against them. Four
 * production sites feed it — `WasmBridge` on every WASM crossing and every
 * copied buffer, `Viewport` on every frame boundary, `ToolManager` after every
 * syncMesh — and all four call it as `window.__AXIA_TELEMETRY_*?.()`.
 *
 * Optional-call is the shape CLAUDE.md names as one of only TWO links that can
 * break silently, and it did: `installTelemetryGlobal()`, which defines all six
 * of those hooks plus the `window.__AXIA_TELEMETRY` reader, was called from its
 * own unit test and NOWHERE ELSE. So every one of those sites was a no-op, the
 * reader did not exist, and nothing said so — measured while asking a
 * performance question and finding `telemetry` unreadable from the page.
 */
import { test, expect } from '@playwright/test';

test.describe('budget telemetry', () => {
  test('the hooks exist and a real session records against them', async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('viewport');
      },
      { timeout: 30000 },
    );

    const r = await page.evaluate(async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const installed = {
        reader: '__AXIA_TELEMETRY' in w,
        reset: typeof w.__AXIA_TELEMETRY_RESET === 'function',
        tick: typeof w.__AXIA_TELEMETRY_TICK === 'function',
        frameStart: typeof w.__AXIA_TELEMETRY_FRAME_START === 'function',
        frameEnd: typeof w.__AXIA_TELEMETRY_FRAME_END === 'function',
        record: typeof w.__AXIA_TELEMETRY_RECORD === 'function',
        copy: typeof w.__AXIA_TELEMETRY_COPY === 'function',
      };

      // Do real work through the real path, then read what it recorded.
      w.__AXIA_DEBUG = true;
      w.__AXIA_TELEMETRY_RESET?.();
      const c = w.__axia;
      const b = c.get('bridge');
      b.create_box?.(0, 0, 100, 200, 200, 200);
      b.drawCircleAsCurve?.(0, 0, 0, 0, 0, 1, 300);
      c.get('syncMesh')();
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      await new Promise((res) => setTimeout(res, 300));

      const snap = w.__AXIA_TELEMETRY ?? null;
      return {
        installed,
        // Only the shape — the numbers are machine-dependent.
        framesObserved: snap?.framesObserved ?? 0,
        tasksObserved: snap?.tasksObserved ?? 0,
        avgCrossingsPerFrame: snap?.avgCrossingsPerFrame ?? 0,
        avgCopyBytesPerFrame: snap?.avgCopyBytesPerFrame ?? 0,
        maxRafChainDepth: snap?.maxRafChainDepth ?? 0,
        violations: snap?.budgetViolations?.length ?? 0,
      };
    });

    console.log(JSON.stringify(r, null, 2));

    // Every hook the production sites call has to exist, or that site is a
    // silent no-op — which is exactly how this was found.
    for (const [name, present] of Object.entries(r.installed)) {
      expect(present, `window.__AXIA_TELEMETRY${name === 'reader' ? '' : '_' + name} missing`).toBe(true);
    }
    // And it has to actually RECORD, or the hooks are installed on a counter
    // nobody increments: frames drawn, and WASM crossings made by the draws.
    expect(r.framesObserved).toBeGreaterThan(0);
    expect(r.avgCrossingsPerFrame).toBeGreaterThan(0);
    // ADR-012 §2 — the rAF chain must never nest.
    expect(r.maxRafChainDepth).toBeLessThanOrEqual(1);
  });
});
