/**
 * ADR-278 follow-up — Path B curved-primitive Boolean SUBTRACT (real Chromium).
 *
 * ADR-278 β made a Path B (analytic self-loop) CYLINDER cut when subtracted
 * (polygonalize the operand at the `boolean_solid` entry → v2 imprint). Sphere/
 * cone/torus were a silent no-op (the box returned unchanged — a real user-
 * facing gap since production defaults Path B ON). This extends the fix to
 * sphere/cone/torus so `booleanSolid(box, curved, 'subtract')` CUTS watertight.
 *
 * Engine authority: axia-geo `adr278_pathb_sphere_cone_torus_subtract_cuts`.
 * This is the real-Chromium tool-path 시연: fresh scene → face ids are 0-based
 * contiguous (box 0..5, then the Path B primitive), so we split by count and
 * subtract via the general `booleanSolid` (the same entry BooleanHandler uses).
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

test.describe('ADR-278 follow-up — Path B sphere/cone/torus subtract cuts', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForBridgeReady(page);
  });

  const subtract = (kind: 'sph' | 'con' | 'tor') => async ({ page }: { page: import('@playwright/test').Page }) => {
    const r = await page.evaluate((k) => {
      const w = window as unknown as AxiaWindow;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = w.__axia!.get<any>('bridge');
      bridge.setSpherePathBDefault?.(true);
      bridge.setConePathBDefault?.(true);
      bridge.setTorusPathBDefault?.(true);
      // Box spans z∈[0,120] (create_box center z=60, height 120... use center 50/120).
      bridge.create_box(0, 0, 50, 120, 120, 120);
      const boxN = bridge.getStats().faces; // 6
      const boxFaces = Array.from({ length: boxN }, (_, i) => i);
      if (k === 'sph') bridge.create_sphere(40, 40, 110, 40, 16, 12);
      else if (k === 'con') bridge.create_cone(30, 30, 60, 40, 160, 24);
      // torus THROUGH the box middle (z=50) — a clean toroidal cut. (z=110
      // grazing the top face is tangential → self-intersects → gate rejects.)
      else bridge.create_torus(0, 0, 50, 40, 15);
      const totalN = bridge.getStats().faces;
      const curvedFaces = Array.from({ length: totalN - boxN }, (_, i) => boxN + i);
      const before = totalN;
      bridge.booleanSolid(new Uint32Array(boxFaces), new Uint32Array(curvedFaces), 'subtract');
      const after = bridge.getStats().faces;
      const outward = bridge.verifyOutwardNormals();
      const inv = bridge.verifyInvariants();
      return {
        curvedCount: curvedFaces.length, before, after,
        cut: after !== before,
        isClosedSolid: outward.isClosedSolid, valid: inv.valid, viol: inv.violationCount,
      };
    }, kind);
    expect(r.curvedCount).toBeGreaterThan(0);   // Path B primitive built
    expect(r.cut).toBe(true);                   // subtract actually cut
    expect(r.isClosedSolid).toBe(true);         // watertight result
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  };

  test('box − Path B sphere = watertight cut', subtract('sph'));
  test('box − Path B cone = watertight cut', subtract('con'));
  test('box − Path B torus = watertight cut', subtract('tor'));
});
