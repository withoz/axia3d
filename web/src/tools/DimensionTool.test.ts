import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DimensionTool } from './DimensionTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  const positions: Record<number, [number, number, number]> = {
    7: [0, 0, 0],
    9: [10, 0, 0],
  };
  return {
    bridge: {
      findVertexIdAt: vi.fn(),
      getVertexPos: vi.fn((id: number) => positions[id] ?? null),
      addDistanceConstraint: vi.fn().mockReturnValue(3),
    },
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => new THREE.Vector3(0, 0, 0)),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
    units: { format: (v: number) => `${v}mm` },
  } as any;
}

describe('DimensionTool (ADR-215)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: DimensionTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new DimensionTool(ctx);
  });

  it('name is "dimension" and wants snap', () => {
    expect(tool.name).toBe('dimension');
    expect(tool.wantsSnap).toBe(true);
  });

  it('first click on a vertex becomes busy (waiting for 2nd)', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(7);
    tool.onMouseDown({} as MouseEvent, null);
    expect(tool.isBusy()).toBe(true);
    expect(ctx.bridge.addDistanceConstraint).not.toHaveBeenCalled();
  });

  it('two vertex clicks create a Distance constraint at the current distance', () => {
    ctx.bridge.findVertexIdAt.mockReturnValueOnce(7).mockReturnValueOnce(9);
    tool.onMouseDown({} as MouseEvent, null); // v1 = 7
    tool.onMouseDown({} as MouseEvent, null); // v2 = 9 → dist 10
    expect(ctx.bridge.addDistanceConstraint).toHaveBeenCalledWith(7, 9, 10);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('click on empty space (no vertex) warns and stays idle', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.addDistanceConstraint).not.toHaveBeenCalled();
  });

  it('clicking the same vertex twice does not create a zero-length dimension', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(7);
    tool.onMouseDown({} as MouseEvent, null); // v1 = 7
    tool.onMouseDown({} as MouseEvent, null); // same vertex → reject
    expect(ctx.bridge.addDistanceConstraint).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true); // still waiting for a valid 2nd vertex
  });

  it('Escape cancels the pending first vertex', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(7);
    tool.onMouseDown({} as MouseEvent, null);
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });
});
