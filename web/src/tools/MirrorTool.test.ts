import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { MirrorTool } from './MirrorTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      mirrorFaces: vi.fn().mockReturnValue([10, 11]),
      mirrorEdges: vi.fn().mockReturnValue([12]),
    },
    getSelectedFaces: vi.fn().mockReturnValue([1, 2]),
    selection: { getSelectedEdges: vi.fn().mockReturnValue([]) },
    syncMesh: vi.fn(),
    viewport: { scene: { add: vi.fn(), remove: vi.fn() } },
  } as any;
}

describe('MirrorTool (ADR-209)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: MirrorTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new MirrorTool(ctx);
  });

  it('name is "mirror"', () => {
    expect(tool.name).toBe('mirror');
  });

  it('is never busy (stateless mode)', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('onActivate shows the mirror-plane indicator', () => {
    tool.onActivate();
    expect(ctx.viewport.scene.add).toHaveBeenCalledTimes(1);
  });

  it('click commits mirrorFaces across the default YZ (X) plane', () => {
    tool.onActivate();
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.mirrorFaces).toHaveBeenCalledWith([1, 2], 0, 0, 0, 1, 0, 0);
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('X/Y/Z keys switch the mirror plane (normal)', () => {
    tool.onActivate();
    tool.onKeyDown({ key: 'y' } as KeyboardEvent);
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.mirrorFaces).toHaveBeenCalledWith([1, 2], 0, 0, 0, 0, 1, 0);
  });

  it('Enter commits', () => {
    tool.onActivate();
    tool.onKeyDown({ key: 'z' } as KeyboardEvent);
    tool.onKeyDown({ key: 'Enter' } as KeyboardEvent);
    expect(ctx.bridge.mirrorFaces).toHaveBeenCalledWith([1, 2], 0, 0, 0, 0, 0, 1);
  });

  it('axis key rebuilds the plane indicator (remove + add)', () => {
    tool.onActivate(); // add #1
    tool.onKeyDown({ key: 'y' } as KeyboardEvent); // remove old + add #2
    expect(ctx.viewport.scene.remove).toHaveBeenCalled();
    expect(ctx.viewport.scene.add).toHaveBeenCalledTimes(2);
  });

  it('commit with no selection does not mirror', () => {
    ctx.getSelectedFaces.mockReturnValue([]);
    tool.onActivate();
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.mirrorFaces).not.toHaveBeenCalled();
    expect(ctx.bridge.mirrorEdges).not.toHaveBeenCalled();
  });

  it('ADR-214 — mirrors selected wire edges when no faces selected', () => {
    ctx.getSelectedFaces.mockReturnValue([]);
    ctx.selection.getSelectedEdges.mockReturnValue([5, 6]);
    tool.onActivate();
    tool.onMouseDown({} as MouseEvent, null);
    expect(ctx.bridge.mirrorEdges).toHaveBeenCalledWith([5, 6], 0, 0, 0, 1, 0, 0); // default x axis
    expect(ctx.bridge.mirrorFaces).not.toHaveBeenCalled();
    expect(ctx.syncMesh).toHaveBeenCalled();
  });

  it('Escape / deactivate removes the plane indicator', () => {
    tool.onActivate();
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(ctx.viewport.scene.remove).toHaveBeenCalled();
  });
});
