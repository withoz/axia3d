// Tier 3 — erase_edge: delete an edge, and every face that shares it.
//
// Destructive, so every call goes through the Tier 3 consent gate (ADR-041
// P26.1). Uses deleteEdgeCascade rather than the legacy delete_edge: both do
// the same thing, but only the cascade variant reports HOW MANY faces went with
// the edge. That number is the point — "delete this edge" quietly taking a
// solid's walls with it is exactly what an agent's caller needs to see.
import { z } from 'zod';
import { EdgeId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  edge_id: EdgeId,
});

type Input = z.infer<typeof InputSchema>;
type Output = { ok: boolean; cascaded_face_count: number };

export const eraseEdgeCapability: CapabilityHandler<Input, Output> = {
  name: 'erase_edge',
  tier: 3,
  // Read off the engine: deleteEdgeCascade returns the cascaded face count,
  // >= 0 on success and -1 on failure, and no-ops to 0 when already gone.
  description:
    'DESTRUCTIVE — delete the edge `edge_id` AND every face that shares it ' +
    '(SketchUp-style cascade). Deleting one edge of a box therefore removes ' +
    'the two faces meeting there, not just the line. `cascaded_face_count` ' +
    'reports how many faces went with it. Wrapped in a single undo step. ' +
    'Deleting an edge that is already gone is a no-op (ok=true, count 0).',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const cascaded = engine.deleteEdgeCascade(input.edge_id);
    return {
      ok: cascaded >= 0,
      cascaded_face_count: cascaded >= 0 ? cascaded : 0,
    };
  },
};
