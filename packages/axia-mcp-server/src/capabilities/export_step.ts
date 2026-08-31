// Tier 1 — export_step: the current scene as a STEP AP214 file.
// Returns base64 bytes; JSON-RPC cannot carry raw binary, and the text formats
// travel the same way so a caller decodes one way for all three.
import { z } from 'zod';
import type { CapabilityHandler } from './types.js';
import { readTriangles } from './read_triangles.js';
import { toStep, statsOf } from './meshExport.js';

const InputSchema = z.object({
  name: z
    .string()
    .max(64)
    .regex(/^[A-Za-z0-9_-]*$/, 'letters, digits, dash and underscore only')
    .optional()
    .describe('Object name to write into the file. Default "axia".'),
}).strict();

const OutputSchema = z.object({
  format: z.literal('STEP'),
  bytes_base64: z.string(),
  size_bytes: z.number().int().nonnegative(),
  vertices: z.number().int().nonnegative(),
  triangles: z.number().int().nonnegative(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const exportStepCapability: CapabilityHandler<Input, Output> = {
  name: 'export_step',
  tier: 1,
  description: 'Export the current scene as STEP (AP214, ISO 10303-21) — a faceted ' +
    'MANIFOLD_SOLID_BREP, one planar ADVANCED_FACE per triangle, in ' +
    'millimetres. NOTE: facets, not analytic surfaces: a cylinder arrives as ' +
    'its triangles, not as a CYLINDRICAL_SURFACE. Returns base64 UTF-8 text.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const tri = readTriangles(engine);
    const text = toStep(tri, input.name || 'axia');
    const buf = Buffer.from(text, 'utf8');
    const s = statsOf(tri);
    return {
      format: 'STEP' as const,
      bytes_base64: buf.toString('base64'),
      size_bytes: buf.byteLength,
      vertices: s.vertices,
      triangles: s.triangles,
    };
  },
};
