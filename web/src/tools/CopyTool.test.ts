import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { CopyTool } from './CopyTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), fromBridgeError: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      arrayLinearFaces: vi.fn().mockReturnValue([42]),
      arrayLinearEdges: vi.fn().mockReturnValue([43]),
    },
    getSelectedFaces: vi.fn().mockReturnValue([1, 2]),
    selection: { getSelectedEdges: vi.fn().mockReturnValue([]) },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('5mm') },
    viewport: { activeCamera: new THREE.PerspectiveCamera() },
    axisLock: null,
    inferredAxis: null,
  } as any;
}

describe('CopyTool (ADR-208)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: CopyTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new CopyTool(ctx);
  });

  it('name is "copy"', () => {
    expect(tool.name).toBe('copy');
  });

  it('isBusy defaults to false', () => {
    expect(tool.isBusy()).toBe(false);
  });

  it('first click with no selection does not become busy', () => {
    ctx.getSelectedFaces.mockReturnValue([]);
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(tool.isBusy()).toBe(false);
    expect(ctx.bridge.arrayLinearFaces).not.toHaveBeenCalled();
  });

  it('first click with selection captures faces + base point', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    expect(ctx.getSelectedFaces).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(true);
    expect(ctx.bridge.arrayLinearFaces).not.toHaveBeenCalled(); // not yet
  });

  it('second click duplicates via arrayLinearFaces(faces, 1, offset)', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 0, 0));
    expect(ctx.bridge.arrayLinearFaces).toHaveBeenCalledTimes(1);
    const [faces, count, offset] = ctx.bridge.arrayLinearFaces.mock.calls[0];
    expect(faces).toEqual([1, 2]);
    expect(count).toBe(1);
    expect(offset[0]).toBeCloseTo(5);
    expect(offset[1]).toBeCloseTo(0);
    expect(offset[2]).toBeCloseTo(0);
    expect(ctx.syncMesh).toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('zero-ish offset does not duplicate', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0.01, 0, 0)); // < MIN_OFFSET
    expect(ctx.bridge.arrayLinearFaces).not.toHaveBeenCalled();
    expect(tool.isBusy()).toBe(false);
  });

  it('VCB applies an axis offset (default X)', () => {
    ctx.inferredAxis = 'y';
    tool.applyVCBValue(8);
    expect(ctx.bridge.arrayLinearFaces).toHaveBeenCalledTimes(1);
    const [faces, count, offset] = ctx.bridge.arrayLinearFaces.mock.calls[0];
    expect(faces).toEqual([1, 2]);
    expect(count).toBe(1);
    expect(offset).toEqual([0, 8, 0]);
  });

  it('Escape cancels', () => {
    tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(tool.isBusy()).toBe(false);
  });
});
