/**
 * ADR-264 embedded boss (fuse) — E2E (real Chromium round-trip).
 *
 * A rect drawn ON a box top = an "embedded boss". Pushing it must FUSE into
 * the solid (remove the profile, build side walls on the shared rim so they
 * re-twin with the surrounding ring) → a closed 2-manifold solid — NOT the
 * legacy ADR-102 cleave (opens the ring → crack) nor profile-preserve (3
 * face-bearing HEs per rim edge → non-manifold). Engine/scene coverage is in
 * axia-core `adr264_*`; this is the real-Chromium tool-path 시연 gate
 * (createSolidExtrude → SolidCreated) with an authoritative `isClosedSolid`
 * assertion via `verifyOutwardNormals`.
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

test.describe('ADR-264 — embedded boss fuses to a closed manifold', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForBridgeReady(page);
  });

  const embeddedBoss = (dist: number) => async ({ page }: { page: import('@playwright/test').Page }) => {
    const r = await page.evaluate((d) => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');
      // Box spans z∈[0,1000] (Z-up: create_box center z=500, height 2000→? use 1000).
      bridge.create_box(0, 0, 500, 2000, 1000, 1000); // top at z=1000
      // Embedded rect ON the top plane (z=1000), fully inside the 2000×1000 top.
      const shapeId = bridge.drawRectAsShape(0, 0, 1000, 0, 0, 1, 0, 1, 0, 800, 600);
      const bossFaces: number[] = bridge.getShapeFaceIds(shapeId);
      const boss = bossFaces && bossFaces.length > 0 ? bossFaces[0] : -1;
      const ok = boss >= 0 ? bridge.createSolidExtrude(boss, d) : false;
      const outward = bridge.verifyOutwardNormals();
      const inv = bridge.verifyInvariants();
      return {
        boss, ok,
        isClosedSolid: outward.isClosedSolid,
        valid: inv.valid, viol: inv.violationCount,
      };
    }, dist);
    expect(r.boss).toBeGreaterThanOrEqual(0);
    expect(r.ok).toBe(true);
    expect(r.isClosedSolid).toBe(true);   // fused to a closed 2-manifold solid
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  };

  // dist>0 — boss extrudes UP, fuses into the box top → closed solid.
  test('box top rect → boss extrude (up) fuses to closed solid', embeddedBoss(600));
  // dist<0 — the same boss inward = a rectangular pocket, also closed.
  test('box top rect → boss pocket (down) is a closed solid', embeddedBoss(-400));
});
