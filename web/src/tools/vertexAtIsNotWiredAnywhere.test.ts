/**
 * Step 3 of `normalizeDrawInput` has never run, and its tests pass by mocking
 * the method it is missing.
 *
 * ADR-170 (LOCKED #71 L-71-2) pins a five-step routine as the single place a
 * draw point is normalised. Step 3 is "vertex_at silent dedup (LOCKED #5
 * 1.5μm spatial-hash)", and it reads:
 *
 * ```ts
 *   const va = (this.bridge as unknown as { vertex_at?: … }).vertex_at;
 *   if (typeof va === 'function') { … }
 * ```
 *
 * Measured 2026-09-29, by the wiring audit:
 *
 * ```text
 *   Mesh::find_existing_vertex      exists (crates/axia-geo/src/mesh.rs)
 *   WASM export                     none
 *   WasmBridge wrapper              none
 *   so `typeof va === 'function'`   is false, always
 * ```
 *
 * ⚠ And `NormalizedDrawInput.vertId`, the only thing the step produces, is read
 * by NOTHING outside tests. So the step is inert twice over: it cannot run, and
 * its output has no consumer.
 *
 * ⚠ The existing tests in `ToolManagerRefactored.test.ts` assign
 * `(bridge as any).vertex_at = vi.fn()` and then check the result. They prove
 * the plumbing around the call, not that the call happens — which is why the
 * gap survived the guard that was written for exactly this shape
 * (`ActionWiring`'s link D reads WasmBridge, and the bridge does not mention
 * `vertex_at` at all).
 *
 * This is not a bug report. Nothing is broken: the 1.5μm dedup that Step 3
 * would pre-resolve happens in the engine anyway, inside `add_vertex`. It is a
 * record that one of five canonical steps is a placeholder, so the next reader
 * of LOCKED #71 does not count it as working.
 *
 * ⚠ THE RETIRE SIGNAL. Wiring `vertex_at` (a WASM export + a bridge wrapper)
 * fails this file. When that happens, give `vertId` a CONSUMER in the same
 * change — an export with nothing reading it buys nothing, and the audit that
 * found this declined to add one for that reason.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

describe('normalizeDrawInput step 3', () => {
  it('calls a bridge method that the bridge does not have', () => {
    const tm = read('src/tools/ToolManagerRefactored.ts');
    // PREMISE: the call site is still there and still written as a cast.
    expect(tm, 'step 3 must still reach for vertex_at').toContain('vertex_at');

    const bridge = read('src/bridge/WasmBridge.ts');
    expect(
      bridge.includes('vertex_at') || bridge.includes('vertexAt'),
      'WasmBridge now has vertex_at — step 3 can run. Give NormalizedDrawInput.vertId ' +
        'a consumer in the same change and rewrite this file to say what it does.',
    ).toBe(false);
  });

  it('and the WASM build does not export it either', () => {
    const dts = read('src/wasm/axia_wasm.d.ts');
    // PREMISE: the .d.ts was read and has content to search.
    expect(dts.length, 'the .d.ts must be present to say anything about it').toBeGreaterThan(1000);
    expect(
      dts.includes('vertex_at') || dts.includes('vertexAt'),
      'the engine now exports it; see the note above before wiring the bridge.',
    ).toBe(false);
  });

  it('so nothing reads the vertId it would produce', () => {
    const tm = read('src/tools/ToolManagerRefactored.ts');
    // The field is declared on the returned shape…
    expect(tm).toContain('vertId?: number');
    // …and outside this one file, and outside tests, no source reads it. The
    // audit grepped every non-test .ts under src/ for `.vertId` and found only
    // ChamferTool's own unrelated field.
    const chamfer = read('src/tools/ChamferTool.ts');
    expect(
      chamfer.includes('normalizeDrawInput'),
      'ChamferTool has its own vertId and does not come from this step',
    ).toBe(false);
  });
});
