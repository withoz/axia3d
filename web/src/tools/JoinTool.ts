/**
 * Join Tool — collinear merge of 2D sketch edges (ADR-213).
 *
 * Click a valence-2 vertex where two collinear straight edges meet → the engine
 * merges them into a single edge (`joinCollinearAt`, the inverse of split),
 * dissolving the shared vertex. Cleans up the fragments left by Trim / auto-split.
 *
 * Instant op — no VCB value. Engine + WASM (joinCollinearAt) are reused
 * (ADR-211 edit_2d) — no new geometry kernel.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const PICK_TOL = 2.0; // mm — snapped point should sit on the shared vertex

export class JoinTool implements ITool {
  readonly name = 'join';

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[JoinTool] Activated — 병합할 일직선 꼭짓점(2-valence)을 클릭하세요');
    Toast.info('병합할 일직선 꼭짓점을 클릭하세요 (두 직선 → 하나)', 2800);
  }

  onDeactivate(): void {
    // stateless
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint(e, raw) ?? raw;
    if (!pt) return;
    const vid = this.ctx.bridge.findVertexIdAt?.(pt.x, pt.y, pt.z, PICK_TOL) ?? -1;
    if (vid < 0) {
      Toast.warning('병합할 꼭짓점 위를 클릭하세요', 2000);
      return;
    }
    const merged = this.ctx.bridge.joinCollinearAt?.(vid) ?? -1;
    if (merged >= 0) {
      this.ctx.syncMesh();
      Toast.info('선 병합 완료', 1500);
      debugLog(`[Join] vertex ${vid} → merged edge ${merged}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '병합 실패 (일직선 2-valence 꼭짓점만 가능)');
    }
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
