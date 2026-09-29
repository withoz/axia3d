/**
 * The `엣지 챔퍼 (Chamfer)…` menu item reaches the chamfer op.
 *
 * It did not until 2026-09-29, and every wiring guard in the repo passed on it
 * the whole time. The command is registered, labelled, in both catalogs, has a
 * handler, writes an OperationLog entry and has a rerun branch — so link A
 * (string → handler), link C (id → tool) and link D (bridge → export) are all
 * satisfied. None of them asks whether the ENGINE ACCEPTS THE ARGUMENTS.
 *
 * The handler called `filletEdge(edge, distance, 1)`, reasoning in a comment
 * that a chamfer is a fillet with one strip segment. The geometry is right;
 * the call was not. Measured (`the_chamfer_the_menu_offers_is_the_chamfer_op`
 * in axia-geo):
 *
 * ```text
 *   fillet_edge(e, 20, 1)   "fillet: segments must be ≥ 2, got 1"
 *   chamfer_edge(e, 20)     facet FaceId(10), faces 6 → 7, closed, 0 SI
 *   chamfer_edge(e, 500)    refused, box untouched
 * ```
 *
 * Nothing clamped it on the way down: the bridge's `segments = 8` is a
 * DEFAULT and the handler passed 1 explicitly, and the WASM entry forwards
 * whatever it is given.
 *
 * ⚠ The two ops are not interchangeable even where both succeed. `chamfer_edge`
 * refuses a distance longer than an incident edge; `fillet_edge` has no such
 * guard, and fillet.rs says why it matters — the facet "folds through a
 * neighbour, a manifold-but-self-intersecting result no downstream check
 * catches". So the browser had the unguarded path while the MCP server, which
 * has called `chamferEdge` all along, had the guarded one.
 *
 * ## What this file holds
 *
 * ```text
 *   Mesh::chamfer_edge       existed
 *   WASM export chamferEdge  existed, gate + transaction wired
 *   WasmBridge  chamferEdge  NEW — typed, so tsc holds the link
 *   both call sites          the handler and the rerun branch
 * ```
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

describe('엣지 챔퍼 — the menu item and the op', () => {
  it('both call sites ask for a chamfer', () => {
    const tm = read('src/tools/ToolManagerRefactored.ts');
    // PREMISE: the file was read and still has the action at all — otherwise
    // every absence below is vacuously true.
    expect(tm, 'the action must still exist to say anything about it').toContain("=== 'chamfer-edge'");

    const calls = [...tm.matchAll(/this\.bridge\.chamferEdge\(/g)];
    expect(calls.length, 'the handler AND the rerun branch').toBe(2);
  });

  it('and no caller asks the fillet for a single segment', () => {
    // ⚠ Comments have to go first. The first draft of this check counted the
    // very comment that explains the fix — `filletEdge(edge, distance, 1)`
    // written as prose — and failed on a file that was already correct.
    const tm = read('src/tools/ToolManagerRefactored.ts')
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/^\s*\/\/.*$/gm, '');
    // PREMISE: stripping comments must not strip the code. If this is empty the
    // check below is vacuous whatever it asserts.
    expect(
      [...tm.matchAll(/this\.bridge\.filletEdge\(/g)].length,
      'the real fillet calls must survive comment-stripping',
    ).toBe(2);

    const oneSegment = [...tm.matchAll(/filletEdge\([^)]*,\s*1\s*\)/g)].map((m) => m[0]);
    expect(
      oneSegment,
      'fillet_edge refuses segments < 2, so a literal 1 here is a call that ' +
        'can only fail — the chamfer op is what this wants',
    ).toEqual([]);
  });

  it('the bridge declares it, so tsc holds the link', () => {
    const bridge = read('src/bridge/WasmBridge.ts');
    expect(bridge, 'the interface entry').toContain('chamferEdge?(edgeId: number, dist: number)');
    expect(bridge, 'and the wrapper').toContain('chamferEdge(edgeId: number, dist: number): number {');
  });

  it('and the WASM build exports it', () => {
    const dts = read('src/wasm/axia_wasm.d.ts');
    // PREMISE: the .d.ts is present and has content to search.
    expect(dts.length, 'the .d.ts must be present to say anything about it').toBeGreaterThan(1000);
    expect(dts, 'the engine exports chamferEdge').toContain('chamferEdge(');
  });
});
