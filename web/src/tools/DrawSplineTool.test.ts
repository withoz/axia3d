import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawSplineTool } from './DrawSplineTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext(bsplineReturn = 0) {
  return {
    bridge: {
      drawBSplineWithCurve: vi.fn().mockReturnValue(bsplineReturn),
      drawPolylineAsShape: vi.fn().mockReturnValue(0),
    },
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    snap: { setReferencePoint: vi.fn(), getSnappedPoint: vi.fn().mockReturnValue(null) },
    syncMesh: vi.fn(),
    setLastDrawnPlane: vi.fn(),
    getDrawPlane: vi.fn().mockReturnValue({ normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), onFace: false }),
    get3DPoint: vi.fn(),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn().mockReturnValue({ ray: { intersectPlane: vi.fn().mockReturnValue(null) } }),
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
}

/** Set internal control points + invoke private commit (skip mouse flow). */
function commitWith(tool: DrawSplineTool, pts: THREE.Vector3[]) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const t = tool as any;
  t.plane = { normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0) };
  t.drawPlane3 = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  t.points = pts;
  t.commit();
}

describe('DrawSplineTool (toolbar Phase 2 — open B-spline)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawSplineTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawSplineTool(ctx);
  });

  it('name is spline', () => {
    expect(tool.name).toBe('spline');
  });

  // ADR-201 β-2 — β-1 bounded the engine B-spline tessellation (64 sub-range
  // segments, no syncMesh freeze), so the tool now emits an ANALYTIC B-spline
  // via drawBSplineWithCurve (clamped knots, degree=min(3,N-1)). Falls back to
  // a de Boor polyline only if the kernel rejects (-1).
  it('5 points → drawBSplineWithCurve analytic B-spline (clamped knots, degree 3)', () => {
    commitWith(tool, [
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(10, 10, 0),
      new THREE.Vector3(20, 0, 0),
      new THREE.Vector3(30, 10, 0),
      new THREE.Vector3(40, 0, 0),
    ]);
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
    const [ctrlFlat, knots, degree] = ctx.bridge.drawBSplineWithCurve.mock.calls[0];
    expect(ctrlFlat.length).toBe(15); // 5 control pts × 3
    expect(degree).toBe(3); // min(3, 5-1)
    expect(knots.length).toBe(5 + 3 + 1); // engine contract: n + degree + 1
    // clamped: leading/trailing (degree+1) knots are repeated.
    expect(knots[0]).toBeCloseTo(0, 9);
    expect(knots[knots.length - 1]).toBeCloseTo(knots[knots.length - 2], 9);
    // control points preserved (start ctrl[0], end ctrl[last]).
    expect(ctrlFlat[0]).toBeCloseTo(0, 3); expect(ctrlFlat[1]).toBeCloseTo(0, 3);
    expect(ctrlFlat[12]).toBeCloseTo(40, 3); expect(ctrlFlat[13]).toBeCloseTo(0, 3);
  });

  it('kernel rejects (-1) → de Boor polyline fallback', () => {
    const ctx2 = mockToolContext(-1);
    const tool2 = new DrawSplineTool(ctx2);
    commitWith(tool2, [
      new THREE.Vector3(0, 0, 0),
      new THREE.Vector3(10, 10, 0),
      new THREE.Vector3(20, 0, 0),
      new THREE.Vector3(30, 10, 0),
      new THREE.Vector3(40, 0, 0),
    ]);
    expect(ctx2.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    expect(ctx2.bridge.drawPolylineAsShape).toHaveBeenCalledTimes(1); // graceful fallback
    const flat = ctx2.bridge.drawPolylineAsShape.mock.calls[0][0];
    expect(flat.length / 3).toBeLessThanOrEqual(100); // bounded polyline
  });

  it('2 points → drawBSplineWithCurve degree 1', () => {
    commitWith(tool, [new THREE.Vector3(0, 0, 0), new THREE.Vector3(30, 0, 0)]);
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    const [, , degree] = ctx.bridge.drawBSplineWithCurve.mock.calls[0];
    expect(degree).toBe(1); // min(3, 2-1)
  });

  it('< 2 points → no commit (no bridge call)', () => {
    commitWith(tool, [new THREE.Vector3(0, 0, 0)]);
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
    expect(ctx.bridge.drawBSplineWithCurve).not.toHaveBeenCalled();
  });

  it('Escape cancels (isBusy false after, no commit)', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (tool as any).points = [new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0)];
    expect(tool.isBusy()).toBe(true);
    tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Escape' }));
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.drawPolylineAsShape).not.toHaveBeenCalled();
  });

  it('Enter commits (≥2 points) via drawBSplineWithCurve then clears', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const t = tool as any;
    t.plane = { normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0) };
    t.drawPlane3 = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
    t.points = [new THREE.Vector3(0, 0, 0), new THREE.Vector3(10, 0, 0), new THREE.Vector3(20, 10, 0)];
    tool.onKeyDown(new KeyboardEvent('keydown', { key: 'Enter' }));
    expect(ctx.bridge.drawBSplineWithCurve).toHaveBeenCalledTimes(1);
    expect(tool.isBusy()).toBe(false);
  });
});
