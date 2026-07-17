/**
 * Chamfer Tool — flat triangular corner cut at a valence-3 vertex (ADR-207 / ADR-024 P10).
 *
 * Flow:
 *   click a corner vertex → resolve the VertId (findVertexIdAt at the snapped point)
 *   → type a radius in the VCB (or click again to reuse the last radius) → commit.
 *
 * Edge chamfer is already available via the `chamfer-edge` action (filletEdge with
 * segments=1); this tool covers the *vertex* 3-way corner cut. Engine + WASM
 * (chamferVertex3way) are reused — no new geometry kernel work.
 */

import * as THREE from 'three';
import { t } from '../i18n';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const LS_KEY = 'axia:chamfer:vertex-radius';
const PICK_TOL = 2.0; // mm — snapped point should sit on the corner vertex

export class ChamferTool implements ITool {
  readonly name = 'chamfer';

  private ctx: ToolContext;
  private vertId = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[ChamferTool] Activated — 챔퍼할 꼭짓점(3-valence)을 클릭하세요');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.vertId < 0) {
      // ═══ Select the corner vertex ═══
      const raw = this.ctx.get3DPoint(e);
      const pt = this.ctx.getSnappedPoint(e, raw) ?? raw;
      if (!pt) return;
      const vid = this.ctx.bridge.findVertexIdAt?.(pt.x, pt.y, pt.z, PICK_TOL) ?? -1;
      if (vid < 0) {
        Toast.warning(t('챔퍼할 꼭짓점 위를 클릭하세요'), 2000);
        return;
      }
      this.vertId = vid;
      Toast.info(t('반지름을 입력하세요 (또는 다시 클릭 = 마지막 값)'), 2500);
    } else {
      // ═══ Second click → commit with the last radius ═══
      this.commit(this.lastRadius());
    }
  }

  onMouseMove(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    // no preview (vertex chamfer is an instant topological op)
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

  private lastRadius(): number {
    const v = Number(localStorage.getItem(LS_KEY) ?? '2');
    return Number.isFinite(v) && v > 0 ? v : 2;
  }

  private commit(radius: number): void {
    if (this.vertId < 0) { this.cleanup(); return; }
    const n = this.ctx.bridge.chamferVertex3way?.(this.vertId, radius) ?? -1;
    if (n >= 0) {
      try { localStorage.setItem(LS_KEY, String(radius)); } catch { /* ignore */ }
      this.ctx.syncMesh();
      Toast.info(t('꼭짓점 챔퍼 완료 (반지름 {radius}mm)', { radius }), 2000);
      debugLog(`[Chamfer] vertex ${this.vertId} radius=${radius} → ${n} faces`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, t('챔퍼 실패 (3-valence 꼭짓점만 가능)'));
    }
    this.cleanup();
  }
}
