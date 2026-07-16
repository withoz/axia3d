// Tier 3 — erase_face: delete a face.
//
// Destructive, so every call goes through the Tier 3 consent gate: the user is
// shown this capability and the face id, and must accept before the engine is
// touched (ADR-041 P26.1). Nothing here re-checks that — the dispatcher refuses
// to invoke a Tier 3 handler without an explicit accept, and fails closed when
// it cannot ask.
import { z } from 'zod';
import { FaceId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  face_id: FaceId,
});

type Input = z.infer<typeof InputSchema>;
type Output = { ok: boolean };

export const eraseFaceCapability: CapabilityHandler<Input, Output> = {
  name: 'erase_face',
  tier: 3,
  // Every clause below is read off the engine, not assumed: delete_face
  // no-ops to true when the face is already gone, wraps itself in one
  // transaction, and returns whether the face is absent afterwards.
  description:
    'DESTRUCTIVE — delete the face `face_id`. Its edges are left behind as ' +
    'wires (ADR-019: line is truth, face is byproduct), so this removes the ' +
    'surface, not the boundary. Wrapped in a single undo step. Deleting a ' +
    'face that is already gone is a no-op and reports ok=true; ok=false means ' +
    'the engine could not remove it.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => ({
    ok: engine.delete_face(input.face_id),
  }),
};
