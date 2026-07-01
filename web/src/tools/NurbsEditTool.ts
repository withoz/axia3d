/**
 * NURBS Edit Tool — edit a NURBS-class patch's per-control-point position AND
 * weight (ADR-233 weight + ADR-234 position + ADR-236 drag-on-release).
 *
 * Flow:
 *   1. Select a NURBS-class face (BezierPatch/BSplineSurface/NURBSSurface) →
 *      the control-net overlay shows (ADR-232).
 *   2. Activate this tool → it reads the selected patch's control net.
 *   3a. CLICK a CP marker (no drag) → unified prompt "x, y, z, weight" pre-filled
 *       with the current values → edit any subset → precise edit.
 *   3b. DRAG a CP marker (ADR-236) → it moves in a screen-parallel plane through
 *       its current depth (X/Y/Z axis-lock via ctx.axisLock) — the overlay marker
 *       + net lines follow live; release commits.
 *   Either commit RE-CREATES the patch (createNurbsSurface edited + deleteFace old)
 *   per ADR-232 de-risk (setFaceSurface* has no NURBS update path — live surface
 *   deform = future A2-full ADR-238). New patch re-selected, overlay refreshes.
 *   Esc / tool switch → end.
 *
 * Left-button drag is free (orbit = middle, pan = right — Viewport.ts:643), so
 * the CP drag never conflicts with the camera.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { recreateNurbsPatch } from './nurbsRecreate';
import type { NurbsSurfaceParams } from '../bridge/WasmBridge';

const NURBS_KINDS = new Set([6, 7, 8]); // BezierPatch / BSplineSurface / NURBSSurface

export class NurbsEditTool implements ITool {
  readonly name = 'nurbs-edit';
  readonly wantsSnap = false;

  private ctx: ToolContext;
  private faceId: number | null = null;
  private params: NurbsSurfaceParams | null = null;

  // ADR-236 drag state
  private grabIdx: number | null = null;
  private dragging = false;
  private grabStartCP: [number, number, number] = [0, 0, 0];
  private grabAnchor: [number, number, number] | null = null;
  private planeNormal: [number, number, number] = [0, 0, 1];
  private grabClientX = 0;
  private grabClientY = 0;
  private liveCP: [number, number, number] = [0, 0, 0];
  private liveActive = false; // ADR-239 — engine live session running this drag
  private static readonly DRAG_PX = 4; // click→drag threshold (px)

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  private resetGrab(): void {
    this.grabIdx = null;
    this.dragging = false;
    this.grabAnchor = null;
    this.liveActive = false;
  }

  /** Edited control-point array: a clone of the current params with CP `idx`
   *  moved to `cp` (used by both the live update and the commit). */
  private _editedCtrl(idx: number, cp: [number, number, number]): number[] {
    const ctrlPts = this.params!.ctrlPts.slice();
    ctrlPts[idx * 3] = cp[0];
    ctrlPts[idx * 3 + 1] = cp[1];
    ctrlPts[idx * 3 + 2] = cp[2];
    return ctrlPts;
  }

  onActivate(): void {
    const sel = this.ctx.getSelectedFaces();
    if (sel.length !== 1) {
      this.faceId = null;
      Toast.warning('NURBS 곡면 1개를 먼저 선택하세요', 2500);
      return;
    }
    const fid = sel[0];
    if (!NURBS_KINDS.has(this.ctx.bridge.faceSurfaceKind(fid))) {
      this.faceId = null;
      Toast.warning('선택한 면이 NURBS 곡면이 아닙니다', 2500);
      return;
    }
    this.faceId = fid;
    this.params = this.ctx.bridge.getNurbsSurfaceParams(fid);
    if (!this.params) {
      this.faceId = null;
      Toast.warning('제어망을 읽을 수 없습니다', 2000);
      return;
    }
    this.ctx.viewport.updateNurbsControlNet(this.params);
    Toast.info('제어점(주황 마커) 클릭=값 입력 / 드래그=이동 (X/Y/Z 축 고정, Esc 종료)', 4000);
  }

  /** ADR-239 — abort an in-progress live session (ESC / tool switch / cleanup):
   *  restore the pre-edit state so no speculative preview leaks. */
  private _cancelLiveIfActive(): void {
    if (this.liveActive || this.ctx.bridge.isLiveNurbsEditActive()) {
      this.ctx.bridge.cancelLiveNurbsEdit();
      this.ctx.syncMesh();
    }
  }

  onDeactivate(): void {
    this._cancelLiveIfActive();
    this.resetGrab();
    this.faceId = null;
    this.params = null;
  }

  // ADR-236 — grab a CP (no prompt yet). Commit happens on mouseUp:
  //   moved beyond DRAG_PX → drag re-create; otherwise → click prompt.
  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    this.resetGrab();
    if (this.faceId == null || !this.params) {
      Toast.warning('NURBS 곡면 선택 후 도구를 다시 활성화하세요', 2000);
      return;
    }
    const idx = this.ctx.viewport.pickControlNetPoint(e);
    if (idx == null || idx < 0 || idx >= this.params.weights.length) {
      Toast.info('제어점 마커를 클릭하세요', 1500);
      return;
    }
    const p = this.params;
    const cp: [number, number, number] = [
      p.ctrlPts[idx * 3],
      p.ctrlPts[idx * 3 + 1],
      p.ctrlPts[idx * 3 + 2],
    ];
    this.grabIdx = idx;
    this.dragging = false;
    this.grabStartCP = cp;
    this.liveCP = cp;
    this.grabClientX = e.clientX;
    this.grabClientY = e.clientY;
    // screen-parallel drag plane through the CP's current position
    this.planeNormal = this.ctx.viewport.cameraForward();
    this.grabAnchor = this.ctx.viewport.rayToPlane(e, cp, this.planeNormal);
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.grabIdx == null || !this.params || !this.grabAnchor) return;
    // click→drag threshold (avoids jitter promoting a click to a drag)
    if (
      !this.dragging &&
      Math.hypot(e.clientX - this.grabClientX, e.clientY - this.grabClientY) <=
        NurbsEditTool.DRAG_PX
    ) {
      return;
    }
    const hit = this.ctx.viewport.rayToPlane(e, this.grabStartCP, this.planeNormal);
    if (!hit) return;
    const firstDragFrame = !this.dragging;
    this.dragging = true;
    let dx = hit[0] - this.grabAnchor[0];
    let dy = hit[1] - this.grabAnchor[1];
    let dz = hit[2] - this.grabAnchor[2];
    // axis-lock (X/Y/Z) — MoveTool 식 (ctx.axisLock)
    const axis = this.ctx.axisLock;
    if (axis === 'x') { dy = 0; dz = 0; }
    else if (axis === 'y') { dx = 0; dz = 0; }
    else if (axis === 'z') { dx = 0; dy = 0; }
    this.liveCP = [
      this.grabStartCP[0] + dx,
      this.grabStartCP[1] + dy,
      this.grabStartCP[2] + dz,
    ];
    const idx = this.grabIdx;
    const p = this.params;
    const ctrlPts = this._editedCtrl(idx, this.liveCP);
    // ADR-239 — begin the engine live session on the first drag frame.
    if (firstDragFrame) {
      this.liveActive = this.ctx.bridge.beginLiveNurbsEdit(this.faceId!);
    }
    // ADR-239 — live surface deform via per-frame re-create (no transaction).
    // Falls back (liveActive=false on legacy build) to marker-only preview;
    // the surface then updates on release via _recreate (ADR-236 behavior).
    if (this.liveActive) {
      const newFaces = this.ctx.bridge.updateLiveNurbsEdit(
        ctrlPts, p.nU, p.nV, p.weights, p.knotsU, p.knotsV, p.degU, p.degV,
      );
      if (newFaces.length) {
        this.faceId = newFaces[0];
        this.ctx.syncMesh();
      }
    }
    // overlay net follows the edited control net
    this.ctx.viewport.updateNurbsControlNet({ ...p, ctrlPts });
  }

  onMouseUp(_e: MouseEvent): void {
    if (this.grabIdx == null || !this.params) {
      this.resetGrab();
      return;
    }
    const idx = this.grabIdx;
    const wasDragging = this.dragging;
    const wasLive = this.liveActive;
    const live = this.liveCP;
    const p = this.params;
    this.resetGrab();
    if (!wasDragging) {
      // click (no drag) — precise unified prompt (ADR-234)
      this._promptEdit(idx);
      return;
    }
    if (wasLive) {
      // ADR-239 — commit the live session: roll back previews + ONE clean
      // replace → single Undo (position changed, weight unchanged).
      const ctrlPts = this._editedCtrl(idx, live);
      const newFaces = this.ctx.bridge.commitLiveNurbsEdit(
        ctrlPts, p.nU, p.nV, p.weights, p.knotsU, p.knotsV, p.degU, p.degV,
      );
      if (newFaces.length) {
        this.faceId = newFaces[0];
        this.ctx.syncMesh();
        this.params = this.ctx.bridge.getNurbsSurfaceParams(this.faceId);
        this.ctx.selection.selectFaces([this.faceId]);
        if (this.params) this.ctx.viewport.updateNurbsControlNet(this.params);
        Toast.success(`제어점 ${idx} 라이브 이동 → (${live[0].toFixed(0)}, ${live[1].toFixed(0)}, ${live[2].toFixed(0)})`, 2000);
      } else {
        Toast.fromBridgeError(this.ctx.bridge, 'NURBS 라이브 편집 commit 실패');
      }
    } else {
      // legacy fallback (no live engine) — commit via re-create (ADR-236).
      this._recreate(idx, live, this.params.weights[idx] ?? 1);
    }
  }

  // ADR-234 — click a CP → unified "x, y, z, weight" prompt → re-create.
  private _promptEdit(idx: number): void {
    const p = this.params!;
    const cx = p.ctrlPts[idx * 3];
    const cy = p.ctrlPts[idx * 3 + 1];
    const cz = p.ctrlPts[idx * 3 + 2];
    const cw = p.weights[idx] ?? 1;
    const def = `${cx}, ${cy}, ${cz}, ${cw}`;
    const input =
      typeof window !== 'undefined' && typeof window.prompt === 'function'
        ? window.prompt(`제어점 ${idx} — x, y, z, weight:`, def)
        : null;
    if (input == null) return;
    const parts = input.split(',').map((s) => parseFloat(s.trim()));
    if (parts.length !== 4 || parts.some((v) => !Number.isFinite(v))) {
      Toast.warning('x, y, z, weight 4개 숫자를 쉼표로 구분해 입력하세요', 2500);
      return;
    }
    const [nx, ny, nz, nw] = parts;
    if (nw <= 0) {
      Toast.warning('weight 는 0보다 큰 값이어야 합니다', 2000);
      return;
    }
    this._recreate(idx, [nx, ny, nz], nw);
  }

  private _recreate(idx: number, pos: [number, number, number], newWeight: number): void {
    const p = this.params!;
    const ctrlPts = p.ctrlPts.slice();
    ctrlPts[idx * 3] = pos[0];
    ctrlPts[idx * 3 + 1] = pos[1];
    ctrlPts[idx * 3 + 2] = pos[2];
    const weights = p.weights.slice();
    weights[idx] = newWeight;
    // ADR-237 — shared re-create SSOT (also used by NurbsPatchPanel)
    const r = recreateNurbsPatch(this.ctx.bridge, this.faceId!, p, ctrlPts, weights, {
      syncMesh: () => this.ctx.syncMesh(),
      selectFaces: (ids) => this.ctx.selection.selectFaces(ids),
      updateOverlay: (np) => this.ctx.viewport.updateNurbsControlNet(np),
    });
    if (!r) return;
    this.faceId = r.newFid;
    this.params = r.newParams;
    Toast.success(
      `제어점 ${idx} → (${pos[0]}, ${pos[1]}, ${pos[2]}) w=${newWeight.toFixed(3)} (패치 재생성)`,
      2500,
    );
    debugLog(`[NurbsEdit] CP ${idx} → [${pos.join(',')}] w=${newWeight} (face → ${r.newFid})`);
  }

  onKeyDown(_e: KeyboardEvent): void {
    // Esc handled by ToolManager (switches to select)
  }

  isBusy(): boolean {
    return this.faceId != null;
  }

  cleanup(): void {
    this._cancelLiveIfActive();
    this.resetGrab();
    this.faceId = null;
    this.params = null;
  }
}
