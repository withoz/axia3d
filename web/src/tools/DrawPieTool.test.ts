import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawPieTool } from './DrawPieTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext() {
  return {
    bridge: { drawPolylineAsShape: vi.fn().mockReturnValue(0) },
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    snap: { setReferencePoint: vi.fn(), getSnappedPoint: vi.fn().mockReturnValue(null) },
    syncMesh: vi.fn(),
    setLastDrawnPlane: vi.fn(),
    getDrawPlane: vi.fn().mockReturnValue({ normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0) }),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn().mockReturnValue({ ray: { intersectPlane: vi.fn().mockReturnValue(null) } }),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
}

function commitWith(tool: DrawPieTool, p0: THREE.Vector3, p1: THREE.Vector3, p2: THREE.Vector3) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const t = tool as any;
  t.plane = { normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0) };
  t.drawPlane3 = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  t.points = [p0, p1, p2];
  t.commit();
}

describe('DrawPieTool (toolbar Phase 4 — sector / 부채꼴)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawPieTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawPieTool(ctx);
  });

  it('name is pie', () => {
    expect(tool.name).toBe('pie');
  });

  it('90° sector → drawPolylineAsShape closed boundary (center…arc…center)', () => {
    // center=(0,0,0), P1=(10,0,0) radius 10 start +X, P2=(0,10,0) → 90° CCW.
    commitWith(tool,
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(10, 0, 0),
      new THREE.Vector3(0, 10, 0),
    );
    expect(ctx.bridge.drawPolylineAsShape).toHaveBeenCalledTimes(1);
    const [flat, normal] = ctx.bridge.drawPolylineAsShape.mock.calls[0];
    const n = flat.length / 3;
    // first + last point = center (closed loop)
    expect(flat[0]).toBeCloseTo(0); expect(flat[1]).toBeCloseTo(0); expect(flat[2]).toBeCloseTo(0);
    expect(flat[(n - 1) * 3]).toBeCloseTo(0); expect(flat[(n - 1) * 3 + 1]).toBeCloseTo(0);
    // arc start (index 1) = P1 = (10,0,0)
    expect(flat[3]).toBeCloseTo(10, 3); expect(flat[4]).toBeCloseTo(0, 3);
    // arc end (index n-2) ≈ (0,10,0)
    expect(flat[(n - 2) * 3]).toBeCloseTo(0, 2); expect(flat[(n - 2) * 3 + 1]).toBeCloseTo(10, 2);
    // every arc point is at radius 10 from center (z=0)
    for (let i = 1; i < n - 1; i++) {
      const r = Math.hypot(flat[i * 3], flat[i * 3 + 1]);
      expect(r).toBeCloseTo(10, 2);
      expect(flat[i * 3 + 2]).toBeCloseTo(0, 6);
    }
    // plane normal forwarded
    expect(normal).toEqual({ x: 0, y: 0, z: 1 });
  });

  it('segment count scales with span (90° → ~16 arc segments)', () => {
    commitWith(tool, new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0), new THREE.Vector3(0, 10, 0));
    const [flat] = ctx.bridge.drawPolylineAsShape.mock.calls[0];
    const arcPts = flat.length / 3 - 2; // minus 2 center points
    // 90° = quarter turn, 64 seg/turn → 16 segments → 17 arc points
    expect(arcPts).toBe(17);
  });

  it('degenerate radius (P1 ≈ center) → no commit', () => {
    commitWith(tool, new THREE.Vector3(0, 0, 0), new THREE.Vector3(0.01, 0, 0), new THREE.Vector3(0, 10, 0));
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
  });

  it('degenerate span (P2 along start dir, span≈0) → no commit', () => {
    commitWith(tool, new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0), new THREE.Vector3(20, 0, 0));
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
  });

  it('Escape cancels (isBusy false, no commit)', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tool as any).points = [new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0)];
    expect(tool.isBusy()).toBe(true);
    tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
  });
});
