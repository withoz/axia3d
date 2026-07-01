import { describe, it, expect, beforeEach, vi } from 'vitest';
import { TrimTool } from './TrimTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

interface MockOpts {
  picked?: { type: string; hit: { index?: number; point?: { x: number; y: number; z: number } } } | null;
  edgeMap?: number[];
  delResult?: number;
}

function mockCtx(opts: MockOpts = {}) {
  const picked =
    'picked' in opts
      ? opts.picked
      : { type: 'edge', hit: { index: 2, point: { x: 7, y: 0, z: 0 } } };
  return {
    viewport: {
      container: { getBoundingClientRect: () => ({ left: 0, top: 0 }) },
      pickEdgeOrFace: vi.fn(() => picked),
    },
    edgeMap: new Uint32Array(opts.edgeMap ?? [100, 42]), // segIdx 1 → 42 (clicked segment)
    bridge: {
      deleteEdgeCascade: vi.fn(() => opts.delResult ?? 0),
    },
    syncMesh: vi.fn(),
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('TrimTool (ADR-211 — segment delete)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: TrimTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new TrimTool(ctx);
  });

  it('name is "trim" and does not want snap', () => {
    expect(tool.name).toBe('trim');
    expect(tool.wantsSnap).toBe(false);
  });

  it('isBusy is always false (stateless)', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('clicking a segment deletes it via deleteEdgeCascade + syncs', () => {
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.deleteEdgeCascade).toHaveBeenCalledWith(42);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('no edge under cursor → warn, no delete', () => {
    ctx = mockCtx({ picked: null });
    tool = new TrimTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.deleteEdgeCascade).not.toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });

  it('engine failure (-1) → no sync', () => {
    ctx = mockCtx({ delResult: -1 });
    tool = new TrimTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.deleteEdgeCascade).toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });

  it('resolves the clicked segment id from edgeMap', () => {
    ctx = mockCtx({ edgeMap: [9, 17] }); // segIdx 1 → 17
    tool = new TrimTool(ctx);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.deleteEdgeCascade).toHaveBeenCalledWith(17);
  });
});
