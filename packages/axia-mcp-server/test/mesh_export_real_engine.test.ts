/**
 * The three mesh exports, driven through the dispatcher against the REAL Node
 * engine — the way an agent calls them.
 *
 * ⚠ Why this file exists: from #234 (2026-08-31) until this test landed,
 * `export_obj`, `export_stl` and `export_step` threw
 * `TypeError: engine.getPositions is not a function` on every call. They read
 * the engine through `getPositions` / `getNormals` / `getIndices`, which are
 * the names on the engine's `DeltaBuffers` class; `AxiaEngine` itself exports
 * `get_positions` / `get_normals` / `get_indices`. Nothing could see it:
 *
 *   - `EngineInstance` is hand-written, so tsc checked the handlers against the
 *     interface and never against the export;
 *   - `mesh_export.test.ts` checks the writers with synthetic triangles, which
 *     is the right test for the writers and says nothing about the read;
 *   - no test dispatched the three against a real engine;
 *   - Tier 1 errors are not audited (ADR-041 P26.7), so the audit trail was
 *     silent too.
 *
 * Tier 1 is on by default, so all three were in `tools/list` the whole time.
 *
 * Each file is decoded and counted against the engine's own buffers, because a
 * test that only checks "something came back" passes on nonsense bytes.
 */
import { describe, it, expect } from 'vitest';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { dispatch } from '../src/dispatcher.js';
import type { EngineInstance, EngineModule } from '../src/capabilities/types.js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const wasmPath = resolve(__dirname, '../../axia-wasm-node/dist/axia_wasm.js');
const wasmBuilt = existsSync(wasmPath);
const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };

async function engineWithARect(): Promise<EngineInstance> {
  const mod = (await import(wasmPath)) as unknown as EngineModule;
  const engine = new mod.AxiaEngine();
  const r = await dispatch(
    'draw_rect',
    { center: [0, 0, 0], normal: [0, 0, 1], up: [1, 0, 0], width: 100, height: 50 },
    { engine, versions: VERSIONS },
  );
  expect((r.output as { shape_id: number }).shape_id).toBeGreaterThan(0);
  return engine;
}

type Exported = { bytes_base64: string; size_bytes: number; vertices: number; triangles: number };

async function exportOf(engine: EngineInstance, capability: string): Promise<Exported> {
  const r = await dispatch(capability, {}, { engine, client: 'test', versions: VERSIONS });
  return r.output as Exported;
}

describe.skipIf(!wasmBuilt)('mesh exports read the real engine', () => {
  it('the scene they export is not empty (premise)', async () => {
    // Without this, a zero-triangle scene would make every count below agree
    // with nothing and pass.
    const engine = await engineWithARect();
    expect(engine.get_positions().length).toBeGreaterThan(0);
    expect(engine.get_indices().length).toBeGreaterThan(0);
  });

  it('export_obj writes one v per engine vertex and one f per engine triangle', async () => {
    const engine = await engineWithARect();
    const vertices = engine.get_positions().length / 3;
    const triangles = engine.get_indices().length / 3;
    const out = await exportOf(engine, 'export_obj');
    const text = Buffer.from(out.bytes_base64, 'base64').toString('utf8');
    expect(out.vertices).toBe(vertices);
    expect(out.triangles).toBe(triangles);
    expect(text.split('\n').filter((l) => l.startsWith('v ')).length).toBe(vertices);
    expect(text.split('\n').filter((l) => l.startsWith('f ')).length).toBe(triangles);
  });

  it('export_stl declares, and holds, one facet per engine triangle', async () => {
    const engine = await engineWithARect();
    const triangles = engine.get_indices().length / 3;
    const out = await exportOf(engine, 'export_stl');
    const bytes = Buffer.from(out.bytes_base64, 'base64');
    expect(out.triangles).toBe(triangles);
    expect(bytes.readUInt32LE(80)).toBe(triangles);
    expect(bytes.length).toBe(84 + 50 * triangles);
  });

  it('export_step writes one face per engine triangle', async () => {
    const engine = await engineWithARect();
    const triangles = engine.get_indices().length / 3;
    const out = await exportOf(engine, 'export_step');
    const text = Buffer.from(out.bytes_base64, 'base64').toString('utf8');
    expect(out.triangles).toBe(triangles);
    expect(text.startsWith('ISO-10303-21;')).toBe(true);
    // The rectangle's triangles are never degenerate, so none is dropped.
    expect(text.match(/=ADVANCED_FACE\(/g)?.length).toBe(triangles);
  });
});
