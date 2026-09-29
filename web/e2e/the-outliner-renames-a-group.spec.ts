/**
 * Renaming a group in the Outliner reaches the engine.
 *
 * `ComponentPanel`'s header has listed 이름 편집 since the file was written and
 * nothing in it did that until 2026-09-29. `Command::RenameGroup`, the
 * `rename_group` export and the `renameGroup` bridge wrapper were all there;
 * the wrapper simply had zero callers.
 *
 * The panel half is held by `theOutlinerRenamesAGroup` in vitest, against a
 * mock bridge: it proves the button calls `renameGroup(id, name)`. A mock
 * cannot say whether the engine then renames anything, which is the half this
 * file measures — the same split that let the chamfer menu item look wired for
 * as long as it did.
 *
 * ⚠ Playwright serves `npm run preview`, a production build. Re-build first.
 */
import { test, expect } from '@playwright/test';

test.describe('a renamed group', () => {
  test('keeps the new name in the engine', async ({ page }) => {
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

      const shape = bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 400, 400);
      const faces: number[] = bridge.getShapeFaceIds(shape);
      const gid = faces.length > 0 ? bridge.createGroup('벽체', faces) : -1;
      const before = gid > 0 ? bridge.getGroupInfo(gid)?.name : null;

      const ok = gid > 0 ? bridge.renameGroup(gid, '기둥') : false;
      const after = gid > 0 ? bridge.getGroupInfo(gid)?.name : null;

      // And it survives a read through the listing the panel actually uses.
      const listed = (bridge.getAllGroups() ?? []).find(
        (g: { id: number }) => g.id === gid,
      )?.name;

      return { gid, before, ok, after, listed, lastError: bridge.lastError?.() ?? '' };
    });

    console.log('  rename:', JSON.stringify(r));
    // PREMISE: a group was made — without one the names below are all null and
    // every assertion would pass on nothing.
    expect(r.gid, 'a group must exist to rename').toBeGreaterThan(0);
    expect(r.before, 'and start with the name it was given').toBe('벽체');

    expect(r.ok, `renameGroup returned false — lastError: ${r.lastError}`).toBe(true);
    expect(r.after, 'the engine keeps the new name').toBe('기둥');
    expect(r.listed, 'and the listing the panel reads shows it').toBe('기둥');
  });
});
