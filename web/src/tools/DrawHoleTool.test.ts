import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawHoleTool } from './DrawHoleTool';

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
import { showCrossingBoreChoiceDialog } from '../ui/CrossingBoreChoiceDialog';

function mockToolContext(onFace = true) {
  return {
    bridge: {
      // Default: drill succeeds (solid) → through-hole (tube-quad count > 0).
      drillThroughHole: vi.fn().mockReturnValue(24),
      punchHole: vi.fn().mockReturnValue(5),
      // Default: nothing lies across this axis, so the face-hole fallback is
      // the right one — the crossing tests override it.
      canDrillCrossingBore: vi.fn().mockReturnValue({ ok: false, crossing: false }),
      drillCrossingBore: vi.fn().mockReturnValue(128),
      lastError: vi.fn().mockReturnValue(''),
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
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace,
    }),
    get3DPoint: vi.fn().mockReturnValue(null),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getRay: vi.fn(),
  } as any;
}

vi.mock('../ui/CrossingBoreChoiceDialog', () => ({
  showCrossingBoreChoiceDialog: vi.fn().mockResolvedValue('cancel'),
}));

describe('DrawHoleTool', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawHoleTool;

  beforeEach(() => {
    vi.clearAllMocks();
    ctx = mockToolContext(true);
    tool = new DrawHoleTool(ctx);
  });

  it('name is "hole"', () => {
    expect(tool.name).toBe('hole');
  });

  it('isBusy defaults to false', () => {
    expect(tool.isBusy()).toBe(false);
  });

  describe('first click', () => {
    it('starts when clicking on a face (onFace=true)', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 20, 0));
      expect(tool.isBusy()).toBe(true);
      expect(ctx.getDrawPlane).toHaveBeenCalled();
    });

    it('refuses + warns when NOT on a face (a hole needs a host)', () => {
      ctx = mockToolContext(false);
      tool = new DrawHoleTool(ctx);
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 20, 0));
      expect(tool.isBusy()).toBe(false);
      expect(Toast.warning).toHaveBeenCalled();
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
    });

    it('does nothing when point is null', () => {
      tool.onMouseDown({} as MouseEvent, null);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('commit (via VCB radius)', () => {
    beforeEach(() => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 20, 0));
    });

    it('drills a through-hole (drillThroughHole) on a solid — punchHole NOT called', () => {
      tool.applyVCBValue(50);
      expect(ctx.bridge.drillThroughHole).toHaveBeenCalledWith(
        [10, 20, 0], [0, 0, 1], 50, 48,
      );
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
      expect(ctx.syncMesh).toHaveBeenCalled();
      expect(Toast.success).toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    it('falls back to punchHole (2D face hole) when drill finds no opposite wall (-1)', () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1); // single sheet face — no through
      tool.applyVCBValue(50);
      expect(ctx.bridge.punchHole).toHaveBeenCalledWith(
        [10, 20, 0], [0, 0, 1], 50, 48,
      );
      expect(ctx.syncMesh).toHaveBeenCalled();
      expect(Toast.success).toHaveBeenCalled();
    });

    // ── A crossing is asked about, never guessed at ──────────────────
    //
    // Measured in the real app before this: the drill refused a crossing, the
    // tool fell through to `punchHole`, that SUCCEEDED, the solid went from
    // closed to open, and the user was told "면 구멍을 뚫었습니다".

    it('does NOT punch a face hole when a bore lies across the axis', async () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1);
      ctx.bridge.canDrillCrossingBore.mockReturnValue({ ok: true, crossing: true });
      tool.applyVCBValue(50);
      await vi.waitFor(() => expect(showCrossingBoreChoiceDialog).toHaveBeenCalled());
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
    });

    it('offers the kernel only when the engine says it can do this crossing', async () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1);
      ctx.bridge.canDrillCrossingBore.mockReturnValue({
        ok: false, crossing: true, reason: '반지름이 다릅니다',
      });
      tool.applyVCBValue(50);
      await vi.waitFor(() => expect(showCrossingBoreChoiceDialog).toHaveBeenCalled());
      const opts = (showCrossingBoreChoiceDialog as unknown as ReturnType<typeof vi.fn>)
        .mock.calls[0][0];
      expect(opts.kernelAvailable).toBe(false);
      expect(opts.reason).toBe('반지름이 다릅니다');
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
    });

    it('drills the crossing when the user picks it', async () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1);
      ctx.bridge.canDrillCrossingBore.mockReturnValue({ ok: true, crossing: true });
      (showCrossingBoreChoiceDialog as unknown as ReturnType<typeof vi.fn>)
        .mockResolvedValue('kernel');
      tool.applyVCBValue(50);
      await vi.waitFor(() =>
        expect(ctx.bridge.drillCrossingBore).toHaveBeenCalledWith(
          [10, 20, 0], [0, 0, 1], 50, 48,
        ));
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('changes nothing when the user cancels', async () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1);
      ctx.bridge.canDrillCrossingBore.mockReturnValue({ ok: true, crossing: true });
      (showCrossingBoreChoiceDialog as unknown as ReturnType<typeof vi.fn>)
        .mockResolvedValue('cancel');
      ctx.syncMesh.mockClear();
      tool.applyVCBValue(50);
      await vi.waitFor(() => expect(showCrossingBoreChoiceDialog).toHaveBeenCalled());
      expect(ctx.bridge.drillCrossingBore).not.toHaveBeenCalled();
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
      expect(ctx.syncMesh).not.toHaveBeenCalled();
    });

    it('surfaces error when BOTH drill and punch fail (-1)', () => {
      ctx.bridge.drillThroughHole.mockReturnValue(-1);
      ctx.bridge.punchHole.mockReturnValue(-1);
      tool.applyVCBValue(50);
      expect(ctx.syncMesh).not.toHaveBeenCalled();
      expect(Toast.fromBridgeError).toHaveBeenCalled();
    });

    it('rejects a radius that is too small (≤1) without drilling or punching', () => {
      tool.applyVCBValue(0.5);
      expect(ctx.bridge.drillThroughHole).not.toHaveBeenCalled();
      expect(ctx.bridge.punchHole).not.toHaveBeenCalled();
      expect(Toast.warning).toHaveBeenCalled();
    });
  });

  describe('lifecycle', () => {
    it('Escape cancels', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 20, 0));
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });

    it('deactivate cleans up', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 20, 0));
      tool.onDeactivate();
      expect(tool.isBusy()).toBe(false);
    });
  });
});
