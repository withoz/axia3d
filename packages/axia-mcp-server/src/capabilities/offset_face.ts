// Tier 2 — offset_face: inset (+) / outset (-) a face boundary by `distance`.
import { z } from 'zod';
import { FaceId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  face_id: FaceId,
  distance: z
    .number()
    .describe(
      'Offset distance in mm. Positive = inward (inset), negative = outward ' +
        '(outset). Multi-loop faces (rings with holes) are rejected (ADR-016 Q2).',
    ),
});

const OutputSchema = z.object({
  ok: z.boolean(),
  /** The new inner face left by the offset. */
  inner_face: FaceId.optional(),
  /** The strip (frame) faces connecting old and new boundaries. */
  strip_faces: z.array(FaceId).optional(),
  total_verts: z.number().int().nonnegative().optional(),
  total_faces: z.number().int().nonnegative().optional(),
  error: z.string().optional(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

interface EngineOffsetResult {
  ok?: boolean;
  innerFace?: number;
  stripFaces?: number[];
  totalVerts?: number;
  totalFaces?: number;
  error?: string;
}

export const offsetFaceCapability: CapabilityHandler<Input, Output> = {
  name: 'offset_face',
  tier: 2,
  description:
    "Offset a face's boundary inward (positive distance) or outward " +
    '(negative), producing a smaller/larger inner face plus a strip of ' +
    'frame faces. Multi-loop faces (rings with holes) are rejected ' +
    '(ADR-016 Q2). Guarded by the closure-preserving + self-intersection ' +
    'gate — an offset that opens a solid or self-intersects is cancelled.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const raw = engine.offset_face(input.face_id, input.distance);
    let parsed: EngineOffsetResult;
    try {
      parsed = JSON.parse(raw) as EngineOffsetResult;
    } catch {
      return { ok: false, error: 'engine returned non-JSON' };
    }
    return {
      ok: parsed.ok ?? false,
      inner_face: parsed.innerFace,
      strip_faces: parsed.stripFaces,
      total_verts: parsed.totalVerts,
      total_faces: parsed.totalFaces,
      error: parsed.error,
    };
  },
};
