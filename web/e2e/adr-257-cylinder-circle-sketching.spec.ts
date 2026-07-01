/**
 * ADR-257 γ — P3-B Cylinder-wall circle sketching closure: drawCircleOnCylinder E2E.
 *
 * End-to-end regression coverage (real Chromium + production build +
 * compiled WASM engine) for drawing closed geodesic "porthole" circles ON a
 * cylinder SIDE face (cap / remainder split, both inheriting the Cylinder
 * surface). The cylinder analogue of the sphere MVP (adr-202 spec).
 *
 * Layering of the coverage:
 *   - Rust unit tests lock the ENGINE geometry — split_cylinder_face_by_circle
 *     (β-3, manifold + outward + A-χ inheritance) and the UV-earcut render
 *     tessellate_cylinder_circle_clipped (β-4, cap bounded + remainder excludes
 *     the porthole, 5-probe de-risk).
 *   - vitest locks the DrawCircleTool cylinder DISPATCH (β-7, mocked bridge).
 *   - THIS spec is the missing end-to-end link: the WASM-compiled engine +
 *     WasmBridge produce a correct cap/remainder split in a real browser, with
 *     both faces inheriting the Cylinder surface and the solid staying full 3D.
 *
 * Only mangling-safe bridge methods are used (object property names survive
 * the production minify; local vars don't — same contract every other E2E
 * spec relies on).
 *
 * Geometry: create_cylinder spans z ∈ [cz, cz+height] (extrudes UP from
 * center, ADR-103 Z-up). A wall point at radius R is (R·cosθ, R·sinθ, z)
 * regardless of ref_dir; the engine projects clicks onto the cylinder anyway.
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any
 * WASM rebuild for this spec to pick up the latest engine.
 */
import { test, expect } from '@playwright/test';

// A cylinder R=10, h=20 at the origin → wall at x²+y²=100, z ∈ [0,20].
// Porthole center at angle 0, mid-height; radius point at angular offset 0.4
// (geodesic radius ρ = R·0.4 = 4 mm — a small porthole well inside the wall).
const CENTER: [number, number, number] = [10, 0, 10];
const RADIUS_PT: [number, number, number] = [10 * Math.cos(0.4), 10 * Math.sin(0.4), 10];

test.describe('ADR-257 — circle sketching on a cylinder wall', () => {
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

  test('single porthole → cap + remainder split (both Cylinder, manifold, full 3D solid)', async ({ page }) => {
    const r = await page.evaluate(
      ([center, radiusPt]) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = window as any;
        const bridge = w.__axia.get('bridge');
        bridge.setCylinderPathBDefault?.(true);
        bridge.create_cylinder(0, 0, 0, 10, 20, 16);
        const before = bridge.getStats().faces;
        // find the Cylinder side face (kind === 2); base/top are Plane (kind 1).
        let side = -1;
        for (let i = 0; i < 40; i++) {
          if (bridge.faceSurfaceKind(i) === 2) { side = i; break; }
        }
        const res = JSON.parse(bridge.drawCircleOnCylinder(side, center, radiusPt));
        const after = bridge.getStats().faces;
        const inv = bridge.verifyInvariants();
        const capKind = bridge.faceSurfaceKind(res.cap);
        const annKind = bridge.faceSurfaceKind(res.annulus);
        w.__axia.get('toolManager')?.syncMesh?.();
        const buf = bridge.getMeshBuffers();
        const pos = buf?.positions || buf?.position || [];
        let zmin = 1e9, zmax = -1e9, onWall = 0;
        for (let i = 0; i < pos.length; i += 3) {
          const x = pos[i], y = pos[i + 1], z = pos[i + 2];
          zmin = Math.min(zmin, z);
          zmax = Math.max(zmax, z);
          if (Math.abs(Math.sqrt(x * x + y * y) - 10) < 0.1) onWall++;
        }
        return {
          before, after, side, cap: res.cap, annulus: res.annulus,
          valid: inv.valid, viol: inv.violationCount, capKind, annKind, zmin, zmax, onWall,
        };
      },
      [CENTER, RADIUS_PT] as const,
    );
    expect(r.side).toBeGreaterThanOrEqual(0);   // a Cylinder side face exists
    expect(r.before).toBe(3);                   // Path B cylinder = base + top + side
    expect(r.after).toBe(4);                    // + cap (remainder = host side)
    expect(r.annulus).toBe(r.side);             // remainder is the host side face
    expect(r.capKind).toBe(2);                  // Cylinder (A-χ inheritance)
    expect(r.annKind).toBe(2);                  // Cylinder (host keeps its surface)
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // the cylinder stays a full 3D solid (z spans ~[0, 20]), NOT collapsed.
    expect(r.zmin).toBeLessThan(1);
    expect(r.zmax).toBeGreaterThan(19);
    // the porthole cap + remainder render ON the cylinder wall (radius 10).
    expect(r.onWall).toBeGreaterThanOrEqual(10);
  });

  test('two separated portholes on the wall → host carries 2 holes, manifold, full 3D', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setCylinderPathBDefault?.(true);
      bridge.create_cylinder(0, 0, 0, 10, 20, 16);
      let side = -1;
      for (let i = 0; i < 40; i++) {
        if (bridge.faceSurfaceKind(i) === 2) { side = i; break; }
      }
      // porthole #1 at angle 0, porthole #2 at angle π (opposite wall) — separated.
      const r1 = JSON.parse(bridge.drawCircleOnCylinder(side, [10, 0, 10], [10 * Math.cos(0.4), 10 * Math.sin(0.4), 10]));
      const host = r1.annulus;
      const r2 = JSON.parse(bridge.drawCircleOnCylinder(host, [-10, 0, 10], [-10 * Math.cos(0.4), -10 * Math.sin(0.4), 10]));
      const stats = bridge.getStats();
      const inv = bridge.verifyInvariants();
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let zmin = 1e9, zmax = -1e9;
      for (let i = 0; i < pos.length; i += 3) {
        zmin = Math.min(zmin, pos[i + 2]);
        zmax = Math.max(zmax, pos[i + 2]);
      }
      return { faces: stats.faces, valid: inv.valid, viol: inv.violationCount, host, cap2: r2.cap, zmin, zmax };
    });
    expect(r.faces).toBe(5);   // base + top + remainder + 2 caps
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    expect(r.zmin).toBeLessThan(1);
    expect(r.zmax).toBeGreaterThan(19);
  });
});
