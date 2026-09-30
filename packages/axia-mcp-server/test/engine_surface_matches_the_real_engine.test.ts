/**
 * Every engine method the server's hand-written types promise exists on the
 * REAL Node engine.
 *
 * The browser has this check (web/src/commands/ActionWiring.test.ts, link D):
 * its bridge's engine names are matched against the WASM export. The server had
 * nothing equivalent, and that is how `export_obj` / `export_stl` /
 * `export_step` shipped calling `getPositions`, a name `AxiaEngine` does not
 * have (see mesh_export_real_engine.test.ts). tsc cannot catch it here:
 * `EngineInstance`, `EngineModule` and `EngineHandle` are written by hand, so
 * the handlers are checked against the interfaces and the interfaces against
 * nothing.
 *
 * This reads the three interfaces from source and asks the engine the server
 * actually loads — `typeof engine[name]`, on the instance for `EngineInstance`
 * and on the module for the other two. Asking the runtime rather than a name
 * list is what makes it class-precise: `getPositions` exists in the .d.ts, on
 * `DeltaBuffers`, and a merged name set would have passed it.
 */
import { describe, it, expect } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import ts from 'typescript';

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(__dirname, '../../axia-wasm-node/dist/axia_wasm.js');
const wasmBuilt = existsSync(wasmPath);

/** Member names of `interface <name>` in a source file. */
function interfaceMembers(file: string, name: string): string[] {
  const path = resolve(__dirname, '..', file);
  const sf = ts.createSourceFile(path, readFileSync(path, 'utf8'), ts.ScriptTarget.Latest, true);
  for (const st of sf.statements) {
    if (ts.isInterfaceDeclaration(st) && st.name.text === name) {
      return st.members.filter((m) => m.name).map((m) => m.name!.getText(sf));
    }
  }
  return [];
}

const INSTANCE = interfaceMembers('src/capabilities/types.ts', 'EngineInstance');
const MODULE = interfaceMembers('src/capabilities/types.ts', 'EngineModule');
const HANDLE = interfaceMembers('src/handshake.ts', 'EngineHandle');

describe('the hand-written engine interfaces were read (premise)', () => {
  // A parser that finds nothing makes the check below pass vacuously.
  it('EngineInstance', () => {
    expect(INSTANCE.length).toBeGreaterThan(20);
    expect(INSTANCE).toContain('draw_rect_as_shape');
  });
  it('EngineModule', () => {
    expect(MODULE).toEqual(expect.arrayContaining(['schema_version', 'engine_version', 'AxiaEngine']));
  });
  it('EngineHandle', () => {
    expect(HANDLE).toEqual(expect.arrayContaining(['schema_version', 'engine_version']));
  });
});

describe.skipIf(!wasmBuilt)('every promised engine method exists on the real Node engine', () => {
  it('EngineInstance → methods of an AxiaEngine instance', async () => {
    const mod = (await import(wasmPath)) as Record<string, unknown> & { AxiaEngine: new () => object };
    const engine = new mod.AxiaEngine() as Record<string, unknown>;
    const missing = INSTANCE.filter((n) => typeof engine[n] !== 'function');
    expect(
      missing,
      'EngineInstance declares methods the real AxiaEngine does not have. A handler ' +
        'calling one throws "is not a function" at runtime while tsc passes. Use the ' +
        "engine's js_name (web/src/wasm/axia_wasm.d.ts, class AxiaEngine) or drop it.",
    ).toEqual([]);
  });

  it('EngineModule / EngineHandle → functions of the loaded module', async () => {
    const mod = (await import(wasmPath)) as Record<string, unknown>;
    const missing = [...new Set([...MODULE, ...HANDLE])].filter((n) => typeof mod[n] !== 'function');
    expect(missing, 'declared on the module handle but not exported by the module').toEqual([]);
  });
});
