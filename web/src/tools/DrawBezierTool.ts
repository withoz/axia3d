/**
 * Draw Bezier Tool — 4-point cubic Bezier curve (Phase I5, 2026-04-20).
 *
 * Flow:
 *   1st click → P0 (시작점) + plane detect
 *   2nd click → P1 (제어점 1)
 *   3rd click → P2 (제어점 2)
 *   4th click → P3 (끝점) → commit
 *
 * 각 단계 사이에 live preview 업데이트. 제어점은 시각적으로 표시되는
 * anchor handle (원형 점).
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';
import { tessellateCurve, nextCurveId, BezierCurve } from '../curves/Curve';
import { getCurveRegistry } from '../curves/CurveRegistry';
import { getDrawCurveMode } from './DrawCurveSettings';

/** ADR-089 A-ψ-β — closure detection threshold (mm). 1e-3 = ADR-026 P12
 *  cardinal snap range. P3 가 P0 와 이 이내 거리이면 closed Bezier 로 처리. */
const BEZIER_CLOSURE_EPSILON_MM = 1e-3;

export class DrawBezierTool implements ITool {
  readonly name = 'bezier';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private previewLine: THREE.Line | null = null;
  private controlHandles: THREE.Object3D[] = [];
  // ADR-284 β-3 — curved-surface draw: a CLOSED bezier loop ON a curved face.
  private curvedKind: 'cylinder' | 'cone' | 'torus' | 'sphere' | null = null;
  private curvedHostFace = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawBezierTool] Activated — P0 → P1 → P2 → P3 (cubic)');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.points.length === 0) {
      // P0 — plane detect
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        this.plane.normal, point,
      );
      this.points.push(point.clone());
      // ADR-284 β-3 — first click (P0) on a curved face → a CLOSED bezier loop
      // will be projected + split onto the surface. Capture host + kind.
      this.curvedKind = null;
      this.curvedHostFace = -1;
      const ck = ({ 2: 'cylinder', 3: 'sphere', 4: 'cone', 5: 'torus' } as const)[
        this.plane.surfaceKind as 2 | 3 | 4 | 5
      ];
      if (ck && typeof this.ctx.viewport?.pick === 'function') {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        if (hit && hit.faceIndex != null) {
          const fid = this.ctx.getFaceId(hit.faceIndex);
          if (fid >= 0) { this.curvedKind = ck; this.curvedHostFace = fid; }
        }
      }
      this.ctx.snap.setReferencePoint(point);
      // ADR-166 β-2 — first_click plane lock (idempotent, L-166-2).
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
    this.points.push(p.clone());

    if (this.points.length === 4) {
      this.commit();
      this.cleanup();
    } else {
      this.ctx.snap.setReferencePoint(p);
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.points.length === 0 || !this.plane) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;

    // 현재까지 클릭한 점 + 마우스 위치로 preview
    const pts = [...this.points, p];
    this.updatePreview(pts);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    }
  }

  applyVCBValue(_value: number): void {
    // Bezier는 숫자 입력 없음
  }

  isBusy(): boolean {
    return this.points.length > 0;
  }

  cleanup(): void {
    this.points = [];
    this.plane = null;
    this.drawPlane3 = null;
    this.curvedKind = null;
    this.curvedHostFace = -1;
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Commit
  // ═══════════════════════════════════════════════════

  private commit(): void {
    if (this.points.length !== 4) return;

    // ADR-089 A-ψ-β — closure auto-detection.
    // L-ψ-1 / L-ψ-2 / L-ψ-3: drawCurveMode ON + P3 ≈ P0 → closed Bezier.
    const p0 = this.points[0];
    const p3 = this.points[3];
    const closureGap = p0.distanceTo(p3);
    const isClosed =
      getDrawCurveMode() && closureGap < BEZIER_CLOSURE_EPSILON_MM;

    if (isClosed) {
      // ADR-284 β-3 — closed bezier on a CURVED face → tessellate the loop +
      // project/split onto the surface (cap + remainder).
      if (this.curvedKind && this.curvedHostFace >= 0
          && typeof this.ctx.bridge.drawPolylineOnCurved === 'function') {
        const loopCurve: BezierCurve = {
          kind: 'bezier',
          id: nextCurveId(),
          controlPoints: [
            [this.points[0].x, this.points[0].y, this.points[0].z],
            [this.points[1].x, this.points[1].y, this.points[1].z],
            [this.points[2].x, this.points[2].y, this.points[2].z],
            [p0.x, p0.y, p0.z],
          ],
          segments: 32,
          planeNormal: this.plane
            ? [this.plane.normal.x, this.plane.normal.y, this.plane.normal.z]
            : [0, 1, 0],
          closed: true,
        };
        const tp = tessellateCurve(loopCurve);
        const pts: Array<[number, number, number]> = [];
        for (const p of tp) {
          if (pts.length === 0 || p.distanceTo(new THREE.Vector3(...pts[pts.length - 1])) >= 0.1) {
            pts.push([p.x, p.y, p.z]);
          }
        }
        if (pts.length >= 3) {
          const res = this.ctx.bridge.drawPolylineOnCurved(this.curvedKind, this.curvedHostFace, pts, true);
          if (!res || res.includes('"error"')) {
            // eslint-disable-next-line no-console
            console.warn(`[Bezier] curved split on ${this.curvedKind} failed: ${res}`);
          } else {
            debugLog(`[Bezier] curved closed loop split on ${this.curvedKind} host=${this.curvedHostFace}`);
          }
          this.ctx.syncMesh();
          return;
        }
      }
      // Closed Bezier: forward to drawClosedBezierAsCurve with P0
      // duplicated as last control point (ensures exact closure on
      // engine side regardless of f32 drift).
      const ctrlFlat = new Float64Array(5 * 3);
      for (let i = 0; i < 4; i++) {
        ctrlFlat[i * 3]     = this.points[i].x;
        ctrlFlat[i * 3 + 1] = this.points[i].y;
        ctrlFlat[i * 3 + 2] = this.points[i].z;
      }
      // Closure: cp[4] = cp[0] (exact)
      ctrlFlat[12] = p0.x;
      ctrlFlat[13] = p0.y;
      ctrlFlat[14] = p0.z;
      const ok = this.ctx.bridge.drawClosedBezierAsCurve(ctrlFlat);
      // ADR-164 β-2 — Sticky last drawn plane (closed Bezier face 합성
      // success only — Q1=a strict). Open Bezier (face 없음) 는 skip.
      if (this.plane && (typeof ok !== 'number' || ok >= 0)) {
        this.ctx.setLastDrawnPlane?.({
          origin: p0,
          normal: this.plane.normal,
          up: this.plane.up,
          source: 'view',
        });
      }
      this.ctx.syncMesh();
      debugLog(
        `[Bezier/Closed] gap=${closureGap.toExponential(2)}mm → ` +
        `drawClosedBezierAsCurve (shapeId=${ok}, kernel-native closed loop)`
      );
      return;
    }

    // ADR-284 β-4-3 — OPEN bezier on a Sphere face → rim-to-rim seam split.
    // The curve arcs from P0 over the hemisphere (P1/P2 pull it into the
    // interior) to P3; tessellate + project/split. (Cylinder/cone/torus open =
    // multi-rim β-4-4; a straight 2-click line is degenerate — ADR-284 §β-4-1.)
    if (this.curvedKind === 'sphere' && this.curvedHostFace >= 0
        && typeof this.ctx.bridge.drawOpenSeamOnSphere === 'function') {
      const openCurve: BezierCurve = {
        kind: 'bezier',
        id: nextCurveId(),
        controlPoints: this.points.map(p => [p.x, p.y, p.z] as [number, number, number]),
        segments: 32,
        planeNormal: this.plane
          ? [this.plane.normal.x, this.plane.normal.y, this.plane.normal.z]
          : [0, 1, 0],
        closed: false,
      };
      const tp = tessellateCurve(openCurve);
      const pts: Array<[number, number, number]> = [];
      for (const p of tp) {
        if (pts.length === 0 || p.distanceTo(new THREE.Vector3(...pts[pts.length - 1])) >= 0.1) {
          pts.push([p.x, p.y, p.z]);
        }
      }
      if (pts.length >= 3) {
        const res = this.ctx.bridge.drawOpenSeamOnSphere(this.curvedHostFace, pts);
        if (!res || res.includes('"error"')) {
          // eslint-disable-next-line no-console
          console.warn(`[Bezier] open seam on sphere failed: ${res}`);
        } else {
          debugLog(`[Bezier] open seam split on sphere host=${this.curvedHostFace}`);
        }
        this.ctx.syncMesh();
        return;
      }
    }

    const curve: BezierCurve = {
      kind: 'bezier',
      id: nextCurveId(),
      controlPoints: this.points.map(p => [p.x, p.y, p.z] as [number, number, number]),
      segments: 32,
      planeNormal: this.plane
        ? [this.plane.normal.x, this.plane.normal.y, this.plane.normal.z]
        : [0, 1, 0],
      closed: false,
    };
    getCurveRegistry().add(curve);

    // ADR-032 P17 — Promote on creation: drawBezierWithCurve atomic API
    // attaches AnalyticCurve::Bezier to each segment edge.
    const ctrlFlat = new Float64Array(curve.controlPoints.length * 3);
    for (let i = 0; i < curve.controlPoints.length; i++) {
      ctrlFlat[i * 3]     = curve.controlPoints[i][0];
      ctrlFlat[i * 3 + 1] = curve.controlPoints[i][1];
      ctrlFlat[i * 3 + 2] = curve.controlPoints[i][2];
    }
    const ok = this.ctx.bridge.drawBezierWithCurve(ctrlFlat, curve.segments ?? 32);

    if (ok < 0) {
      // Fallback to plain polyline.
      const pts = tessellateCurve(curve);
      const filtered: Array<{ x: number; y: number; z: number }> = [];
      for (let i = 0; i < pts.length; i++) {
        if (filtered.length === 0 || pts[i].distanceTo(pts[i - 1]) >= 0.1) {
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
        // ADR-087 K-ζ — kernel-aware drawPolylineAsShape only.
        this.ctx.bridge.drawPolylineAsShape(flat);
      }
    }
    this.ctx.syncMesh();
    debugLog(`[Bezier] 4 control points → drawBezierWithCurve (ok=${ok}, analytic Bezier attached)`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(ctrl: THREE.Vector3[]): void {
    this.removePreview();
    if (ctrl.length < 2) return;

    // 제어 polygon (점선)
    const polygonGeo = new THREE.BufferGeometry().setFromPoints(ctrl);
    const polygonMat = new THREE.LineDashedMaterial({
      color: 0x888888,
      dashSize: 5,
      gapSize: 3,
    });
    const polyLine = new THREE.Line(polygonGeo, polygonMat);
    (polyLine as any).computeLineDistances?.();
    polyLine.renderOrder = 998;
    this.ctx.viewport.scene.add(polyLine);
    this.controlHandles.push(polyLine);

    // 곡선 preview (4점 이상이면 Bezier, 아니면 직선/부분)
    let curvePts: THREE.Vector3[] = ctrl;
    if (ctrl.length === 4) {
      const tempCurve: BezierCurve = {
        kind: 'bezier',
        id: 0,
        controlPoints: ctrl.map(p => [p.x, p.y, p.z] as [number, number, number]),
        segments: 24,
        planeNormal: this.plane
          ? [this.plane.normal.x, this.plane.normal.y, this.plane.normal.z]
          : [0, 1, 0],
        closed: false,
      };
      curvePts = tessellateCurve(tempCurve);
    }
    const geo = new THREE.BufferGeometry().setFromPoints(curvePts);
    const mat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 2 });
    this.previewLine = new THREE.Line(geo, mat);
    this.previewLine.renderOrder = 999;
    this.ctx.viewport.scene.add(this.previewLine);

    // 제어점 시각화 — 작은 구
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
