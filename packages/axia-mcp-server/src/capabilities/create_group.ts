// Tier 1 — create_group: bundle a set of faces into a named Group.
import { z } from 'zod';
import { FaceId, GroupId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  name: z.string().min(1).describe('Human-readable group name.'),
  face_ids: z
    .array(FaceId)
    .min(1)
    .describe(
      'Face IDs to include in the group. Owner IDs only — NOT raw triangle ' +
        'indices (ADR-041 P26.3). Groups reference faces; they do not own them.',
    ),
});

const OutputSchema = z.object({
  ok: z.boolean(),
  /** New group's owner ID. Present only when ok. */
  group_id: GroupId.optional(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const createGroupCapability: CapabilityHandler<Input, Output> = {
  name: 'create_group',
  tier: 1,
  description:
    'Create a named Group referencing the given faces (SketchUp-style ' +
    'selection set — UI grouping, not geometric ownership). Returns the new ' +
    'GroupId. Groups may be nested and toggled visible/locked. Does not ' +
    'mutate geometry (ADR: Group is a Semantic Layer reference).',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const faceIds = new Uint32Array(input.face_ids);
    const gid = engine.create_group(input.name, faceIds);
    if (gid > 0) {
      return { ok: true, group_id: gid };
    }
    return { ok: false };
  },
};
