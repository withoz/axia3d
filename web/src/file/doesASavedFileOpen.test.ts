/**
 * Does a file this app saves, open again in this app?
 *
 * Measured 2026-08-23, after a user hit a browser `prompt()` on save and asked
 * why. Reading the wiring first: there are TWO save paths and they write
 * DIFFERENT formats under the same `.xia` extension.
 *
 *   메뉴 "저장" / Ctrl+S        -> ProjectSerializer.saveProject
 *                                  JSON.stringify(project), text
 *   메뉴 "다른 이름으로 저장"    -> FileManager.saveAsProject
 *   / Ctrl+Shift+S                 [magic][version][len][meta][snapshot], binary
 *                                  (and a native prompt() — that dialog)
 *
 * And only ONE open path is wired:
 *
 *   메뉴 "열기"                 -> ProjectSerializer.openProject
 *                                  file.text() -> JSON.parse
 *
 * `FileManager.openProject()` has zero production callers — checked by grep
 * over `web/src` excluding tests and its own file, then again through every
 * module that receives a `fileManager` (ServiceContainer, InitialScene,
 * MenuBar). MenuBar touches it exactly once, for `saveAsProject()`.
 *
 * So the reading says one of the two save paths writes a file nothing in the
 * UI can read. This file MEASURES that rather than leaving it read-only, by
 * running both formats through both parsers.
 *
 * Each parser is exercised where the app actually parses:
 *   - FileManager  `loadFromArrayBuffer`, its public seam (used by the
 *                  existing FileManager.test.ts the same way)
 *   - ProjectSerializer  `JSON.parse(await file.text())`, which is the whole
 *                  of its parse step (ui/ProjectSerializer.ts, openProject's
 *                  onChange)
 */
import { describe, it, expect, vi } from 'vitest';
import { FileManager } from './FileManager';

/** Exactly what FileManager.createAxiaFile writes. */
function binaryAxia(metadata: unknown, snapshot: Uint8Array): Uint8Array {
  const metaBytes = new TextEncoder().encode(JSON.stringify(metadata));
  const buf = new Uint8Array(12 + metaBytes.length + snapshot.length);
  const view = new DataView(buf.buffer);
  view.setUint32(0, 0x41584941, true); // 'AXIA'
  view.setUint32(4, 2, true);
  view.setUint32(8, metaBytes.length, true);
  buf.set(metaBytes, 12);
  buf.set(snapshot, 12 + metaBytes.length);
  return buf;
}

/** Exactly what ProjectSerializer.saveProject writes. */
function jsonProject(): string {
  return JSON.stringify(
    {
      version: 1,
      mesh: 'AAECAw==', // base64, as fromBase64() expects
      groups: [],
      units: 'mm',
    },
    null,
    2,
  );
}

function stubBridge() {
  return {
    exportSnapshot: vi.fn(() => new Uint8Array([1, 2, 3])),
    importSnapshot: vi.fn(() => true),
    getStats: vi.fn(() => ({ vertices: 0, edges: 0, faces: 0 })),
  } as never;
}

/** ProjectSerializer's parse step, verbatim. */
function parseAsProjectSerializer(text: string): { ok: boolean; why?: string } {
  try {
    const project = JSON.parse(text);
    return { ok: typeof project === 'object' && project !== null };
  } catch (e) {
    return { ok: false, why: String(e).split('\n')[0] };
  }
}

describe('a file this app saves must open in this app', () => {
  it('저장 (JSON) round-trips through 열기', () => {
    const r = parseAsProjectSerializer(jsonProject());
    expect(r.ok, `the only wired open path must read the only wired save path: ${r.why}`).toBe(
      true,
    );
  });

  it('다른 이름으로 저장 (binary) does NOT open — nothing in the UI reads it', async () => {
    const bin = binaryAxia({ version: 2, timestamp: 'x', name: 'demo' }, new Uint8Array([9, 9]));

    // The binary file is well-formed: FileManager's own parser takes it.
    const fm = new FileManager(stubBridge());
    await expect(
      fm.loadFromArrayBuffer(bin, 'demo.xia'),
      'premise: the binary really is a valid AXIA file, so this is not a bad fixture',
    ).resolves.toBe(true);

    // ⚠ But that parser is unreachable from the UI, and the one that IS
    // reachable cannot read it: the first byte is 'A' (0x41), so JSON.parse
    // fails immediately.
    const asText = new TextDecoder().decode(bin);
    const r = parseAsProjectSerializer(asText);
    expect(
      r.ok,
      'if this ever passes, either the binary became JSON or 열기 learned to ' +
        'sniff the magic number — both would close the gap this test documents',
    ).toBe(false);
  });

  it('and the reverse: FileManager cannot read what 저장 writes', async () => {
    const fm = new FileManager(stubBridge());
    const bytes = new TextEncoder().encode(jsonProject());
    await expect(
      fm.loadFromArrayBuffer(bytes, 'saved.xia'),
      'the JSON file has no AXIA magic, so parseAxiaFile must refuse it',
    ).resolves.toBe(false);
  });
});
