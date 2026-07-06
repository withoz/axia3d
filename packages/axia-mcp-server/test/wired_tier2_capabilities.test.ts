// Newly-wired Tier 2 modificative capabilities — rotate_xia / scale_xia /
// offset_face / boolean_union / boolean_intersect. Mirrors
// tier2_capabilities.test.ts.

import { describe, it, expect } from 'vitest';
import { dispatch, CapabilityInputError } from '../src/dispatcher.js';
import { CapabilityBlockedError } from '../src/tiers.js';
import type { EngineInstance } from '../src/capabilities/types.js';

const VERSIONS = { engine_version: '0.1.0', schema_version: '1.0.0' };
const TIER2_POLICY = {
  enabled_tiers: [0 as const, 1 as const, 2 as const],
  allow_caps: new Set<string>(),
  deny_caps: new Set<string>(),
};

function mockEngine(overrides: Partial<EngineInstance> = {}): EngineInstance {
  return {
    draw_rect_as_shape: () => 1,
    draw_circle_as_shape: () => 2,
    draw_line_as_shape: () => 3,
    create_solid_extrude: () => true,
    exportSnapshotStrict: () => new Uint8Array(),
    allXiaIds: () => new Uint32Array([1, 2]),
    sceneSummary: () => '{}',
    getXiaStats: () => '{}',
    getXiaFaceIds: () => new Uint32Array([10, 11]),
    boolean_op: () =>
      JSON.stringify({ ok: true, resultFaces: [99], totalVerts: 8, totalFaces: 6 }),
    filletEdge: () => 4,
    translateVerts: () => true,
    rotateVerts: () => true,
    scaleVerts: () => true,
    offset_face: () =>
      JSON.stringify({
        ok: true,
        innerFace: 50,
        stripFaces: [51, 52],
        totalFaces: 8,
        totalVerts: 12,
      }),
    drawPolylineAsShape: () => 0,
    create_group: () => 1,
    getFaceVertices: () => new Uint32Array([100, 101, 102]),
    faceArea: () => 100,
    isFaceInVolume: () => true,
    faceInnerLoopCount: () => 0,
    edgeCurveKind: () => 0,
    faceSurfaceKind: () => 1,
    tessellateEdge: () => new Float64Array(),
    get_all_groups: () => '[]',
    ...overrides,
  };
}

describe('rotate_xia (Tier 2)', () => {
  it('blocked at default tier', async () => {
    await expect(
      dispatch(
        'rotate_xia',
        { xia_id: 1, center: [0, 0, 0], axis: [0, 0, 1], angle_deg: 90 },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityBlockedError);
  });

  it('collects unique verts, calls rotateVerts once', async () => {
    let captured:
      | { verts: number[]; cx: number; cy: number; cz: number; ax: number; ay: number; az: number; deg: number }
      | null = null;
    const engine = mockEngine({
      getXiaFaceIds: () => new Uint32Array([10, 11]),
      getFaceVertices: (fid) =>
        fid === 10 ? new Uint32Array([100, 101, 102]) : new Uint32Array([102, 103]),
      rotateVerts: (verts, cx, cy, cz, ax, ay, az, deg) => {
        captured = { verts: Array.from(verts), cx, cy, cz, ax, ay, az, deg };
        return true;
      },
    });
    const result = await dispatch(
      'rotate_xia',
      { xia_id: 1, center: [1, 2, 3], axis: [0, 0, 1], angle_deg: 45 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(captured).not.toBeNull();
    expect(captured!.verts.sort((a, b) => a - b)).toEqual([100, 101, 102, 103]);
    expect(captured!.cx).toBe(1);
    expect(captured!.az).toBe(1);
    expect(captured!.deg).toBe(45);
    expect(result.output).toEqual({ ok: true, vertex_count: 4, is_no_op: false });
  });

  it('empty XIA → no-op', async () => {
    const engine = mockEngine({ getXiaFaceIds: () => new Uint32Array() });
    const result = await dispatch(
      'rotate_xia',
      { xia_id: 1, center: [0, 0, 0], axis: [0, 0, 1], angle_deg: 90 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toEqual({ ok: true, vertex_count: 0, is_no_op: true });
  });

  it('engine failure → ok:false', async () => {
    const engine = mockEngine({ rotateVerts: () => false });
    const result = await dispatch(
      'rotate_xia',
      { xia_id: 1, center: [0, 0, 0], axis: [0, 0, 1], angle_deg: 90 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect((result.output as { ok: boolean }).ok).toBe(false);
  });

  it('rejects float xia_id', async () => {
    await expect(
      dispatch(
        'rotate_xia',
        { xia_id: 1.5, center: [0, 0, 0], axis: [0, 0, 1], angle_deg: 90 },
        { engine: mockEngine(), policy: TIER2_POLICY, versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});

describe('scale_xia (Tier 2)', () => {
  it('blocked at default tier', async () => {
    await expect(
      dispatch(
        'scale_xia',
        { xia_id: 1, center: [0, 0, 0], scale: [2, 2, 2] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityBlockedError);
  });

  it('collects unique verts, calls scaleVerts once', async () => {
    let captured: { verts: number[]; cx: number; sx: number; sy: number; sz: number } | null = null;
    const engine = mockEngine({
      getXiaFaceIds: () => new Uint32Array([10]),
      getFaceVertices: () => new Uint32Array([100, 101, 100]),
      scaleVerts: (verts, cx, _cy, _cz, sx, sy, sz) => {
        captured = { verts: Array.from(verts), cx, sx, sy, sz };
        return true;
      },
    });
    const result = await dispatch(
      'scale_xia',
      { xia_id: 1, center: [5, 0, 0], scale: [2, 3, 1] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(captured!.verts.sort((a, b) => a - b)).toEqual([100, 101]);
    expect(captured!.cx).toBe(5);
    expect(captured!.sx).toBe(2);
    expect(captured!.sy).toBe(3);
    expect(captured!.sz).toBe(1);
    expect(result.output).toEqual({ ok: true, vertex_count: 2, is_no_op: false });
  });

  it('empty XIA → no-op', async () => {
    const engine = mockEngine({ getXiaFaceIds: () => new Uint32Array() });
    const result = await dispatch(
      'scale_xia',
      { xia_id: 1, center: [0, 0, 0], scale: [2, 2, 2] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toMatchObject({ is_no_op: true });
  });

  it('engine failure (reflecting scale rejected) → ok:false', async () => {
    const engine = mockEngine({ scaleVerts: () => false });
    const result = await dispatch(
      'scale_xia',
      { xia_id: 1, center: [0, 0, 0], scale: [-1, 1, 1] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect((result.output as { ok: boolean }).ok).toBe(false);
  });
});

describe('offset_face (Tier 2)', () => {
  it('blocked at default tier', async () => {
    await expect(
      dispatch(
        'offset_face',
        { face_id: 1, distance: 2 },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityBlockedError);
  });

  it('passes face + distance, parses JSON envelope', async () => {
    let captured: { id: number; dist: number } | null = null;
    const engine = mockEngine({
      offset_face: (id, dist) => {
        captured = { id, dist };
        return JSON.stringify({
          ok: true,
          innerFace: 50,
          stripFaces: [51, 52],
          totalFaces: 8,
          totalVerts: 12,
        });
      },
    });
    const result = await dispatch(
      'offset_face',
      { face_id: 7, distance: -3 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(captured).toEqual({ id: 7, dist: -3 });
    expect(result.output).toEqual({
      ok: true,
      inner_face: 50,
      strip_faces: [51, 52],
      total_faces: 8,
      total_verts: 12,
    });
  });

  it('engine error → ok:false with error', async () => {
    const engine = mockEngine({
      offset_face: () =>
        JSON.stringify({ ok: false, error: 'multi-loop face Offset unsupported' }),
    });
    const result = await dispatch(
      'offset_face',
      { face_id: 1, distance: 1 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toMatchObject({ ok: false, error: expect.stringMatching(/multi-loop/) });
  });

  it('non-JSON → graceful error', async () => {
    const engine = mockEngine({ offset_face: () => 'not json' });
    const result = await dispatch(
      'offset_face',
      { face_id: 1, distance: 1 },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toEqual({ ok: false, error: 'engine returned non-JSON' });
  });

  it('rejects negative face_id', async () => {
    await expect(
      dispatch(
        'offset_face',
        { face_id: -1, distance: 1 },
        { engine: mockEngine(), policy: TIER2_POLICY, versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});

describe('boolean_union (Tier 2)', () => {
  it('blocked at default tier', async () => {
    await expect(
      dispatch(
        'boolean_union',
        { faces_a: [1], faces_b: [2] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityBlockedError);
  });

  it('passes faces with op=union, parses envelope', async () => {
    let captured: { a: number[]; b: number[]; op: string } | null = null;
    const engine = mockEngine({
      boolean_op: (a, b, op) => {
        captured = { a: Array.from(a), b: Array.from(b), op };
        return JSON.stringify({ ok: true, resultFaces: [99], totalVerts: 8, totalFaces: 6 });
      },
    });
    const result = await dispatch(
      'boolean_union',
      { faces_a: [1, 2], faces_b: [3] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(captured!.op).toBe('union');
    expect(captured!.a).toEqual([1, 2]);
    expect(result.output).toEqual({
      ok: true,
      result_faces: [99],
      total_verts: 8,
      total_faces: 6,
    });
  });

  it('non-JSON → graceful error', async () => {
    const engine = mockEngine({ boolean_op: () => 'nope' });
    const result = await dispatch(
      'boolean_union',
      { faces_a: [1], faces_b: [2] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toEqual({ ok: false, error: 'engine returned non-JSON' });
  });

  it('rejects empty faces_a', async () => {
    await expect(
      dispatch(
        'boolean_union',
        { faces_a: [], faces_b: [2] },
        { engine: mockEngine(), policy: TIER2_POLICY, versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityInputError);
  });
});

describe('boolean_intersect (Tier 2)', () => {
  it('blocked at default tier', async () => {
    await expect(
      dispatch(
        'boolean_intersect',
        { faces_a: [1], faces_b: [2] },
        { engine: mockEngine(), versions: VERSIONS },
      ),
    ).rejects.toThrow(CapabilityBlockedError);
  });

  it('passes faces with op=intersect', async () => {
    let capturedOp = '';
    const engine = mockEngine({
      boolean_op: (_a, _b, op) => {
        capturedOp = op;
        return JSON.stringify({ ok: true, resultFaces: [7], totalVerts: 4, totalFaces: 3 });
      },
    });
    const result = await dispatch(
      'boolean_intersect',
      { faces_a: [1], faces_b: [2] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(capturedOp).toBe('intersect');
    expect(result.output).toMatchObject({ ok: true, result_faces: [7] });
  });

  it('engine ok:false passthrough', async () => {
    const engine = mockEngine({
      boolean_op: () => JSON.stringify({ ok: false, error: 'no intersection volume' }),
    });
    const result = await dispatch(
      'boolean_intersect',
      { faces_a: [1], faces_b: [2] },
      { engine, policy: TIER2_POLICY, versions: VERSIONS },
    );
    expect(result.output).toMatchObject({ ok: false, error: expect.stringMatching(/intersection/) });
  });
});
