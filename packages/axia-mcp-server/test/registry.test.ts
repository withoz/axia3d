// ADR-041 P26.8 — handler registry vs tier SSOT regression.
import { describe, it, expect } from 'vitest';
import {
  ALL_CAPABILITY_HANDLERS,
  listRegisteredCapabilities,
} from '../src/capabilities/index.js';
import { tierOf, isKnownCapability } from '../src/tiers.js';

describe('capability handler registry', () => {
  it('every registered handler has a name listed in tiers.ts', () => {
    for (const cap of ALL_CAPABILITY_HANDLERS) {
      expect(isKnownCapability(cap.name)).toBe(true);
    }
  });

  it('handler.tier matches the tier declared in tiers.ts', () => {
    for (const cap of ALL_CAPABILITY_HANDLERS) {
      expect(cap.tier).toBe(tierOf(cap.name));
    }
  });

  it('current registry surface (Stage 3 + #2 + Tier 2 expansion + wired caps)', () => {
    // Adding/removing handlers requires updating this list AND the
    // tier declarations in tiers.ts. Drift between the two = bug.
    expect(listRegisteredCapabilities().sort()).toEqual([
      'boolean_intersect',
      'boolean_subtract',
      'boolean_union',
      'create_group',
      'draw_circle',
      'draw_line',
      'draw_polyline',
      'draw_rect',
      'export_axia',
      'fillet_edge',
      'get_edge_info',
      'get_face_info',
      'get_scene_summary',
      'get_schema_version',
      'get_xia_geometry_state',
      'list_groups',
      'list_xias',
      'move_xia',
      'offset_face',
      'push_pull',
      'rotate_xia',
      'scale_xia',
    ]);
  });

  it('every handler has non-empty description (MCP tool listing requirement)', () => {
    for (const cap of ALL_CAPABILITY_HANDLERS) {
      expect(cap.description.length).toBeGreaterThan(20);
    }
  });
});
