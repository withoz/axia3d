/**
 * Radial Dimension Tool — create a persistent, editable RADIAL dimension
 * (ADR-217). Click a Circle/Arc edge → a driving Radius constraint is created
 * at the current radius. The dimension persists (snapshot), shows an editable
 * "R…" label (DimensionManager), and DRIVES geometry when edited (the solver
 * resizes the circle/arc, keeping its center fixed).
 *
 * Reuses the Radius constraint (engine) + addRadiusConstraint + DimensionLabel
 * straight line — Pattern-12 over set_curve_radius.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { pickClickedEdge } from './edgePick';
import { t } from '../i18n';

export class RadialDimensionTool implements ITool {
  readonly name = 'radial-dimension';
  readonly wantsSnap = false;

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[RadialDimensionTool] Activated');
    Toast.info(t('반지름 치수: 원 또는 호 엣지를 클릭하세요 (영구·편집)'), 3500);
  }

  onDeactivate(): void {
    // stateless
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) {
      Toast.warning(t('반지름을 잴 원/호 엣지를 클릭하세요'), 2000);
      return;
    }
    const radius = this.ctx.bridge.edgeCurveRadius(picked.edgeId);
    if (!(radius > 0)) {
      Toast.warning(t('원 또는 호 엣지가 아닙니다'), 2200);
      return;
    }
    const verts = this.ctx.bridge.getEdgeEndpoints(picked.edgeId);
    if (verts.length < 1) return;
    const refVert = verts[0]; // circle anchor (self-loop) or arc endpoint

    const id = this.ctx.bridge.addRadiusConstraint(refVert, radius);
    if (id > 0) {
      this.ctx.syncMesh();
      Toast.info(t('반지름 치수 (R{radius}) — 라벨 클릭으로 편집', { radius: this.ctx.units.format(radius) }), 2500);
      debugLog(`[RadialDimension] constraint ${id}: R${radius}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '반지름 치수 생성 실패');
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
