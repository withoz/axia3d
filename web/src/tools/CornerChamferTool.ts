/**
 * Corner Chamfer Tool — cut a 2D sketch corner with a straight line (ADR-212).
 *
 * Flow: click a valence-2 corner vertex → resolve the VertId (findVertexIdAt) →
 * type a distance in the VCB (or click again to reuse the last) → commit.
 *
 * Distinct from the 3D vertex 3-way chamfer (`tool-chamfer`, ADR-207). Engine +
 * WASM (chamferCorner2d) are reused — no new geometry kernel (ADR-211 edit_2d).
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const LS_KEY = 'axia:corner-chamfer:dist';
const PICK_TOL = 2.0; // mm — snapped point should sit on the corner vertex

export class CornerChamferTool implements ITool {
  readonly name = 'corner-chamfer';

  private ctx: ToolContext;
  private vertId = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[CornerChamferTool] Activated — 모따기할 코너(2-valence 꼭짓점)를 클릭하세요');
    Toast.info('모따기할 코너 꼭짓점을 클릭하세요', 2500);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.vertId < 0) {
      const raw = this.ctx.get3DPoint(e);
      const pt = this.ctx.getSnappedPoint(e, raw) ?? raw;
      if (!pt) return;
      const vid = this.ctx.bridge.findVertexIdAt?.(pt.x, pt.y, pt.z, PICK_TOL) ?? -1;
      if (vid < 0) {
        Toast.warning('모따기할 코너 꼭짓점 위를 클릭하세요', 2000);
        return;
      }
      this.vertId = vid;
      Toast.info('거리를 입력하세요 (또는 다시 클릭 = 마지막 값)', 2500);
    } else {
      this.commit(this.lastDist());
    }
  }

  onMouseMove(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    // no preview (instant topological op)
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(value: number): void {
    if (this.vertId < 0 || value <= 0) return;
    this.commit(value);
  }

  isBusy(): boolean {
    return this.vertId >= 0;
  }

  cleanup(): void {
    this.vertId = -1;
  }

  private lastDist(): number {
    const v = Number(localStorage.getItem(LS_KEY) ?? '3');
    return Number.isFinite(v) && v > 0 ? v : 3;
  }

  private commit(dist: number): void {
    if (this.vertId < 0) { this.cleanup(); return; }
    const e = this.ctx.bridge.chamferCorner2d?.(this.vertId, dist) ?? -1;
    if (e >= 0) {
      try { localStorage.setItem(LS_KEY, String(dist)); } catch { /* ignore */ }
      this.ctx.syncMesh();
      Toast.info(`코너 모따기 완료 (거리 ${dist}mm)`, 2000);
      debugLog(`[CornerChamfer] vertex ${this.vertId} dist=${dist} → edge ${e}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '모따기 실패 (2-valence 코너만 가능 · 거리 확인)');
    }
    this.cleanup();
  }
}
