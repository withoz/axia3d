/**
 * Copy / Duplicate Tool — duplicate the selected faces at a click-defined offset
 * (ADR-208).
 *
 * Flow (MoveTool-style 2-click):
 *   select faces → click 1 base point → click 2 target point
 *   → arrayLinearFaces(faces, count=1, offset = target − base) → 1 copy, original kept.
 *
 * Engine + WASM + bridge (arrayLinearFaces) already exist; Copy = count=1 (one
 * translated copy, the original preserved). UI-only, no new kernel work (Pattern-12).
 * Distinct from clipboard-copy (Ctrl+C/V/D) — this is an in-place duplicate-at-offset.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const MIN_OFFSET = 0.1; // mm — below this the duplicate would coincide with the original

export class CopyTool implements ITool {
  readonly name = 'copy';

  private ctx: ToolContext;
  private faces: number[] | null = null;
  private edges: number[] | null = null; // ADR-214 — wire-edge copy fallback
  private basePt: THREE.Vector3 | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[CopyTool] Activated — 복제할 면을 선택하고 기준점을 클릭하세요');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.basePt) {
      // ═══ Click 1: capture the selected faces (or wire edges) + base point ═══
      const faces = this.ctx.getSelectedFaces();
      const edges = this.ctx.selection.getSelectedEdges();
      if (faces.length === 0 && edges.length === 0) {
        Toast.info('복제할 면 또는 엣지를 먼저 선택하세요', 2000);
        return;
      }
      if (!point) return;
      if (faces.length > 0) this.faces = faces.slice();
      else this.edges = edges.slice();
      this.basePt = point.clone();
      Toast.info('도착점을 클릭하세요 (Esc: 취소)', 2500);
    } else {
      // ═══ Click 2: offset = target − base → 1 translated copy ═══
      if (!point || (!this.faces && !this.edges)) { this.cleanup(); return; }
      const offset = point.clone().sub(this.basePt);
      this.commit(offset);
    }
  }

  onMouseMove(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.basePt || !point) return;
    const d = point.clone().sub(this.basePt);
    this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
      { from: this.basePt.clone(), to: point.clone(),
        text: '복제 ' + this.ctx.units.format(d.length()), color: '#63e6be' },
    ]);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  /** VCB: distance along the locked/inferred axis (default X). */
  applyVCBValue(value: number): void {
    if (value === 0) return;
    const faces = this.faces ?? this.ctx.getSelectedFaces();
    const edges = this.edges ?? this.ctx.selection.getSelectedEdges();
    if ((!faces || faces.length === 0) && (!edges || edges.length === 0)) {
      Toast.info('복제할 면 또는 엣지를 먼저 선택하세요', 2000);
      return;
    }
    if (faces && faces.length > 0) this.faces = faces.slice();
    else this.edges = edges.slice();
    const axis = this.ctx.axisLock || this.ctx.inferredAxis;
    const offset = new THREE.Vector3(
      axis === 'y' || axis === 'z' ? 0 : value,
      axis === 'y' ? value : 0,
      axis === 'z' ? value : 0,
    );
    this.commit(offset);
  }

  isBusy(): boolean {
    return this.basePt !== null;
  }

  cleanup(): void {
    this.faces = null;
    this.edges = null;
    this.basePt = null;
    this.ctx.dimLabel.clear();
  }

  private commit(offset: THREE.Vector3): void {
    const hasFaces = !!this.faces && this.faces.length > 0;
    const hasEdges = !!this.edges && this.edges.length > 0;
    if (!hasFaces && !hasEdges) { this.cleanup(); return; }
    if (offset.length() < MIN_OFFSET) { this.cleanup(); return; }
    const off: [number, number, number] = [offset.x, offset.y, offset.z];
    const out = hasFaces
      ? this.ctx.bridge.arrayLinearFaces(this.faces!, 1, off)
      : this.ctx.bridge.arrayLinearEdges(this.edges!, 1, off);
    if (out.length > 0) {
      this.ctx.syncMesh();
      const kind = hasFaces ? '면' : '엣지';
      const n = hasFaces ? this.faces!.length : this.edges!.length;
      Toast.info(`복제 완료 (${n}개 ${kind})`, 2000);
      debugLog(`[Copy] ${n} ${hasFaces ? 'faces' : 'edges'} duplicated by (${offset.x.toFixed(1)},${offset.y.toFixed(1)},${offset.z.toFixed(1)})`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '복제 실패');
    }
    this.cleanup();
  }
}
