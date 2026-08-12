import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { setLocale } from '../i18n';
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
      // Default: nothing lies across the opening, so the face-window fallback
      // is the right one — the crossing tests override it.
      drillProfileWouldCross: vi.fn().mockReturnValue(false),
      recordRectOpening: vi.fn(),
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
  // jsdom's navigator.language is 'en-US'; these assert Korean copy.
  beforeEach(() => setLocale('ko'));

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

  // ── A crossing is refused, not quietly punched ──────────────────
  //
  // Measured in the real app before this: on a Ø80-bored 200³ box the drill
  // refused with the crossing reason, the tool fell through to `punchRectHole`,
  // that SUCCEEDED (41), the solid went from closed to OPEN, and the user was
  // told "창을 냈습니다". The punch has no reason to refuse — the rect fits
  // its host face; it is the far side that is missing — so the tool has to ask.

  it('does NOT punch a face window when the opening crosses an existing hole', () => {
    ctx.bridge.drillRectThroughHole.mockReturnValue(-1);
    ctx.bridge.drillProfileWouldCross.mockReturnValue(true);
    drawOpening(tool);
    expect(ctx.bridge.punchRectHole).not.toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
    expect(ctx.bridge.recordRectOpening).not.toHaveBeenCalled();
    expect(Toast.warning).toHaveBeenCalledWith(expect.stringContaining('교차'));
    expect(Toast.success).not.toHaveBeenCalled();
  });

  it('asks with the same corners and normal the drill was given', () => {
    ctx.bridge.drillRectThroughHole.mockReturnValue(-1);
    drawOpening(tool);
    const [points, normal] = ctx.bridge.drillProfileWouldCross.mock.calls[0];
    const drillArgs = ctx.bridge.drillRectThroughHole.mock.calls[0];
    expect(points).toEqual([drillArgs[0], drillArgs[1]]);
    expect(normal).toEqual(drillArgs[2]);
  });

  it('only asks once the drill has refused — a successful drill asks nothing', () => {
    drawOpening(tool); // drill 24 (default)
    expect(ctx.bridge.drillProfileWouldCross).not.toHaveBeenCalled();
  });

  // ADR-203 opening round-trip  a successful opening is recorded so IFC export
  // can re-emit it as an IfcOpeningElement.
  it('records the opening on the door success path', () => {
    ctx.bridge.cutWallDoorOpening.mockReturnValue(3);
    drawOpening(tool);
    expect(ctx.bridge.recordRectOpening).toHaveBeenCalledTimes(1);
  });

  it('records the opening on the through-window path', () => {
    drawOpening(tool); // door -1, drill 24 (default)
    expect(ctx.bridge.recordRectOpening).toHaveBeenCalledTimes(1);
  });

  it('records the opening on the face-window path', () => {
    ctx.bridge.drillRectThroughHole.mockReturnValue(-1);
    drawOpening(tool); // door -1, drill -1, punch 7
    expect(ctx.bridge.recordRectOpening).toHaveBeenCalledTimes(1);
  });

  it('does not record an opening when every punch fails', () => {
    ctx.bridge.drillRectThroughHole.mockReturnValue(-1);
    ctx.bridge.punchRectHole.mockReturnValue(-1);
    drawOpening(tool);
    expect(ctx.bridge.recordRectOpening).not.toHaveBeenCalled();
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
