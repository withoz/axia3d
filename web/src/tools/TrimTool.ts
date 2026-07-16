/**
 * Trim Tool — 2D sketch editing (ADR-211).
 *
 * In AxiA the engine auto-splits crossing wire lines at their intersections
 * (ADR-172 "선만 그려, 케이크는 알아서 나뉜다"), so every line is already
 * segmented at its crossing points. TRIM = click the segment to remove; the
 * engine deletes just that segment (`deleteEdgeCascade`), cutting the line back
 * to the nearest intersection. No cutting-edge selection needed.
 *
 * (The "split-at-boundary-then-remove" approach is redundant here — the split
 * already happened on draw — so this tool reuses the existing segment delete.)
 */

import * as THREE from 'three';
import { t } from '../i18n';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { pickClickedEdge } from './edgePick';

export class TrimTool implements ITool {
  readonly name = 'trim';
  readonly wantsSnap = false;

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    Toast.info(t('잘라낼 선 구간을 클릭하세요 (교차점 사이가 한 구간 · Esc 종료)'), 3500);
    debugLog('[TrimTool] Activated');
  }

  onDeactivate(): void {
    // stateless
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) {
      Toast.warning(t('잘라낼 선 구간을 클릭하세요'), 1800);
      return;
    }
    const r = this.ctx.bridge.deleteEdgeCascade(picked.edgeId);
    if (r >= 0) {
      this.ctx.syncMesh();
      Toast.info(t('선 구간 자르기 완료'), 1500);
      debugLog(`[Trim] deleted segment edge=${picked.edgeId}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, t('자르기 실패 (자유 와이어 구간이 아님)'));
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
