// Tier 0 — get_xia_geometry_state: derive an XIA's geometry state.
//
// The engine computes geometry state from owned geometry (never stored) —
// see the Geometry/Semantic Layer ADR. `getXiaStats` returns per-XIA JSON
// carrying `shapeType` (Dissolved/Point/Edge/Face/Volume proxy), `isSolid`,
// and entity counts, or `{ empty: true }` for an unknown XIA.
import { z } from 'zod';
import { XiaId } from '../schema.js';
import type { CapabilityHandler } from './types.js';

const InputSchema = z.object({
  xia_id: XiaId,
});

const OutputSchema = z.object({
  xia_id: XiaId,
  /** True if the XIA has no faces (empty / unknown). */
  empty: z.boolean(),
  /** Geometry state proxy: "Dissolved" | "Point" | "Edge" | "Face" | "Volume". */
  shape_type: z.string().optional(),
  /** True if the XIA forms a closed solid (Volume). */
  is_solid: z.boolean().optional(),
  face_count: z.number().int().nonnegative().optional(),
  edge_count: z.number().int().nonnegative().optional(),
  vert_count: z.number().int().nonnegative().optional(),
});

type Input = z.infer<typeof InputSchema>;
type Output = z.infer<typeof OutputSchema>;

interface XiaStats {
  empty?: boolean;
  shapeType?: unknown;
  isSolid?: unknown;
  faceCount?: unknown;
  edgeCount?: unknown;
  vertCount?: unknown;
}

function numOrUndef(v: unknown): number | undefined {
  return typeof v === 'number' && Number.isFinite(v) ? v : undefined;
}

export const getXiaGeometryStateCapability: CapabilityHandler<Input, Output> = {
  name: 'get_xia_geometry_state',
  tier: 0,
  description:
    "Return an XIA's derived geometry state — shape_type " +
    '(Dissolved/Point/Edge/Face/Volume), is_solid flag, and entity counts. ' +
    'State is computed from owned geometry, never stored (Geometry/Semantic ' +
    'Layer ADR). Read-only. Empty/unknown XIA → { empty: true }.',
  inputSchema: InputSchema,
  handler: ({ engine }, input) => {
    const raw = engine.getXiaStats(input.xia_id);
    let parsed: XiaStats;
    try {
      parsed = JSON.parse(raw) as XiaStats;
    } catch {
      return { xia_id: input.xia_id, empty: true };
    }
    if (parsed.empty === true) {
      return { xia_id: input.xia_id, empty: true };
    }
    return {
      xia_id: input.xia_id,
      empty: false,
      shape_type: typeof parsed.shapeType === 'string' ? parsed.shapeType : undefined,
      is_solid: typeof parsed.isSolid === 'boolean' ? parsed.isSolid : undefined,
      face_count: numOrUndef(parsed.faceCount),
      edge_count: numOrUndef(parsed.edgeCount),
      vert_count: numOrUndef(parsed.vertCount),
    };
  },
};
