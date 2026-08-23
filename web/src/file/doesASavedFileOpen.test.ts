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
      // ⚠ `format` is load-bearing: openProject refuses anything whose
      // `format !== 'xia'` before it looks at the mesh. A first version of this
      // fixture omitted it and the JSON test failed with nothing called —
      // which is how the field was found.
      format: 'xia',
      version: '1.0.0',
      mesh: 'AAECAw==', // base64, as fromBase64() expects
      units: { unit: 'mm', precision: 2 },
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

/**
 * The routing decision `ProjectSerializer.openProject` makes, in the same
 * shape. Kept beside the parsers it dispatches to so a change to either side
 * shows up here.
 */
const AXIA_MAGIC = 0x41584941;
function routeOf(bytes: Uint8Array): 'binary' | 'json' {
  if (bytes.length >= 4
      && new DataView(bytes.buffer, bytes.byteOffset, 4).getUint32(0, true) === AXIA_MAGIC) {
    return 'binary';
  }
  return 'json';
}

describe('a file this app saves must open in this app', () => {
  it('저장 (JSON) round-trips through 열기', () => {
    const r = parseAsProjectSerializer(jsonProject());
    expect(r.ok, `the only wired open path must read the only wired save path: ${r.why}`).toBe(
      true,
    );
  });

  it('다른 이름으로 저장 (binary) opens too — 열기 sniffs the magic', async () => {
    const bin = binaryAxia({ version: 2, timestamp: 'x', name: 'demo' }, new Uint8Array([9, 9]));

    // The binary is well-formed: FileManager's parser takes it.
    const fm = new FileManager(stubBridge());
    await expect(
      fm.loadFromArrayBuffer(bin, 'demo.xia'),
      'premise: the binary really is a valid AXIA file, so this is not a bad fixture',
    ).resolves.toBe(true);

    // ⚠ Before 2026-08-23 that parser was unreachable from the UI and this
    // file was lost. 열기 now reads the first four bytes and routes binary to
    // FileManager, so what is pinned is the ROUTING DECISION itself.
    expect(new DataView(bin.buffer).getUint32(0, true), 'magic must be AXIA').toBe(0x41584941);
    expect(routeOf(bin), 'a magic-bearing file must route to the binary parser').toBe('binary');

    // And it must NOT be handed to JSON.parse, which is what used to happen.
    const asText = new TextDecoder().decode(bin);
    expect(parseAsProjectSerializer(asText).ok, 'JSON.parse cannot read it').toBe(false);
  });

  it('a JSON file still routes to the JSON parser, not the binary one', () => {
    const bytes = new TextEncoder().encode(jsonProject());
    expect(routeOf(bytes), 'no magic — must stay on the text path').toBe('json');
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

/**
 * ⚠ The block above pins the routing PREDICATE. On its own that is not enough:
 * a predicate can be right while nothing calls it. These drive the real
 * `openProject()` end to end, through the file input it creates, and assert on
 * what the wiring did.
 */
describe('열기 itself routes both formats', () => {
  function harness() {
    const calls: string[] = [];
    const bridge = {
      importSnapshot: vi.fn(() => {
        calls.push('bridge.importSnapshot');
        return true;
      }),
      getAllGroups: vi.fn(() => []),
      exportSnapshot: vi.fn(() => new Uint8Array([1, 2, 3])),
      setBooleanGroupTag: vi.fn(),
      clearBooleanGroupTags: vi.fn(),
      getBooleanGroupAFaces: vi.fn(() => []),
      getBooleanGroupBFaces: vi.fn(() => []),
    } as never;
    const toolManager = {
      syncMesh: vi.fn(() => calls.push('syncMesh')),
      selection: { syncGroupsFromWasm: vi.fn(), restoreGroupTags: vi.fn(), getSelectedFaces: () => [] },
    } as never;
    const viewport = { setCameraState: vi.fn(), updateBackground: vi.fn() } as never;
    const units = { unit: 'mm', precision: 2 } as never;
    const fileManager = {
      loadFromArrayBuffer: vi.fn(async () => {
        calls.push('fileManager.loadFromArrayBuffer');
        return true;
      }),
    };
    return { calls, bridge, toolManager, viewport, units, fileManager };
  }

  /**
   * jsdom's `File` has NEITHER `arrayBuffer()` nor `text()` — both come back
   * `undefined` (measured 2026-08-23). That is why nothing has ever driven
   * `openProject` in this suite: its very first statement would throw, and the
   * handler's own try/catch swallowed it into an `alert` jsdom does not
   * implement either. So the two readers are supplied here. This is a gap in
   * the test environment, not in the app — a browser has both.
   */
  function readableFile(bytes: Uint8Array, name: string): File {
    const f = new File([bytes], name);
    Object.defineProperties(f, {
      arrayBuffer: {
        value: async () =>
          bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
      },
      text: { value: async () => new TextDecoder().decode(bytes) },
    });
    return f;
  }

  /** Drive openProject() with a File, by intercepting the input it creates. */
  async function open(h: ReturnType<typeof harness>, file: File) {
    const { initProjectSerializer } = await import('../ui/ProjectSerializer');
    const real = document.createElement.bind(document);
    let input: HTMLInputElement | null = null;
    vi.spyOn(document, 'createElement').mockImplementation((tag: string) => {
      const el = real(tag as 'input');
      if (tag === 'input') input = el as HTMLInputElement;
      return el;
    });
    const { openProject } = initProjectSerializer(h as never);
    openProject();
    vi.mocked(document.createElement).mockRestore();
    if (!input) throw new Error('openProject did not create a file input');
    Object.defineProperty(input, 'files', { value: [file], configurable: true });
    // ⚠ openProject binds with addEventListener('change'), not `onchange`.
    // A first version of this harness fired `input.onchange?.()` and both tests
    // failed with nothing called at all — which is how the difference showed up.
    input.dispatchEvent(new Event('change'));
    // let the async handler settle (file.arrayBuffer + await chain)
    await new Promise((r) => setTimeout(r, 0));
    await new Promise((r) => setTimeout(r, 0));
  }

  it('a binary .xia reaches FileManager and still refreshes the view', async () => {
    const h = harness();
    const bin = binaryAxia({ version: 2, timestamp: 'x', name: 'demo' }, new Uint8Array([9, 9]));
    await open(h, readableFile(bin, 'demo.xia'));

    expect(h.fileManager.loadFromArrayBuffer, 'binary must go to the binary parser').toHaveBeenCalled();
    // ⚠ FileManager imports the snapshot but never touches the viewport. If the
    // shared post-import step is skipped the file "opens" and the screen does
    // not change — the exact way a naive delegation would fail.
    expect(h.calls, 'the view must be refreshed after a binary load').toContain('syncMesh');
  });

  it('a JSON .xia does not go near FileManager', async () => {
    const h = harness();
    await open(h, readableFile(new TextEncoder().encode(jsonProject()), 'saved.xia'));

    expect(h.fileManager.loadFromArrayBuffer, 'JSON must stay on the text path').not.toHaveBeenCalled();
    expect(h.calls).toContain('bridge.importSnapshot');
    expect(h.calls).toContain('syncMesh');
  });
});
