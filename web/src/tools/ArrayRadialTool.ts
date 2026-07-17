/**
 * Array Radial Tool — interactive radial (circular) array of the selected faces
 * around a world axis (ADR-209). The one-shot array-radial action still exists.
 *
 * Flow:
 *   select faces → enter the tool → X / Y / Z chooses the rotation axis (default Z)
 *   → (VCB sets the copy count, default 6) → click or Enter commits a full-circle
 *   radial array (arrayRadialFaces, total angle 2π) around the world origin → repeat.
 *
 * Engine + WASM + bridge (arrayRadialFaces) already exist → UI-only (Pattern-12).
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { t } from '../i18n';

type Axis = 'x' | 'y' | 'z';

export class ArrayRadialTool implements ITool {
  readonly name = 'array-radial';

  private ctx: ToolContext;
  private axis: Axis = 'z';
  private count = 6;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    Toast.info(t('회전축 X/Y/Z 선택 + VCB=개수 → 클릭(또는 Enter)으로 원형 배열, Esc 종료'), 3500);
    debugLog('[ArrayRadialTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    this.commit();
  }

  onMouseMove(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    // axis chosen by keys; no per-move work
  }

  onKeyDown(e: KeyboardEvent): void {
    const k = e.key.toLowerCase();
    if (k === 'escape') { this.cleanup(); return; }
    if (k === 'enter') { this.commit(); return; }
    if (k === 'x' || k === 'y' || k === 'z') { this.axis = k; }
  }

  applyVCBValue(value: number): void {
    const n = Math.round(value);
    if (n >= 2) {
      this.count = n;
      debugLog(`[ArrayRadial] count = ${this.count}`);
    }
  }

  isBusy(): boolean {
    return false;
  }

  cleanup(): void {
    // stateless
  }

  private commit(): void {
    const faces = this.ctx.getSelectedFaces();
    const edges = this.ctx.selection.getSelectedEdges();
    if (faces.length === 0 && edges.length === 0) {
      Toast.warning(t('배열할 면 또는 엣지를 먼저 선택하세요'), 2000);
      return;
    }
    const axisVec: [number, number, number] =
      this.axis === 'x' ? [1, 0, 0] : this.axis === 'y' ? [0, 1, 0] : [0, 0, 1];
    // Faces take precedence; otherwise array the selected wire edges (ADR-214).
    const out = faces.length > 0
      ? this.ctx.bridge.arrayRadialFaces(faces, this.count, [0, 0, 0], axisVec, Math.PI * 2)
      : this.ctx.bridge.arrayRadialEdges(edges, this.count, [0, 0, 0], axisVec, Math.PI * 2);
    if (out.length > 0) {
      this.ctx.syncMesh();
      const kind = faces.length > 0 ? '면' : '엣지';
      const n = faces.length > 0 ? faces.length : edges.length;
      Toast.info(t('원형 배열 완료 ({count}개 · {axis}축)', { count: this.count, axis: this.axis.toUpperCase() }), 2000);
      debugLog(`[ArrayRadial] ${n} ${faces.length > 0 ? 'faces' : 'edges'} × ${this.count} around ${this.axis}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '원형 배열 실패');
    }
  }
}
