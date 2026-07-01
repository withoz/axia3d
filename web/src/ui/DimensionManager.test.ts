import { describe, it, expect, beforeEach, vi } from 'vitest';

// Mock DimensionLabel so the manager test avoids DOM / ResizeObserver.
let lastLines: Array<{ text: string; editable?: boolean }> = [];
let capturedOnEdit: ((idx: number, v: number) => void) | null = null;
vi.mock('./DimensionLabel', () => {
  class FakeDimensionLabel {
    isEditing = false;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    constructor(_c: any) {}
    set onEdit(cb: (idx: number, v: number) => void) { capturedOnEdit = cb; }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    update(_cam: any, lines: any[]) { lastLines = lines; }
    clear() {}
  }
  return { DimensionLabel: FakeDimensionLabel };
});

import { DimensionManager } from './DimensionManager';

function mockOpts() {
  const positions: Record<number, [number, number, number]> = {
    7: [0, 0, 0],
    9: [10, 0, 0],
    // angle: edge A (10→11) +x, edge B (10→12) +y, sharing corner 10 → 90°
    10: [0, 0, 0],
    11: [10, 0, 0],
    12: [0, 10, 0],
    // radius: circle anchor 13 at (5,0,0), center (0,0,0)
    13: [5, 0, 0],
  };
  const listeners: (() => void)[] = [];
  const bridge = {
    listConstraints: vi.fn(() => [
      { id: 3, kind: 'distance', active: true, value: 10, refs: [{ vertex: 7 }, { vertex: 9 }] },
      { id: 4, kind: 'parallel', active: true, refs: [{ edge: [1, 2] }, { edge: [3, 4] }] },
      { id: 5, kind: 'distance', active: false, value: 5, refs: [{ vertex: 7 }, { vertex: 9 }] },
      { id: 6, kind: 'angle', active: true, value: Math.PI / 2, refs: [{ edge: [10, 11] }, { edge: [10, 12] }] },
      { id: 8, kind: 'radius', active: true, value: 5, refs: [{ vertex: 13 }] },
      // ADR-218 reference (read-only) dims — value omitted (None) → parenthesised, non-editable.
      { id: 20, kind: 'distance', active: true, refs: [{ vertex: 7 }, { vertex: 9 }] },
      { id: 21, kind: 'angle', active: true, refs: [{ edge: [10, 11] }, { edge: [10, 12] }] },
      { id: 22, kind: 'radius', active: true, refs: [{ vertex: 13 }] },
    ]),
    getVertexPos: vi.fn((id: number) => positions[id] ?? null),
    radiusDimAt: vi.fn((_v: number) => [0, 0, 0, 5] as [number, number, number, number]),
    setConstraintValue: vi.fn().mockReturnValue(true),
    onConstraintsChanged: vi.fn((cb: () => void) => { listeners.push(cb); return () => {}; }),
  };
  return {
    container: document.createElement('div'),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    bridge: bridge as any,
    units: { format: (v: number) => `${v.toFixed(0)}mm` } as any,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    getCamera: () => ({}) as any,
    onGeometryEdited: vi.fn(),
    fire: () => listeners.forEach((c) => c()),
  };
}

describe('DimensionManager (ADR-215)', () => {
  let opts: ReturnType<typeof mockOpts>;
  let mgr: DimensionManager;

  beforeEach(() => {
    lastLines = [];
    capturedOnEdit = null;
    opts = mockOpts();
    mgr = new DimensionManager(opts);
  });

  it('caches active Distance + Angle + Radius (filters inactive / other kinds)', () => {
    mgr.update();
    // driving id 3/6/8 + reference id 20/21/22; id 4 (parallel) + id 5 (inactive) filtered
    expect(lastLines.length).toBe(6);
    expect(lastLines[0].text).toBe('10mm');
    expect(lastLines[0].editable).toBe(true);
  });

  it('ADR-218 — reference dims (value=None) render parenthesised + read-only', () => {
    mgr.update();
    // indices 3/4/5 = reference distance(20) / angle(21) / radius(22).
    const refDist = lastLines[3] as { text: string; editable?: boolean };
    const refAng = lastLines[4] as { text: string; editable?: boolean };
    const refRad = lastLines[5] as { text: string; editable?: boolean };
    expect(refDist.text).toBe('(10mm)'); expect(refDist.editable).toBe(false);
    expect(refAng.text).toBe('(90.0°)'); expect(refAng.editable).toBe(false);
    expect(refRad.text).toBe('(R5mm)'); expect(refRad.editable).toBe(false);
  });

  it('renders a radius constraint as a center→point line with an "R" label', () => {
    mgr.update();
    const rad = lastLines[2] as { text: string; editable?: boolean };
    expect(rad.text).toBe('R5mm');
    expect(rad.editable).toBe(true);
    expect(opts.bridge.radiusDimAt).toHaveBeenCalledWith(13);
  });

  it('editing a radius label sets the radius directly (no conversion)', () => {
    mgr.update(); // index 2 = radius(8)
    capturedOnEdit!(2, 12);
    expect(opts.bridge.setConstraintValue).toHaveBeenCalledWith(8, 12);
  });

  it('renders an angle constraint as an angular dim line (arc + degree label)', () => {
    mgr.update();
    const ang = lastLines[1] as { text: string; editable?: boolean; angular?: { valueDeg: number } };
    expect(ang.text).toBe('90.0°');
    expect(ang.editable).toBe(true);
    expect(ang.angular).toBeDefined();
    expect(ang.angular!.valueDeg).toBeCloseTo(90, 4);
  });

  it('editing an angular label converts degrees → radians for setConstraintValue', () => {
    mgr.update(); // index 0 = distance(3), index 1 = angle(6)
    expect(capturedOnEdit).not.toBeNull();
    capturedOnEdit!(1, 45); // user types 45 degrees
    expect(opts.bridge.setConstraintValue).toHaveBeenCalledWith(6, Math.PI / 4);
    expect(opts.onGeometryEdited).toHaveBeenCalled();
  });

  it('builds the dim line from getVertexPos of the two vertices', () => {
    mgr.update();
    expect(opts.bridge.getVertexPos).toHaveBeenCalledWith(7);
    expect(opts.bridge.getVertexPos).toHaveBeenCalledWith(9);
  });

  it('editing a label sets the constraint value and re-syncs geometry', () => {
    mgr.update(); // populate lineIds (index 0 → constraint 3)
    expect(capturedOnEdit).not.toBeNull();
    capturedOnEdit!(0, 25);
    expect(opts.bridge.setConstraintValue).toHaveBeenCalledWith(3, 25);
    expect(opts.onGeometryEdited).toHaveBeenCalled();
  });

  it('setVisible(false) suppresses rendering', () => {
    mgr.setVisible(false);
    mgr.update();
    expect(lastLines.length).toBe(0); // update() returned early
  });

  it('subscribes to bridge constraint changes', () => {
    expect(opts.bridge.onConstraintsChanged).toHaveBeenCalled();
    // firing the event refreshes the cache without throwing
    expect(() => opts.fire()).not.toThrow();
  });
});
