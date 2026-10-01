/**
 * The three group edits reach the engine.
 *
 * `addFacesToGroup`, `removeFacesFromGroup` and `setGroupParent` had zero
 * callers until 2026-10-01: the UI could create a group and dissolve one, and
 * nothing in between. ⚠ The panel RENDERS a nested tree while `create_group`
 * leaves `parent` unset and nothing called `set_parent` — it drew a tree the
 * user could not build.
 *
 * The panel half is held by `theOutlinerEditsAGroup` in vitest, against a mock
 * bridge: it proves the buttons call the right op with the right arguments. A
 * mock cannot say whether the engine then changes anything, which is the half
 * this file measures — the same split that let the chamfer menu item look wired
 * for as long as it did.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('the three group edits', () => {
  test('change what a group holds, and nest one inside another', async ({ page }) => {
    test.setTimeout(60_000);
    await page.goto('/');
    await page.waitForFunction(
      () => {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const w = (window as any).__axia;
        return w && typeof w.get === 'function' && w.get('bridge');
      },
      { timeout: 30000 },
    );

    const r = await page.evaluate(() => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const bridge = (window as any).__axia.get('bridge');

      // A real solid, built the way the app builds one, so the faces are real.
      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
      const base: number[] = bridge.getShapeFaceIds(shape);
      const built = base.length > 0 && bridge.createSolidExtrude(base[0], 200);
      // Four real face ids off the solid we just built.
      const faces: number[] = [];
      for (let f = 0; f < 64 && faces.length < 4; f++) {
        if (bridge.faceSurfaceKind?.(f) >= 0) faces.push(f);
      }

      const outer = bridge.createGroup('벽체', faces.slice(0, 1));
      const inner = bridge.createGroup('기둥', faces.slice(1, 2));
      // ⚠ `faceCount` is the field the panel DISPLAYS and the engine does not
      // send — the bridge fills it from `faceIds`. Reading it here exercises
      // that fix end to end; `faceIds.length` is carried alongside as the
      // independent witness.
      const before = bridge.getGroupInfo(outer)?.faceCount;
      const beforeIds = bridge.getGroupInfo(outer)?.faceIds?.length;

      const added = bridge.addFacesToGroup(outer, faces.slice(2, 4));
      const afterAdd = bridge.getGroupInfo(outer)?.faceCount;

      const removed = bridge.removeFacesFromGroup(outer, faces.slice(2, 3));
      const afterRemove = bridge.getGroupInfo(outer)?.faceCount;

      const nested = bridge.setGroupParent(inner, outer);
      const childParent = bridge.getGroupInfo(inner)?.parent;
      const parentChildren = bridge.getGroupInfo(outer)?.children ?? [];

      // The cycle the recursive render depends on being impossible.
      const cycle = bridge.setGroupParent(outer, inner);

      return {
        built, faceCount: faces.length, outer, inner,
        before, beforeIds, added, afterAdd, removed, afterRemove,
        nested, childParent, parentChildren, cycle,
        lastError: bridge.lastError?.() ?? '',
      };
    });

    console.log('  group edits:', JSON.stringify(r));
    // PREMISE: a solid and two groups exist — without them every count below
    // is about nothing.
    expect(r.built, 'the solid must be built').toBeTruthy();
    expect(r.faceCount, 'four real faces to move around').toBeGreaterThanOrEqual(4);
    expect(r.outer, 'outer group').toBeGreaterThan(0);
    expect(r.inner, 'inner group').toBeGreaterThan(0);
    expect(r.before, 'outer starts with one face — and faceCount is filled, not undefined').toBe(1);
    expect(r.beforeIds, 'the witness agrees').toBe(1);

    expect(r.added, `addFacesToGroup — lastError: ${r.lastError}`).toBe(true);
    expect(r.afterAdd, 'two more went in').toBe(3);
    expect(r.removed, 'removeFacesFromGroup').toBe(true);
    expect(r.afterRemove, 'one came out').toBe(2);

    expect(r.nested, 'setGroupParent').toBe(true);
    expect(r.childParent, 'the child knows its parent').toBe(r.outer);
    expect(r.parentChildren, 'and the parent knows its child — the panel reads this side')
      .toContain(r.inner);

    expect(r.cycle, 'and a cycle is refused, which the recursive render needs').toBe(false);
  });
});
