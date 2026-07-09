/**
 * DrawFreehandTool — form-mode dispatch coverage.
 *
 * ADR-087 K-γ: form-mode 활성 시 `bridge.drawPolylineAsShape` (Plane
 * attach hint 전달) 라우팅, 비활성 시 legacy `bridge.drawPolyline`.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawFreehandTool } from './DrawFreehandTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

// Stub curve helpers so commitFreehand path doesn't depend on heavy modules.
vi.mock('../curves/Curve', () => ({
  freehandFromPoints: vi.fn((pts: THREE.Vector3[]) => ({ pts })),
  tessellateCurve: vi.fn((curve: { pts: THREE.Vector3[] }) => curve.pts),
}));
vi.mock('../curves/CurveRegistry', () => ({
  getCurveRegistry: () => ({ add: vi.fn() }),
}));

function mockToolContext() {
  return {
    bridge: {
      drawPolyline: vi.fn().mockReturnValue(0),
      drawPolylineAsShape: vi.fn().mockReturnValue(0),
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
    },
    syncMesh: vi.fn(),
    snap: { setReferencePoint: vi.fn() },
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      origin: new THREE.Vector3(),
    }),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn(),
    getRay: vi.fn(),
  } as any;
}

describe('DrawFreehandTool — ADR-087 K-ε kernel-aware dispatch', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawFreehandTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawFreehandTool(ctx);
  });

  function injectPoints(t: DrawFreehandTool, pts: [number, number, number][]) {
    // Simulate mousedown to set plane + drawing flag, then inject raw points
    // bypassing mousemove (which depends on getPointOnDrawPlane internals).
    t.onMouseDown({} as MouseEvent, new THREE.Vector3(...pts[0]));
    (t as any).rawPoints = pts.map(([x, y, z]) => new THREE.Vector3(x, y, z));
  }

  it('always calls bridge.drawPolylineAsShape with normal hint', () => {
    injectPoints(tool, [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]]);
    tool.onMouseUp({} as MouseEvent);

    expect(ctx.bridge.drawPolylineAsShape).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawPolyline).not.toHaveBeenCalled();
    // Verify normal hint = (0, 0, 1) (the mocked draw plane normal)
    const args = ctx.bridge.drawPolylineAsShape.mock.calls[0];
    const normalArg = args[1];
    expect(normalArg).toEqual({ x: 0, y: 0, z: 1 });
  });

  it('preserves polyline points order', () => {
    injectPoints(tool, [[10, 0, 0], [10, 5, 0], [0, 5, 0], [0, 0, 0]]);
    tool.onMouseUp({} as MouseEvent);

    const args = ctx.bridge.drawPolylineAsShape.mock.calls[0];
    const flat = args[0] as Float64Array;
    // 4 points × 3 coords = 12 entries.
    expect(flat.length).toBe(12);
    expect(flat[0]).toBe(10); // x0
    expect(flat[3]).toBe(10); // x1
    expect(flat[7]).toBe(5);  // y2
  });
});

describe('DrawFreehandTool — ADR-284 β-4-3 curved-face dispatch', () => {
  // A mock context whose draw plane reports a SPHERE face (surfaceKind 3) and
  // whose pick resolves a host face → the curved branch engages.
  function sphereCtx() {
    return {
      bridge: {
        drawPolyline: vi.fn().mockReturnValue(0),
        drawPolylineAsShape: vi.fn().mockReturnValue(0),
        drawPolylineOnCurved: vi.fn().mockReturnValue('{"cap":4,"annulus":0}'),
        drawOpenSeamOnCurved: vi.fn().mockReturnValue('{"a":4,"b":5}'),
      },
      viewport: {
        scene: { add: vi.fn(), remove: vi.fn() },
        pick: vi.fn().mockReturnValue({ faceIndex: 0 }),
      },
      syncMesh: vi.fn(),
      snap: { setReferencePoint: vi.fn() },
      lockPlane: vi.fn(),
      getFaceId: vi.fn().mockReturnValue(0),
      getDrawPlane: vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1),
        up: new THREE.Vector3(0, 1, 0),
        origin: new THREE.Vector3(),
        surfaceKind: 3, // Sphere
      }),
      get3DPoint: vi.fn(),
      getSnappedPoint: vi.fn(),
      getRay: vi.fn(),
    } as any;
  }

  function draw(ctx: any, pts: [number, number, number][]) {
    const tool = new DrawFreehandTool(ctx);
    tool.onMouseDown({ clientX: 5, clientY: 5 } as MouseEvent, new THREE.Vector3(...pts[0]));
    (tool as any).rawPoints = pts.map(([x, y, z]) => new THREE.Vector3(x, y, z));
    tool.onMouseUp({} as MouseEvent);
  }

  it('OPEN stroke on a sphere face → drawOpenSeamOnCurved (not planar wire)', () => {
    const ctx = sphereCtx();
    // rim A → interior(z>0) → rim B: first/last far apart → open.
    draw(ctx, [[10, 0, 0], [3, 3, 8], [0, 10, 0]]);
    expect(ctx.bridge.drawOpenSeamOnCurved).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawOpenSeamOnCurved.mock.calls[0][0]).toBe(0); // host face id
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
    expect(ctx.bridge.drawPolylineOnCurved).not.toHaveBeenCalled();
  });

  it('CLOSED loop on a sphere face → drawPolylineOnCurved (cap+remainder)', () => {
    const ctx = sphereCtx();
    // near-closed loop (first ≈ last) around a small region.
    draw(ctx, [[5, 0, 8], [5, 2, 7], [3, 2, 8], [5, 0.1, 8]]);
    expect(ctx.bridge.drawPolylineOnCurved).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawPolylineOnCurved.mock.calls[0][0]).toBe('sphere');
    expect(ctx.bridge.drawOpenSeamOnCurved).not.toHaveBeenCalled();
  });

  it('β-4-4 — OPEN stroke on a CONE face (surfaceKind 4) → drawOpenSeamOnCurved', () => {
    const ctx = sphereCtx();
    ctx.getDrawPlane = vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      origin: new THREE.Vector3(),
      surfaceKind: 4, // Cone
    });
    // base rim A → point up the cone toward apex → base rim B (open).
    draw(ctx, [[5, 0, 0], [2, 2, 10], [0, 5, 0]]);
    expect(ctx.bridge.drawOpenSeamOnCurved).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawOpenSeamOnCurved.mock.calls[0][0]).toBe(0);
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
  });
});
