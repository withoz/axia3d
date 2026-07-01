import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { ChamferTool } from './ChamferTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      findVertexIdAt: vi.fn().mockReturnValue(7),
      chamferVertex3way: vi.fn().mockReturnValue(3),
    },
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => new THREE.Vector3(0, 0, 0)),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
  } as any;
}

describe('ChamferTool (ADR-207)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: ChamferTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new ChamferTool(ctx);
    localStorage.clear();
  });

  it('name is "chamfer"', () => {
    expect(tool.name).toBe('chamfer');
  });

  it('isBusy defaults to false', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('click resolves the vertex via findVertexIdAt and becomes busy', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.findVertexIdAt).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true);
  });

  it('click with no vertex under cursor does not become busy', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.chamferVertex3way).not.toHaveBeenCalled();
  });

  it('VCB radius commits chamferVertex3way(vertId, radius)', () => {
    tool.onMouseDown({} as MouseEvent, null); // selects vertId 7
    tool.applyVCBValue(2.5);
    expect(ctx.bridge.chamferVertex3way).toHaveBeenCalledWith(7, 2.5);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('second click commits with the last radius (default 2)', () => {
    tool.onMouseDown({} as MouseEvent, null); // select
    tool.onMouseDown({} as MouseEvent, null); // commit with last radius
    expect(ctx.bridge.chamferVertex3way).toHaveBeenCalledWith(7, 2);
    expect(tool.isBusy()).toBe(false);
  });

  it('persists the radius to localStorage for reuse', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(4);
    expect(localStorage.getItem('axia:chamfer:vertex-radius')).toBe('4');
  });

  it('failed chamfer (engine -1) cleans up without sync', () => {
    ctx.bridge.chamferVertex3way.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    tool.applyVCBValue(2);
    expect(ctx.syncMesh).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('Escape cancels selection', () => {
    tool.onMouseDown({} as MouseEvent, null);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });

  it('VCB with no vertex selected is a no-op', () => {
    tool.applyVCBValue(2);
    expect(ctx.bridge.chamferVertex3way).not.toHaveBeenCalled();
  });
});
