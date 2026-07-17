// ADR-041 P26.3 — Owner ID Only (cross-boundary)
//
// All cross-boundary IDs are semantic (XiaId / FaceId / EdgeId / VertexId /
// GroupId, all u32). Raw triangle/segment indices NEVER surface.
//
// Common Zod schemas reused across capability handlers — DO NOT inline
// `z.number().int()` for IDs at handler sites; always use these.

import { z } from 'zod';

/** u32 owner ID — XiaId / FaceId / EdgeId / VertexId / GroupId. */
export const OwnerId = z
  .number()
  .int('Owner IDs must be integers (semantic IDs, not raw indices). ADR-041 P26.3.')
  .nonnegative('Owner IDs are non-negative u32 (0 is reserved for "none" in some APIs).')
  .max(0xffff_ffff, 'Owner IDs are u32 — max 4294967295.')
  .describe('Owner IDs — semantic XiaId/FaceId/EdgeId/VertexId/GroupId (ADR-041 P26.3)');

// Alias descriptions intentionally retain the "Owner ID" sentinel so the
// surface-drift scan can distinguish them from hand-rolled int fields.
export const XiaId = OwnerId.describe('Owner ID — XiaId (semantic Object/XIA identifier)');
/** ADR-050 — form-layer Shape owner (rect/circle/line draws default to Shapes). */
export const ShapeId = OwnerId.describe('Owner ID — ShapeId (form-layer Shape identifier, ADR-050)');
export const FaceId = OwnerId.describe('Owner ID — FaceId (Pick→Promote semantic face identifier)');
export const EdgeId = OwnerId.describe('Owner ID — EdgeId (Pick→Promote semantic edge identifier)');
export const VertexId = OwnerId.describe('Owner ID — VertexId (semantic vertex identifier)');
export const GroupId = OwnerId.describe('Owner ID — GroupId (semantic group identifier)');

/**
 * ADR-050 — material-library reference (u32). Deliberately NOT an OwnerId: a
 * material is a data-layer reference, not a geometry owner, so it does not
 * carry the P26.3 sentinel. `0` is the FORM_MATERIAL sentinel (no material —
 * i.e. a Shape, ADR-050 P-5e-β), so a real Xia needs `material_id >= 1`.
 */
export const MaterialId = z
  .number()
  .int('Material IDs are integer material-library references.')
  .min(
    1,
    'material_id 0 is the FORM_MATERIAL sentinel (no material) — promoting to ' +
      'a Xia requires a real material (ADR-050 P-5e-β).',
  )
  .max(0xffff_ffff, 'Material IDs are u32 — max 4294967295.')
  .describe(
    'Material-library reference (u32, ADR-050) assigned on promotion. Not a ' +
      'geometry ID.',
  );

/** 3D point in millimetres (engine native unit). */
export const Vec3 = z.tuple([z.number(), z.number(), z.number()]).describe(
  '3D point [x, y, z] in millimetres — engine native unit. Cardinal-plane ' +
    'snap (|n.{x|y|z}|>0.999) auto-applied at the bridge layer (ADR-026 P12).',
);

/** ISO-8601 timestamp string. */
export const IsoTimestamp = z
  .string()
  .datetime({ offset: true })
  .describe('ISO-8601 UTC timestamp');

/**
 * Extract all `z.number().int()`-shaped fields from a schema and confirm
 * they all derive from `OwnerId` (P26.3 enforcement). Used by the
 * `mcp_owner_ids_only_no_raw_indices` regression test.
 *
 * Implementation note: Zod 3 keeps schemas opaque, so we tag `OwnerId`
 * with a sentinel `description` substring "semantic" / "Owner IDs" and
 * scan capability schemas for non-matching int fields at test time.
 */
export const OWNER_ID_SENTINEL = 'Owner ID';
