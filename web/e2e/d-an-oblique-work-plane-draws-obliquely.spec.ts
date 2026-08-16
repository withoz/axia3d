/**
 * Track D — a work plane that is not axis-aligned draws on itself.
 *
 * Every grid in this repo drew on `z = 0`, `z = 100`, or a box wall at
 * `x = 200`. All cardinal. The engine turns out to be axis-agnostic — a
 * (1,1,1) plane holds all eight closed-shape tools, and an axis-snapping
 * mutation is caught by the oblique row alone while XY, XZ and YZ pass it
 * (`what_a_tilted_plane_can_hold.rs`). But an engine that can is not an app
 * that does: C-3 found tool wiring that had been dead since ADR-284 while both
 * the unit tests and the Rust tests were green, because each proved its own
 * half and nothing joined them.
 *
 * So this asks the app, through the same three clicks a user makes.
 *
 * The three clicks have to land on GEOMETRY. In an empty scene `get3DPoint`
 * answers with the ground plane, so three clicks in mid-air define `z = 0` no
 * matter where they look — an oblique plane cannot be built out of thin air,
 * and the first version of this spec tried to. They land on three different
 * faces of a box instead, one point per face, and the plane through them is
 * exactly (1,1,1)/√3.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

/** Box 0..300 on every axis; one point on each of its three visible faces. */
const ON_FACES = [
  [300, 100, 100], // +X
  [100, 300, 100], // +Y
  [100, 100, 300], // +Z
] as const;

test.describe('Track D — an oblique work plane', () => {
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

  test('a rect drawn on a three-point oblique plane faces that plane', async ({ page }) => {
    const s = await page.evaluate(async (pts: typeof ON_FACES) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const tm = ax.get('toolManager');
      const vp = ax.get('viewport');
      bridge.create_box(150, 150, 150, 300, 300, 300);
      tm.syncMesh();
      // Looking down the (1,1,1) diagonal, so all three faces are visible.
      vp.setCameraState({
        radius: 1100, phi: 0.955, theta: 0.785,
        targetX: 150, targetY: 150, targetZ: 150, orthoZoom: 4, viewMode: '3d',
      });
      // setCameraState lands on the NEXT frame — projecting immediately uses
      // the previous camera and silently yields off-screen coordinates.
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      const cam = vp.activeCamera;
      const rect = vp.renderer.domElement.getBoundingClientRect();
      const proj = (p: readonly number[]) => {
        const v = cam.position.clone(); v.set(p[0], p[1], p[2]); v.project(cam);
        return {
          x: Math.round(rect.left + ((v.x + 1) / 2) * rect.width),
          y: Math.round(rect.top + ((1 - v.y) / 2) * rect.height),
        };
      };
      tm.setTool('plane');

      // Two rect corners ON the plane, either side of its centroid. The
      // in-plane axes are built from the same three points, so nothing here
      // is axis-derived either.
      const sub = (a: readonly number[], b: readonly number[]) =>
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]] as [number, number, number];
      const norm = (a: [number, number, number]) => {
        const l = Math.hypot(a[0], a[1], a[2]);
        return [a[0] / l, a[1] / l, a[2] / l] as [number, number, number];
      };
      const cross = (a: readonly number[], b: readonly number[]) =>
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] as
          [number, number, number];
      const n = norm(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
      const u = norm(sub(pts[1], pts[0]));
      const v = norm(cross(n, u));
      const centroid = [
        (pts[0][0] + pts[1][0] + pts[2][0]) / 3,
        (pts[0][1] + pts[1][1] + pts[2][1]) / 3,
        (pts[0][2] + pts[1][2] + pts[2][2]) / 3,
      ] as const;
      const at = (du: number, dv: number) =>
        [centroid[0] + u[0] * du + v[0] * dv,
         centroid[1] + u[1] * du + v[1] * dv,
         centroid[2] + u[2] * du + v[2] * dv] as const;

      return {
        a: proj(pts[0]), b: proj(pts[1]), c: proj(pts[2]),
        r0: proj(at(-60, -40)), r1: proj(at(60, 40)),
        n,
      };
    }, ON_FACES);

    // Build the work plane.
    await page.mouse.click(s.a.x, s.a.y);
    await page.mouse.click(s.b.x, s.b.y);
    await page.mouse.click(s.c.x, s.c.y);
    await page.waitForTimeout(150);

    const plane = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tm = (window as any).__axia.get('toolManager');
      const info = tm.getSketchInfo?.();
      return info
        ? { on: true, n: [info.normal.x, info.normal.y, info.normal.z] }
        : { on: false, n: [0, 0, 0] };
    });
    expect(plane.on, 'the three clicks opened a sketch on the plane they span').toBe(true);

    // The claim is that the plane is the diagonal one and that it is oblique —
    // said as those two things rather than as three decimal comparisons. The
    // clicks land on integer pixels, so the picked points sit a little off the
    // face centres they were aimed at and the plane tilts by about 0.07%;
    // asking each component to be 1/√3 to three places failed on that and on
    // nothing else. A dot product against the diagonal is the same claim
    // without the quantisation in it.
    const d = 1 / Math.sqrt(3);
    const along = Math.abs(plane.n[0] * d + plane.n[1] * d + plane.n[2] * d);
    expect(along, `the plane is the diagonal one, got ${JSON.stringify(plane.n)}`)
      .toBeGreaterThan(0.999);
    for (const c of plane.n) {
      // No component near 0 or 1: not a cardinal plane by any reading.
      expect(Math.abs(c), `component ${c} is axis-like`).toBeGreaterThan(0.3);
      expect(Math.abs(c), `component ${c} is axis-like`).toBeLessThan(0.8);
    }

    const before = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      ax.get('toolManager').setTool('rect');
      return ax.get('bridge').getStats().faces;
    });

    await page.mouse.click(s.r0.x, s.r0.y);
    await page.mouse.click(s.r1.x, s.r1.y);
    await page.waitForTimeout(250);

    const after = await page.evaluate((n: number[]) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia; const bridge = ax.get('bridge');
      ax.get('toolManager').syncMesh();
      // How much surface faces the oblique plane, out of everything drawn.
      // Asked by area rather than by face id because a box is six cardinal
      // faces and the question is whether anything NEW joined them facing the
      // other way — a face id would need the split to be found first.
      const buf = bridge.getMeshBuffers();
      if (!buf) return { faces: 0, oblique: 0, valid: false, viol: -1 };
      const { positions: pos, normals, indices: idx } = buf;
      let oblique = 0;
      for (let t = 0; t + 2 < idx.length; t += 3) {
        const [i, j, k] = [idx[t], idx[t + 1], idx[t + 2]];
        const p = (m: number) => [pos[3 * m], pos[3 * m + 1], pos[3 * m + 2]] as const;
        const [a, b, c] = [p(i), p(j), p(k)];
        const e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        const e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        const cx = e1[1] * e2[2] - e1[2] * e2[1];
        const cy = e1[2] * e2[0] - e1[0] * e2[2];
        const cz = e1[0] * e2[1] - e1[1] * e2[0];
        const area = 0.5 * Math.hypot(cx, cy, cz);
        const nv = [normals[3 * i], normals[3 * i + 1], normals[3 * i + 2]];
        if (Math.abs(nv[0] * n[0] + nv[1] * n[1] + nv[2] * n[2]) > 0.999) oblique += area;
      }
      const inv = bridge.verifyInvariants();
      return { faces: bridge.getStats().faces, oblique, valid: inv.valid, viol: inv.violationCount };
    }, plane.n);

    expect(after.faces, 'the rect made a face').toBeGreaterThan(before);
    // A box has no obliquely-facing surface at all, so any is the rect's. The
    // floor is generous on purpose: this asks that the shape landed on the
    // plane, not how big it came out.
    expect(after.oblique, 'surface facing the oblique plane must exist').toBeGreaterThan(100);
    expect(after.valid).toBe(true);
    expect(after.viol).toBe(0);
  });

  /**
   * And a LINE, which is the case the work plane's sketch exists for.
   *
   * The rect above would pass without a sketch at all: `DrawPlaneTool` also
   * sets the ADR-166 plane LOCK, and the lock alone governs `getDrawPlane`,
   * which is what rect, circle and polygon read. Measured — making the rect
   * ignore the sketch entirely leaves that test green.
   *
   * A line's ends come from `get3DPoint`, which the lock does not govern.
   * That is the split the tool's own comment records from 2026-08-05: after an
   * oblique plane was set, rectangles landed on it and lines landed on the
   * ground. The sketch is what closes it, and nothing measured that until here.
   */
  test('a line drawn on the same plane lands on it too', async ({ page }) => {
    const s = await page.evaluate(async (pts: typeof ON_FACES) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge');
      const tm = ax.get('toolManager');
      const vp = ax.get('viewport');
      bridge.create_box(150, 150, 150, 300, 300, 300);
      tm.syncMesh();
      vp.setCameraState({
        radius: 1100, phi: 0.955, theta: 0.785,
        targetX: 150, targetY: 150, targetZ: 150, orthoZoom: 4, viewMode: '3d',
      });
      await new Promise((res) => requestAnimationFrame(() => requestAnimationFrame(res)));
      const cam = vp.activeCamera;
      const rect = vp.renderer.domElement.getBoundingClientRect();
      const proj = (p: readonly number[]) => {
        const v = cam.position.clone(); v.set(p[0], p[1], p[2]); v.project(cam);
        return {
          x: Math.round(rect.left + ((v.x + 1) / 2) * rect.width),
          y: Math.round(rect.top + ((1 - v.y) / 2) * rect.height),
        };
      };
      tm.setTool('plane');
      const sub = (a: readonly number[], b: readonly number[]) =>
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]] as [number, number, number];
      const norm = (a: [number, number, number]) => {
        const l = Math.hypot(a[0], a[1], a[2]);
        return [a[0] / l, a[1] / l, a[2] / l] as [number, number, number];
      };
      const cross = (a: readonly number[], b: readonly number[]) =>
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] as
          [number, number, number];
      const n = norm(cross(sub(pts[1], pts[0]), sub(pts[2], pts[0])));
      const u = norm(sub(pts[1], pts[0]));
      const v = norm(cross(n, u));
      const centroid = [
        (pts[0][0] + pts[1][0] + pts[2][0]) / 3,
        (pts[0][1] + pts[1][1] + pts[2][1]) / 3,
        (pts[0][2] + pts[1][2] + pts[2][2]) / 3,
      ] as const;
      const at = (du: number, dv: number) =>
        [centroid[0] + u[0] * du + v[0] * dv,
         centroid[1] + u[1] * du + v[1] * dv,
         centroid[2] + u[2] * du + v[2] * dv] as const;
      return { a: proj(pts[0]), b: proj(pts[1]), c: proj(pts[2]),
               l0: proj(at(-70, 20)), l1: proj(at(70, -20)) };
    }, ON_FACES);

    await page.mouse.click(s.a.x, s.a.y);
    await page.mouse.click(s.b.x, s.b.y);
    await page.mouse.click(s.c.x, s.c.y);
    await page.waitForTimeout(150);

    /** Edge-line points lying on the sketch plane. A box has none. */
    const onPlane = () => page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const info = ax.get('toolManager').getSketchInfo?.();
      if (!info) return -1;
      const n = [info.normal.x, info.normal.y, info.normal.z];
      const d = n[0] * info.origin.x + n[1] * info.origin.y + n[2] * info.origin.z;
      const lines = ax.get('bridge').getEdgeLines();
      if (!lines) return 0;
      let hits = 0;
      for (let i = 0; i + 2 < lines.length; i += 3) {
        const off = Math.abs(lines[i] * n[0] + lines[i + 1] * n[1] + lines[i + 2] * n[2] - d);
        if (off < 0.5) hits++;
      }
      return hits;
    });

    expect(await onPlane(), 'the box alone puts nothing on the oblique plane').toBe(0);

    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').setTool('line');
    });
    await page.mouse.click(s.l0.x, s.l0.y);
    await page.mouse.click(s.l1.x, s.l1.y);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
    await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).__axia.get('toolManager').syncMesh();
    });

    expect(await onPlane(), "the line's ends are on the plane, not on the ground")
      .toBeGreaterThan(0);
  });
});
