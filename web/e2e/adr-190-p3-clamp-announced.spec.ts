/**
 * ADR-190 Phase 3 — the inward clamp must announce itself.
 *
 * ADR-196 stops a whole-face inward push just short of the far side so the
 * solid cannot flip inside-out. That is correct — a whole-face push is
 * ambiguous, and auto-cutting through would be exactly the heuristic-automation
 * trap (메타-원칙 #16). But it was SILENT: measured through the production
 * bridge, a 2000×1000×1000 box pushed −1500 collapses to 0.001mm thick,
 * `createSolidExtrude` returns `true`, `verifyInvariants` is clean, and
 * `lastError()` is empty (ADR-293 §5). The user gets a sliver and no reason.
 *
 * This spec pins the engine reading the tool relies on, and the Toast the tool
 * raises through the real DOM.
 *
 * NOTE: Playwright uses `npm run preview` (production build). Re-build
 * (`npm run build`, plus `npm run build:wasm` after Rust changes) first.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-190 Phase 3 — inward clamp is announced', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge');
      },
      { timeout: 30000 },
    );
  });

  // One scenario per fresh page. Stacking a flat rect and a box in one scene
  // made ids collide and produced a confidently wrong reading — the same trap
  // ADR-293 §7 (L-293-5) records.

  test('moveOnlyMaxInward = -1 for a flat open profile (no walls → unclamped)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      const flat = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 100, 100);
      tm.syncMesh();
      return { flat, normal: [...(bridge.getFaceNormal(flat) || [])], limit: bridge.moveOnlyMaxInward(flat) };
    });
    expect(r.normal, 'sanity: the id must really name the flat rect').toEqual([0, 0, 1]);
    expect(r.limit).toBe(-1);
  });

  test('moveOnlyMaxInward = the solid thickness for a box top', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      const seed = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 2000, 1000);
      tm.syncMesh();
      const boxed = bridge.createSolidExtrude(seed, 1000);
      tm.syncMesh();
      let top = -1;
      for (let f = 0; f < 24; f++) {
        const n = bridge.getFaceNormal(f);
        if (n && n[2] > 0.9) { top = f; break; }
      }
      return { boxed, faces: bridge.getStats().faces, top, limit: bridge.moveOnlyMaxInward(top) };
    });
    expect(r.boxed, 'sanity: the box must exist before its thickness means anything').toBe(true);
    expect(r.faces, 'a box is 6 faces').toBe(6);
    expect(r.top).toBeGreaterThanOrEqual(0);
    expect(r.limit, 'bounded by its own 1000mm thickness').toBeCloseTo(1000, 3);
  });

  test('an over-push is clamped AND explained; a push within thickness stays quiet', async ({ page }) => {
    // build a 1000-tall box and arm Push/Pull on its top face
    const armed = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      const seed = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 2000, 1000);
      tm.syncMesh();
      bridge.createSolidExtrude(seed, 1000);
      tm.syncMesh();
      let top = -1;
      for (let f = 0; f < 24; f++) {
        const n = bridge.getFaceNormal(f);
        if (n && n[2] > 0.9) { top = f; break; }
      }
      tm.setTool('pushpull');
      const sel = ax.get('selection');
      sel.selectFaces([top]);            // NOT selectFace — that name does not exist
      return { top, selected: sel.getSelectedFaces() };
    });
    expect(armed.top).toBeGreaterThanOrEqual(0);
    expect(armed.selected, 'the top face must actually be armed').toContain(armed.top);

    // ── over-push: -1500 into a 1000-thick solid ──
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').applyVCBValue(-1500);
    });
    const toastText = await page.locator('#axia-toast-container').innerText();
    expect(toastText, 'the silent clamp must now say why it stopped').toContain('멈췄습니다');
    expect(toastText, 'and name the actual limit').toContain('1000.0mm');

    // the clamp itself is unchanged: the solid stuck instead of inverting
    const after = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia; const bridge = ax.get('bridge');
      ax.get('toolManager').syncMesh();
      const b = bridge.getMeshBuffers();
      let hi = -1e9;
      for (let i = 2; i < b.positions.length; i += 3) hi = Math.max(hi, b.positions[i]);
      const inv = bridge.verifyInvariants();
      return { hi, valid: inv.valid, viol: inv.violationCount };
    });
    expect(after.hi, 'the face still sticks just above the far side (ADR-196)').toBeLessThan(1);
    expect(after.valid).toBe(true);
    expect(after.viol).toBe(0);
  });
});
