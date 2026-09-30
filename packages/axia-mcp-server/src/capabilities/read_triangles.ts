/**
 * The engine's triangles, in the shape the mesh writers want.
 *
 * `get_positions` / `get_normals` / `get_indices` are what the browser's own
 * viewport reads (`WasmBridge.getMeshBuffers`), so an exported file and what a
 * user sees on screen come from one tessellation, not two.
 */
import type { EngineInstance } from './types.js';
import type { Triangles } from './meshExport.js';

export function readTriangles(engine: EngineInstance): Triangles {
  return {
    positions: engine.get_positions(),
    normals: engine.get_normals(),
    indices: engine.get_indices(),
  };
}
