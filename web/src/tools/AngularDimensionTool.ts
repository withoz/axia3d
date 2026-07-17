/**
 * Angular Dimension Tool — create a persistent, editable ANGULAR dimension
 * (ADR-216). Click edge 1 → click edge 2 → a driving Angle constraint is created
 * at the current angle between them. The dimension persists (snapshot), shows an
 * editable arc + angle label (DimensionManager), and DRIVES geometry when edited
 * (the solver rotates the driven edge, pivoting on the shared corner vertex).
 *
 * Reuses the Angle constraint (engine) + addAngleConstraint + DimensionLabel arc
 * — Pattern-12 over the generalized edge-pair solver.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { pickClickedEdge } from './edgePick';
import { t } from '../i18n';

function dirOf(p0: [number, number, number], p1: [number, number, number]): [number, number, number] {
  const d: [number, number, number] = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
  const len = Math.hypot(d[0], d[1], d[2]) || 1;
  return [d[0] / len, d[1] / len, d[2] / len];
}

export class AngularDimensionTool implements ITool {
  readonly name = 'angular-dimension';
  readonly wantsSnap = false;

  private ctx: ToolContext;
  private edge1 = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[AngularDimensionTool] Activated');
    Toast.info(t('각도 치수: 첫 엣지 클릭 → 둘째 엣지 클릭 (영구·편집)'), 3500);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) {
      Toast.warning(t('각도를 잴 엣지를 클릭하세요'), 2000);
      return;
    }
    if (this.edge1 < 0) {
      this.edge1 = picked.edgeId;
      Toast.info(t('둘째 엣지를 클릭하세요 (Esc 취소)'), 2500);
      return;
    }
    if (picked.edgeId === this.edge1) {
      Toast.warning(t('서로 다른 엣지를 선택하세요'), 2000);
      return;
    }

    const eA = this.ctx.bridge.getEdgeEndpoints(this.edge1);
    const eB = this.ctx.bridge.getEdgeEndpoints(picked.edgeId);
    if (eA.length < 2 || eB.length < 2) { this.cleanup(); return; }
    const pa0 = this.ctx.bridge.getVertexPos(eA[0]);
    const pa1 = this.ctx.bridge.getVertexPos(eA[1]);
    const pb0 = this.ctx.bridge.getVertexPos(eB[0]);
    const pb1 = this.ctx.bridge.getVertexPos(eB[1]);
    if (!pa0 || !pa1 || !pb0 || !pb1) { this.cleanup(); return; }

    const dA = dirOf(pa0, pa1);
    const dB = dirOf(pb0, pb1);
    const dot = Math.max(-1, Math.min(1, dA[0] * dB[0] + dA[1] * dB[1] + dA[2] * dB[2]));
    const angle = Math.acos(dot); // radians
    if (angle < 1e-3 || angle > Math.PI - 1e-3) {
      Toast.warning(t('평행/일직선 엣지는 각도 치수 불가'), 2200);
      this.cleanup();
      return;
    }

    const id = this.ctx.bridge.addAngleConstraint(eA[0], eA[1], eB[0], eB[1], angle);
    if (id > 0) {
      this.ctx.syncMesh();
      Toast.info(t('각도 치수 ({angle}°) — 라벨 클릭으로 편집', { angle: ((angle * 180) / Math.PI).toFixed(1) }), 2500);
      debugLog(`[AngularDimension] constraint ${id}: ${(angle * 180) / Math.PI}°`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '각도 치수 생성 실패');
    }
    this.cleanup();
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  isBusy(): boolean {
    return this.edge1 >= 0;
  }

  cleanup(): void {
    this.edge1 = -1;
  }
}
