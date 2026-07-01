import { describe, it, expect, beforeEach, vi } from 'vitest';
import { RadialDimensionTool } from './RadialDimensionTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  // edgeMap: segIdx 0 → edge 100 (circle r5), segIdx 1 → edge 200 (not a curve)
  const edgeMap = new Uint32Array([100, 200]);
  const radii: Record<number, number> = { 100: 5, 200: -1 };
  const endpoints: Record<number, number[]> = { 100: [7], 200: [3, 4] };
  return {
    viewport: {
      container: { getBoundingClientRect: () => ({ left: 0, top: 0 }) },
      pickEdgeOrFace: vi.fn(),
    },
    edgeMap,
    bridge: {
      edgeCurveRadius: vi.fn((e: number) => radii[e] ?? -1),
      getEdgeEndpoints: vi.fn((e: number) => endpoints[e] ?? []),
      addRadiusConstraint: vi.fn().mockReturnValue(8),
    },
    syncMesh: vi.fn(),
    units: { format: (v: number) => `${v}mm` },
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('RadialDimensionTool (ADR-217)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: RadialDimensionTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new RadialDimensionTool(ctx);
  });

  it('name is "radial-dimension" and is stateless', () => {
    expect(tool.name).toBe('radial-dimension');
    expect(tool.isBusy()).toBe(false);
  });

  it('clicking a circle edge creates a Radius constraint at its radius', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 0 } }); // → edge 100
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.edgeCurveRadius).toHaveBeenCalledWith(100);
    expect(ctx.bridge.addRadiusConstraint).toHaveBeenCalledWith(7, 5); // anchor 7, radius 5
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('clicking a non-curve edge warns and does nothing', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 2 } }); // → edge 200 (r -1)
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addRadiusConstraint).not.toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });

  it('no edge under cursor → warn, nothing', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue(null);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.edgeCurveRadius).not.toHaveBeenCalled();
    expect(ctx.bridge.addRadiusConstraint).not.toHaveBeenCalled();
  });
});
