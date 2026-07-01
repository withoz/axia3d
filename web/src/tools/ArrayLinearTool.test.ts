import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { ArrayLinearTool } from './ArrayLinearTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      arrayLinearFaces: vi.fn().mockReturnValue([10, 11, 12]),
      arrayLinearEdges: vi.fn().mockReturnValue([13, 14]),
    },
    getSelectedFaces: vi.fn().mockReturnValue([1, 2]),
    selection: { getSelectedEdges: vi.fn().mockReturnValue([]) },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('5mm') },
    viewport: { activeCamera: new THREE.PerspectiveCamera() },
  } as any;
}

describe('ArrayLinearTool (ADR-209)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: ArrayLinearTool;

  beforeEach(() => { ctx = mockToolContext(); tool = new ArrayLinearTool(ctx); });

  it('name is "array-linear"', () => { expect(tool.name).toBe('array-linear'); });
  it('isBusy defaults to false', () => { expect(tool.isBusy()).toBe(false); });

  it('first click with no selection is a no-op', () => {
    ctx.getSelectedFaces.mockReturnValue([]);
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(tool.isBusy()).toBe(false);
  });

  it('first click captures the base; second click arrays with default count 3', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(tool.isBusy()).toBe(true);
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 0, 0));
    expect(ctx.bridge.arrayLinearFaces).toHaveBeenCalledTimes(1);
    const [faces, count, spacing] = ctx.bridge.arrayLinearFaces.mock.calls[0];
    expect(faces).toEqual([1, 2]);
    expect(count).toBe(3);
    expect(spacing[0]).toBeCloseTo(5);
    expect(tool.isBusy()).toBe(false);
  });

  it('VCB sets the copy count', () => {
    tool.applyVCBValue(5);
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(2, 0, 0));
    expect(ctx.bridge.arrayLinearFaces.mock.calls[0][1]).toBe(5);
  });

  it('Escape cancels', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });
});
