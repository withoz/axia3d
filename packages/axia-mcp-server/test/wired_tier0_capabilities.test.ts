// Newly-wired Tier 0 read capabilities — get_schema_version /
// get_xia_geometry_state. Mock engine + dispatch, mirrors
// tier0_capabilities.test.ts.

import { describe, it, expect } from 'vitest';
import { dispatch, CapabilityInputError } from '../src/dispatcher.js';
import type { EngineInstance } from '../src/capabilities/types.js';

const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };

function mockEngine(overrides: Partial<EngineInstance> = {}): EngineInstance {
  return {
    draw_rect_as_shape: () => 1,
    draw_circle_as_shape: () => 2,
    draw_line_as_shape: () => 3,
    create_solid_extrude: () => true,
    exportSnapshotStrict: () => new Uint8Array(),
    allXiaIds: () => new Uint32Array([1, 2]),
    sceneSummary: () =>
      JSON.stringify({
        xia_count: 2,
        face_count: 6,
        edge_count: 12,
        free_edge_count: 0,
        constraint_count: 0,
        engine_version: '0.2.1',
        schema_version: '1.3.0',
      }),
    getXiaStats: () =>
      JSON.stringify({
        empty: false,
        shapeType: 'Volume',
        isSolid: true,
        faceCount: 6,
        edgeCount: 12,
        vertCount: 8,
      }),
    getXiaFaceIds: () => new Uint32Array([10, 11]),
    boolean_op: () => '{}',
    filletEdge: () => 0,
    translateVerts: () => true,
    rotateVerts: () => true,
    scaleVerts: () => true,
    offset_face: () => '{}',
    drawPolylineAsShape: () => 0,
    create_group: () => 1,
    getFaceVertices: () => new Uint32Array([10, 11, 12, 13]),
    faceArea: () => 100,
    isFaceInVolume: () => true,
    faceInnerLoopCount: () => 0,
    edgeCurveKind: () => 0,
    faceSurfaceKind: () => 1,
    tessellateEdge: () => new Float64Array([0, 0, 0, 10, 0, 0]),
    get_all_groups: () => '[]',
    ...overrides,
  };
}

describe('get_schema_version (Tier 0, default)', () => {
  it('extracts schema + engine version from scene summary', async () => {
    const result = await dispatch(
      'get_schema_version',
      {},
      { engine: mockEngine(), versions: VERSIONS },
    );
    expect(result.output).toEqual({
      schema_version: '1.3.0',
      engine_version: '0.2.1',
    });
  });

  it('missing version fields → "unknown"', async () => {
    const engine = mockEngine({ sceneSummary: () => '{}' });
    const result = await dispatch(
      'get_schema_version',
      {},
      { engine, versions: VERSIONS },
    );
    expect(result.output).toEqual({
      schema_version: 'unknown',
      engine_version: 'unknown',
    });
  });

  it('malformed JSON → throws', async () => {
    const engine = mockEngine({ sceneSummary: () => 'not json' });
    await expect(
      dispatch('get_schema_version', {}, { engine, versions: VERSIONS }),
    ).rejects.toThrow(/malformed/);
  });
});

describe('get_xia_geometry_state (Tier 0, default)', () => {
  it('returns shape_type + is_solid + counts', async () => {
    let capturedId = -1;
    const engine = mockEngine({
      getXiaStats: (id) => {
        capturedId = id;
        return JSON.stringify({
          empty: false,
          shapeType: 'Face',
          isSolid: false,
          faceCount: 1,
          edgeCount: 4,
          vertCount: 4,
        });
      },
    });
    const result = await dispatch(
      'get_xia_geometry_state',
      { xia_id: 7 },
      { engine, versions: VERSIONS },
    );
    expect(capturedId).toBe(7);
    expect(result.output).toEqual({
      xia_id: 7,
      empty: false,
      shape_type: 'Face',
      is_solid: false,
      face_count: 1,
      edge_count: 4,
      vert_count: 4,
    });
  });

  it('empty XIA → { empty: true }', async () => {
    const engine = mockEngine({ getXiaStats: () => '{"empty":true}' });
    const result = await dispatch(
      'get_xia_geometry_state',
      { xia_id: 99 },
      { engine, versions: VERSIONS },
    );
    expect(result.output).toEqual({ xia_id: 99, empty: true });
  });

  it('malformed JSON → { empty: true } (graceful)', async () => {
    const engine = mockEngine({ getXiaStats: () => 'not json' });
    const result = await dispatch(
      'get_xia_geometry_state',
      { xia_id: 3 },
      { engine, versions: VERSIONS },
    );
    expect(result.output).toEqual({ xia_id: 3, empty: true });
  });

  it('rejects negative xia_id', async () => {
    await expect(
      dispatch(
        'get_xia_geometry_state',
        { xia_id: -1 },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});
