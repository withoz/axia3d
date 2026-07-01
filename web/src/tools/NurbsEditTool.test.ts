import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { NurbsEditTool } from './NurbsEditTool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), success: vi.fn(), fromBridgeError: vi.fn() },
}));

const PARAMS = {
  kind: 'NURBSSurface',
  nU: 5,
  nV: 2,
  degU: 2,
  degV: 1,
  ctrlPts: Array.from({ length: 5 * 2 * 3 }, (_, i) => i), // CP 2 = [6,7,8]
  weights: [1, 1, 0.7071, 0.7071, 1, 1, 0.7071, 0.7071, 1, 1],
  knotsU: [0, 0, 0, 0.5, 0.5, 1, 1, 1],
  knotsV: [0, 0, 1, 1],
};

// mouse helper at a screen position
const M = (x: number, y: number) => ({ clientX: x, clientY: y }) as MouseEvent;

function makeCtx(overrides: Record<string, unknown> = {}) {
  const bridge = {
    faceSurfaceKind: vi.fn(() => 8), // NURBSSurface
    getNurbsSurfaceParams: vi.fn(() => ({ ...PARAMS, weights: PARAMS.weights.slice() })),
    createNurbsSurface: vi.fn(() => [42]),
    replaceNurbsSurface: vi.fn(() => [42]), // ADR-238 single-Undo SSOT (click/prompt path)
    deleteFace: vi.fn(() => true),
    // ADR-239 live session (drag path)
    beginLiveNurbsEdit: vi.fn(() => true),
    updateLiveNurbsEdit: vi.fn(() => [43]),
    commitLiveNurbsEdit: vi.fn(() => [42]),
    cancelLiveNurbsEdit: vi.fn(() => true),
    isLiveNurbsEditActive: vi.fn(() => false),
    recordBridgeError: vi.fn(),
    ...overrides,
  };
  const viewport = {
    updateNurbsControlNet: vi.fn(),
    pickControlNetPoint: vi.fn(() => 2), // CP index 2
    cameraForward: vi.fn(() => [0, 0, 1] as [number, number, number]),
    // deterministic plane hit derived from cursor: x→x-100, y→y-100, z→(x-100)*0.5
    rayToPlane: vi.fn(
      (e: MouseEvent) =>
        [e.clientX - 100, e.clientY - 100, (e.clientX - 100) * 0.5] as [number, number, number],
    ),
  };
  const selection = { selectFaces: vi.fn() };
  const ctx = {
    bridge,
    viewport,
    selection,
    syncMesh: vi.fn(),
    getSelectedFaces: vi.fn(() => [7]),
    axisLock: null,
  } as unknown as ConstructorParameters<typeof NurbsEditTool>[0];
  return { ctx, bridge, viewport, selection };
}

describe('NurbsEditTool (ADR-233/234/236)', () => {
  let promptSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => { promptSpy = vi.spyOn(window, 'prompt'); });
  afterEach(() => { promptSpy.mockRestore(); });

  it('name / wantsSnap / not busy initially', () => {
    const { ctx } = makeCtx();
    const t = new NurbsEditTool(ctx);
    expect(t.name).toBe('nurbs-edit');
    expect(t.wantsSnap).toBe(false);
    expect(t.isBusy()).toBe(false);
  });

  it('onActivate loads the selected NURBS patch + shows overlay', () => {
    const { ctx, viewport } = makeCtx();
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    expect(t.isBusy()).toBe(true);
    expect(viewport.updateNurbsControlNet).toHaveBeenCalled();
  });

  it('onActivate refuses a non-NURBS face', () => {
    const { ctx } = makeCtx({ faceSurfaceKind: vi.fn(() => 1) });
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    expect(t.isBusy()).toBe(false);
  });

  it('onActivate refuses when not exactly one face selected', () => {
    const { ctx } = makeCtx();
    (ctx.getSelectedFaces as ReturnType<typeof vi.fn>).mockReturnValue([1, 2]);
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    expect(t.isBusy()).toBe(false);
  });

  // ── CLICK (no drag) → unified prompt → re-create (ADR-234) ──
  it('click a CP (no drag) → unified prompt → re-creates with edited pos + weight', () => {
    const { ctx, bridge, selection } = makeCtx();
    promptSpy.mockReturnValue('100, 200, 300, 0.5');
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null); // grab CP 2
    t.onMouseUp(M(100, 100));         // no move → prompt path
    // ADR-238 — single-transaction replaceNurbsSurface(oldFid, ctrlPts, nU, nV, weights, ...)
    expect(bridge.replaceNurbsSurface).toHaveBeenCalledTimes(1);
    const args = bridge.replaceNurbsSurface.mock.calls[0];
    expect(args[0]).toBe(7);                                  // old faceId
    expect([args[1][6], args[1][7], args[1][8]]).toEqual([100, 200, 300]);
    expect([args[1][0], args[1][1], args[1][2]]).toEqual([0, 1, 2]); // CP 0 unchanged
    expect(args[4][2]).toBe(0.5);                             // weights
    expect(selection.selectFaces).toHaveBeenCalledWith([42]);
  });

  // ── DRAG → live session (begin/update/commit), single Undo (ADR-239) ──
  it('drag a CP → live begin/update + commit with dragged position, weight unchanged', () => {
    const { ctx, bridge } = makeCtx();
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);  // grab CP 2 (start [6,7,8], anchor [0,0,0])
    t.onMouseMove(M(160, 140), null);  // 72px → drag; hit [60,40,30] → delta [60,40,30]
    t.onMouseUp(M(160, 140));
    expect(bridge.beginLiveNurbsEdit).toHaveBeenCalledWith(7); // session for orig faceId
    expect(bridge.updateLiveNurbsEdit).toHaveBeenCalled();     // live deform during drag
    expect(bridge.commitLiveNurbsEdit).toHaveBeenCalledTimes(1);
    const args = bridge.commitLiveNurbsEdit.mock.calls[0];
    // liveCP = start [6,7,8] + delta [60,40,30] = [66,47,38]; args = (ctrl, uc, vc, weights, ...)
    expect([args[0][6], args[0][7], args[0][8]]).toEqual([66, 47, 38]);
    expect(args[3][2]).toBe(0.7071); // weight unchanged by drag
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled(); // live path, not commit-recreate
  });

  it('drag with axis-lock Z → only Z moves (live commit)', () => {
    const { ctx, bridge } = makeCtx();
    (ctx as unknown as { axisLock: string }).axisLock = 'z';
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);  // start [6,7,8]
    t.onMouseMove(M(160, 140), null);  // hit [60,40,30]; lock z → dz=30 only
    t.onMouseUp(M(160, 140));
    const args = bridge.commitLiveNurbsEdit.mock.calls[0];
    expect([args[0][6], args[0][7], args[0][8]]).toEqual([6, 7, 38]); // only z += 30
  });

  it('legacy build (no live engine) → drag falls back to replaceNurbsSurface', () => {
    const { ctx, bridge } = makeCtx({ beginLiveNurbsEdit: vi.fn(() => false) });
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);
    t.onMouseMove(M(160, 140), null);
    t.onMouseUp(M(160, 140));
    expect(bridge.commitLiveNurbsEdit).not.toHaveBeenCalled();
    expect(bridge.replaceNurbsSurface).toHaveBeenCalledTimes(1); // ADR-236 fallback
    const args = bridge.replaceNurbsSurface.mock.calls[0];
    expect([args[1][6], args[1][7], args[1][8]]).toEqual([66, 47, 38]);
  });

  it('tiny move (< DRAG_PX) stays a click → prompt path, not drag', () => {
    const { ctx, bridge } = makeCtx();
    promptSpy.mockReturnValue('1, 2, 3, 1');
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);
    t.onMouseMove(M(102, 101), null); // ~2.2px ≤ 4 → not a drag
    t.onMouseUp(M(102, 101));
    expect(promptSpy).toHaveBeenCalled();          // click prompt fired
    expect(bridge.beginLiveNurbsEdit).not.toHaveBeenCalled(); // no drag → no live session
    expect(bridge.replaceNurbsSurface).toHaveBeenCalledTimes(1);
    const args = bridge.replaceNurbsSurface.mock.calls[0];
    expect([args[1][6], args[1][7], args[1][8]]).toEqual([1, 2, 3]); // from prompt
  });

  it('no CP under cursor → no grab, no re-create', () => {
    const { ctx, bridge, viewport } = makeCtx();
    (viewport.pickControlNetPoint as ReturnType<typeof vi.fn>).mockReturnValue(null);
    promptSpy.mockReturnValue('1, 2, 3, 1');
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);
    t.onMouseMove(M(160, 140), null);
    t.onMouseUp(M(160, 140));
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled();
  });

  it('cancelled prompt (click) → no re-create', () => {
    const { ctx, bridge } = makeCtx();
    promptSpy.mockReturnValue(null);
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    t.onMouseDown(M(100, 100), null);
    t.onMouseUp(M(100, 100));
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled();
  });

  it('invalid prompt input (wrong count / NaN / weight ≤ 0) → no re-create', () => {
    const { ctx, bridge } = makeCtx();
    const t = new NurbsEditTool(ctx);
    t.onActivate();
    for (const bad of ['1, 2, 3', 'a, 2, 3, 1', '1, 2, 3, 0', '1, 2, 3, 4, 5']) {
      promptSpy.mockReturnValue(bad);
      t.onMouseDown(M(100, 100), null);
      t.onMouseUp(M(100, 100));
    }
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled();
  });
});
