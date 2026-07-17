// Tier 1 — create_xia: promote a form-layer Shape to a property-layer Xia.
//
// The Two-Layer Citizenship model (ADR-049/050): a draw_* call produces a
// Shape — pure geometry, no material, 0-thickness allowed. A Shape becomes a
// Xia (a member with identity) only by being given a material AND satisfying
// the four conditions the engine checks on promotion. This capability is that
// promotion, and the only way an AI agent reaches the property layer.
import { z } from 'zod';
import { ShapeId, XiaId, MaterialId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  shape_id: ShapeId.describe(
    'The form-layer Shape to promote — the id returned by a prior draw_rect / ' +
      'draw_circle / draw_line / draw_polyline (those return ShapeIds, ADR-050).',
  ),
  material_id: MaterialId,
});

const OutputSchema = z.object({
  ok: z.boolean(),
  /** The new Xia's owner ID. Present only when ok. */
  xia_id: XiaId.optional(),
  /**
   * Why the promotion was refused, verbatim from the engine's four-condition
   * check (no geometry / invalid material / zero volume / zero dimension /
   * not watertight / not manifold). Present only when not ok.
   */
  error: z.string().optional(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

export const createXiaCapability: CapabilityHandler<Input, Output> = {
  name: 'create_xia',
  tier: 1,
  description:
    'Promote a form-layer Shape (from a draw_* call) to a property-layer Xia ' +
    'by assigning a material. Succeeds only when the Shape satisfies the ' +
    'ADR-050 four conditions: it has geometry, the material is valid, its ' +
    'volume/section is positive, and it is watertight + manifold. Returns the ' +
    'new XiaId. Two-Layer Citizenship — a Shape is geometry, a Xia is a member ' +
    'with identity; this is the only crossing between them. Transaction-wrapped ' +
    '(a single Undo restores the pre-promote state).',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    // promoteShapeToXia is strict: it THROWS on any four-condition failure
    // (ADR-050 P-2-c — no silent skip). Catch it here so the agent gets a
    // structured negative result with the reason, the way offset_face does,
    // rather than a raw exception surfacing as a dispatcher error.
    try {
      const xiaId = engine.promoteShapeToXia(input.shape_id, input.material_id);
      return { ok: true, xia_id: xiaId };
    } catch (e) {
      return { ok: false, error: e instanceof Error ? e.message : String(e) };
    }
  },
};
