import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ExtendTool } from './ExtendTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

interface MockOpts {
  picked?: { type: string; hit: { index?: number; point?: { x: number; y: number; z: number } } } | null;
  edgeMap?: number[];
  boundaries?: number[];
  extendResult?: number;
}

function mockCtx(opts: MockOpts = {}) {
  const picked =
    'picked' in opts
      ? opts.picked
      : { type: 'edge', hit: { index: 2, point: { x: 5, y: 0, z: 0 } } };
  return {
    viewport: {
      container: { getBoundingClientRect: () => ({ left: 0, top: 0 }) },
      pickEdgeOrFace: vi.fn(() => picked),
    },
    edgeMap: new Uint32Array(opts.edgeMap ?? [100, 42]), // segIdx 1 → 42 (target)
    selection: { getSelectedEdges: vi.fn(() => opts.boundaries ?? [100]) },
    bridge: {
      extendEdge: vi.fn(() => opts.extendResult ?? 0),
    },
    syncMesh: vi.fn(),
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('ExtendTool (ADR-211)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: ExtendTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new ExtendTool(ctx);
  });

  it('name is "extend" and does not want snap', () => {
    expect(tool.name).toBe('extend');
    expect(tool.wantsSnap).toBe(false);
  });

  it('isBusy is always false (stateless)', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('warns and skips when no boundary edge is selected', () => {
    ctx = mockCtx({ boundaries: [] });
    tool = new ExtendTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).not.toHaveBeenCalled();
  });

  it('clicking a target edge extends it to the selected boundary', () => {
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).toHaveBeenCalledWith(42, 100);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('refuses to extend a boundary edge itself', () => {
    ctx = mockCtx({ edgeMap: [42, 100], boundaries: [100] });
    tool = new ExtendTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).not.toHaveBeenCalled();
  });

  it('no edge under cursor → warn, no extend', () => {
    ctx = mockCtx({ picked: null });
    tool = new ExtendTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).not.toHaveBeenCalled();
  });

  it('engine failure (all boundaries return -1) → no sync', () => {
    ctx = mockCtx({ extendResult: -1 });
    tool = new ExtendTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });

  it('tries each boundary, commits the first that succeeds', () => {
    ctx = mockCtx({ boundaries: [100, 200] });
    ctx.bridge.extendEdge = vi
      .fn()
      .mockReturnValueOnce(-1)
      .mockReturnValueOnce(0);
    tool = new ExtendTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.extendEdge).toHaveBeenCalledTimes(2);
    expect(ctx.bridge.extendEdge).toHaveBeenLastCalledWith(42, 200);
    expect(ctx.syncMesh).toHaveBeenCalledTimes(1);
  });
});
