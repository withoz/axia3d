/**
 * Array Linear Tool — interactive linear array of the selected faces (ADR-209).
 * The one-shot array-linear action still exists; this tool adds a 2-click mode
 * (base → spacing) with a VCB-settable copy count.
 *
 * Flow:
 *   select faces → click 1 base → (optional VCB count, default 3) → click 2 target
 *   → arrayLinearFaces(faces, count, spacing = target − base) → `count` copies.
 *
 * Engine + WASM + bridge (arrayLinearFaces) already exist → UI-only (Pattern-12).
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const MIN_SPACING = 0.1; // mm

export class ArrayLinearTool implements ITool {
  readonly name = 'array-linear';

  private ctx: ToolContext;
  private faces: number[] | null = null;
  private edges: number[] | null = null; // ADR-214 — wire-edge array fallback
  private basePt: THREE.Vector3 | null = null;
  private count = 3;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[ArrayLinearTool] Activated — 면 선택 후 기준점/방향 클릭 (VCB=개수)');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.basePt) {
      const faces = this.ctx.getSelectedFaces();
      const edges = this.ctx.selection.getSelectedEdges();
      if (faces.length === 0 && edges.length === 0) {
        Toast.info('배열할 면 또는 엣지를 먼저 선택하세요', 2000);
        return;
      }
      if (!point) return;
      // Faces take precedence; otherwise array the selected wire edges (ADR-214).
      if (faces.length > 0) this.faces = faces.slice();
      else this.edges = edges.slice();
      this.basePt = point.clone();
      Toast.info(`방향/간격 점을 클릭하세요 (개수 ${this.count} · VCB로 변경 · Esc 취소)`, 2500);
    } else {
      if (!point || (!this.faces && !this.edges)) { this.cleanup(); return; }
      this.commit(point.clone().sub(this.basePt));
    }
  }

  onMouseMove(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.basePt || !point) return;
    const d = point.clone().sub(this.basePt);
    this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
      { from: this.basePt.clone(), to: point.clone(),
        text: `간격 ${this.ctx.units.format(d.length())} × ${this.count}`, color: '#74c0fc' },
    ]);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  /** VCB: while picking → set the copy count; after base+selection → also re-usable. */
  applyVCBValue(value: number): void {
    const n = Math.round(value);
    if (n >= 1) {
      this.count = n;
      debugLog(`[ArrayLinear] count = ${this.count}`);
    }
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

  private commit(spacing: THREE.Vector3): void {
    const hasFaces = !!this.faces && this.faces.length > 0;
    const hasEdges = !!this.edges && this.edges.length > 0;
    if (!hasFaces && !hasEdges) { this.cleanup(); return; }
    if (spacing.length() < MIN_SPACING) { this.cleanup(); return; }
    const off: [number, number, number] = [spacing.x, spacing.y, spacing.z];
    const out = hasFaces
      ? this.ctx.bridge.arrayLinearFaces(this.faces!, this.count, off)
      : this.ctx.bridge.arrayLinearEdges(this.edges!, this.count, off);
    if (out.length > 0) {
      this.ctx.syncMesh();
      Toast.info(`선형 배열 완료 (${this.count}개)`, 2000);
      debugLog(`[ArrayLinear] ${hasFaces ? this.faces!.length + ' faces' : this.edges!.length + ' edges'} × ${this.count}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '선형 배열 실패');
    }
    this.cleanup();
  }
}
