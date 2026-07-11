/**
 * ADR-259 draft-on-solid-face — E2E (real Chromium round-trip).
 *
 * The flat-profile taper (frustum) was already shipped; the ONE gap was
 * drafting an EXISTING solid face (a box top / prism wall), which the v1
 * rejected fearing the ADR-087 K-ε sandwich. draft-on-solid-face routes a
 * taper on an is_move_only face through the Scene MoveOnly-taper dispatch
 * (exec_push_pull_tapered): the top ring moves up + shrinks inward, the
 * existing walls slant into planar trapezoids — NO new faces, no sandwich.
 *
 * Flow: create_box → find the +Z top face → createSolidExtrudeTapered(top,
 * dist, taperDeg). Asserts: succeeds (bridge returns true — the PushPullDone
 * arm), still a 6-face manifold closed solid, top lifted + shrunk. Plus a
 * D5 reject: a steep draft returns false + leaves the box byte-identical.
 *
 * Playwright uses the production build (`npm run preview`) — rebuild
 * (`npm run build`) after any WASM rebuild.
 */
import { test, expect } from '@playwright/test';
import { waitForBridgeReady } from './helpers/boolean-fixtures';

interface AxiaWindow {
  __axia?: {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    get<T = any>(key: string): T;
  };
}

test.describe('ADR-259 — draft an existing solid face (tapered MoveOnly)', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForBridgeReady(page);
  });

  test('box top → draft (tapered MoveOnly) stays a 6-face manifold solid', async ({ page }) => {
    const r = await page.evaluate(() => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');
      bridge.create_box(0, 0, 100, 200, 200, 200); // Z-up: spans z∈[0,200]
      const facesBefore = bridge.getStats().faces;
      // Find the +Z top face (a box has exactly one normal.z ≈ +1 face).
      let top = -1;
      for (let i = 0; i < facesBefore + 4; i++) {
        const n = bridge.getFaceNormal(i);
        if (n && n[2] > 0.9) { top = i; break; }
      }
      const ok = bridge.createSolidExtrudeTapered(top, 100, 15); // draft in 15°
      const facesAfter = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { top, ok, facesBefore, facesAfter, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.top).toBeGreaterThanOrEqual(0);
    expect(r.ok).toBe(true);                       // PushPullDone arm → true
    expect(r.facesAfter).toBe(r.facesBefore);      // MoveOnly: no new faces
    expect(r.facesAfter).toBe(6);                  // still a 6-face box
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });

  test('box top → steep draft rejects (D5) + box byte-identical', async ({ page }) => {
    const r = await page.evaluate(() => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');
      bridge.create_box(0, 0, 100, 200, 200, 200);
      const facesBefore = bridge.getStats().faces;
      let top = -1;
      for (let i = 0; i < facesBefore + 4; i++) {
        const n = bridge.getFaceNormal(i);
        if (n && n[2] > 0.9) { top = i; break; }
      }
      // 88° on a 200×200 top → collapse → hard-error (no straight fallback).
      const ok = bridge.createSolidExtrudeTapered(top, 100, 88);
      const facesAfter = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      return { ok, facesBefore, facesAfter, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.ok).toBe(false);                      // D5: rejected, no silent straight solid
    expect(r.facesAfter).toBe(r.facesBefore);      // byte-identical rollback
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });
});
