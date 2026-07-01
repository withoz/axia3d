/**
 * Dimension Tool — create a persistent, editable LINEAR dimension (ADR-215).
 *
 * Flow: click vertex 1 → click vertex 2 → a driving Distance constraint is
 * created at the current distance. The dimension persists with the scene
 * (snapshot), shows an editable label (DimensionManager), and DRIVES geometry
 * when edited (the constraint solver moves the second vertex).
 *
 * Distinct from Measure (transient, no mesh edit). Reuses the existing Distance
 * constraint (storage + solve + persist) + addDistanceConstraint — Pattern-12.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const PICK_TOL = 2.0; // mm — snapped point should sit on a vertex

export class DimensionTool implements ITool {
  readonly name = 'dimension';
  readonly wantsSnap = true;

  private ctx: ToolContext;
  private v1 = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DimensionTool] Activated');
    Toast.info('치수: 첫 정점 클릭 → 둘째 정점 클릭 (영구·편집가능 치수)', 3500);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint(e, raw) ?? raw ?? point;
    if (!pt) return;
    const vid = this.ctx.bridge.findVertexIdAt?.(pt.x, pt.y, pt.z, PICK_TOL) ?? -1;
    if (vid < 0) {
      Toast.warning('치수를 잴 정점 위를 클릭하세요', 2000);
      return;
    }
    if (this.v1 < 0) {
      this.v1 = vid;
      Toast.info('둘째 정점을 클릭하세요 (Esc 취소)', 2500);
      return;
    }
    if (vid === this.v1) {
      Toast.warning('서로 다른 정점을 선택하세요', 2000);
      return;
    }

    const pa = this.ctx.bridge.getVertexPos(this.v1);
    const pb = this.ctx.bridge.getVertexPos(vid);
    if (!pa || !pb) { this.cleanup(); return; }
    const dist = Math.hypot(pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]);
    if (dist < 1e-6) {
      Toast.warning('두 정점이 같은 위치입니다', 2000);
      this.cleanup();
      return;
    }

    const id = this.ctx.bridge.addDistanceConstraint(this.v1, vid, dist);
    if (id > 0) {
      this.ctx.syncMesh();
      Toast.info(`치수 생성 (${this.ctx.units.format(dist)}) — 라벨 클릭으로 편집`, 2500);
      debugLog(`[Dimension] constraint ${id}: ${this.v1}↔${vid} = ${dist}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '치수 생성 실패');
    }
    this.cleanup();
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  isBusy(): boolean {
    return this.v1 >= 0;
  }

  cleanup(): void {
    this.v1 = -1;
  }
}
