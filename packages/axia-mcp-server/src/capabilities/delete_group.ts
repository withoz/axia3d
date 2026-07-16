// Tier 3 — delete_group: dissolve a group.
//
// Declared Tier 3 by tiers.ts, so it goes through the consent gate like the
// erase capabilities (ADR-041 P26.1). Worth being precise in the prompt: this
// one does NOT destroy geometry. Read off GroupManager::delete_group — it drops
// the group, un-indexes its faces, and re-parents its children. The faces
// themselves survive, ungrouped. A user approving this should know their model
// is not going anywhere.
import { z } from 'zod';
import { GroupId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  group_id: GroupId,
});

type Input = z.infer<typeof InputSchema>;
type Output = { ok: boolean };

export const deleteGroupCapability: CapabilityHandler<Input, Output> = {
  name: 'delete_group',
  tier: 3,
  description:
    'Dissolve the group `group_id`. The grouping is removed — its faces ' +
    'SURVIVE and become ungrouped, and any child groups are re-parented to ' +
    "this group's parent. Geometry is not deleted; this is ungroup, not " +
    'erase. Returns ok=false if no such group exists.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => ({
    ok: engine.delete_group(input.group_id),
  }),
};
