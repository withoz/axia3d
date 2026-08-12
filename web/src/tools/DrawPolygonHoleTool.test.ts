import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { setLocale } from '../i18n';
import { DrawPolygonHoleTool } from './DrawPolygonHoleTool';

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

function mockToolContext() {
  return {
    bridge: {
      drillPolygonThroughHole: vi.fn().mockReturnValue(12), // through-hole default
      punchPolygonHole: vi.fn().mockReturnValue(7),
      // Default: nothing lies across the loop, so the face-hole fallback is the
      // right one — the crossing tests override it.
      drillProfileWouldCross: vi.fn().mockReturnValue(false),
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
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn(),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } as any;
}

/**
 * Drive the tool to `finalize()` with a square loop.
 *
 * The first click captures the plane; the profile points are then seeded
 * directly, because the 2nd..Nth clicks resolve through `THREE.Plane`
 * intersection, which the headless THREE mock does not carry — and the ray
 * projection is orthogonal to the drill/punch ROUTING under test.
 */
function drawSquare(tool: DrawPolygonHoleTool): void {
  tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0)); // captures the plane
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (tool as any).points = [
    new THREE.Vector3(0, 0, 0),
    new THREE.Vector3(10, 0, 0),
    new THREE.Vector3(10, 10, 0),
    new THREE.Vector3(0, 10, 0),
  ];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  (tool as any).finalize();
}

describe('DrawPolygonHoleTool — through / face routing', () => {
  // jsdom's navigator.language is 'en-US'; these assert Korean copy.
  beforeEach(() => setLocale('ko'));

  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawPolygonHoleTool;

  beforeEach(() => {
    vi.clearAllMocks();
    ctx = mockToolContext();
    tool = new DrawPolygonHoleTool(ctx);
  });

  it('THROUGH — drillPolygonThroughHole succeeds → no punch', () => {
    drawSquare(tool);
    expect(ctx.bridge.drillPolygonThroughHole).toHaveBeenCalledTimes(1);
    expect(ctx.bridge.punchPolygonHole).not.toHaveBeenCalled();
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(Toast.success).toHaveBeenCalled();
  });

  it('FACE hole — drill -1 (a sheet has no far wall) → punchPolygonHole', () => {
    ctx.bridge.drillPolygonThroughHole.mockReturnValue(-1);
    drawSquare(tool);
    expect(ctx.bridge.punchPolygonHole).toHaveBeenCalledTimes(1);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(Toast.success).toHaveBeenCalled();
  });

  // ── A crossing is refused, not quietly punched ────────────────────
  //
  // Measured in the real app before this: on a Ø80-bored 200³ box the drill
  // refused with the crossing reason, the tool fell through to
  // `punchPolygonHole`, that SUCCEEDED (41), the solid went from closed to OPEN,
  // and the user was told "면 구멍을 뚫었습니다". The punch has no reason to
  // refuse — the loop fits its host face; it is the far side that is missing.

  it('does NOT punch a face hole when the loop crosses an existing hole', () => {
    ctx.bridge.drillPolygonThroughHole.mockReturnValue(-1);
    ctx.bridge.drillProfileWouldCross.mockReturnValue(true);
    drawSquare(tool);
    expect(ctx.bridge.punchPolygonHole).not.toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
    expect(Toast.warning).toHaveBeenCalledWith(expect.stringContaining('교차'));
    expect(Toast.success).not.toHaveBeenCalled();
  });

  it('asks with the same loop and normal the drill was given', () => {
    ctx.bridge.drillPolygonThroughHole.mockReturnValue(-1);
    drawSquare(tool);
    const [points, normal] = ctx.bridge.drillProfileWouldCross.mock.calls[0];
    const drillArgs = ctx.bridge.drillPolygonThroughHole.mock.calls[0];
    expect(points).toEqual(drillArgs[0]);
    expect(normal).toEqual(drillArgs[1]);
  });

  it('only asks once the drill has refused — a successful drill asks nothing', () => {
    drawSquare(tool); // drill 12 (default)
    expect(ctx.bridge.drillProfileWouldCross).not.toHaveBeenCalled();
  });

  it('refusing leaves the tool ready to draw again', () => {
    ctx.bridge.drillPolygonThroughHole.mockReturnValue(-1);
    ctx.bridge.drillProfileWouldCross.mockReturnValue(true);
    drawSquare(tool);
    // The points are kept so the user can nudge the loop rather than restart —
    // but nothing was committed.
    expect(ctx.syncMesh).not.toHaveBeenCalled();
    tool.cleanup();
    expect(tool.isBusy()).toBe(false);
  });
});
