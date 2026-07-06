// Tier 2 — scale_xia: scale all geometry of an XIA about a center.
//
// Implementation mirrors move_xia: collect unique vertex IDs from the XIA's
// owned faces, then `scaleVerts` in one Rust call. Non-uniform scale
// supported via [sx, sy, sz]. ADR-007 winding invariants preserved by the
// engine (a negative/reflecting scale that self-intersects is rejected).
import { z } from 'zod';
import { Vec3, XiaId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  xia_id: XiaId,
  center: Vec3.describe('Scale center [cx, cy, cz] in mm (fixed point).'),
  scale: Vec3.describe('Per-axis scale factors [sx, sy, sz]. 1 = unchanged.'),
});

const OutputSchema = z.object({
  ok: z.boolean(),
  vertex_count: z.number().int().nonnegative(),
  /** Some XIA may be empty (no faces) — scaling it is a no-op. */
  is_no_op: z.boolean(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const scaleXiaCapability: CapabilityHandler<Input, Output> = {
  name: 'scale_xia',
  tier: 2,
  description:
    'Scale all geometry of an XIA (Object) about a center point by ' +
    'per-axis factors [sx, sy, sz]. Preserves topology and materials; ' +
    'the engine rejects a reflecting/self-intersecting scale (ADR-007). ' +
    'For face-only or edge-only scale use scale_verts (low-level).',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const faceIds = engine.getXiaFaceIds(input.xia_id);
    if (faceIds.length === 0) {
      return { ok: true, vertex_count: 0, is_no_op: true };
    }
    const vertSet = new Set<number>();
    for (let i = 0; i < faceIds.length; i++) {
      const verts = engine.getFaceVertices(faceIds[i]!);
      for (let j = 0; j < verts.length; j++) {
        vertSet.add(verts[j]!);
      }
    }
    if (vertSet.size === 0) {
      return { ok: true, vertex_count: 0, is_no_op: true };
    }
    const vertArr = new Uint32Array([...vertSet]);
    const [cx, cy, cz] = input.center;
    const [sx, sy, sz] = input.scale;
    const ok = engine.scaleVerts(vertArr, cx, cy, cz, sx, sy, sz);
    return {
      ok,
      vertex_count: vertSet.size,
      is_no_op: false,
    };
  },
};
