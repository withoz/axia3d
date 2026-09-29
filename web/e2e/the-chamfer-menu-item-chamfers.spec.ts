/**
 * The `엣지 챔퍼 (Chamfer)…` menu item cuts a facet.
 *
 * It did nothing until 2026-09-29. The handler called
 * `filletEdge(edge, distance, 1)` — sound geometry, wrong call, because
 * `fillet_edge` opens with `ensure!(segments >= 2)` and nothing on the way
 * down clamps it. Every wiring guard passed the whole time: the command is
 * registered, labelled, in both catalogs, and its handler does reach the
 * bridge. None of them asks whether the ENGINE ACCEPTS THE ARGUMENTS.
 *
 * Two source-level guards hold the halves (`the_chamfer_the_menu_offers_is_
 * the_chamfer_op` in axia-geo, `theChamferMenuItemChamfers` in vitest). This
 * one is here because neither can answer the question the user asks: does
 * clicking it do something. So it drives `executeAction('chamfer-edge')` —
 * the same entry the menu, the palette and the Capability Explorer all use —
 * in a real browser against the real engine.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first,
 * and `npm run build:wasm` too if the engine changed.
 */
import { test, expect } from '@playwright/test';

test.describe('엣지 챔퍼', () => {
  test('cuts one flat facet and leaves the solid closed', async ({ page }) => {
    // The handler asks for the distance with window.prompt.
    page.on('dialog', (d) => d.accept('20'));

    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge') && w.get('toolManager');
      },
      { timeout: 30000 },
    );

    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const c = (window as any).__axia;
      const bridge = c.get('bridge');
      const tm = c.get('toolManager');

      // A real solid, built the way the app builds one.
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
      const faces: number[] = bridge.getShapeFaceIds(shape);
      const built = faces.length > 0 && bridge.createSolidExtrude(faces[0], 200);
      const before = bridge.getStats().faces;

      // Every edge of the box qualifies; take the first the engine accepts.
      let used = -1;
      let after = before;
      for (let e = 0; e < 64 && used < 0; e++) {
        const ends = bridge.getEdgeEndpoints?.(e);
        if (!ends || ends.length < 2) continue;
        tm.selection.clearSelection();
        tm.selection.handleEdgeClick(e, false, false);
        if (tm.selection.getSelectedEdges().length !== 1) continue;
        tm.executeAction('chamfer-edge');
        const now = bridge.getStats().faces;
        if (now > before) { used = e; after = now; }
      }

      const inv = bridge.verifyInvariants();
      const outward = bridge.verifyOutwardNormals();
      return {
        built,
        before,
        after,
        used,
        valid: inv?.valid ?? inv?.isValid ?? null,
        violations: (inv?.violations ?? []).length,
        closed: outward?.isClosedSolid ?? null,
        lastError: bridge.lastError?.() ?? '',
      };
    });

    console.log('  chamfer:', JSON.stringify(r));
    expect(r.built, 'the box must be built before anything is chamfered').toBeTruthy();
    // PREMISE: an unbuilt or empty scene would make every count below vacuous.
    expect(r.before, 'a closed box has six faces').toBeGreaterThanOrEqual(6);

    expect(r.used, `no edge was chamfered — lastError: ${r.lastError}`).toBeGreaterThanOrEqual(0);
    expect(r.after, 'one flat facet replaces the edge').toBe(r.before + 1);
    expect(r.violations, 'and the mesh stays sound').toBe(0);
    expect(r.closed, 'and the solid stays closed').toBe(true);
  });
});
