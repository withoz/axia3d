/**
 * Draw Spline Tool — open B-spline curve from N control points
 * (ADR-186 toolbar Phase 2, 2026-06-05).
 *
 * Flow:
 *   1st click → P0 (시작점) + plane detect + plane lock
 *   N-th click → control point N
 *   Enter / double-click (마지막 점 근처 재클릭) → commit
 *   Escape → cancel
 *
 * Engine: `drawBSplineWithCurve(controlPts, knots, degree)` — clamped
 * uniform knot vector (length = N + degree + 1), degree = min(3, N-1).
 * Falls back to `drawPolylineAsShape` if the kernel rejects the spline.
 *
 * Mirrors DrawBezierTool (plane lock ADR-166, sticky plane ADR-164,
 * preview = control polygon + curve + handles). Preview curve uses a
 * de Boor evaluation that matches the engine B-spline (not interpolating).
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

/** Finish-on-duplicate threshold (mm) — a double-click lands two points at
 *  (near) the same spot; treat that as the commit gesture. */
const SPLINE_DBLCLICK_EPSILON_MM = 0.5;
/** Default cubic degree (clamped to N-1 for few points). */
const SPLINE_MAX_DEGREE = 3;
/** Preview tessellation sample count. */
const SPLINE_PREVIEW_SAMPLES = 48;
/** Committed-curve tessellation sample count. Bounded (NOT the engine's
 *  drawBSplineWithCurve ~4096-edge granularity, which freezes syncMesh). */
const SPLINE_COMMIT_SAMPLES = 96;

export class DrawSplineTool implements ITool {
  readonly name = 'spline';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private previewLine: THREE.Line | null = null;
  private controlHandles: THREE.Object3D[] = [];

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawSplineTool] Activated — click control points, Enter/double-click to finish');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.points.length === 0) {
      // P0 — plane detect + lock.
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        this.plane.normal, point,
      );
      this.points.push(point.clone());
      this.ctx.snap.setReferencePoint(point);
      // ADR-166 — first_click plane lock (idempotent).
      this.ctx.lockPlane?.({
        origin: point,
        normal: this.plane.normal,
        up: this.plane.up,
        source: 'first_click',
      });
      return;
    }

    const p = this.getPointOnDrawPlane(e);
    if (!p) return;

    // Double-click / duplicate-point → finish (≥2 points already placed).
    const last = this.points[this.points.length - 1];
    if (this.points.length >= 2 && p.distanceTo(last) < SPLINE_DBLCLICK_EPSILON_MM) {
      this.commit();
      this.cleanup();
      return;
    }

    this.points.push(p.clone());
    this.ctx.snap.setReferencePoint(p);
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.points.length === 0 || !this.plane) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    this.updatePreview([...this.points, p]);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    } else if (e.key === 'Enter' || e.key === 'Return') {
      // Explicit finish (≥2 control points).
      if (this.points.length >= 2) {
        this.commit();
      }
      this.cleanup();
    }
  }

  applyVCBValue(_value: number): void {
    // Spline은 숫자 입력 없음
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
  //  Knot vector + de Boor (shared by commit + preview)
  // ═══════════════════════════════════════════════════

  /** Clamped uniform knot vector for `n` control points at `degree`.
   *  Length = n + degree + 1 (engine contract). */
  private clampedKnots(n: number, degree: number): number[] {
    const knots: number[] = [];
    for (let i = 0; i <= degree; i++) knots.push(0);
    for (let i = 1; i < n - degree; i++) knots.push(i);
    const last = n - degree;
    for (let i = 0; i <= degree; i++) knots.push(last);
    return knots;
  }

  /** degree = min(SPLINE_MAX_DEGREE, n - 1) — cubic when ≥4 points. */
  private degreeFor(n: number): number {
    return Math.min(SPLINE_MAX_DEGREE, n - 1);
  }

  /** de Boor evaluation at parameter `t` (matches the engine B-spline).
   *  Works in plain xyz components (no Vector3 methods) for portability. */
  private deBoor(t: number, ctrl: THREE.Vector3[], knots: number[], degree: number): THREE.Vector3 {
    const n = ctrl.length;
    // knot span k: knots[k] <= t < knots[k+1]
    let k = degree;
    while (k < n - 1 && t >= knots[k + 1]) k++;
    const dx: number[] = [], dy: number[] = [], dz: number[] = [];
    for (let j = 0; j <= degree; j++) {
      const c = ctrl[j + k - degree];
      dx.push(c.x); dy.push(c.y); dz.push(c.z);
    }
    for (let r = 1; r <= degree; r++) {
      for (let j = degree; j >= r; j--) {
        const denom = knots[j + 1 + k - r] - knots[j + k - degree];
        const alpha = denom > 1e-12 ? (t - knots[j + k - degree]) / denom : 0;
        dx[j] = dx[j - 1] + (dx[j] - dx[j - 1]) * alpha;
        dy[j] = dy[j - 1] + (dy[j] - dy[j - 1]) * alpha;
        dz[j] = dz[j - 1] + (dz[j] - dz[j - 1]) * alpha;
      }
    }
    return new THREE.Vector3(dx[degree], dy[degree], dz[degree]);
  }

  /** Tessellate the open B-spline through `ctrl` into `samples`+1 points. */
  private tessellate(ctrl: THREE.Vector3[], samples = SPLINE_PREVIEW_SAMPLES): THREE.Vector3[] {
    const n = ctrl.length;
    if (n < 2) return ctrl.slice();
    const degree = this.degreeFor(n);
    const knots = this.clampedKnots(n, degree);
    const tMin = knots[degree];
    const tMax = knots[n];
    const out: THREE.Vector3[] = [];
    for (let i = 0; i <= samples; i++) {
      const t = tMin + (tMax - tMin) * (i / samples);
      // clamp the last sample just inside the range for the span search.
      const tc = Math.min(t, tMax - 1e-9);
      out.push(this.deBoor(tc, ctrl, knots, degree));
    }
    return out;
  }

  // ═══════════════════════════════════════════════════
  //  Commit
  // ═══════════════════════════════════════════════════

  private commit(): void {
    const n = this.points.length;
    if (n < 2) return;

    // ADR-201 β-2 — analytic B-spline via `drawBSplineWithCurve`. β-1 이 엔진
    // tessellation 을 64 sub-range 세그먼트로 bound (이전 ~4096 → syncMesh freeze
    // 라 polyline fallback 했음). 이제 analytic B-spline 정체성 보존 + auto-
    // division 참여 (ADR-200 §3.6). 커널이 거부(-1)하면 bounded de Boor polyline
    // 로 graceful fallback.
    const degree = this.degreeFor(n);
    const knots = this.clampedKnots(n, degree);
    const ctrlFlat = new Float64Array(n * 3);
    for (let i = 0; i < n; i++) {
      ctrlFlat[i * 3]     = this.points[i].x;
      ctrlFlat[i * 3 + 1] = this.points[i].y;
      ctrlFlat[i * 3 + 2] = this.points[i].z;
    }
    let ok = -1;
    if (typeof this.ctx.bridge.drawBSplineWithCurve === 'function') {
      ok = this.ctx.bridge.drawBSplineWithCurve(ctrlFlat, new Float64Array(knots), degree);
    }
    if (ok < 0) {
      // Fallback: bounded de Boor polyline (kernel rejected the spline).
      const pts = this.tessellate(this.points, SPLINE_COMMIT_SAMPLES);
      const filtered: THREE.Vector3[] = [];
      for (let i = 0; i < pts.length; i++) {
        if (filtered.length === 0 || pts[i].distanceTo(filtered[filtered.length - 1]) >= 0.1) {
          filtered.push(pts[i]);
        }
      }
      if (filtered.length >= 2) {
        const flat = new Float64Array(filtered.length * 3);
        for (let i = 0; i < filtered.length; i++) {
          flat[i * 3]     = filtered[i].x;
          flat[i * 3 + 1] = filtered[i].y;
          flat[i * 3 + 2] = filtered[i].z;
        }
        this.ctx.bridge.drawPolylineAsShape(flat);
      }
    }

    // ADR-164 — sticky last drawn plane (open spline has no face; keep for
    // subsequent draws sharing the plane).
    if (this.plane) {
      this.ctx.setLastDrawnPlane?.({
        origin: this.points[0],
        normal: this.plane.normal,
        up: this.plane.up,
        source: 'view',
      });
    }
    this.ctx.syncMesh();
    debugLog(`[Spline] ${n} ctrl pts (deg ${degree}) → ${ok >= 0 ? 'analytic B-spline (ADR-201)' : 'polyline fallback'}`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(ctrl: THREE.Vector3[]): void {
    this.removePreview();
    if (ctrl.length < 2) return;

    // 제어 polygon (점선)
    const polygonGeo = new THREE.BufferGeometry().setFromPoints(ctrl);
    const polygonMat = new THREE.LineDashedMaterial({ color: 0x888888, dashSize: 5, gapSize: 3 });
    const polyLine = new THREE.Line(polygonGeo, polygonMat);
    (polyLine as any).computeLineDistances?.();
    polyLine.renderOrder = 998;
    this.ctx.viewport.scene.add(polyLine);
    this.controlHandles.push(polyLine);

    // 곡선 preview (de Boor, ≥2점)
    const curvePts = ctrl.length >= 2 ? this.tessellate(ctrl) : ctrl;
    const geo = new THREE.BufferGeometry().setFromPoints(curvePts);
    const mat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 2 });
    this.previewLine = new THREE.Line(geo, mat);
    this.previewLine.renderOrder = 999;
    this.ctx.viewport.scene.add(this.previewLine);

    // 제어점 핸들 — 작은 점
    for (let i = 0; i < this.points.length; i++) {
      const sphereGeo = new THREE.BufferGeometry();
      sphereGeo.setFromPoints([this.points[i]]);
      const sphereMat = new THREE.PointsMaterial({ color: 0xffaa00, size: 6 });
      const pts = new THREE.Points(sphereGeo, sphereMat);
      pts.renderOrder = 1000;
      this.ctx.viewport.scene.add(pts);
      this.controlHandles.push(pts);
    }
  }

  private removePreview(): void {
    if (this.previewLine) {
      this.ctx.viewport.scene.remove(this.previewLine);
      (this.previewLine.geometry as THREE.BufferGeometry).dispose();
      (this.previewLine.material as THREE.Material).dispose();
      this.previewLine = null;
    }
    for (const h of this.controlHandles) {
      this.ctx.viewport.scene.remove(h);
      if ('geometry' in h && (h as any).geometry?.dispose) (h as any).geometry.dispose();
      if ('material' in h && (h as any).material?.dispose) (h as any).material.dispose();
    }
    this.controlHandles = [];
  }

  // ═══════════════════════════════════════════════════
  //  Plane ray intersection
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
