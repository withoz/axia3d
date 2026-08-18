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
    // the circle really split the sphere: 1 (axis-definition sphere) + 1 cap
    expect(setup.faces).toBeGreaterThan(1);

    // The snap cache refreshes on an idle callback — wait for it to be warm
    // rather than betting on a fixed sleep.
    await page.waitForFunction(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      () => ((window as any).__axia.get('toolManager').snap?.vertices?.length ?? 0) > 4,
      { timeout: 10000 },
    );

    // (The projection right after `setCameraState` used to be taken off a
    // camera whose matrices were a frame behind — fixed at the source, guarded
    // by `a-moved-camera-is-projectable-at-once.spec.ts`.)

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

      // How far is a result from the NEAREST snap candidate? A point that
      // landed on a vertex sits exactly on one; a raw ray-surface hit does not
      // (the chance of coinciding is nil). That is what separates "snapped"
      // from "not snapped" without naming which vertex won — the rim's spacing
      // is ~1 mm here, so a 7 px offset may well fall closer to a neighbour
      // than to the one this test picked out, and either is a snap.
      const distToNearestVertex = (q: { x: number; y: number; z: number } | null) =>
        q == null
          ? Infinity
          : Math.min(...verts.map((v) => Math.hypot(v.x - q.x, v.y - q.y, v.z - q.z)));

      return {
        canvasOk: rect.width > 10 && rect.height > 10,
        target: [target.x, target.y, target.z],
        near: near ? [near.x, near.y, near.z] : null,
        far: far ? [far.x, far.y, far.z] : null,
        nearSnapDist: distToNearestVertex(near),
        farSnapDist: distToNearestVertex(far),
      };
    }, R);

    expect(r.noTarget).toBeFalsy();
    expect(r.canvasOk).toBe(true);

    // (1) the 7 px offset is erased — the point lands ON a vertex, not at the
    // raw ray hit. Which vertex is not the claim: the rim's spacing is ~1 mm,
    // so the offset can fall nearer a neighbour than the one picked above, and
    // that is still a snap. Measured 2026-08-07 after the sphere became one
    // axis-defined face: it landed 1.115 from `target` and 0.000 from its
    // neighbour, with z identical to 14 decimals.
    expect(r.near).not.toBeNull();
    expect(r.nearSnapDist).toBeLessThan(1e-6);

    // (2) and it is ON the sphere — the invariant, restated for a surface.
    expect(Math.abs(Math.hypot(r.near![0], r.near![1], r.near![2]) - R)).toBeLessThan(0.01);

    // (3) a click away from everything is the raw surface hit, and NOT dragged
    //     to the vertex. It sits on the tessellated MESH, so it is inside the
    //     analytic sphere by up to the chord sag — measured 0.067 mm on CI at
    //     r=100, where the canvas is a different size and the ray lands
    //     mid-triangle rather than near a vertex. Only the SNAPPED point is
    //     analytically exact; asserting 0.01 mm on this one was asserting the
    //     tessellation, not the behaviour.
    expect(r.far).not.toBeNull();
    expect(Math.abs(Math.hypot(r.far![0], r.far![1], r.far![2]) - R)).toBeLessThan(1);
    expect(
      Math.hypot(r.far![0] - r.target![0], r.far![1] - r.target![1], r.far![2] - r.target![2]),
    ).toBeGreaterThan(5);
    // The control for (1): away from everything the point is NOT on a vertex.
    // Without this, "landed on a vertex" would also pass on a snap that dragged
    // every click to the nearest one.
    expect(r.farSnapDist).toBeGreaterThan(1e-6);
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
