/**
 * Draw Polygon-Hole Tool — punch / drill an ARBITRARY closed-polygon opening
 * into an existing coplanar face (ADR-249 P5). The free-form analog of the Hole
 * (circle) and Window (rect) tools.
 *
 * Flow (N-point capture on a face, mirrors DrawWindowTool's plane capture +
 * DrawPolyline's point accumulation):
 *   1st click → must land ON a face → capture point #1 + the face plane/normal
 *   clicks 2..N → add profile points (each ray ∩ the captured face plane)
 *   close → click near point #1 (≥3 points) OR Enter / double-click
 *           → `bridge.drillPolygonThroughHole` (solid) → tube
 *             fallback `bridge.punchPolygonHole` (sheet) → ring-with-hole
 *   Escape → cancel
 *
 * Like Hole/Window the host face is resolved fresh by the engine at commit time
 * (no stale face id). The profile loop is wound CCW around the face normal before
 * it is sent (the engine expects hole loops CCW around +n, per punch_circular).
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { t } from '../i18n';

const MAX_DRAW_DISTANCE = 50000;
const HOLE_COLOR = 0x4dabf7;
const MIN_POINTS = 3;

export class DrawPolygonHoleTool implements ITool {
  readonly name = 'polygon-hole';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawPolygonHoleTool] Activated');
    Toast.info(t('구멍을 낼 면 위를 클릭해 윤곽 점을 찍으세요 (Enter/더블클릭/첫 점 클릭으로 닫기)'), 3000);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.points.length === 0) {
      // First click must be ON a face (a hole needs a host).
      if (!point) return;
      const plane = this.ctx.getDrawPlane(e);
      if (!plane.onFace) {
        Toast.warning(t('다각형 구멍은 기존 면 위에 내야 합니다 — 면을 클릭하세요'));
        return;
      }
      this.plane = plane;
      this.points.push(point.clone());
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        plane.normal, point,
      );
      this.ctx.snap.setReferencePoint(point);
      return;
    }

    const planePoint = this.getPointOnDrawPlane(e);
    if (!planePoint) return;

    // Double-click, or a click near point #1 (≥3 points), closes the loop.
    if (e.detail >= 2 && this.points.length >= MIN_POINTS) {
      this.finalize();
      return;
    }
    if (this.points.length >= MIN_POINTS && planePoint.distanceTo(this.points[0]) <= this.closeThreshold()) {
      this.finalize();
      return;
    }
    this.points.push(planePoint);
    this.ctx.snap.setReferencePoint(planePoint);
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.points.length === 0 || !this.plane) {
      this.removePreview();
      return;
    }
    const planePoint = this.getPointOnDrawPlane(e);
    if (!planePoint) {
      this.removePreview();
      return;
    }
    this.updatePreview(planePoint);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    } else if ((e.key === 'Enter' || e.key === 'Return') && this.points.length >= MIN_POINTS) {
      this.finalize();
    }
  }

  applyVCBValue(_value: number): void {
    // Defined by free clicks (no numeric entry in this MVP).
  }

  isBusy(): boolean {
    return this.points.length > 0;
  }

  cleanup(): void {
    this.points = [];
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Winding + commit
  // ═══════════════════════════════════════════════════

  /** In-plane basis (e1, e2) — mirrors the engine punchers' `basis(n)`. */
  private faceBasis(n: THREE.Vector3): { e1: THREE.Vector3; e2: THREE.Vector3 } {
    let t = new THREE.Vector3(1, 0, 0);
    if (t.clone().cross(n).lengthSq() < 1e-6) t = new THREE.Vector3(0, 1, 0);
    const e1 = t.clone().addScaledVector(n, -t.dot(n)).normalize();
    const e2 = n.clone().cross(e1).normalize();
    return { e1, e2 };
  }

  /** Loop-close pick threshold — 2% of the accumulated extent, min 2mm. */
  private closeThreshold(): number {
    if (this.points.length < 2) return 2;
    const box = new THREE.Box3().setFromPoints(this.points);
    const ext = box.getSize(new THREE.Vector3()).length();
    return Math.max(2, ext * 0.02);
  }

  /** Profile points wound CCW around the face normal (engine expects CCW +n). */
  private ccwLoop(): [number, number, number][] {
    const n = this.plane!.normal.clone().normalize();
    const { e1, e2 } = this.faceBasis(n);
    // Signed area (shoelace) in the face basis.
    let area2 = 0;
    const uv = this.points.map((p) => [p.dot(e1), p.dot(e2)] as [number, number]);
    for (let i = 0; i < uv.length; i++) {
      const [u0, v0] = uv[i];
      const [u1, v1] = uv[(i + 1) % uv.length];
      area2 += u0 * v1 - u1 * v0;
    }
    const ordered = area2 < 0 ? [...this.points].reverse() : this.points;
    return ordered.map((p) => [p.x, p.y, p.z] as [number, number, number]);
  }

  private finalize(): void {
    if (!this.plane || this.points.length < MIN_POINTS) {
      Toast.warning(t('점이 3개 이상 필요합니다'));
      return;
    }
    const loop = this.ccwLoop();
    const n = this.plane.normal;
    const normal: [number, number, number] = [n.x, n.y, n.z];

    // 1) Through-hole (solid). Succeeds on a closed solid (the −normal ray reaches
    //    an anti-parallel opposite wall); returns the tube-quad count (> 0). On a
    //    single sheet face there is no opposite wall → drillPolygonThroughHole
    //    rolls back its own snapshot (ADR-190 P0.2) and returns −1 → fall back to
    //    a 2D face hole. The same fallback covers a missing engine export (−1).
    const tube = this.ctx.bridge.drillPolygonThroughHole(loop, normal);
    if (tube > 0) {
      debugLog(`[PolygonHole] Drilled through → ${tube} tube quads (${loop.length}-gon)`);
      Toast.success(t('관통 구멍을 뚫었습니다'));
      this.cleanup();
      this.ctx.syncMesh();
      return;
    }

    // 2) Fallback — 2D face hole (ring-with-hole on the single host face).
    const faceId = this.ctx.bridge.punchPolygonHole(loop, normal);
    if (faceId < 0) {
      Toast.fromBridgeError(
        this.ctx.bridge,
        '다각형 구멍 실패 — 면 경계 안에서 단순 다각형으로 다시 시도하세요',
      );
      return;
    }
    debugLog(`[PolygonHole] Punched face hole → ring face ${faceId} (${loop.length}-gon)`);
    Toast.success(t('면 구멍을 뚫었습니다'));
    this.cleanup();
    this.ctx.syncMesh();
  }

  // ═══════════════════════════════════════════════════
  //  Preview + plane ray intersection (mirrors DrawWindowTool)
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    if (this.points.length === 0 || !this.plane) return;
    const n = this.plane.normal;
    const pts = [...this.points, cursor].map((p) => p.clone().addScaledVector(n, 0.5));
    if (this.points.length >= MIN_POINTS) pts.push(pts[0].clone()); // hint the close
    const geo = new THREE.BufferGeometry().setFromPoints(pts);
    const mat = new THREE.LineBasicMaterial({ color: HOLE_COLOR, linewidth: 2 });
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

  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3 || this.points.length === 0) return null;
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
    if (!hit) return null;
    if (target.distanceTo(this.points[0]) > MAX_DRAW_DISTANCE) return null;
    return target;
  }
}
