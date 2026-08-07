/**
 * ADR-267 γ — the curved sketch-split had no integrity gate.
 *
 * Every other topology-mutating cut op runs under `integrity_gate_passed`
 * (punch / drill / carve / slice / door / split / trim). The 9 curved
 * sketch-split entry points did not: their only guard was `is_finite()` plus a
 * degenerate check. Measured through the production bridge, on ALL FOUR curved
 * primitives:
 *
 *     cylinder → circle → carve a pocket → circle again on the SAME rim
 *       ⇒ verifyInvariants: 55 violations ("shared by 3 active faces")
 *       ⇒ drawCircleOnCylinder returns {"cap":60,"annulus":2}  ← SUCCESS
 *       ⇒ no error, no panic, no console output
 *
 * The trigger is exact-radius coincidence: r=59.8 is fine, r=60.0 (the pocket's
 * own radius) corrupts. That is hard to hit with a mouse — pixel quantisation
 * lands on 60.2 — but the fail-open is real, and "returns success on a broken
 * mesh" is exactly what ADR-267 exists to prevent. LOCKED #88 Phase 3's rule is
 * to measure which ops corrupt and gate those; these do.
 *
 * NOTE: Playwright uses `npm run preview` (production build). Re-build
 * (`npm run build:wasm` after Rust changes, then `npm run build`) first.
 */
import { test, expect } from '@playwright/test';

/**
 * ⚠ 2026-08-07 — measured all four through the production bridge, one per fresh
 * page. Only the CYLINDER is corrupted by the repeat sketch:
 *
 *     cylinder  2nd circle REFUSED   59→59 rolled back, valid  ← real overlap
 *     sphere    2nd circle ok        26→27  valid, bnd 0, nm 0, si 0, closed
 *     cone      2nd circle ok        50→51  valid, bnd 0, nm 0, si 0, closed
 *     torus     2nd circle ok        49→50  valid, nm 0, si 0 (its seam boundary
 *                                            edge is there from creation)
 *
 * The three had been added to the same loop as the cylinder, but this file's own
 * header only ever cited the cylinder measurement. On those three the second
 * sketch damages nothing, so the gate letting it through is the correct answer
 * and asserting a refusal was asserting a defect that is not there.
 *
 * `corrupts` says which behaviour is expected. This is not a weaker guard: the
 * three still have to come out healthy, so if one of them ever starts stacking
 * faces the `valid` assertion fails and says so.
 */
const SURFACES = [
  { name: 'cylinder', kind: 2, centre: [200, 0, 200], rim: [200, 0, 260], corrupts: true },
  { name: 'sphere', kind: 3, centre: [200, 0, 0], rim: [190, 0, 60], corrupts: false },
  { name: 'cone', kind: 4, centre: [100, 0, 200], rim: [110, 0, 240], corrupts: false },
  { name: 'torus', kind: 5, centre: [380, 0, 0], rim: [380, 0, 50], corrupts: false },
] as const;

test.describe('ADR-267 γ — curved sketch-split integrity gate', () => {
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

  // One scenario per fresh page — stacking curved solids in one scene makes face
  // ids collide and produced a confidently wrong reading during this audit
  // (the same trap ADR-293 §7 records).
  for (const s of SURFACES) {
    const title = s.corrupts
      ? `${s.name}: a sketch that would corrupt is refused and rolled back`
      : `${s.name}: the repeat sketch damages nothing, so it is allowed through`;
    test(title, async ({ page }) => {
      const r = await page.evaluate((cfg) => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const ax = (window as any).__axia;
        const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
        if (cfg.name === 'cylinder') bridge.create_cylinder(0, 0, 0, 200, 400, 24);
        else if (cfg.name === 'sphere') bridge.create_sphere(0, 0, 0, 200, 24, 12);
        else if (cfg.name === 'cone') bridge.create_cone(0, 0, 0, 200, 400, 24);
        else bridge.create_torus(0, 0, 0, 300, 80, 24, 16);
        tm.syncMesh();
        let host = -1;
        for (let f = 0; f < 12; f++) if (bridge.faceSurfaceKind(f) === cfg.kind) { host = f; break; }
        const draw = (a: number[], b: number[]) =>
          cfg.name === 'cylinder' ? bridge.drawCircleOnCylinder(host, a, b)
            : cfg.name === 'sphere' ? bridge.drawCircleOnSphere(host, a, b)
              : cfg.name === 'cone' ? bridge.drawCircleOnCone(host, a, b)
                : bridge.drawCircleOnTorus(host, a, b);

        const c = [...cfg.centre], rim = [...cfg.rim];
        const first = draw(c, rim); tm.syncMesh();
        const p1 = JSON.parse(first);
        const walls = bridge.carveCurvedPocket(p1.cap, 40); tm.syncMesh();
        const mid = bridge.verifyInvariants();
        const facesBefore = bridge.getStats().faces;

        // the corrupting move: the same circle again, exactly on the pocket rim
        const second = draw(c, rim); tm.syncMesh();
        const after = bridge.verifyInvariants();
        return {
          host, walls, midValid: mid.valid, second,
          facesBefore, facesAfter: bridge.getStats().faces,
          valid: after.valid, viol: after.violationCount,
          si: bridge.detectSelfIntersections().count,
        };
      }, s);

      // sanity: the setup really did build a pocket on a clean solid
      expect(r.host, 'the curved host face must be found').toBeGreaterThanOrEqual(0);
      expect(r.walls, 'the pocket must exist for the repro to mean anything').toBeGreaterThan(0);
      expect(r.midValid).toBe(true);

      if (s.corrupts) {
        // the gate refuses instead of silently corrupting
        expect(r.second, 'the corrupting sketch must be refused').toContain('error');
        expect(r.second).toContain('무결성');
        // ...and leaves the model exactly as it was
        expect(r.facesAfter, 'rolled back — no face survives the refusal').toBe(r.facesBefore);
      } else {
        // nothing is damaged here, so refusing would be the defect
        expect(r.second, 'a sketch that damages nothing must go through').toContain('cap');
        expect(r.second).not.toContain('error');
        expect(r.facesAfter, 'the sketch adds its face').toBeGreaterThan(r.facesBefore);
      }
      // either way the model must come out healthy — this is what would catch a
      // future regression on the three that pass, and a bad rollback on the one
      // that refuses
      expect(r.valid, 'the model must still be valid').toBe(true);
      expect(r.viol).toBe(0);
      expect(r.si, 'no two faces may end up in the same place').toBe(0);
    });
  }

  test('the gate does not false-reject a legitimate curved sketch', async ({ page }) => {
    // A gate that refused honest work would be worse than the bug. The first
    // circle + its pocket + a second circle AWAY from the pocket must all pass.
    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const ax = (window as any).__axia;
      const bridge = ax.get('bridge'); const tm = ax.get('toolManager');
      bridge.create_cylinder(0, 0, 0, 200, 400, 24); tm.syncMesh();
      let side = -1;
      for (let f = 0; f < 10; f++) if (bridge.faceSurfaceKind(f) === 2) { side = f; break; }
      const first = bridge.drawCircleOnCylinder(side, [200, 0, 200], [200, 0, 260]);
      tm.syncMesh();
      const p1 = JSON.parse(first);
      const walls = bridge.carveCurvedPocket(p1.cap, 40); tm.syncMesh();
      // a second, honest circle well clear of the pocket
      const second = bridge.drawCircleOnCylinder(side, [200, 0, 360], [200, 0, 385]);
      tm.syncMesh();
      const inv = bridge.verifyInvariants();
      return { first, walls, second, valid: inv.valid, viol: inv.violationCount };
    });
    expect(r.first, 'a clean first sketch must pass the gate').toContain('cap');
    expect(r.walls).toBeGreaterThan(0);
    expect(r.second, 'a sketch clear of the pocket must pass the gate').toContain('cap');
    expect(r.second).not.toContain('error');
    expect(r.valid).toBe(true);
    expect(r.viol).toBe(0);
  });
});
