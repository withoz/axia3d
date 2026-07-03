/**
 * Draw Plane Tool — define the active work plane from 3 points.
 * (24-tool toolbar — 3-Point Plane, 2026-06-08)
 *
 * Flow:
 *   click P0, P1, P2 → lock the plane through them as the active drawing plane
 *   (ADR-166 strong lock). Subsequent draws project onto it (across tools)
 *   until you unlock (Ctrl+Shift+P) or change view.
 *   Escape → cancel
 *
 * The three clicks snap to existing geometry (vertices / edges / faces) or
 * fall back to the ground, so picking three box corners at different heights
 * yields a slanted work plane. Pure TS (plane state only) — no engine geometry.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const MIN_SEP_MM = 0.5;

export class DrawPlaneTool implements ITool {
  readonly name = 'plane';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawPlaneTool] Activated — click 3 points to set the work plane');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    const raw = this.ctx.get3DPoint(e);
    const p = this.ctx.getSnappedPoint(e, raw) ?? raw ?? point;
    if (!p) return;
    // reject a point coincident with one already placed
    if (this.points.some((q) => q.distanceTo(p) < MIN_SEP_MM)) return;
    this.points.push(p.clone());
    this.ctx.snap.setReferencePoint(p);
    if (this.points.length === 3) {
      this.commit();
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.points.length === 0) return;
    const raw = this.ctx.get3DPoint(e);
    const p = this.ctx.getSnappedPoint(e, raw) ?? raw;
    if (!p) return;
    this.updatePreview(p);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(_value: number): void {
    // Plane is defined purely by clicks (no numeric entry).
  }

  isBusy(): boolean {
    return this.points.length > 0;
  }

  cleanup(): void {
    this.points = [];
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Commit — compute + lock the work plane
  // ═══════════════════════════════════════════════════

  private commit(): void {
    if (this.points.length !== 3) return;
    const [a, b, c] = this.points;
    const u = b.clone().sub(a);
    const w = c.clone().sub(a);
    const normal = u.clone().cross(w);
    if (normal.lengthSq() < 1e-9 || u.lengthSq() < 1e-9) {
      Toast.warning('세 점이 일직선이거나 너무 가깝습니다 — 평면을 만들 수 없어요', 3000);
      return;
    }
    normal.normalize();
    const up = u.clone().normalize(); // in-plane reference (perpendicular to normal)
    this.ctx.lockPlane?.({ origin: a.clone(), normal, up, source: 'manual' });
    Toast.info('작업 평면 설정 완료 — 이후 그리기가 이 평면에 투영됩니다 (Home 키로 해제)', 4000);
    debugLog(`[Plane] normal=(${normal.x.toFixed(2)}, ${normal.y.toFixed(2)}, ${normal.z.toFixed(2)})`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview — the forming triangle through the picked points
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    const pts = [...this.points, cursor];
    if (this.points.length === 2) pts.push(this.points[0]); // close the triangle
    const geo = new THREE.BufferGeometry().setFromPoints(pts);
    const mat = new THREE.LineBasicMaterial({ color: 0x9b59ff, linewidth: 2 });
    this.preview = new THREE.Line(geo, mat);
    this.preview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.preview);
  }

  private removePreview(): void {
    if (this.preview) {
      this.ctx.viewport.scene.remove(this.preview);
      (this.preview.geometry as THREE.BufferGeometry).dispose();
      (this.preview.material as THREE.Material).dispose();
      this.preview = null;
    }
  }
}
