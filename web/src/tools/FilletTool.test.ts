import { describe, it, expect, beforeEach, vi } from 'vitest';
import { FilletTool } from './FilletTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: { filletEdge: vi.fn().mockReturnValue(8) },
    selection: { getSelectedEdges: vi.fn().mockReturnValue([3, 4]) },
    syncMesh: vi.fn(),
  } as any;
}

describe('FilletTool (ADR-209)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: FilletTool;

  beforeEach(() => { ctx = mockToolContext(); tool = new FilletTool(ctx); localStorage.clear(); });

  it('name is "fillet"', () => { expect(tool.name).toBe('fillet'); });
  it('is never busy', () => { expect(tool.isBusy()).toBe(false); });

  it('VCB radius fillets each selected edge', () => {
    tool.applyVCBValue(3);
    expect(ctx.bridge.filletEdge).toHaveBeenCalledTimes(2);
    expect(ctx.bridge.filletEdge).toHaveBeenCalledWith(3, 3, 8);
    expect(ctx.bridge.filletEdge).toHaveBeenCalledWith(4, 3, 8);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('persists the radius to localStorage', () => {
    tool.applyVCBValue(4);
    expect(localStorage.getItem('axia:fillet:radius')).toBe('4');
  });

  it('click reuses the last radius (default 5)', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.filletEdge).toHaveBeenCalledWith(3, 5, 8);
  });

  it('no selected edge is a no-op', () => {
    ctx.selection.getSelectedEdges.mockReturnValue([]);
    tool.applyVCBValue(3);
    expect(ctx.bridge.filletEdge).not.toHaveBeenCalled();
  });

  it('Escape does not throw', () => {
    expect(() => tool.onKeyDown({ key: 'Escape' } as KeyboardEvent)).not.toThrow();
  });
});
