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

function mockToolContext(onFace = true) {
  return {
    bridge: {
      // Default: drill succeeds (solid) → through-hole (tube-quad count > 0).
      drillThroughHole: vi.fn().mockReturnValue(24),
      punchHole: vi.fn().mockReturnValue(5),
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
