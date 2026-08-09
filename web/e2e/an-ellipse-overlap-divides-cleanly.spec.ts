/**
 * AN ELLIPSE THAT OVERLAPS SOMETHING DIVIDES IT — through the real bridge.
 *
 * Engine-side this is settled by `an_ellipse_overlap_divides_cleanly` and by
 * three unit tests over `is_reverse_twin`. What those cannot show is that the
 * production path reaches it: the arrangement only runs on a draw, behind two
 * scene flags that the browser sets from localStorage, and the fix lives inside
 * `union_outline` several layers below `drawEllipseAsCurve`.
 *
 * Measured in real Chromium BEFORE the fix, on a box's top face:
 *
 *   rect then circle    9 faces, 0 violations, closed        ✓
 *   rect then ellipse   9 faces, 4 violations, OPEN          ✗
 *
 * The circle is kept here as the control. Without it a spec that passes
 * everything looks exactly like a spec that checks nothing.
 *
 * ⚠ `drawEllipseAsCurve` takes ref_dir BEFORE normal
 * (cx,cy,cz, rdx,rdy,rdz, nx,ny,nz, rX,rY). Passing them the other way round
 * draws on a vertical plane and reports a clean result for the wrong reason.
 *
 * NOTE: Playwright uses `npm run preview` (production build) — re-build first.
 */
import { test, expect } from '@playwright/test';

const TOP_Z = 100;

test.describe('an ellipse overlap divides cleanly (production path)', () => {
  test.beforeEach(async ({ page }) => {
    // The arrangement and the auto-intersect are what divide an overlap; both
    // are ON in production via these keys (ADR-176), and a fresh test profile
    // has no localStorage, so set them before the app boots.
    await page.addInitScript(() => {
      localStorage.setItem('axia:auto-intersect-on-draw', 'true');
      localStorage.setItem('axia:auto-face-synthesis-on-draw', 'true');
      localStorage.setItem('axia:face-rederive-on-draw', 'true');
      localStorage.setItem('axia:freeform-overlap-on-draw', 'true');
    });
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

  for (const shape of ['ellipse', 'circle'] as const) {
    test(`box top + rect + ${shape} overlapping it${shape === 'circle' ? ' (control)' : ''}`, async ({
      page,
    }) => {
      const out = await page.evaluate(
        ({ kind, z }) => {
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const b = (window as any).__axia.get('bridge');
          b.create_box(0, 0, 50, 200, 100, 200);
          b.drawRectAsShape(0, 0, z, 0, 0, 1, 0, 1, 0, 80, 80);
          const drawn =
            kind === 'ellipse'
              ? // centre, ref_dir, normal, rX, rY — ref_dir comes FIRST
                b.drawEllipseAsCurve(30, 30, z, 1, 0, 0, 0, 0, 1, 40, 25)
              : b.drawCircleAsCurve(30, 30, z, 0, 0, 1, 40);
          const inv = b.verifyInvariants();
          const norm = b.verifyOutwardNormals();
          return {
            drawn,
            faces: b.getStats().faces,
            violations: inv.violations.length,
            valid: inv.valid,
            closed: norm.isClosedSolid,
            firstViolations: inv.violations.slice(0, 3),
          };
        },
        { kind: shape, z: TOP_Z },
      );

      // The premise: something WAS drawn. A refusal would make the rest vacuous.
      expect(out.drawn, `${shape} must be drawn, got ${out.drawn}`).toBeGreaterThan(0);
      // The top is cut into regions — the rect and the round shape each leave
      // pieces, so the box's six faces become nine.
      expect(out.faces, `${shape}: the top is divided`).toBe(9);
      // And nothing covers the same ground twice. The ellipse used to report 4.
      expect(
        out.violations,
        `${shape}: ${JSON.stringify(out.firstViolations)}`,
      ).toBe(0);
      expect(out.valid).toBe(true);
      expect(out.closed, `${shape}: the solid stays closed`).toBe(true);
    });
  }
});
