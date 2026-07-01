/**
 * NurbsPatchPanel — inline NURBS control-point editor (ADR-237, A2-MVP-5).
 *
 * Shows automatically when a single NURBS-class face is selected (alongside the
 * ADR-232 control-net overlay). Lists every control point with x/y/z number
 * fields + a weight slider/number. Editing a field commits on change (Enter/blur,
 * slider release) → the patch is RE-CREATED via the shared recreateNurbsPatch
 * SSOT (createNurbsSurface edited + deleteFace old) — coexists with the drag
 * (ADR-236) and click-prompt (ADR-233/234) flows.
 *
 * Live surface deform during edit = future A2-full-1 (ADR-238 setFaceSurfaceNurbs).
 */

import { recreateNurbsPatch } from '../tools/nurbsRecreate';
import type { WasmBridge, NurbsSurfaceParams } from '../bridge/WasmBridge';
import { Toast } from './Toast';

export interface NurbsPatchPanelCallbacks {
  syncMesh: () => void;
  selectFaces: (ids: number[]) => void;
  updateOverlay: (params: NurbsSurfaceParams | null) => void;
}

const KIND_LABEL: Record<string, string> = {
  BezierPatch: 'Bezier',
  BSplineSurface: 'B-spline',
  NURBSSurface: 'NURBS',
};

const WEIGHT_MIN = 0.05;
const WEIGHT_MAX = 5;

export class NurbsPatchPanel {
  private bridge: WasmBridge;
  private cb: NurbsPatchPanelCallbacks;

  private panelEl: HTMLElement;
  private listEl: HTMLElement;
  private titleEl: HTMLElement;

  private faceId: number | null = null;
  private params: NurbsSurfaceParams | null = null;
  private visible = false;
  private recreating = false; // guard: own re-create must not trigger showFor re-render

  constructor(
    container: HTMLElement,
    bridge: WasmBridge,
    cb: NurbsPatchPanelCallbacks,
  ) {
    this.bridge = bridge;
    this.cb = cb;

    this.panelEl = document.createElement('div');
    this.panelEl.id = 'nurbs-patch-panel';
    this.panelEl.className = 'npp-panel';
    this.panelEl.innerHTML = `
      <div class="npp-header">
        <span class="npp-title">NURBS 제어점</span>
      </div>
      <div class="npp-cols">
        <span>CP</span><span>x</span><span>y</span><span>z</span><span>weight</span><span></span>
      </div>
      <div class="npp-list"></div>
    `;
    this.panelEl.style.display = 'none';
    container.appendChild(this.panelEl);

    this.listEl = this.panelEl.querySelector('.npp-list') as HTMLElement;
    this.titleEl = this.panelEl.querySelector('.npp-title') as HTMLElement;

    this.injectStyles();
  }

  /** Show the panel for a selected NURBS-class face (reads its control net). */
  showFor(faceId: number): void {
    if (this.recreating) return; // own re-create updates state + renders directly
    const params = this.bridge.getNurbsSurfaceParams(faceId);
    if (!params) {
      this.hide();
      return;
    }
    this.faceId = faceId;
    this.params = params;
    this.visible = true;
    this.panelEl.style.display = 'block';
    this.render();
  }

  hide(): void {
    if (this.recreating) return;
    this.visible = false;
    this.faceId = null;
    this.params = null;
    this.panelEl.style.display = 'none';
  }

  isVisible(): boolean {
    return this.visible;
  }

  private render(): void {
    const p = this.params;
    if (!p) return;
    this.titleEl.textContent = `NURBS 제어점 — ${KIND_LABEL[p.kind] ?? 'patch'} (${p.nU}×${p.nV})`;
    this.listEl.innerHTML = '';
    const n = p.weights.length;
    for (let i = 0; i < n; i++) {
      const u = Math.floor(i / p.nV);
      const v = i % p.nV;
      const x = p.ctrlPts[i * 3];
      const y = p.ctrlPts[i * 3 + 1];
      const z = p.ctrlPts[i * 3 + 2];
      const w = p.weights[i];

      const row = document.createElement('div');
      row.className = 'npp-row';
      row.dataset.cp = String(i);
      row.innerHTML = `
        <span class="npp-idx">#${i}<small>(${u},${v})</small></span>
        <input class="npp-x" type="number" step="1" value="${x}" title="x">
        <input class="npp-y" type="number" step="1" value="${y}" title="y">
        <input class="npp-z" type="number" step="1" value="${z}" title="z">
        <input class="npp-ws" type="range" min="${WEIGHT_MIN}" max="${WEIGHT_MAX}" step="0.01" value="${w}" title="weight">
        <input class="npp-wn" type="number" min="0.01" step="0.01" value="${w}" title="weight">
      `;
      const xi = row.querySelector('.npp-x') as HTMLInputElement;
      const yi = row.querySelector('.npp-y') as HTMLInputElement;
      const zi = row.querySelector('.npp-z') as HTMLInputElement;
      const ws = row.querySelector('.npp-ws') as HTMLInputElement;
      const wn = row.querySelector('.npp-wn') as HTMLInputElement;

      const commit = () => {
        this.editCP(
          i,
          [parseFloat(xi.value), parseFloat(yi.value), parseFloat(zi.value)],
          parseFloat(wn.value),
        );
      };
      xi.addEventListener('change', commit);
      yi.addEventListener('change', commit);
      zi.addEventListener('change', commit);
      // slider drag: sync the number display live (no re-create); commit on release
      ws.addEventListener('input', () => { wn.value = ws.value; });
      ws.addEventListener('change', () => { wn.value = ws.value; commit(); });
      wn.addEventListener('change', () => { ws.value = wn.value; commit(); });

      this.listEl.appendChild(row);
    }
  }

  /** Re-create the patch with control point `i` edited to `pos` + `weight`. */
  private editCP(i: number, pos: [number, number, number], weight: number): void {
    const p = this.params;
    if (!p || this.faceId == null) return;
    if (pos.some((c) => !Number.isFinite(c)) || !Number.isFinite(weight)) {
      Toast.warning('x, y, z, weight 는 숫자여야 합니다', 2000);
      return;
    }
    if (weight <= 0) {
      Toast.warning('weight 는 0보다 큰 값이어야 합니다', 2000);
      return;
    }
    const ctrlPts = p.ctrlPts.slice();
    ctrlPts[i * 3] = pos[0];
    ctrlPts[i * 3 + 1] = pos[1];
    ctrlPts[i * 3 + 2] = pos[2];
    const weights = p.weights.slice();
    weights[i] = weight;

    this.recreating = true;
    const r = recreateNurbsPatch(this.bridge, this.faceId, p, ctrlPts, weights, {
      syncMesh: this.cb.syncMesh,
      selectFaces: this.cb.selectFaces,
      updateOverlay: this.cb.updateOverlay,
    });
    this.recreating = false;
    if (!r) return;
    this.faceId = r.newFid;
    this.params = r.newParams;
    this.render();
  }

  private injectStyles(): void {
    if (document.getElementById('nurbs-patch-panel-styles')) return;
    const style = document.createElement('style');
    style.id = 'nurbs-patch-panel-styles';
    style.textContent = `
      .npp-panel {
        position: fixed; right: 12px; top: 200px; width: 340px; max-height: 60vh;
        background: rgba(30,30,36,0.92); color: #dcdde4;
        border: 1px solid rgba(255,255,255,0.1); border-radius: 8px;
        font-family: "Pretendard Variable", Pretendard, sans-serif; font-size: 12px;
        box-shadow: 0 6px 20px rgba(0,0,0,0.4); z-index: 181; overflow: hidden;
        display: flex; flex-direction: column;
      }
      .npp-header {
        padding: 8px 10px; background: rgba(0,0,0,0.3);
        border-bottom: 1px solid rgba(255,255,255,0.08);
      }
      .npp-title { font-weight: 500; letter-spacing: 0.3px; }
      .npp-cols, .npp-row {
        display: grid; grid-template-columns: 52px 1fr 1fr 1fr 70px 48px;
        gap: 4px; align-items: center; padding: 3px 8px;
      }
      .npp-cols { color: #888; font-size: 10px; border-bottom: 1px solid rgba(255,255,255,0.06); }
      .npp-list { overflow-y: auto; }
      .npp-row { border-bottom: 1px solid rgba(255,255,255,0.04); }
      .npp-idx { color: #ffaa33; font-size: 10px; white-space: nowrap; }
      .npp-idx small { color: #777; margin-left: 2px; }
      .npp-row input[type=number] {
        width: 100%; background: rgba(255,255,255,0.06); color: #eee;
        border: 1px solid rgba(255,255,255,0.1); border-radius: 3px;
        padding: 2px 3px; font-size: 11px;
      }
      .npp-row input[type=range] { width: 100%; }
    `;
    document.head.appendChild(style);
  }
}
