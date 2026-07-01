import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawEllipseTool } from './DrawEllipseTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext() {
  // Shared point the camera-ray ∩ plane resolves to (set per click in tests).
  const state = { nextPt: new THREE.Vector3() };
  return {
    _state: state,
    bridge: {
      drawEllipseAsCurve: vi.fn().mockReturnValue(0),
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
      activeCamera: new THREE.PerspectiveCamera(),
      renderer: {
        domElement: {
          getBoundingClientRect: () => ({
            left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600,
          }),
        },
      },
    },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('40mm') },
    snap: { setReferencePoint: vi.fn() },
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: false,
    }),
    get3DPoint: vi.fn(() => null),
    // null snap → tool uses the camera-ray ∩ plane path (avoids the THREE.Plane
    // mock's missing distanceToPoint); the ray resolves to state.nextPt.
    getSnappedPoint: vi.fn(() => null),
    getRay: vi.fn(() => ({
      ray: {
        intersectPlane: (_p: unknown, target: THREE.Vector3) => {
          target.copy(state.nextPt);
          return target;
        },
      },
    })),
    lockPlane: vi.fn(),
    setLastDrawnPlane: vi.fn(),
  } as any;
}

describe('DrawEllipseTool (ADR-206)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawEllipseTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawEllipseTool(ctx);
  });

  it('name is "ellipse"', () => {
    expect(tool.name).toBe('ellipse');
  });

  it('isBusy defaults to false', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('first click sets center + locks the plane', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(tool.isBusy()).toBe(true);
    expect(ctx.getDrawPlane).toHaveBeenCalled();
    expect(ctx.lockPlane).toHaveBeenCalledWith(expect.objectContaining({ source: 'first_click' }));
  });

  it('does nothing when first-click point is null', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(tool.isBusy()).toBe(false);
  });

  it('3-click flow commits drawEllipseAsCurve(center, refDir, normal, rx, ry)', () => {
    // click 1 — center (0,0,0) on the z=0 plane
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    // click 2 — major endpoint (40,0,0) → refDir=(1,0,0), rx=40
    ctx._state.nextPt.set(40, 0, 0);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.drawEllipseAsCurve).not.toHaveBeenCalled(); // not yet
    // click 3 — minor (0,20,0) → ry = |(0,20,0)·(normal×refDir=(0,1,0))| = 20
    ctx._state.nextPt.set(0, 20, 0);
    tool.onMouseDown({} as MouseEvent, null);

    expect(ctx.bridge.drawEllipseAsCurve).toHaveBeenCalledTimes(1);
    const a = ctx.bridge.drawEllipseAsCurve.mock.calls[0];
    expect([a[0], a[1], a[2]]).toEqual([0, 0, 0]);          // center
    expect(a[3]).toBeCloseTo(1); expect(a[4]).toBeCloseTo(0); expect(a[5]).toBeCloseTo(0); // refDir
    expect(a[6]).toBeCloseTo(0); expect(a[7]).toBeCloseTo(0); expect(a[8]).toBeCloseTo(1); // normal
    expect(a[9]).toBeCloseTo(40); expect(a[10]).toBeCloseTo(20); // rx, ry
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('VCB path (rx then ry) commits an axis-aligned ellipse', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.applyVCBValue(40); // rx along plane.right
    expect(ctx.bridge.drawEllipseAsCurve).not.toHaveBeenCalled();
    tool.applyVCBValue(20); // ry → commit
    expect(ctx.bridge.drawEllipseAsCurve).toHaveBeenCalledTimes(1);
    const a = ctx.bridge.drawEllipseAsCurve.mock.calls[0];
    expect(a[9]).toBeCloseTo(40);
    expect(a[10]).toBeCloseTo(20);
    expect(tool.isBusy()).toBe(false);
  });

  it('tiny major axis (rx < 1) does not advance', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    ctx._state.nextPt.set(0.2, 0, 0); // rx = 0.2 (< 1)
    tool.onMouseDown({} as MouseEvent, null);
    // still stage 2 (refDir not set) — the next click re-attempts the major axis
    ctx._state.nextPt.set(40, 0, 0);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.drawEllipseAsCurve).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true);
  });

  it('Escape cancels', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });

  it('deactivate cleans up', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onDeactivate();
    expect(tool.isBusy()).toBe(false);
  });
});
