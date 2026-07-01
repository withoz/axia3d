import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawNurbsTool } from './DrawNurbsTool';
import { setNurbsPatchMode } from './NurbsPatchSettings';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext() {
  return {
    bridge: {
      createBezierPatch: vi.fn().mockReturnValue([7]),
      createNurbsSurface: vi.fn().mockReturnValue([8]),
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
      activeCamera: new THREE.PerspectiveCamera(),
    },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('100mm') },
    snap: { setReferencePoint: vi.fn() },
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 1, 0),
      up: new THREE.Vector3(0, 0, 1),
      right: new THREE.Vector3(1, 0, 0),
      onFace: false,
    }),
    get3DPoint: vi.fn().mockReturnValue(null),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn(),
  } as any;
}

describe('DrawNurbsTool', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawNurbsTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawNurbsTool(ctx);
    setNurbsPatchMode('bezier'); // ADR-231 — reset module-level mode between tests
  });

  describe('name / isBusy', () => {
    it('is "nurbs"', () => {
      expect(tool.name).toBe('nurbs');
    });
    it('defaults isBusy false', () => {
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onMouseDown — first click', () => {
    it('sets the first corner + detects the draw plane', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 0, 10));
      expect(tool.isBusy()).toBe(true);
      expect(ctx.getDrawPlane).toHaveBeenCalled();
      expect(ctx.snap.setReferencePoint).toHaveBeenCalled();
    });
    it('does nothing when the point is null', () => {
      tool.onMouseDown({} as MouseEvent, null);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onActivate / onDeactivate / Escape / cleanup', () => {
    it('activate does not throw', () => {
      expect(() => tool.onActivate()).not.toThrow();
    });
    it('deactivate cleans up', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3());
      tool.onDeactivate();
      expect(tool.isBusy()).toBe(false);
    });
    it('Escape cancels', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3());
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });
  });

  // Note: the two-click commit path (onMouseDown → getPointOnDrawPlane →
  // THREE.Plane projection) is exercised by the canonical in-browser
  // verification (ADR-087 K-ζ) — the headless THREE mock doesn't implement
  // Plane.distanceToPoint / ray.intersectPlane. The VCB path below drives the
  // same commit chain (buildControlGrid → createBezierPatch → sync → cleanup)
  // deterministically without plane projection.
  describe('VCB — square patch by side length', () => {
    it('creates a square patch with the interior control points raised along the normal', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(100);

      expect(ctx.bridge.createBezierPatch).toHaveBeenCalledTimes(1);
      const call = ctx.bridge.createBezierPatch.mock.calls[0];
      const flat: number[] = call[0];
      expect(flat.length).toBe(4 * 4 * 3); // 48 floats, row-major 4×4 grid
      expect(call[1]).toBe(4); // uCount
      expect(call[2]).toBe(4); // vCount
      // Plane normal is +Y → interior control points are raised in the y slot.
      const ys = flat.filter((_v: number, i: number) => i % 3 === 1);
      expect(ys.some((y: number) => y > 1)).toBe(true); // bulge present
      expect(ys.some((y: number) => Math.abs(y) < 1e-9)).toBe(true); // flat boundary
      expect(ctx.syncMesh).toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    it('ignores a VCB value before the first click', () => {
      tool.applyVCBValue(100);
      expect(ctx.bridge.createBezierPatch).not.toHaveBeenCalled();
    });

    it('ignores a sub-minimum VCB side length', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(0.5);
      expect(ctx.bridge.createBezierPatch).not.toHaveBeenCalled();
      expect(tool.isBusy()).toBe(true); // still armed, not committed
    });
  });

  // ADR-231 — vault mode: exact rational half-cylinder via createNurbsSurface.
  describe('vault mode (ADR-231 — rational half-cylinder)', () => {
    it('dispatches to createNurbsSurface (not Bezier) with a 5×2 degree-(2,1) rational arc', () => {
      setNurbsPatchMode('vault');
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(1000);

      expect(ctx.bridge.createBezierPatch).not.toHaveBeenCalled();
      expect(ctx.bridge.createNurbsSurface).toHaveBeenCalledTimes(1);
      const [controlPts, uCount, vCount, weights, uKnots, vKnots, degreeU, degreeV] =
        ctx.bridge.createNurbsSurface.mock.calls[0];
      expect(uCount).toBe(5);
      expect(vCount).toBe(2);
      expect(degreeU).toBe(2);
      expect(degreeV).toBe(1);
      expect(controlPts.length).toBe(5 * 2 * 3); // 30 floats
      expect(weights.length).toBe(5 * 2);        // 10 weights
      expect(uKnots).toEqual([0, 0, 0, 0.5, 0.5, 1, 1, 1]);
      expect(vKnots).toEqual([0, 0, 1, 1]);
      // canonical rational semicircle → middle (corner) weights = 1/√2
      expect(weights.some((w: number) => Math.abs(w - Math.SQRT1_2) < 1e-12)).toBe(true);
      expect(weights.filter((w: number) => w === 1).length).toBe(6); // 3 unit-weight arc CPs × 2 v
      expect(ctx.syncMesh).toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    it('peak rises to radius = half the footprint width along the plane normal', () => {
      // plane normal = +Y; width 1000 → radius 500 → peak control points at y≈500.
      setNurbsPatchMode('vault');
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(1000);
      const controlPts: number[] = ctx.bridge.createNurbsSurface.mock.calls[0][0];
      const ys = controlPts.filter((_v, i) => i % 3 === 1);
      expect(ys.some((y) => Math.abs(y - 500) < 1e-6)).toBe(true); // arc peak/corner at r=500
      expect(ys.some((y) => Math.abs(y) < 1e-9)).toBe(true);       // base edge at y=0
    });
  });
});
