// Tier 1 — draw_polyline: a multi-segment polyline as a form-layer Shape.
//
// ADR-050 P-5c — draws default to form-layer Shapes. The WASM
// `drawPolylineAsShape` returns a status flag (0 success / -1 error), not a
// ShapeId (unlike draw_rect_as_shape), so this capability surfaces `ok`
// rather than a shape_id. Use `promote_shape_to_xia` (future Tier 2) to
// attach a material.
import { z } from 'zod';
import { Vec3 } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  points: z
    .array(Vec3)
    .min(2)
    .describe(
      'Ordered polyline vertices [[x,y,z], …] in mm. At least 2 points. ' +
        'Cardinal-plane snap is auto-applied at the bridge layer (ADR-026 P12).',
    ),
  normal: Vec3.describe(
    'Optional plane-normal hint for closed-loop face synthesis. [0,0,0] = ' +
      'inferred from the free-edge planar pipeline. Default = inferred.',
  ).default([0, 0, 0]),
});

const OutputSchema = z.object({
  ok: z.boolean(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const drawPolylineCapability: CapabilityHandler<Input, Output> = {
  name: 'draw_polyline',
  tier: 1,
  description:
    'Draw a multi-segment polyline through the given points as a form-layer ' +
    'Shape (ADR-050). A closed loop (last point ≈ first) synthesizes a face. ' +
    'Returns ok. Coordinates on a cardinal plane are snapped exactly ' +
    '(ADR-026 P12). Use `promote_shape_to_xia` to gain property-layer status.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const flat: number[] = [];
    for (const [x, y, z] of input.points) {
      flat.push(x, y, z);
    }
    const [nx, ny, nz] = input.normal;
    const status = engine.drawPolylineAsShape(new Float64Array(flat), nx, ny, nz);
    return { ok: status >= 0 };
  },
};
