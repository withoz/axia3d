/**
 * A SNAP NEVER LEAVES THE ACTIVE TARGET — including when it is a surface.
 *
 * ADR-292 re-introduced object snap plane-consistently: a candidate is
 * re-projected onto the active draw plane, so a snap moves the in-plane
 * position and never the plane-normal coordinate. On a CURVED face `get3DPoint`
 * skipped snap altogether, and the code said why:
 *
 *   "a snap must be re-projected onto the ACTIVE PLANE, and a curved face has
 *    none. Snapping on curved surfaces needs its own design."
 *
 * A curved face has a SURFACE, which is the same thing one step out. The four
 * projections have always been in `axia-geo`; nothing outside could reach them
 * until `projectPointToFaceSurface` (2026-08-05). This proves, through the real
 * WASM engine and real canvas layout, that a click near an existing point on a
 * sphere lands ON that point AND on the sphere — and that a click away from
 * everything is still the raw surface hit.
 *
 * NOTE: Playwright serves the production build via `npm run preview`, so run
 * `npm run build` after changing anything here or in the tool layer.
 */
import { test, expect } from '@playwright/test';

const R = 100;

test.describe('object snap on a curved face', () => {
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

  test('a click near a point on a sphere snaps to it, and stays on the sphere', async ({ page }) => {
    // A sphere with a circle drawn on it — the circle's rim gives real
    // vertices ON the surface to snap to.
    const setup = await page.evaluate((r) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      bridge.create_sphere(0, 0, 0, r);
      // A closed patch on the +X side of the sphere → a rim of vertices that
      // are ON the surface, which is what there is to snap to. (Measured: this
      // takes the sphere from 2 faces to 3.)
      const d = r * 0.4;
      const res = bridge.drawPolylineOnCurved(
        'sphere', 0,
        [[r, -d, -d], [r, d, -d], [r, d, d], [r, -d, d]],
        true,
      );
      tm.syncMesh();
      vp.setCameraState({
        radius: r * 4, phi: 1.0, theta: 0.4,
        targetX: 0, targetY: 0, targetZ: 0, orthoZoom: 4, viewMode: '3d',
      });
      tm.setTool('line');
      return { circle: res, faces: bridge.getStats().faces };
    }, R);
    expect(setup.faces).toBeGreaterThan(2); // the circle really split the sphere

    // The snap cache refreshes on an idle callback — wait for it to be warm
    // rather than betting on a fixed sleep.
    await page.waitForFunction(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => ((window as any).__axia.get('toolManager').snap?.vertices?.length ?? 0) > 4,
      { timeout: 10000 },
    );

    const r = await page.evaluate((radius) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const tm = ax.get('toolManager'); const vp = ax.get('viewport');
      const cam = vp.activeCamera; const canvas = vp.renderer.domElement;
      const rect = canvas.getBoundingClientRect();
      const V3 = cam.position.constructor;

      // Pick a cached snap vertex that is ON the sphere and facing the camera.
      const verts: Array<{ x: number; y: number; z: number }> = tm.snap.vertices;
      const onSphere = verts.filter(
        (v) => Math.abs(Math.hypot(v.x, v.y, v.z) - radius) < 0.5 && v.x > radius * 0.5,
      );
      if (onSphere.length === 0) return { noTarget: true };
      const target = onSphere[0];

      const p = new V3(target.x, target.y, target.z).project(cam);
      const sx = rect.left + (p.x * 0.5 + 0.5) * rect.width;
      const sy = rect.top + (-p.y * 0.5 + 0.5) * rect.height;

      // ~7 px off the vertex — inside the snap threshold.
      const near = tm.get3DPoint({ clientX: sx + 7, clientY: sy - 5 });
      // Well away from anything, but still over the sphere.
      const far = tm.get3DPoint({ clientX: rect.left + rect.width * 0.5, clientY: rect.top + rect.height * 0.5 });

      return {
        canvasOk: rect.width > 10 && rect.height > 10,
        target: [target.x, target.y, target.z],
        near: near ? [near.x, near.y, near.z] : null,
        far: far ? [far.x, far.y, far.z] : null,
      };
    }, R);

    expect(r.noTarget).toBeFalsy();
    expect(r.canvasOk).toBe(true);

    // (1) the 7 px offset is erased — the point lands ON the existing vertex.
    expect(r.near).not.toBeNull();
    const d = Math.hypot(
      r.near![0] - r.target![0], r.near![1] - r.target![1], r.near![2] - r.target![2],
    );
    expect(d).toBeLessThan(0.5);

    // (2) and it is ON the sphere — the invariant, restated for a surface.
    expect(Math.abs(Math.hypot(r.near![0], r.near![1], r.near![2]) - R)).toBeLessThan(0.01);

    // (3) a click away from everything is the raw surface hit, still on the
    //     sphere and NOT dragged to the vertex.
    expect(r.far).not.toBeNull();
    expect(Math.abs(Math.hypot(r.far![0], r.far![1], r.far![2]) - R)).toBeLessThan(0.01);
    expect(
      Math.hypot(r.far![0] - r.target![0], r.far![1] - r.target![1], r.far![2] - r.target![2]),
    ).toBeGreaterThan(5);
  });

  test('the engine tells where a point lands on each curved surface', async ({ page }) => {
    const r = await page.evaluate((radius) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any; const ax = w.__axia;
      const bridge = ax.get('bridge');
      bridge.create_sphere(0, 0, 0, radius);
      const far = bridge.projectPointToFaceSurface(0, 0, radius * 3, 0);
      const inside = bridge.projectPointToFaceSurface(0, 10, 10, 10);
      const centre = bridge.projectPointToFaceSurface(0, 0, 0, 0);
      return {
        exists: typeof bridge.projectPointToFaceSurface === 'function',
        far: far ? Array.from(far as Float64Array) : null,
        inside: inside ? Array.from(inside as Float64Array) : null,
        centre: centre ? Array.from(centre as Float64Array) : null,
      };
    }, R);

    expect(r.exists).toBe(true);
    // Straight out along +Y from far away.
    expect(Math.abs(r.far![1] - R)).toBeLessThan(1e-6);
    // From inside, the same ray out of the centre.
    expect(Math.abs(Math.hypot(...(r.inside as number[])) - R)).toBeLessThan(1e-6);
    // The centre has no nearest point, and says so instead of guessing.
    expect(r.centre).toBeNull();
  });
});
