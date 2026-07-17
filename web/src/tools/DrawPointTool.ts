/**
 * Draw Point Tool — place standalone construction Points (ADR-219).
 *
 * Each click places one Point: a Form-citizen Shape owning a single isolated
 * mesh vertex (pinned in the engine so it survives every cleanup pass). The
 * tool is "continuous" — click after click drops independent points, with no
 * multi-click state; Esc / a tool switch ends the session (handled by
 * ToolManager). Uses the existing endpoint/vertex snap (Q4) via get3DPoint +
 * getSnappedPoint, so a click near existing geometry snaps onto it.
 *
 * Pattern-12: reuses drawPointAsShape (engine create + pin) — no new kernel.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { t } from '../i18n';

export class DrawPointTool implements ITool {
  readonly name = 'point';
  readonly wantsSnap = true;

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawPointTool] Activated');
    Toast.info(t('점: 클릭하여 작도 점 배치 (연속, Esc 종료)'), 3500);
  }

  onDeactivate(): void {
    // stateless — nothing to clean up
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint(e, raw) ?? raw ?? point;
    if (!pt) {
      Toast.warning(t('점을 배치할 위치를 클릭하세요'), 1800);
      return;
    }
    const id = this.ctx.bridge.drawPointAsShape(pt.x, pt.y, pt.z);
    if (id >= 0) {
      this.ctx.syncMesh();
      debugLog(`[DrawPoint] shape ${id} at (${pt.x.toFixed(2)}, ${pt.y.toFixed(2)}, ${pt.z.toFixed(2)})`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '점 생성 실패');
    }
  }

  onKeyDown(_e: KeyboardEvent): void {
    // Esc handled by ToolManager (switches to select)
  }

  isBusy(): boolean {
    // Each click is independent — no in-progress multi-click state.
    return false;
  }

  cleanup(): void {
    // stateless
  }
}
