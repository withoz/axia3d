// Tier 1 — export_obj: the current scene as a Wavefront OBJ.
// Returns base64 bytes; JSON-RPC cannot carry raw binary, and the text formats
// travel the same way so a caller decodes one way for all three.
import { z } from 'zod';
import type { CapabilityHandler } from './types.js';
import { readTriangles } from './read_triangles.js';
import { toObj, statsOf } from './meshExport.js';

const InputSchema = z.object({
  name: z
    .string()
    .max(64)
    .regex(/^[A-Za-z0-9_-]*$/, 'letters, digits, dash and underscore only')
    .optional()
    .describe('Object name to write into the file. Default "axia".'),
}).strict();

const OutputSchema = z.object({
  format: z.literal('OBJ'),
  bytes_base64: z.string(),
  size_bytes: z.number().int().nonnegative(),
  vertices: z.number().int().nonnegative(),
  triangles: z.number().int().nonnegative(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const exportObjCapability: CapabilityHandler<Input, Output> = {
  name: 'export_obj',
  tier: 1,
  description: 'Export the current scene as a Wavefront OBJ. Triangles with ' +
    'per-vertex normals, millimetres, Z-up. Returns base64-encoded UTF-8 text.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const tri = readTriangles(engine);
    const text = toObj(tri, input.name || 'axia');
    const buf = Buffer.from(text, 'utf8');
    const s = statsOf(tri);
    return {
      format: 'OBJ' as const,
      bytes_base64: buf.toString('base64'),
      size_bytes: buf.byteLength,
      vertices: s.vertices,
      triangles: s.triangles,
    };
  },
};
