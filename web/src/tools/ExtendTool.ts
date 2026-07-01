/**
 * Extend Tool — 2D sketch editing (ADR-211). AutoCAD EXTEND model:
 *   1. select the boundary edge(s) to extend TO (via SelectTool),
 *   2. switch to Extend, click a wire edge to lengthen.
 *
 * The clicked edge = target; the engine (`extendEdge`) moves the target's
 * nearest endpoint out to meet the boundary's supporting line. Engine + WASM +
 * bridge already exist (Pattern-12) — this is the UI layer. Free wire edges only.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { pickClickedEdge } from './edgePick';

export class ExtendTool implements ITool {
  readonly name = 'extend';
  readonly wantsSnap = false;

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    Toast.info('늘일 기준(경계) 엣지를 먼저 선택한 뒤, 늘일 엣지를 클릭하세요 (Esc 종료)', 3500);
    debugLog('[ExtendTool] Activated');
  }

  onDeactivate(): void {
    // stateless — boundary selection persists for repeated extends
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    const boundaries = this.ctx.selection.getSelectedEdges();
    if (boundaries.length === 0) {
      Toast.warning('늘일 기준이 될 경계 엣지를 먼저 선택하세요', 2200);
      return;
    }

    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) {
      Toast.warning('늘일 엣지를 클릭하세요', 1800);
      return;
    }
    const { edgeId: target } = picked;
    if (boundaries.includes(target)) {
      Toast.warning('경계 엣지 자신은 늘일 수 없습니다', 2000);
      return;
    }

    // Extend to the first selected boundary that the target's line can reach.
    for (const boundary of boundaries) {
      const r = this.ctx.bridge.extendEdge(target, boundary);
      if (r >= 0) {
        this.ctx.syncMesh();
        Toast.info('엣지 늘이기 완료', 1600);
        debugLog(`[Extend] target=${target} boundary=${boundary}`);
        return;
      }
    }
    Toast.fromBridgeError(this.ctx.bridge, '늘이기 실패 (경계에 닿지 않거나 자유 와이어 엣지가 아님)');
  }

  onKeyDown(_e: KeyboardEvent): void {
    // Esc handled by ToolManager
  }

  isBusy(): boolean {
    return false;
  }

  cleanup(): void {
    // stateless
  }
}
