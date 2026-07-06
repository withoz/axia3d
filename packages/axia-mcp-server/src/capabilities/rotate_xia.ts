// Tier 2 — rotate_xia: rotate all geometry of an XIA about a center + axis.
//
// Implementation mirrors move_xia: collect unique vertex IDs from the XIA's
// owned faces, then `rotateVerts` in one Rust call. ADR-007 face-orientation
// invariants preserved (rotation is rigid).
import { z } from 'zod';
import { Vec3, XiaId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  xia_id: XiaId,
  center: Vec3.describe('Rotation center [cx, cy, cz] in mm.'),
  axis: Vec3.describe('Rotation axis direction [ax, ay, az] (need not be unit).'),
  angle_deg: z.number().describe('Rotation angle in degrees (CCW about axis).'),
});

const OutputSchema = z.object({
  ok: z.boolean(),
  vertex_count: z.number().int().nonnegative(),
  /** Some XIA may be empty (no faces) — rotating it is a no-op. */
  is_no_op: z.boolean(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const rotateXiaCapability: CapabilityHandler<Input, Output> = {
  name: 'rotate_xia',
  tier: 2,
  description:
    'Rotate all geometry of an XIA (Object) about a center point by ' +
    'angle_deg degrees around the given axis. Preserves topology, ' +
    'materials, and ADR-007 winding invariants. For face-only or ' +
    'edge-only rotation use rotate_verts (low-level).',
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
    const [ax, ay, az] = input.axis;
    const ok = engine.rotateVerts(vertArr, cx, cy, cz, ax, ay, az, input.angle_deg);
    return {
      ok,
      vertex_count: vertSet.size,
      is_no_op: false,
    };
  },
};
