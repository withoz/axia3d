import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { CornerChamferTool } from './CornerChamferTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  return {
    bridge: {
      findVertexIdAt: vi.fn().mockReturnValue(7),
      chamferCorner2d: vi.fn().mockReturnValue(21),
    },
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => new THREE.Vector3(0, 0, 0)),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
  } as any;
}

describe('CornerChamferTool (ADR-212)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: CornerChamferTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new CornerChamferTool(ctx);
    localStorage.clear();
  });

  it('name is "corner-chamfer"', () => {
    expect(tool.name).toBe('corner-chamfer');
  });

  it('isBusy defaults to false', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('click resolves the corner vertex via findVertexIdAt and becomes busy', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.findVertexIdAt).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true);
  });

  it('click with no vertex under cursor does not become busy', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.chamferCorner2d).not.toHaveBeenCalled();
  });

  it('VCB distance commits chamferCorner2d(vertId, dist)', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(4);
    expect(ctx.bridge.chamferCorner2d).toHaveBeenCalledWith(7, 4);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('second click commits with the last distance (default 3)', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.chamferCorner2d).toHaveBeenCalledWith(7, 3);
    expect(tool.isBusy()).toBe(false);
  });

  it('persists the distance to localStorage', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(6);
    expect(localStorage.getItem('axia:corner-chamfer:dist')).toBe('6');
  });

  it('failed chamfer (engine -1) cleans up without sync', () => {
    ctx.bridge.chamferCorner2d.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(3);
    expect(ctx.syncMesh).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('Escape cancels selection', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });

  it('VCB with no vertex selected is a no-op', () => {
    tool.applyVCBValue(3);
    expect(ctx.bridge.chamferCorner2d).not.toHaveBeenCalled();
  });
});
