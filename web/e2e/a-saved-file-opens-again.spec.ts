/**
 * A file this app saves must open again in this app — in a real browser.
 *
 * A user hit a native `prompt()` on save and asked why. It turned out the File
 * menu has TWO save entries writing DIFFERENT formats under one `.xia`
 * extension, and 열기 read only one of them:
 *
 *   메뉴 "저장" / Ctrl+S        JSON            열림
 *   "다른 이름으로 저장"         binary AXIA     열리지 않음
 *   / Ctrl+Shift+S
 *
 * `열기` now sniffs the four magic bytes and routes binary to FileManager,
 * which is the only parser that reads it.
 *
 * ⚠ Why this exists on top of the vitest coverage: those tests supply
 * `File.text()` and `File.arrayBuffer()` themselves, because jsdom has NEITHER.
 * A polyfill can hide a real-browser difference, and the whole reported problem
 * was real-browser behaviour. This drives the actual file input, with the
 * browser's own File API, through the real `openProject()`.
 *
 * The repo's other project round-trip (`project-roundtrip.spec.ts`) states it
 * deliberately avoids the DOM file dialog. This is that gap, for this one case.
 */
import { test, expect } from '@playwright/test';
import { waitForBridgeReady } from './helpers/boolean-fixtures';

/** Build a binary .xia in the page, exactly as FileManager.createAxiaFile does. */
async function binaryXiaBytes(page: import('@playwright/test').Page): Promise<Buffer> {
  const arr = await page.evaluate(() => {
    const bridge = (window as unknown as { __axia?: { get(k: string): unknown } }).__axia?.get(
      'bridge',
    ) as { exportSnapshot(): Uint8Array | null };
    const snapshot = bridge.exportSnapshot();
    if (!snapshot) throw new Error('exportSnapshot returned null');
    const meta = { version: 2, timestamp: new Date().toISOString(), name: 'e2e' };
    const metaBytes = new TextEncoder().encode(JSON.stringify(meta));
    const out = new Uint8Array(12 + metaBytes.length + snapshot.length);
    const view = new DataView(out.buffer);
    view.setUint32(0, 0x41584941, true);
    view.setUint32(4, 2, true);
    view.setUint32(8, metaBytes.length, true);
    out.set(metaBytes, 12);
    out.set(snapshot, 12 + metaBytes.length);
    return Array.from(out);
  });
  return Buffer.from(arr);
}

async function faceCount(page: import('@playwright/test').Page): Promise<number> {
  return page.evaluate(() => {
    const b = (window as unknown as { __axia?: { get(k: string): unknown } }).__axia?.get(
      'bridge',
    ) as { getStats(): { faces: number } };
    return b.getStats().faces;
  });
}

test.describe('a saved file opens again', () => {
  test('a binary .xia opens through 열기 and reaches the engine', async ({ page }) => {
    await page.goto('/');
    await waitForBridgeReady(page);

    // Something to save, so the snapshot is not empty.
    await page.evaluate(() => {
      const bridge = (window as unknown as { __axia?: { get(k: string): unknown } }).__axia?.get(
        'bridge',
      ) as { drawRectAsShape(...a: number[]): number };
      bridge.drawRectAsShape(0, 0, 0, 0, 0, 1, 1, 0, 0, 300, 200);
    });
    const before = await faceCount(page);
    expect(before, 'premise: there is geometry to save').toBeGreaterThan(0);

    const bytes = await binaryXiaBytes(page);
    // PREMISE: it really is the binary format, not JSON.
    expect(bytes[0], "first byte must be 'A' of the magic").toBe(0x41);

    // ⚠ There is no general "clear scene" on the bridge, so a first version of
    // this test 'wiped' with `clearAll?.() ?? newScene?.()` — neither exists,
    // the call did nothing, and the test passed WITH THE FIX DISABLED. Instead:
    // change the face count after saving, and require the open to put it back
    // to exactly the saved number. Only a real import can do that.
    await page.evaluate(() => {
      const b = (window as unknown as { __axia?: { get(k: string): unknown } }).__axia?.get(
        'bridge',
      ) as { drawRectAsShape(...a: number[]): number };
      b.drawRectAsShape(900, 900, 0, 0, 0, 1, 1, 0, 0, 100, 100);
    });
    const dirtied = await faceCount(page);
    expect(dirtied, 'premise: the scene must now differ from what was saved').toBeGreaterThan(
      before,
    );

    // Drive the real 열기. openProject() creates a hidden input and clicks it.
    const chooserPromise = page.waitForEvent('filechooser');
    await page.evaluate(() => {
      document.querySelector<HTMLElement>('[data-action="file-open"]')?.click();
    });
    const chooser = await chooserPromise;
    await chooser.setFiles({
      name: 'e2e.xia',
      mimeType: 'application/octet-stream',
      buffer: bytes,
    });

    await expect
      .poll(() => faceCount(page), {
        message:
          'the binary file must reach the engine and restore the SAVED face count — ' +
          'before the fix 열기 handed it to JSON.parse and the load died on the first byte',
        timeout: 10_000,
      })
      .toBe(before);
  });
});
