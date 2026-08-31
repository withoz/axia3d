// Tier 1 — export_stl: the current scene as a binary STL.
// Returns base64 bytes; JSON-RPC cannot carry raw binary, and the text formats
// travel the same way so a caller decodes one way for all three.
import { z } from 'zod';
import type { CapabilityHandler } from './types.js';
import { readTriangles } from './read_triangles.js';
import { toStlBinary, statsOf } from './meshExport.js';

const InputSchema = z.object({
  name: z
    .string()
    .max(64)
    .regex(/^[A-Za-z0-9_-]*$/, 'letters, digits, dash and underscore only')
    .optional()
    .describe('Object name to write into the file. Default "axia".'),
}).strict();

const OutputSchema = z.object({
  format: z.literal('STL'),
  bytes_base64: z.string(),
  size_bytes: z.number().int().nonnegative(),
  vertices: z.number().int().nonnegative(),
  triangles: z.number().int().nonnegative(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const exportStlCapability: CapabilityHandler<Input, Output> = {
  name: 'export_stl',
  tier: 1,
  description: 'Export the current scene as a BINARY STL. Millimetres, Z-up. Facet ' +
    'normals are computed from the winding. Returns base64-encoded bytes.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const tri = readTriangles(engine);
    const bytes = toStlBinary(tri, input.name || 'axia');
    const s = statsOf(tri);
    return {
      format: 'STL' as const,
      bytes_base64: Buffer.from(bytes).toString('base64'),
      size_bytes: bytes.byteLength,
      vertices: s.vertices,
      triangles: s.triangles,
    };
  },
};
