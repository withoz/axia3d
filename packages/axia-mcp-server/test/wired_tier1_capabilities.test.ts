// Newly-wired Tier 1 constructive capabilities — draw_polyline / create_group.

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
    allXiaIds: () => new Uint32Array(),
    sceneSummary: () => '{}',
    getXiaStats: () => '{}',
    getXiaFaceIds: () => new Uint32Array(),
    boolean_op: () => '{}',
    filletEdge: () => 0,
    translateVerts: () => true,
    rotateVerts: () => true,
    scaleVerts: () => true,
    offset_face: () => '{}',
    drawPolylineAsShape: () => 0,
    create_group: () => 5,
    getFaceVertices: () => new Uint32Array(),
    faceArea: () => 0,
    isFaceInVolume: () => false,
    faceInnerLoopCount: () => 0,
    edgeCurveKind: () => 0,
    faceSurfaceKind: () => 0,
    tessellateEdge: () => new Float64Array(),
    get_all_groups: () => '[]',
    ...overrides,
  };
}

describe('draw_polyline (Tier 1, default)', () => {
  it('flattens points, passes normal, returns ok', async () => {
    let captured: { pts: number[]; nx: number; ny: number; nz: number } | null = null;
    const engine = mockEngine({
      drawPolylineAsShape: (pts, nx, ny, nz) => {
        captured = { pts: Array.from(pts), nx, ny, nz };
        return 0;
      },
    });
    const result = await dispatch(
      'draw_polyline',
      { points: [[0, 0, 0], [10, 0, 0], [10, 10, 0]], normal: [0, 0, 1] },
      { engine, versions: VERSIONS },
    );
    expect(captured).not.toBeNull();
    expect(captured!.pts).toEqual([0, 0, 0, 10, 0, 0, 10, 10, 0]);
    expect(captured!.nz).toBe(1);
    expect(result.output).toEqual({ ok: true });
  });

  it('default normal = [0,0,0] (inferred)', async () => {
    let capturedN: [number, number, number] | null = null;
    const engine = mockEngine({
      drawPolylineAsShape: (_pts, nx, ny, nz) => {
        capturedN = [nx, ny, nz];
        return 0;
      },
    });
    await dispatch(
      'draw_polyline',
      { points: [[0, 0, 0], [1, 1, 0]] },
      { engine, versions: VERSIONS },
    );
    expect(capturedN).toEqual([0, 0, 0]);
  });

  it('engine -1 → ok:false', async () => {
    const engine = mockEngine({ drawPolylineAsShape: () => -1 });
    const result = await dispatch(
      'draw_polyline',
      { points: [[0, 0, 0], [1, 1, 0]] },
      { engine, versions: VERSIONS },
    );
    expect(result.output).toEqual({ ok: false });
  });

  it('rejects fewer than 2 points', async () => {
    await expect(
      dispatch(
        'draw_polyline',
        { points: [[0, 0, 0]] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});

describe('create_group (Tier 1, default)', () => {
  it('passes name + faces, returns group_id', async () => {
    let captured: { name: string; faces: number[] } | null = null;
    const engine = mockEngine({
      create_group: (name, faces) => {
        captured = { name, faces: Array.from(faces) };
        return 42;
      },
    });
    const result = await dispatch(
      'create_group',
      { name: 'walls', face_ids: [10, 11, 12] },
      { engine, versions: VERSIONS },
    );
    expect(captured).toEqual({ name: 'walls', faces: [10, 11, 12] });
    expect(result.output).toEqual({ ok: true, group_id: 42 });
  });

  it('engine returns 0 → ok:false, no group_id', async () => {
    const engine = mockEngine({ create_group: () => 0 });
    const result = await dispatch(
      'create_group',
      { name: 'x', face_ids: [1] },
      { engine, versions: VERSIONS },
    );
    expect(result.output).toEqual({ ok: false });
  });

  it('rejects empty face_ids', async () => {
    await expect(
      dispatch(
        'create_group',
        { name: 'x', face_ids: [] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });

  it('rejects empty name', async () => {
    await expect(
      dispatch(
        'create_group',
        { name: '', face_ids: [1] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});
