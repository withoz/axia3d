import { describe, it, expect, beforeEach, vi } from 'vitest';
import { DrawPointTool } from './DrawPointTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx(point: { x: number; y: number; z: number } | null = { x: 5, y: 5, z: 0 }) {
  return {
    get3DPoint: vi.fn(() => point),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: unknown) => raw),
    bridge: {
      drawPointAsShape: vi.fn().mockReturnValue(1),
    },
    syncMesh: vi.fn(),
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('DrawPointTool (ADR-219)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: DrawPointTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new DrawPointTool(ctx);
  });

  it('name is "point" and is stateless (never busy)', () => {
    expect(tool.name).toBe('point');
    expect(tool.isBusy()).toBe(false);
  });

  it('clicking places a Point at the (snapped) 3D position', () => {
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.drawPointAsShape).toHaveBeenCalledWith(5, 5, 0);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false); // still continuous, no in-progress state
  });

  it('uses the snapped point over the raw point', () => {
    ctx.getSnappedPoint.mockReturnValue({ x: 12, y: 0, z: 0 });
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.drawPointAsShape).toHaveBeenCalledWith(12, 0, 0);
  });

  it('continuous: two clicks place two independent Points', () => {
    tool.onMouseDown(ev, null);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.drawPointAsShape).toHaveBeenCalledTimes(2);
    expect(ctx.syncMesh).toHaveBeenCalledTimes(2);
  });

  it('no 3D point under cursor → warn, no draw', () => {
    const empty = mockCtx(null);
    const t = new DrawPointTool(empty);
    t.onMouseDown(ev, null);
    expect(empty.bridge.drawPointAsShape).not.toHaveBeenCalled();
    expect(empty.syncMesh).not.toHaveBeenCalled();
  });

  it('engine error (-1) → no syncMesh', () => {
    ctx.bridge.drawPointAsShape.mockReturnValue(-1);
    tool.onMouseDown(ev, null);
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });
});
