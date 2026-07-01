import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ReferenceDimensionTool } from './ReferenceDimensionTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  // edgeMap: seg0 → edge 100 (circle r5), seg1 → edge 200 (straight), seg2 → edge 300 (straight)
  const edgeMap = new Uint32Array([100, 200, 300]);
  const radii: Record<number, number> = { 100: 5, 200: -1, 300: -1 };
  const endpoints: Record<number, number[]> = { 100: [7], 200: [3, 4], 300: [5, 6] };
  const positions: Record<number, [number, number, number]> = {
    3: [0, 0, 0], 4: [10, 0, 0],   // edge 200 → +x
    5: [0, 0, 0], 6: [0, 10, 0],   // edge 300 → +y (90° to 200)
    7: [5, 0, 0],
    8: [0, 0, 0], 9: [10, 0, 0],   // linear pair
  };
  return {
    get3DPoint: vi.fn(() => ({ x: 0, y: 0, z: 0 })),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: unknown) => raw),
    viewport: {
      container: { getBoundingClientRect: () => ({ left: 0, top: 0 }) },
      pickEdgeOrFace: vi.fn(),
    },
    edgeMap,
    bridge: {
      findVertexIdAt: vi.fn(() => -1),
      getVertexPos: vi.fn((id: number) => positions[id] ?? null),
      edgeCurveRadius: vi.fn((e: number) => radii[e] ?? -1),
      getEdgeEndpoints: vi.fn((e: number) => endpoints[e] ?? []),
      addReferenceDistance: vi.fn().mockReturnValue(10),
      addReferenceAngle: vi.fn().mockReturnValue(11),
      addReferenceRadius: vi.fn().mockReturnValue(12),
    },
    syncMesh: vi.fn(),
    units: { format: (v: number) => `${v}mm` },
  } as any;
}

const ev = { clientX: 10, clientY: 10 } as MouseEvent;

describe('ReferenceDimensionTool (ADR-218)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: ReferenceDimensionTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new ReferenceDimensionTool(ctx);
  });

  it('name is "reference-dimension" and starts idle', () => {
    expect(tool.name).toBe('reference-dimension');
    expect(tool.isBusy()).toBe(false);
  });

  it('vertex → vertex creates a reference Distance (read-only)', () => {
    ctx.bridge.findVertexIdAt.mockReturnValueOnce(8);
    tool.onMouseDown(ev, null);
    expect(tool.isBusy()).toBe(true); // linear mode armed
    ctx.bridge.findVertexIdAt.mockReturnValueOnce(9);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addReferenceDistance).toHaveBeenCalledWith(8, 9);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('clicking a circle edge creates a reference Radius instantly', () => {
    // no vertex under cursor → falls to edge pick → circle (edge 100, r5)
    ctx.viewport.pickEdgeOrFace.mockReturnValue({ type: 'edge', hit: { index: 0 } });
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addReferenceRadius).toHaveBeenCalledWith(7); // anchor 7
    expect(tool.isBusy()).toBe(false); // instant, no second click
  });

  it('edge → edge creates a reference Angle (read-only)', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValueOnce({ type: 'edge', hit: { index: 2 } }); // edge 200
    tool.onMouseDown(ev, null);
    expect(tool.isBusy()).toBe(true); // angular mode armed
    ctx.viewport.pickEdgeOrFace.mockReturnValueOnce({ type: 'edge', hit: { index: 4 } }); // edge 300
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addReferenceAngle).toHaveBeenCalledWith(3, 4, 5, 6);
    expect(tool.isBusy()).toBe(false);
  });

  it('Escape cancels an armed measurement', () => {
    ctx.bridge.findVertexIdAt.mockReturnValueOnce(8);
    tool.onMouseDown(ev, null);
    expect(tool.isBusy()).toBe(true);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });

  it('clicking empty space (no vertex, no edge) does nothing', () => {
    ctx.viewport.pickEdgeOrFace.mockReturnValue(null);
    tool.onMouseDown(ev, null);
    expect(ctx.bridge.addReferenceDistance).not.toHaveBeenCalled();
    expect(ctx.bridge.addReferenceRadius).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });
});
