/**
 * Draw Pie / Sector Tool — circular sector (부채꼴) from center + radius +
 * angular sweep (ADR-186 toolbar Phase 4, 2026-06-05).
 *
 * Flow:
 *   1st click → P0 (중심) + plane detect + lock
 *   2nd click → P1 (반지름 + 시작각: radius = |P1-P0|, start dir = P1-P0)
 *   3rd click → P2 (끝각: CCW sweep toward P2) → commit
 *   Escape → cancel
 *
 * Boundary = [center, arc samples (start→end CCW), center] → closed loop
 * via `drawPolylineAsShape` → sector face (Plane attached). The arc is
 * polygon-approximated (~64 segments / full turn); an analytic Arc-edge
 * refinement is a future step.
 *
 * Basis: u = normalize(P1-C) (so start angle = 0), v = N × u (CCW), so the
 * sweep angle increases counter-clockwise toward P2.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

const MIN_RADIUS_MM = 0.1;
const MIN_SPAN_RAD = 1e-3;
const SEGMENTS_PER_TURN = 64;
const TWO_PI = Math.PI * 2;

export class DrawPieTool implements ITool {
  readonly name = 'pie';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawPieTool] Activated — center → radius/start → end angle');
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
    // Pie는 자유 클릭 (VCB 미사용)
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
  //  Sector boundary
  // ═══════════════════════════════════════════════════

  /** Build the sector boundary points (center, arc start→end CCW, center).
   *  Returns null if degenerate (radius or span too small). */
  private sectorBoundary(c: THREE.Vector3, p1: THREE.Vector3, p2: THREE.Vector3): THREE.Vector3[] | null {
    if (!this.plane) return null;
    const n = this.plane.normal.clone().normalize();
    const radVec = p1.clone().sub(c);
    const radius = radVec.length();
    if (radius < MIN_RADIUS_MM) return null;
    const u = radVec.clone().normalize();                 // start direction (angle 0)
    const v = n.clone().cross(u).normalize();             // CCW perpendicular in plane
    // end angle from P2 (projected to the u/v basis).
    const d2 = p2.clone().sub(c);
    let span = Math.atan2(d2.dot(v), d2.dot(u));
    if (span < 0) span += TWO_PI;                          // CCW span in [0, 2π)
    if (span < MIN_SPAN_RAD) return null;

    const segs = Math.max(2, Math.ceil((span / TWO_PI) * SEGMENTS_PER_TURN));
    const pts: THREE.Vector3[] = [c.clone()];
    for (let i = 0; i <= segs; i++) {
      const a = span * (i / segs);
      const cosA = Math.cos(a), sinA = Math.sin(a);
      pts.push(new THREE.Vector3(
        c.x + radius * (cosA * u.x + sinA * v.x),
        c.y + radius * (cosA * u.y + sinA * v.y),
        c.z + radius * (cosA * u.z + sinA * v.z),
      ));
    }
    pts.push(c.clone()); // close back to center
    return pts;
  }

  private commit(): void {
    if (this.points.length !== 3 || !this.plane) return;
    const boundary = this.sectorBoundary(this.points[0], this.points[1], this.points[2]);
    if (!boundary) {
      debugLog('[Pie] degenerate (radius or span too small) — skipped');
      return;
    }
    const flat = new Float64Array(boundary.length * 3);
    for (let i = 0; i < boundary.length; i++) {
      flat[i * 3]     = boundary[i].x;
      flat[i * 3 + 1] = boundary[i].y;
      flat[i * 3 + 2] = boundary[i].z;
    }
    const n = this.plane.normal;
    const ok = this.ctx.bridge.drawPolylineAsShape(flat, { x: n.x, y: n.y, z: n.z });
    if (typeof ok !== 'number' || ok >= 0) {
      this.ctx.setLastDrawnPlane?.({
        origin: this.points[0], normal: n, up: this.plane.up, source: 'view',
      });
    }
    this.ctx.syncMesh();
    debugLog(`[Pie] sector (${boundary.length - 2} arc pts) → drawPolylineAsShape (ok=${ok})`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    let pts: THREE.Vector3[];
    if (this.points.length === 1) {
      // center fixed, cursor = radius/start → show the radius line.
      pts = [this.points[0], cursor];
    } else {
      // center + start fixed, cursor = end angle → show the sector outline.
      const boundary = this.sectorBoundary(this.points[0], this.points[1], cursor);
      pts = boundary ?? [this.points[0], this.points[1]];
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
  //  Plane ray intersection (mirror DrawBezier/Spline/RotRect)
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
