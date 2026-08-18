/**
 * A camera that has just moved can be projected through, without waiting.
 *
 * `Vector3.project(camera)` reads `matrixWorldInverse`, and three.js only
 * refreshes that inside `renderer.render()`. So code that moved the camera and
 * then worked out where a world point lands on screen was using the PREVIOUS
 * camera. Measured — move the camera to look at the origin, then project the
 * origin:
 *
 * ```text
 *   right after the move   [748, 317]     wrong
 *   two frames later       [640, 360]     the canvas centre, correct
 * ```
 *
 * Whether that mattered depended on which came first, the projection or the
 * next frame, so it surfaced as two E2E specs failing about one full-suite run
 * in two — and the tell pointed elsewhere. `adr-190-p3-repeat-last` reported
 * that "a real double-click did not repeat the distance" when what had failed
 * was the FIRST push, one step earlier: its synthetic click was aimed at a
 * screen point computed from the stale camera and landed off the face.
 *
 * ⚠ Three wrong turns before this, all from stopping at the first plausible
 * story: the double-click timing (it was step 1, not step 2); a probe that read
 * only `matrixWorldInverse`'s TRANSLATION row, which `lookAt` does update, so
 * the matrix looked fresh while its rotation was a frame behind; and a first
 * version of this very test that rendered before projecting and therefore
 * passed with the fix removed. What settles it is comparing the projection with
 * and without the frames — which is what this asserts.
 */
import { test, expect } from '@playwright/test';

test.describe('a moved camera', () => {
  test('projects to the same place with and without a frame in between', async ({ page }) => {
    await page.goto('/');
    await page.waitForFunction(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = (window as any).__axia;
      return w && typeof w.get === 'function' && w.get('bridge') && w.get('viewport');
    });

    const r = await page.evaluate(async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const vp = ax.get('viewport');
      const b = ax.get('bridge');
      const tm = ax.get('toolManager');

      b.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 200, 200);
      tm.syncMesh();

      // Move the camera to look at the origin — no render, no frame.
      vp.setCameraState({
        radius: 1600, phi: 0.85, theta: 0.6,
        targetX: 0, targetY: 0, targetZ: 0, viewMode: '3d',
      });

      const rect = vp.renderer.domElement.getBoundingClientRect();
      // Borrow a Vector3 instance from the scene rather than importing three.
      const V3 = (vp.scene.position as { constructor: new (x: number, y: number, z: number) => any })
        .constructor;
      const toScreen = () => {
        const p = new V3(0, 0, 0).project(vp.activeCamera);
        return [
          rect.left + ((p.x + 1) / 2) * rect.width,
          rect.top + ((1 - p.y) / 2) * rect.height,
        ];
      };

      const before = toScreen();
      const pickedBefore = vp.pick(before[0], before[1])?.faceIndex != null;
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      const after = toScreen();
      const pickedAtBeforePoint = vp.pick(before[0], before[1])?.faceIndex != null;

      return {
        canvasOk: rect.width > 10 && rect.height > 10,
        before: before.map(Math.round),
        after: after.map(Math.round),
        drift: Math.round(Math.hypot(before[0] - after[0], before[1] - after[1])),
        pickedBefore,
        pickedAtBeforePoint,
      };
    });

    console.log(JSON.stringify(r));

    expect(r.canvasOk).toBe(true);
    // The whole claim: the answer does not depend on whether a frame happened.
    expect(
      r.drift,
      `the same world point projected to ${r.before} before a frame and ` +
        `${r.after} after — a screen point taken right after a camera move is ` +
        'aimed at where the camera USED to be',
    ).toBeLessThanOrEqual(1);
    // And the point it names is on the face, both times.
    expect(r.pickedBefore, `nothing at ${r.before}`).toBe(true);
    expect(r.pickedAtBeforePoint, `the pre-frame point stopped hitting`).toBe(true);
  });
});
