import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ArrayRadialTool } from './ArrayRadialTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      arrayRadialFaces: vi.fn().mockReturnValue([20, 21, 22, 23, 24]),
      arrayRadialEdges: vi.fn().mockReturnValue([30, 31, 32, 33, 34]),
    },
    getSelectedFaces: vi.fn().mockReturnValue([1, 2]),
    selection: { getSelectedEdges: vi.fn().mockReturnValue([]) },
    syncMesh: vi.fn(),
  } as any;
}

describe('ArrayRadialTool (ADR-209)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: ArrayRadialTool;

  beforeEach(() => { ctx = mockToolContext(); tool = new ArrayRadialTool(ctx); });

  it('name is "array-radial"', () => { expect(tool.name).toBe('array-radial'); });
  it('is never busy', () => { expect(tool.isBusy()).toBe(false); });

  it('click commits a full-circle radial array around the default Z axis', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.arrayRadialFaces).toHaveBeenCalledTimes(1);
    const [faces, count, origin, axis, angle] = ctx.bridge.arrayRadialFaces.mock.calls[0];
    expect(faces).toEqual([1, 2]);
    expect(count).toBe(6);
    expect(origin).toEqual([0, 0, 0]);
    expect(axis).toEqual([0, 0, 1]);
    expect(angle).toBeCloseTo(Math.PI * 2);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('X/Y/Z keys switch the rotation axis', () => {
    tool.onKeyDown({ key: 'x' } as KeyboardEvent);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.arrayRadialFaces.mock.calls[0][3]).toEqual([1, 0, 0]);
  });

  it('VCB sets the copy count (≥2)', () => {
    tool.applyVCBValue(8);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.arrayRadialFaces.mock.calls[0][1]).toBe(8);
  });

  it('Enter commits', () => {
    tool.onKeyDown({ key: 'Enter' } as KeyboardEvent);
    expect(ctx.bridge.arrayRadialFaces).toHaveBeenCalledTimes(1);
  });

  it('commit with no selection is a no-op', () => {
    ctx.getSelectedFaces.mockReturnValue([]);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.arrayRadialFaces).not.toHaveBeenCalled();
  });
});
