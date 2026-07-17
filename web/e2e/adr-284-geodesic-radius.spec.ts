/**
 * ADR-284 follow-up — a TYPED radius on a curved host means what it says.
 *
 * circle_on_* take a centre and a radius POINT and derive the radius as the
 * distance along the surface. Fine for a mouse. For a typed "50" the caller has
 * to produce a point whose GEODESIC distance is 50, and offsetting 50 in the
 * tangent plane is not that: measured here, it lands at 48.996 on a r=200
 * cylinder — 2% short — and ~7% short at 100. So this path first drew a flat
 * circle regardless, then declined (7c6e4c2) rather than approximate. The
 * engine now answers it exactly (`surfacePointAtGeodesicDistance`).
 *
 * NOTE: Playwright uses `npm run preview` (production build). Re-build
 * (`npm run build:wasm` after Rust changes, then `npm run build`) first.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-284 follow-up — geodesic radius from a typed value', () => {
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

  test('cylinder: the point is ON the surface at exactly the typed distance', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      bridge.create_cylinder(0, 0, 0, 200, 400, 24); tm.syncMesh();
      let side = -1;
      for (let f = 0; f < 10; f++) if (bridge.faceSurfaceKind(f) === 2) { side = f; break; }
      const centre: [number, number, number] = [200, 0, 200];
      const d = 50;
      const rp = bridge.surfacePointAtGeodesicDistance(side, centre, d);
      const split = rp ? bridge.drawCircleOnCylinder(side, centre, rp) : null;
      tm.syncMesh();
      const inv = bridge.verifyInvariants();
      return {
        side, rp,
        axialOffset: rp ? rp[2] - centre[2] : null,
        axisDistance: rp ? Math.hypot(rp[0], rp[1]) : null,
        split, faces: bridge.getStats().faces, valid: inv.valid,
        // what the tangent-plane shortcut would have measured
        naive: 200 * Math.atan(d / 200),
      };
    });

    expect(r.side, 'the cylinder side face must be found').toBeGreaterThanOrEqual(0);
    expect(r.rp, 'the engine must answer for a Cylinder host').not.toBeNull();
    // exact, not approximate: the axial offset IS the geodesic distance here
    expect(r.axialOffset!).toBeCloseTo(50, 6);
    expect(r.axisDistance!, 'and the point stays on the surface').toBeCloseTo(200, 6);
    // the whole reason this exists
    expect(r.naive, 'the tangent-plane shortcut falls ~2% short').toBeLessThan(49.1);
    // and the point is usable: the engine splits with it
    expect(r.split).toContain('cap');
    expect(r.faces).toBe(4);
    expect(r.valid).toBe(true);
  });

  test('sphere: r·sin(d/r) — the engine agrees with its own metric', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      bridge.create_sphere(0, 0, 0, 200, 24, 12); tm.syncMesh();
      let host = -1;
      for (let f = 0; f < 10; f++) if (bridge.faceSurfaceKind(f) === 3) { host = f; break; }
      const centre: [number, number, number] = [200, 0, 0];
      const d = 50;
      const rp = bridge.surfacePointAtGeodesicDistance(host, centre, d);
      const split = rp ? bridge.drawCircleOnSphere(host, centre, rp) : null;
      tm.syncMesh();
      const inv = bridge.verifyInvariants();
      return {
        host, rp,
        radiusFromCentre: rp ? Math.hypot(rp[0], rp[1], rp[2]) : null,
        // the swept angle must be exactly d/r
        sweptAngle: rp ? Math.acos((rp[0] * 200) / (200 * Math.hypot(rp[0], rp[1], rp[2]))) : null,
        split, valid: inv.valid,
      };
    });

    expect(r.host).toBeGreaterThanOrEqual(0);
    expect(r.rp).not.toBeNull();
    expect(r.radiusFromCentre!, 'still on the sphere').toBeCloseTo(200, 5);
    expect(r.sweptAngle! * 200, 'geodesic distance = r·α = d').toBeCloseTo(50, 4);
    expect(r.split).toContain('cap');
    expect(r.valid).toBe(true);
  });

  test('refuses what it cannot answer, rather than guessing', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      const seed = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 200, 200);
      tm.syncMesh();
      bridge.create_sphere(2000, 0, 0, 10, 24, 12); tm.syncMesh();
      let sph = -1;
      for (let f = 0; f < 20; f++) if (bridge.faceSurfaceKind(f) === 3) { sph = f; break; }
      return {
        // a planar face has no geodesic-vs-chord gap — the caller uses its own path
        planar: bridge.surfacePointAtGeodesicDistance(seed, [0, 0, 0], 50),
        // d = 0 is not a radius
        zero: bridge.surfacePointAtGeodesicDistance(sph, [2010, 0, 0], 0),
        // past half a turn the circle wraps the far pole — an ambiguity, not an answer
        pastHalfTurn: bridge.surfacePointAtGeodesicDistance(sph, [2010, 0, 0], 100),
      };
    });
    expect(r.planar, 'a plane is answered by the planar path, not this one').toBeNull();
    expect(r.zero).toBeNull();
    expect(r.pastHalfTurn, 'd > πr must be refused, not wrapped').toBeNull();
  });
});
