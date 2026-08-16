/**
 * A rect drawn on a solid the app BUILT is still something you can push.
 *
 * The rule (사용자 결재 2026-08-11): a region cut out of somebody's face stays
 * theirs, so a closed solid keeps its shell whole. A rect drawn on such a face
 * therefore belongs to the solid, and the drawn Shape owns nothing.
 *
 * That leaves the question this spec is for. `drawRectAsShape →
 * getShapeFaceIds → createSolidExtrude` is how DrawWallTool builds a wall, how
 * SliceTool finds its faces, and how ADR-264's boss tests find the face to
 * push. If the Shape owns nothing and remembers nothing, that chain answers
 * with nothing and the user cannot push what they just drew.
 *
 * `adr-264-embedded-boss` does cover the same chain, and its box turns out to
 * be owned too — the WASM `create_box` calls `create_xia_with_faces`. This
 * file covers the route a user actually takes to get a solid, drawing and
 * extruding rather than dropping a primitive, and it says out loud which call
 * in the chain has to keep answering.
 *
 * ⚠ Measuring this needs `npm run build:wasm`, not just `npm run build`. The
 * latter rebuilds the TypeScript and leaves `axia_wasm_bg.wasm` where it was,
 * so an engine change under test simply is not in the bundle — which is how
 * the parked change was briefly, and wrongly, reported as costing nothing.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('a rect drawn on a solid the app built', () => {
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

  test('can still be pushed, through the same call chain the tools use', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');

      // Build the solid the way the app does: draw, then extrude.
      const baseShape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
      const baseFaces: number[] = bridge.getShapeFaceIds(baseShape);
      const built = baseFaces.length > 0 && bridge.createSolidExtrude(baseFaces[0], 200);

      // Now draw on its top — this is the case the primitive box misses.
      const drawn = bridge.drawRectAsShape(0, 0, 200, 0, 0, 1, 1, 0, 0, 120, 120);
      const handle: number[] = bridge.getShapeFaceIds(drawn);
      const facesBefore = bridge.getStats().faces;
      const pushed = handle.length > 0 ? bridge.createSolidExtrude(handle[0], 60) : false;
      const inv = bridge.verifyInvariants();
      const outward = bridge.verifyOutwardNormals();
      return {
        built,
        handle,
        facesBefore,
        facesAfter: bridge.getStats().faces,
        pushed,
        valid: inv.valid,
        viol: inv.violationCount,
        isClosedSolid: outward.isClosedSolid,
      };
    });

    expect(r.built, 'the base rect extruded into a solid').toBe(true);
    // The claim: the Shape answers, even though it owns nothing.
    expect(r.handle.length, 'the drawn Shape must still point at what it drew').toBeGreaterThan(0);
    expect(r.pushed, 'and that face must be pushable').toBe(true);
    expect(r.facesAfter, 'pushing it adds the boss walls').toBeGreaterThan(r.facesBefore);
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // The point of giving the region to the solid in the first place.
    expect(r.isClosedSolid, 'and the shell stayed whole').toBe(true);
  });
});
