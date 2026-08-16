/**
 * C-3 — a line drawn across a cone divides it, from real clicks.
 *
 * Two clicks cannot divide any curved face: a geodesic through two rim points
 * is the rim, so it separates nothing (LOCKED #104, settled). Three or more
 * can, and the engine has been able to do it since ADR-284. What was missing
 * was that DrawLine committed each segment as it went and so never had three
 * points to hand over; it showed a hint pointing at the freehand and Bezier
 * tools instead.
 *
 * This spec exists because nothing else joins the two halves. The unit tests
 * mock the bridge, so they prove the tool calls `drawOpenSeamOnCurved` with the
 * right host and points; the Rust tests prove the engine splits from points
 * chosen in Rust. Neither says a real chain of real clicks makes points the
 * engine accepts — and no E2E covered `drawOpenSeamOnCurved` at all, for this
 * tool or for the two that have used it since ADR-284.
 *
 * Two things were measured while writing it, and both shaped what it asks:
 *
 *  - The endpoints cannot be ON the rim, because a cone's rim belongs to the
 *    base disc as much as to the side, and a click there picks the disc. They
 *    are 20 mm up the side instead; the engine projects onto the host and
 *    accepts endpoints at least 50 mm up
 *    (`an_open_seam_on_the_shapes_the_app_builds.rs`).
 *  - The claim is asked as "one Cone face became two", not as a face count.
 *    `getStats().faces` counts a husk the split leaves behind (measured: area
 *    0), and `faceArea` on a curved face reports the analytic surface rather
 *    than the piece — 251,327 mm² for each of two halves of a side whose whole
 *    lateral area is 280,993. Counting through either would be counting
 *    through an instrument that does not measure what it is being asked.
 *
 * The cone, not the sphere: the seam path trims a circular rim, which the
 * cone's base is, and the sphere this app builds is the axis-native one — a
 * single face with a meridian seam, declined both ways.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

/** How many active faces carry a Cone surface (kind 4). */
const COUNT_CONES = () => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const bridge = (window as any).__axia.get('bridge');
  let n = 0;
  for (let i = 0; i < 32; i++) {
    try { if (bridge.faceSurfaceKind(i) === 4) n++; } catch { /* absent */ }
  }
  return n;
};

test.describe('C-3 — a line seam on a curved face', () => {
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

  test('three clicks up and over split the cone side; Escape is what ends the chain', async ({ page }) => {
    const s = await page.evaluate(async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const tm = ax.get('toolManager');
      const vp = ax.get('viewport');
      bridge.create_cone(0, 0, 0, 200, 400, 24);
      tm.syncMesh();
      vp.setCameraState({
        radius: 900, phi: 1.35, theta: 0,
        targetX: 0, targetY: 0, targetZ: 150, orthoZoom: 4, viewMode: '3d',
      });
      // setCameraState lands on the NEXT frame — projecting immediately uses
      // the previous camera and silently yields off-screen coordinates.
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      const cam = vp.activeCamera;
      const rect = vp.renderer.domElement.getBoundingClientRect();
      const proj = (x: number, y: number, z: number) => {
        const v = cam.position.clone(); v.set(x, y, z); v.project(cam);
        return {
          x: Math.round(rect.left + ((v.x + 1) / 2) * rect.width),
          y: Math.round(rect.top + ((1 - v.y) / 2) * rect.height),
        };
      };
      tm.setTool('line');
      // 20 mm up the side and back down, over the near face. At height z the
      // cone's radius is 200·(1 − z/400), so 190 at z = 20.
      return {
        a: proj(171, -83, 20),
        b: proj(100, 0, 200),
        c: proj(171, 83, 20),
      };
    });

    const before = await page.evaluate(COUNT_CONES);
    expect(before, 'a Path B cone has one Cone face — its side').toBe(1);

    await page.mouse.move(s.a.x, s.a.y);
    await page.mouse.click(s.a.x, s.a.y);
    await page.mouse.click(s.b.x, s.b.y);
    await page.mouse.click(s.c.x, s.c.y);

    // The clicks are collected, not drawn one at a time — asked of the tool,
    // because "the mesh did not change" would also be true if the clicks never
    // landed, which is how the first version of this passed for nothing.
    const chain = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tm = (window as any).__axia.get('toolManager');
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tool = (tm as any).tools?.get?.('line');
      return { seam: String(tool?.seamKind), pts: tool?.seamPoints?.length ?? -1 };
    });
    expect(chain.seam, 'the cone side was captured as the seam host').toBe('cone');
    expect(chain.pts, 'three clicks were collected').toBe(3);
    expect(await page.evaluate(COUNT_CONES), 'and nothing is split yet').toBe(1);

    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);

    const after = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia; const bridge = ax.get('bridge');
      ax.get('toolManager').syncMesh();
      const inv = bridge.verifyInvariants();
      return { valid: inv.valid, viol: inv.violationCount };
    });
    expect(await page.evaluate(COUNT_CONES), 'the side is now two pieces').toBe(2);
    expect(after.valid).toBe(true);
    expect(after.viol).toBe(0);
  });
});
