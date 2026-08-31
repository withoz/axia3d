/**
 * The engine's triangles, in the shape the mesh writers want.
 *
 * `getPositions` / `getNormals` / `getIndices` are what the browser's own
 * viewport reads (`WasmBridge.getMeshBuffers`), so an exported file and what a
 * user sees on screen come from one tessellation, not two.
 */
import type { EngineInstance } from './types.js';
import type { Triangles } from './meshExport.js';

export function readTriangles(engine: EngineInstance): Triangles {
  return {
    positions: engine.getPositions(),
    normals: engine.getNormals(),
    indices: engine.getIndices(),
  };
}
