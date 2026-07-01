import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { JoinTool } from './JoinTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { warning: vi.fn(), info: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockCtx() {
  return {
    bridge: {
      findVertexIdAt: vi.fn().mockReturnValue(7),
      joinCollinearAt: vi.fn().mockReturnValue(30),
    },
    syncMesh: vi.fn(),
    get3DPoint: vi.fn(() => new THREE.Vector3(0, 0, 0)),
    getSnappedPoint: vi.fn((_e: MouseEvent, raw: THREE.Vector3 | null) => raw),
  } as any;
}

describe('JoinTool (ADR-213)', () => {
  let ctx: ReturnType<typeof mockCtx>;
  let tool: JoinTool;

  beforeEach(() => {
    ctx = mockCtx();
    tool = new JoinTool(ctx);
  });

  it('name is "join"', () => {
    expect(tool.name).toBe('join');
  });

  it('isBusy is always false (instant op)', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('click resolves the vertex and merges via joinCollinearAt + syncs', () => {
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.findVertexIdAt).toHaveBeenCalled();
    expect(ctx.bridge.joinCollinearAt).toHaveBeenCalledWith(7);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('click with no vertex under cursor → warn, no join', () => {
    ctx.bridge.findVertexIdAt.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.joinCollinearAt).not.toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });

  it('engine failure (-1, not collinear) → no sync', () => {
    ctx.bridge.joinCollinearAt.mockReturnValue(-1);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.joinCollinearAt).toHaveBeenCalled();
    expect(ctx.syncMesh).not.toHaveBeenCalled();
  });
});
