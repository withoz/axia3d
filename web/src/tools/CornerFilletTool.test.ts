import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { CornerFilletTool } from './CornerFilletTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  return {
    bridge: {
      findVertexIdAt: vi.fn().mockReturnValue(7),
      filletCorner2d: vi.fn().mockReturnValue(20),
    },
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => new THREE.Vector3(0, 0, 0)),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
  } as any;
}

describe('CornerFilletTool (ADR-212)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: CornerFilletTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new CornerFilletTool(ctx);
    localStorage.clear();
  });

  it('name is "corner-fillet"', () => {
    expect(tool.name).toBe('corner-fillet');
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
    expect(ctx.bridge.filletCorner2d).not.toHaveBeenCalled();
  });

  it('VCB radius commits filletCorner2d(vertId, radius)', () => {
    tool.onMouseDown({} as MouseEvent, null); // selects vertId 7
    tool.applyVCBValue(4);
    expect(ctx.bridge.filletCorner2d).toHaveBeenCalledWith(7, 4);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('second click commits with the last radius (default 3)', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.filletCorner2d).toHaveBeenCalledWith(7, 3);
    expect(tool.isBusy()).toBe(false);
  });

  it('persists the radius to localStorage', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(5);
    expect(localStorage.getItem('axia:corner-fillet:radius')).toBe('5');
  });

  it('failed fillet (engine -1) cleans up without sync', () => {
    ctx.bridge.filletCorner2d.mockReturnValue(-1);
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
    expect(ctx.bridge.filletCorner2d).not.toHaveBeenCalled();
  });
});
