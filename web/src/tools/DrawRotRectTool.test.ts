import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawRotRectTool } from './DrawRotRectTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext() {
  return {
    bridge: { drawRectAsShape: vi.fn().mockReturnValue(0) },
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    snap: { setReferencePoint: vi.fn(), getSnappedPoint: vi.fn().mockReturnValue(null) },
    syncMesh: vi.fn(),
    setLastDrawnPlane: vi.fn(),
    getDrawPlane: vi.fn().mockReturnValue({ normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), isSketch: false }),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn().mockReturnValue({ ray: { intersectPlane: vi.fn().mockReturnValue(null) } }),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
}

function commitWith(tool: DrawRotRectTool, p0: THREE.Vector3, p1: THREE.Vector3, p2: THREE.Vector3) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const t = tool as any;
  t.plane = { normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), isSketch: false };
  t.drawPlane3 = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  t.points = [p0, p1, p2];
  t.commit();
}

describe('DrawRotRectTool (toolbar Phase 3 — rotated rectangle)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawRotRectTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawRotRectTool(ctx);
  });

  it('name is rotrect', () => {
    expect(tool.name).toBe('rotrect');
  });

  it('45° rect → drawRectAsShape with non-cardinal up, correct center/w/h', () => {
    // P0=(0,0,0), P1=(10,10,0) edge at 45° (len 10√2), P2=(0,10,0) perpendicular.
    commitWith(tool,
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(10, 10, 0),
      new THREE.Vector3(0, 10, 0),
    );
    expect(ctx.bridge.drawRectAsShape).toHaveBeenCalledTimes(1);
    const [cx, cy, cz, nx, ny, nz, ux, uy, uz, w, h] = ctx.bridge.drawRectAsShape.mock.calls[0];
    // width = |P1-P0| = 10√2 ≈ 14.142; height = perpendicular dist ≈ 7.071
    expect(w).toBeCloseTo(Math.sqrt(200), 3);
    expect(h).toBeCloseTo(Math.sqrt(50), 3);
    // up is non-cardinal (rotated): (-0.707, 0.707, 0)
    expect(ux).toBeCloseTo(-Math.SQRT1_2, 3);
    expect(uy).toBeCloseTo(Math.SQRT1_2, 3);
    expect(uz).toBeCloseTo(0, 6);
    // normal preserved (Z-up plane)
    expect(nx).toBeCloseTo(0); expect(ny).toBeCloseTo(0); expect(nz).toBeCloseTo(1);
    // center = (2.5, 7.5, 0)
    expect(cx).toBeCloseTo(2.5, 3); expect(cy).toBeCloseTo(7.5, 3); expect(cz).toBeCloseTo(0, 6);
  });

  it('axis-aligned rect (P1 along +X, P2 up +Y) → cardinal up', () => {
    commitWith(tool,
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(20, 0, 0),
      new THREE.Vector3(0, 8, 0),
    );
    const [, , , , , , ux, uy, , w, h] = ctx.bridge.drawRectAsShape.mock.calls[0];
    expect(w).toBeCloseTo(20, 3);
    expect(h).toBeCloseTo(8, 3);
    expect(ux).toBeCloseTo(0, 6); // up = +Y
    expect(uy).toBeCloseTo(1, 6);
  });

  it('degenerate (P2 on the P0-P1 line, h≈0) → no commit', () => {
    commitWith(tool,
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(10, 0, 0),
      new THREE.Vector3(5, 0, 0), // collinear → perpendicular dist 0
    );
    expect(ctx.bridge.drawRectAsShape).not.toHaveBeenCalled();
  });

  it('zero-length first edge (P0==P1) → no commit', () => {
    commitWith(tool,
      new THREE.Vector3(3, 3, 0),
      new THREE.Vector3(3, 3, 0),
      new THREE.Vector3(5, 5, 0),
    );
    expect(ctx.bridge.drawRectAsShape).not.toHaveBeenCalled();
  });

  it('Escape cancels (isBusy false, no commit)', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tool as any).points = [new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0)];
    expect(tool.isBusy()).toBe(true);
    tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.drawRectAsShape).not.toHaveBeenCalled();
  });
});
