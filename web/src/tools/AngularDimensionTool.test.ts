import { describe, it, expect, beforeEach, vi } from 'vitest';
import { AngularDimensionTool } from './AngularDimensionTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  // edgeMap: segIdx 0 → edge 100, segIdx 1 → edge 200
  const edgeMap = new Uint32Array([100, 200]);
  const endpoints: Record<number, number[]> = { 100: [0, 1], 200: [2, 3] };
  const positions: Record<number, [number, number, number]> = {
    0: [0, 0, 0], 1: [10, 0, 0], // edge 100 along +x
    2: [0, 0, 0], 3: [0, 10, 0], // edge 200 along +y  → 90°
  };
  return {
    viewport: {
      container: { getBoundingClientRect: () => ({ left: 0, top: 0 }) },
      pickEdgeOrFace: vi.fn(),
    },
    edgeMap,
    bridge: {
      getEdgeEndpoints: vi.fn((e: number) => endpoints[e] ?? []),
      getVertexPos: vi.fn((v: number) => positions[v] ?? null),
      addAngleConstraint: vi.fn().mockReturnValue(5),
    },
    syncMesh: vi.fn(),
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('AngularDimensionTool (ADR-216)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: AngularDimensionTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new AngularDimensionTool(ctx);
  });

  it('name is "angular-dimension"', () => {
    expect(tool.name).toBe('angular-dimension');
  });

  it('first edge click becomes busy', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 0 } });
    tool.onMouseDown(ev, null);
    expect(tool.isBusy()).toBe(true);
    expect(ctx.bridge.addAngleConstraint).not.toHaveBeenCalled();
  });

  it('two edge clicks create an Angle constraint at the measured angle (90°)', () => {
    ctx.viewport.pickEdgeOrFace
      .mockReturnValueOnce({ type: 'edge', hit: { index: 0 } })  // → edge 100
      .mockReturnValueOnce({ type: 'edge', hit: { index: 2 } }); // → edge 200
    tool.onMouseDown(ev, null);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addAngleConstraint).toHaveBeenCalledTimes(1);
    const [a0, a1, b0, b1, ang] = ctx.bridge.addAngleConstraint.mock.calls[0];
    expect([a0, a1, b0, b1]).toEqual([0, 1, 2, 3]);
    expect(ang).toBeCloseTo(Math.PI / 2, 6); // +x vs +y = 90°
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('no edge under cursor → warn, idle', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue(null);
    tool.onMouseDown(ev, null);
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.addAngleConstraint).not.toHaveBeenCalled();
  });

  it('same edge twice does not create a dimension', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 0 } }); // always edge 100
    tool.onMouseDown(ev, null);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addAngleConstraint).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true);
  });

  it('Escape cancels', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 0 } });
    tool.onMouseDown(ev, null);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });
});
