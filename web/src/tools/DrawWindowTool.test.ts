import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawWindowTool } from './DrawWindowTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: {
    info: vi.fn(),
    success: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
    fromBridgeError: vi.fn(),
  },
}));

import { Toast } from '../ui/Toast';

const CORNER_B = new THREE.Vector3(300, 200, 0);

function mockToolContext() {
  return {
    bridge: {
      // ADR-262 β-3 — door attempt first. Default: NOT a door (-1) → window path.
      cutWallDoorOpening: vi.fn().mockReturnValue(-1),
      drillRectThroughHole: vi.fn().mockReturnValue(24), // through-window default
      punchRectHole: vi.fn().mockReturnValue(7),
      lastError: vi.fn().mockReturnValue(''),
    },
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
    syncMesh: vi.fn(),
    dimLabel: { clear: vi.fn() },
    snap: { setReferencePoint: vi.fn() },
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: true,
    }),
    get3DPoint: vi.fn().mockReturnValue(null),
    // 2nd-click point resolution (getPointOnDrawPlane uses the snapped point).
    getSnappedPoint: vi.fn().mockReturnValue(CORNER_B),
    getRay: vi.fn(),
  } as any;
}

/** Drive the flow → commitWindow. 1st click captures cornerA + plane; then
 *  commitWindow is invoked directly (the 2nd-click `getPointOnDrawPlane` relies
 *  on THREE.Plane.distanceToPoint, which the headless THREE mock lacks — the
 *  plane projection is orthogonal to the door/window ROUTING under test). */
function drawOpening(tool: DrawWindowTool): void {
  tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0)); // corner A (on face)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (tool as any).commitWindow(CORNER_B.clone()); // corner B → commit (routing)
}

describe('DrawWindowTool — ADR-262 β-3 door/window routing', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawWindowTool;

  beforeEach(() => {
    vi.clearAllMocks();
    ctx = mockToolContext();
    tool = new DrawWindowTool(ctx);
  });

  it('DOOR — cutWallDoorOpening succeeds (jambs > 0) → door, no drill/punch', () => {
    ctx.bridge.cutWallDoorOpening.mockReturnValue(3);
    drawOpening(tool);
    expect(ctx.bridge.cutWallDoorOpening).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drillRectThroughHole).not.toHaveBeenCalled();
    expect(ctx.bridge.punchRectHole).not.toHaveBeenCalled();
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(Toast.success).toHaveBeenCalledWith(expect.stringContaining('문'));
  });

  it('WINDOW (through) — door rejected (-1) → drillRectThroughHole', () => {
    // cutWallDoorOpening default -1; drillRectThroughHole default 24.
    drawOpening(tool);
    expect(ctx.bridge.cutWallDoorOpening).toHaveBeenCalledTimes(1); // tried first
    expect(ctx.bridge.drillRectThroughHole).toHaveBeenCalledTimes(1); // fell through
    expect(ctx.bridge.punchRectHole).not.toHaveBeenCalled();
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('FACE window — door -1 + drill -1 → punchRectHole fallback', () => {
    ctx.bridge.drillRectThroughHole.mockReturnValue(-1);
    drawOpening(tool);
    expect(ctx.bridge.cutWallDoorOpening).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.drillRectThroughHole).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.punchRectHole).toHaveBeenCalledTimes(1);
  });

  it('door tried BEFORE drill (ordering) — same corners + normal forwarded', () => {
    ctx.bridge.cutWallDoorOpening.mockReturnValue(3);
    drawOpening(tool);
    const callArgs = ctx.bridge.cutWallDoorOpening.mock.calls[0];
    // [cornerA(3), cornerB(3), normal(3)]
    expect(callArgs[0]).toEqual([0, 0, 0]);
    expect(callArgs[2]).toEqual([0, 0, 1]); // plane normal
  });

  it('first click off-face → refuse (no opening)', () => {
    ctx.getDrawPlane.mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: false,
    });
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(tool.isBusy()).toBe(false);
    expect(Toast.warning).toHaveBeenCalled();
    expect(ctx.bridge.cutWallDoorOpening).not.toHaveBeenCalled();
  });
});
