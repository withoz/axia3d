/**
 * Step 3 of `normalizeDrawInput` runs now, and something reads what it says.
 *
 * ADR-170 (LOCKED #71 L-71-2) pins a five-step routine as the single place a
 * draw point is normalised. Step 3 is "vertex_at silent dedup (LOCKED #5
 * 0.15μm spatial-hash)", and until 2026-09-29 it read:
 *
 * ```ts
 *   const va = (this.bridge as unknown as { vertex_at?: … }).vertex_at;
 *   if (typeof va === 'function') { … }
 * ```
 *
 * ⚠ `vertex_at` existed in NO layer — not the bridge, not the WASM export —
 * so `typeof va === 'function'` was false, always. The step had never run. Its
 * tests passed because they assigned `(bridge as any).vertex_at = vi.fn()`
 * first: they proved the plumbing AROUND the call, not that the call happened.
 * And a cast around a name nothing declares is invisible to `ActionWiring`'s
 * link-D guard, which reads WasmBridge.
 *
 * This file was written as the record of that, with a retire signal saying:
 * *"wiring it fails this file — give `vertId` a consumer in the same change"*.
 * It fired. This is the rewrite.
 *
 * ## What it is wired to, and why that consumer is not invented
 *
 * ```text
 *   Mesh::find_existing_vertex   existed all along
 *   WASM export  vertexAt        NEW — returns the raw id, or -1
 *   WasmBridge   vertexAt        NEW — typed, so tsc holds the link
 *   step 3                       a typed call, no cast
 * ```
 *
 * The consumer is BoundaryTool, and it comes from a measurement rather than
 * from wanting one. `boundary_from_point` on two squares sharing an edge:
 *
 * ```text
 *   inside the left square       Ok(FaceId(0))        area 10000
 *   on the shared edge           Ok(FaceId(0))        area 10000
 *   exactly on a corner vertex   Err(NoEnclosingCycle)
 *   exactly on an outer corner   Err(NoEnclosingCycle)
 * ```
 *
 * A click on a vertex fails, and from the caller's side that is indistinguish-
 * able from "there is nothing here". Step 3 knows which it was, because the
 * engine's dedup distance IS the distance at which a draw would reuse that
 * vertex. So the tool says "that is a corner" instead of the generic refusal.
 *
 * ⚠ The engine still dedups at `add_vertex` whatever this does. Step 3 is not
 * what keeps duplicate vertices out — it is what lets the TOOL layer know, one
 * call earlier, that a point is already taken.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

describe('normalizeDrawInput step 3', () => {
  it('calls a bridge method the bridge actually has', () => {
    const tm = read('src/tools/ToolManagerRefactored.ts');
    expect(tm, 'step 3 must still make the call').toContain('this.bridge.vertexAt');
    expect(
      tm.includes('vertex_at?: (x: number'),
      'the cast is gone — a name nothing declares is invisible to the link-D guard',
    ).toBe(false);

    const bridge = read('src/bridge/WasmBridge.ts');
    expect(bridge, 'and the bridge declares it').toContain('vertexAt(x: number, y: number, z: number)');
  });

  it('and the WASM build exports it', () => {
    const dts = read('src/wasm/axia_wasm.d.ts');
    // PREMISE: the .d.ts was read and has content to search.
    expect(dts.length, 'the .d.ts must be present to say anything about it').toBeGreaterThan(1000);
    expect(dts, 'the engine exports vertexAt').toContain('vertexAt');
  });

  it('and BoundaryTool reads the vertId it produces', () => {
    const bt = read('src/tools/BoundaryTool.ts');
    expect(
      bt,
      'the only production caller of normalizeDrawInput must use vertId, or the ' +
        'step is producing a field nobody reads again',
    ).toContain('normalized.vertId');
    expect(bt, 'and say which refusal it is').toContain('꼭짓점을 클릭했습니다');
  });
});
