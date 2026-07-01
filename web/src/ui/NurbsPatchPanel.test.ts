import { describe, it, expect, beforeEach, vi } from 'vitest';
import { NurbsPatchPanel } from './NurbsPatchPanel';

vi.mock('./Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), success: vi.fn(), fromBridgeError: vi.fn() },
}));

const PARAMS = {
  kind: 'NURBSSurface',
  nU: 3,
  nV: 2,
  degU: 2,
  degV: 1,
  ctrlPts: Array.from({ length: 3 * 2 * 3 }, (_, i) => i), // CP 2 = [6,7,8]
  weights: [1, 1, 0.7, 0.7, 1, 1],
  knotsU: [0, 0, 0, 1, 1, 1],
  knotsV: [0, 0, 1, 1],
};

function makeBridge(overrides: Record<string, unknown> = {}) {
  return {
    getNurbsSurfaceParams: vi.fn(() => ({ ...PARAMS, ctrlPts: PARAMS.ctrlPts.slice(), weights: PARAMS.weights.slice() })),
    createNurbsSurface: vi.fn(() => [42]),
    replaceNurbsSurface: vi.fn(() => [42]), // ADR-238 single-Undo SSOT
    deleteFace: vi.fn(() => true),
    recordBridgeError: vi.fn(),
    ...overrides,
  } as unknown as ConstructorParameters<typeof NurbsPatchPanel>[1];
}

function makePanel(bridge = makeBridge()) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const cb = { syncMesh: vi.fn(), selectFaces: vi.fn(), updateOverlay: vi.fn() };
  const panel = new NurbsPatchPanel(container, bridge, cb);
  return { panel, container, bridge, cb };
}

const fireChange = (el: Element, value: string) => {
  (el as HTMLInputElement).value = value;
  el.dispatchEvent(new Event('change'));
};

describe('NurbsPatchPanel (ADR-237)', () => {
  beforeEach(() => { document.body.innerHTML = ''; });

  it('starts hidden', () => {
    const { panel } = makePanel();
    expect(panel.isVisible()).toBe(false);
  });

  it('showFor a NURBS face → visible + one row per control point', () => {
    const { panel, container } = makePanel();
    panel.showFor(7);
    expect(panel.isVisible()).toBe(true);
    expect(container.querySelectorAll('.npp-row').length).toBe(6); // nU*nV
    expect((container.querySelector('.npp-title') as HTMLElement).textContent)
      .toContain('NURBS');
  });

  it('showFor with no params → hidden', () => {
    const bridge = makeBridge({ getNurbsSurfaceParams: vi.fn(() => null) });
    const { panel } = makePanel(bridge);
    panel.showFor(7);
    expect(panel.isVisible()).toBe(false);
  });

  it('hide() clears the panel', () => {
    const { panel } = makePanel();
    panel.showFor(7);
    panel.hide();
    expect(panel.isVisible()).toBe(false);
  });

  it('edit a position field (x) → re-creates with edited ctrlPts', () => {
    const { panel, container, bridge, cb } = makePanel();
    panel.showFor(7);
    const row = container.querySelectorAll('.npp-row')[2]; // CP 2 = [6,7,8]
    fireChange(row.querySelector('.npp-x')!, '999');
    // ADR-238 — single-transaction replaceNurbsSurface(oldFid, ctrlPts, nU, nV, weights, ...)
    expect(bridge.replaceNurbsSurface).toHaveBeenCalledTimes(1);
    const args = (bridge.replaceNurbsSurface as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(args[0]).toBe(7);                                       // old faceId
    expect([args[1][6], args[1][7], args[1][8]]).toEqual([999, 7, 8]); // x edited, y/z kept
    expect(args[4][2]).toBe(0.7); // weight unchanged
    expect(cb.selectFaces).toHaveBeenCalledWith([42]);
    expect(cb.syncMesh).toHaveBeenCalled();
  });

  it('edit weight number → re-creates with edited weight', () => {
    const { panel, container, bridge } = makePanel();
    panel.showFor(7);
    const row = container.querySelectorAll('.npp-row')[2];
    fireChange(row.querySelector('.npp-wn')!, '0.4');
    const args = (bridge.replaceNurbsSurface as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(args[4][2]).toBe(0.4);                       // weight edited
    expect([args[1][6], args[1][7], args[1][8]]).toEqual([6, 7, 8]); // pos kept
  });

  it('weight slider change → re-creates + syncs number field', () => {
    const { panel, container, bridge } = makePanel();
    panel.showFor(7);
    const row = container.querySelectorAll('.npp-row')[2];
    const ws = row.querySelector('.npp-ws') as HTMLInputElement;
    const wn = row.querySelector('.npp-wn') as HTMLInputElement;
    fireChange(ws, '0.3');
    expect(wn.value).toBe('0.3'); // number synced from slider
    const args = (bridge.replaceNurbsSurface as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(args[4][2]).toBe(0.3);
  });

  it('invalid weight (≤ 0) → no re-create', () => {
    const { panel, container, bridge } = makePanel();
    panel.showFor(7);
    const row = container.querySelectorAll('.npp-row')[2];
    fireChange(row.querySelector('.npp-wn')!, '0');
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled();
  });

  it('non-finite position → no re-create', () => {
    const { panel, container, bridge } = makePanel();
    panel.showFor(7);
    const row = container.querySelectorAll('.npp-row')[2];
    fireChange(row.querySelector('.npp-x')!, ''); // parseFloat('') → NaN
    expect(bridge.replaceNurbsSurface).not.toHaveBeenCalled();
  });
});
