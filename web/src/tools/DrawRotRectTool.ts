/**
 * Draw Rotated Rectangle Tool — 3-click arbitrary-angle rectangle
 * (ADR-186 toolbar Phase 3, 2026-06-05).
 *
 * Flow:
 *   1st click → P0 (첫 코너) + plane detect + lock
 *   2nd click → P1 (첫 변의 끝 — 너비 방향 + 길이)
 *   3rd click → P2 (수직 방향 범위 — 높이) → commit
 *   Escape → cancel
 *
 * Reuses the existing `drawRectAsShape(center, normal, up, w, h)` engine
 * API: the rectangle is rotated by passing a NON-cardinal `up` vector
 * (the perpendicular direction toward P2). The bridge cardinal snap only
 * touches the center, so the rotation is preserved. No new engine work.
 *
 * Mirrors DrawRectTool (plane lock ADR-166, sticky plane ADR-164).
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

const MIN_DIM_MM = 0.1;

export class DrawRotRectTool implements ITool {
  readonly name = 'rotrect';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawRotRectTool] Activated — P0 → P1 (edge) → P2 (height)');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.points.length === 0) {
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(this.plane.normal, point);
      this.points.push(point.clone());
      this.ctx.snap.setReferencePoint(point);
      this.ctx.lockPlane?.({
        origin: point, normal: this.plane.normal, up: this.plane.up, source: 'first_click',
      });
      return;
    }

    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    this.points.push(p.clone());
    this.ctx.snap.setReferencePoint(p);

    if (this.points.length === 3) {
      this.commit();
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.points.length === 0 || !this.plane) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    this.updatePreview(p);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(_value: number): void {
    // RotRect는 자유 클릭 (VCB 미사용)
  }

  isBusy(): boolean {
    return this.points.length > 0;
  }

  cleanup(): void {
    this.points = [];
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Rect geometry: P0,P1,P2 → center / up / w / h
  // ═══════════════════════════════════════════════════

  /** Returns {center, up, w, h, corners} or null if degenerate. */
  private rectFrom(p0: THREE.Vector3, p1: THREE.Vector3, p2: THREE.Vector3) {
    const rightVec = p1.clone().sub(p0);
    const w = rightVec.length();
    if (w < MIN_DIM_MM) return null;
    const rightDir = rightVec.clone().normalize();
    const d = p2.clone().sub(p0);
    const along = d.dot(rightDir);
    const perpVec = d.clone().addScaledVector(rightDir, -along); // d - along*rightDir
    const h = perpVec.length();
    if (h < MIN_DIM_MM) return null;
    const perpDir = perpVec.clone().normalize();
    const center = p0.clone()
      .addScaledVector(rightDir, w / 2)
      .addScaledVector(perpDir, h / 2);
    const corners = [
      p0.clone(),
      p0.clone().addScaledVector(rightDir, w),
      p0.clone().addScaledVector(rightDir, w).addScaledVector(perpDir, h),
      p0.clone().addScaledVector(perpDir, h),
    ];
    return { center, up: perpDir, w, h, corners };
  }

  private commit(): void {
    if (this.points.length !== 3 || !this.plane) return;
    const r = this.rectFrom(this.points[0], this.points[1], this.points[2]);
    if (!r) {
      debugLog('[RotRect] degenerate (w or h < min) — skipped');
      return;
    }
    const n = this.plane.normal;
    const shapeRaw = this.ctx.bridge.drawRectAsShape(
      r.center.x, r.center.y, r.center.z,
      n.x, n.y, n.z,
      r.up.x, r.up.y, r.up.z,
      r.w, r.h,
    );
    if (typeof shapeRaw === 'number' && shapeRaw >= 0) {
      this.ctx.setLastDrawnPlane?.({
        origin: r.center,
        normal: n,
        up: this.plane.up,
        source: 'view',
      });
    }
    this.ctx.syncMesh();
    debugLog(`[RotRect] ${r.w.toFixed(2)} × ${r.h.toFixed(2)} (rotated up) → drawRectAsShape (ok=${shapeRaw})`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    let pts: THREE.Vector3[];
    if (this.points.length === 1) {
      // P0 fixed, cursor = P1 candidate → show the first edge.
      pts = [this.points[0], cursor];
    } else {
      // P0, P1 fixed, cursor = P2 candidate → show the rotated rect outline.
      const r = this.rectFrom(this.points[0], this.points[1], cursor);
      if (!r) {
        pts = [this.points[0], this.points[1]];
      } else {
        pts = [...r.corners, r.corners[0]]; // closed loop
      }
    }
    const geo = new THREE.BufferGeometry().setFromPoints(pts);
    const mat = new THREE.LineBasicMaterial({ color: 0x4dabf7, linewidth: 2 });
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

  // ═══════════════════════════════════════════════════
  //  Plane ray intersection (mirror DrawBezier/Spline)
  // ═══════════════════════════════════════════════════

  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3) return null;
    const rawPt = this.ctx.get3DPoint(e);
    const snapped = this.ctx.getSnappedPoint(e, rawPt);
    if (snapped) {
      const projected = snapped.clone();
      const dist = this.drawPlane3.distanceToPoint(projected);
      projected.addScaledVector(this.drawPlane3.normal, -dist);
      return projected;
    }
    const ray = this.ctx.getRay(e);
    const target = new THREE.Vector3();
    const hit = ray.ray.intersectPlane(this.drawPlane3, target);
    return hit ?? null;
  }
}
