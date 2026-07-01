/**
 * ADR-263 γ — P3-C Cone + Torus wall circle sketching closure:
 * drawCircleOnCone / drawCircleOnTorus E2E.
 *
 * End-to-end regression (real Chromium + production build + compiled WASM)
 * for drawing closed "porthole" circles ON a Cone side face / Torus face
 * (cap / remainder split, both inheriting the host surface). Completes the
 * curved-sketch foundation (Sphere ADR-202 / Cylinder ADR-257 / Cone+Torus
 * ADR-263) — all 4 curved primitives sketchable.
 *
 * Layering of the coverage:
 *   - Rust unit tests lock the ENGINE geometry (project_to_cone/torus +
 *     circle_on_cone/torus, β-1/β-4) + split (split_cone/torus_face_by_circle,
 *     β-2/β-5, manifold + outward + A-χ inheritance) + UV-earcut render
 *     (tessellate_cone/torus_circle_clipped, β-2/β-5; torus is doubly-periodic).
 *   - axia-core locks the Scene dual-path ownership + single-Undo (β-3/β-6).
 *   - vitest locks the DrawCircleTool surfaceKind===4/5 DISPATCH (mocked bridge).
 *   - THIS spec is the end-to-end link: the WASM engine + WasmBridge produce a
 *     correct cap/remainder split in a real browser, both faces inheriting the
 *     Cone/Torus surface and the solid staying full 3D.
 *
 * Only mangling-safe bridge methods are used (object property names survive the
 * production minify; local vars don't — same contract as every other E2E spec).
 *
 * NOTE: Playwright uses `npm run preview` (production build) per
 * playwright.config.ts. Re-build the prod bundle (`npm run build`) AFTER any
 * WASM rebuild for this spec to pick up the latest engine.
 */
import { test, expect } from '@playwright/test';

test.describe('ADR-263 — circle sketching on cone + torus walls', () => {
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

  test('cone wall porthole → cap + remainder split (both Cone, manifold, full 3D)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setConePathBDefault?.(true);
      // cone r=500 base, h=1000: base disk (Plane) + side (Cone). Apex degenerate.
      bridge.create_cone(0, 0, 0, 500, 1000, 32);
      const before = bridge.getStats().faces;
      // find the Cone side face (kind === 3 cone... actual enum: 4 = Cone).
      let side = -1;
      for (let i = 0; i < 40; i++) {
        if (bridge.faceSurfaceKind(i) === 4) { side = i; break; }
      }
      // porthole at angle 0, mid-height on the wall (z=500 → cone radius 250).
      const res = JSON.parse(bridge.drawCircleOnCone(side, [250, 0, 500], [250, 0, 600]));
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      const capKind = bridge.faceSurfaceKind(res.cap);
      const annKind = bridge.faceSurfaceKind(res.annulus);
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let zmin = 1e9, zmax = -1e9;
      for (let i = 0; i < pos.length; i += 3) {
        zmin = Math.min(zmin, pos[i + 2]);
        zmax = Math.max(zmax, pos[i + 2]);
      }
      return {
        before, after, side, cap: res.cap, annulus: res.annulus,
        valid: inv.valid, viol: inv.violationCount, capKind, annKind, zmin, zmax,
      };
    });
    expect(r.side).toBeGreaterThanOrEqual(0);   // a Cone side face exists
    expect(r.before).toBe(2);                   // Path B cone = base + side
    expect(r.after).toBe(3);                    // + cap (remainder = host side)
    expect(r.annulus).toBe(r.side);             // remainder is the host side face
    expect(r.capKind).toBe(4);                  // Cone (A-χ inheritance)
    expect(r.annKind).toBe(4);                  // Cone (host keeps its surface)
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // the cone stays a full 3D solid (z spans ~[0, 1000]), NOT collapsed.
    expect(r.zmax - r.zmin).toBeGreaterThan(900);
  });

  test('torus wall porthole → cap + remainder split (both Torus, manifold, full 3D)', async ({ page }) => {
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const bridge = w.__axia.get('bridge');
      bridge.setTorusPathBDefault?.(true);
      // torus R=500, r=100 → single Torus face (Path B).
      bridge.create_torus(0, 0, 0, 500, 100);
      const before = bridge.getStats().faces;
      let side = -1;
      for (let i = 0; i < 40; i++) {
        if (bridge.faceSurfaceKind(i) === 5) { side = i; break; }
      }
      // porthole near the outer equator (600,0,0); radius point a small offset.
      const res = JSON.parse(bridge.drawCircleOnTorus(side, [600, 0, 0], [580, 0, 80]));
      const after = bridge.getStats().faces;
      const inv = bridge.verifyInvariants();
      const capKind = bridge.faceSurfaceKind(res.cap);
      const annKind = bridge.faceSurfaceKind(res.annulus);
      w.__axia.get('toolManager')?.syncMesh?.();
      const buf = bridge.getMeshBuffers();
      const pos = buf?.positions || buf?.position || [];
      let zmin = 1e9, zmax = -1e9, rmax = 0;
      for (let i = 0; i < pos.length; i += 3) {
        const x = pos[i], y = pos[i + 1], z = pos[i + 2];
        zmin = Math.min(zmin, z);
        zmax = Math.max(zmax, z);
        rmax = Math.max(rmax, Math.sqrt(x * x + y * y));
      }
      return {
        before, after, side, cap: res.cap, annulus: res.annulus,
        valid: inv.valid, viol: inv.violationCount, capKind, annKind, zmin, zmax, rmax,
      };
    });
    expect(r.side).toBeGreaterThanOrEqual(0);   // a Torus face exists
    expect(r.before).toBe(1);                   // Path B torus = single face
    expect(r.after).toBe(2);                    // + cap (remainder = host)
    expect(r.annulus).toBe(r.side);             // remainder is the host face
    expect(r.capKind).toBe(5);                  // Torus (A-χ inheritance)
    expect(r.annKind).toBe(5);                  // Torus (host keeps its surface)
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
    // the torus stays full 3D — tube spans z ∈ ~[-100,100], outer radius ~600.
    expect(r.zmax - r.zmin).toBeGreaterThan(150);
    expect(r.rmax).toBeGreaterThan(550);
  });
});
