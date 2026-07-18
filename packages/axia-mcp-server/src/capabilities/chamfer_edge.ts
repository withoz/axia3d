// Tier 2 — chamfer_edge: bevel a sharp edge with a single flat facet.
//
// The flat sibling of fillet_edge: where fillet sweeps an arc, chamfer joins
// the two roll-back points with one planar face. Same MVP constraints (edge
// shared by exactly two active planar faces, ≤ 3 faces per endpoint, convex,
// `dist` shorter than every incident edge) — the engine enforces them and
// returns -1 with the reason on lastError if they are violated.
import { z } from 'zod';
import { EdgeId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  edge_id: EdgeId,
  dist: z
    .number()
    .positive()
    .describe(
      'Set-back distance in mm, along each edge adjacent to the endpoints. ' +
        'Must be strictly shorter than every incident edge, or the facet ' +
        'overshoots the neighbour (engine rejects it).',
    ),
});

const OutputSchema = z.object({
  ok: z.boolean(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const chamferEdgeCapability: CapabilityHandler<Input, Output> = {
  name: 'chamfer_edge',
  tier: 2,
  description:
    'Chamfer (flat-bevel) a sharp manifold edge, setting back `dist` mm along ' +
    'each adjacent edge — the flat sibling of fillet_edge, leaving a single ' +
    'planar facet instead of an arc strip. Convex two-face edges only; fails ' +
    '(ok:false) on non-manifold or concave edges, or when `dist` overshoots an ' +
    'incident edge. For 3-way corner singularities use chamfer_vertex_3way.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    // chamferEdge returns 1 (the single facet) on success, -1 on failure.
    return { ok: engine.chamferEdge(input.edge_id, input.dist) > 0 };
  },
};
