/**
 * Draw Hole Tool — punch a circular hole through an existing coplanar face.
 *
 * Flow (a constrained Circle tool):
 *   1st click → must land ON an existing face → set hole center + capture the
 *               face plane (its DCEL normal is the punch normal hint)
 *   mouse move → ray ∩ face plane → preview the hole circle
 *   2nd click → ray ∩ face plane → `bridge.punchHole(...)` → manifold ring-with-hole
 *
 * Unlike "draw a circle, then merge it as a hole" (whose intermediate face id
 * goes stale after the face-synthesis re-derive — CLAUDE.md LOCKED #40 /
 * ADR-101 / 메타-원칙 #15), this resolves the host face *fresh* from the
 * world-space center at commit time and promotes the hole in a single atomic
 * engine call. The caller never passes a (stale) face id.
 *
 * The first click MUST be on a face — a hole needs a host. Clicking empty
 * space / the ground warns and does nothing.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

/** Max distance from center to prevent runaway geometry when ray grazes the plane. */
const MAX_DRAW_DISTANCE = 50000;
/** Circle segment count for the punched hole (matches the preview). */
const HOLE_SEGMENTS = 48;
/** Hole accent color (distinct from the Circle tool's purple). */
const HOLE_COLOR = 0xff6b6b;

export class DrawHoleTool implements ITool {
  readonly name = 'hole';

  private ctx: ToolContext;
  private holeCenter: THREE.Vector3 | null = null;
  private circlePreview: THREE.Line | null = null;
  private circleFill: THREE.Mesh | null = null;

  // Host face plane (captured at the first click — must be onFace).
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawHoleTool] Activated');
    Toast.info('구멍을 뚫을 면 위를 클릭해 중심을 지정하세요', 2500);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.holeCenter) {
      // ═══ First click: must be ON a face — capture its plane + center ═══
      if (!point) return;
      const plane = this.ctx.getDrawPlane(e);
      if (!plane.onFace) {
        // A hole needs a host face — refuse to start on the ground / empty space.
        Toast.warning('구멍은 기존 면 위에 뚫어야 합니다 — 면을 클릭하세요');
        return;
      }
      this.plane = plane;
      // Keep the exact pick point on the face (no cardinal zeroing — that is
      // only for ground planes; here the host face owns the coordinate).
      this.holeCenter = point.clone();
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        plane.normal, this.holeCenter,
      );
      this.ctx.snap.setReferencePoint(point);
    } else {
      // ═══ Second click: radius point on the face plane → punch ═══
      const planePoint = this.getPointOnDrawPlane(e);
      if (!planePoint || !this.plane) {
        this.cleanup();
        return;
      }
      const radius = this.holeCenter.distanceTo(planePoint);
      this.commitHole(radius);
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.holeCenter || !this.plane) {
      this.removePreview();
      return;
    }
    const planePoint = this.getPointOnDrawPlane(e);
    if (!planePoint) {
      this.removePreview();
      return;
    }
    const radius = this.holeCenter.distanceTo(planePoint);
    if (radius > 0.1) {
      this.updatePreview(this.holeCenter, radius);
      this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
        {
          from: this.holeCenter.clone(),
          to: planePoint,
          text: 'R ' + this.ctx.units.format(radius),
          color: '#ff6b6b',
        },
      ]);
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    }
  }

  applyVCBValue(value: number): void {
    if (!this.holeCenter || !this.plane) return;
    this.commitHole(value);
    this.cleanup();
  }

  isBusy(): boolean {
    return this.holeCenter !== null;
  }

  cleanup(): void {
    this.holeCenter = null;
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Commit
  // ═══════════════════════════════════════════════════

  /**
   * Make the hole via the atomic engine API. The host face is resolved by the
   * engine from `holeCenter` + the face normal — no (stale) face id is passed.
   *
   * ADR-194 β-2 — try a true **3D through-hole** first (`drillThroughHole`). It
   * succeeds when the click is on a closed solid (the −normal ray reaches an
   * anti-parallel opposite wall) and returns the tube-quad count (> 0). On a
   * single **sheet** face there is no opposite wall → `drillThroughHole`
   * rolls back its own snapshot (ADR-190 P0.2) and returns −1, so we fall back
   * to a **2D face hole** (`punchHole`) on the clean mesh. The same fallback
   * also covers an engine without the `drillThroughHole` export (graceful −1).
   */
  private commitHole(radius: number): void {
    if (!this.holeCenter || !this.plane) return;
    if (radius <= 1) {
      Toast.warning('구멍 반지름이 너무 작습니다');
      return;
    }
    const c = this.holeCenter;
    const n = this.plane.normal;
    const center: [number, number, number] = [c.x, c.y, c.z];
    const normal: [number, number, number] = [n.x, n.y, n.z];

    // 1) Through-hole (solid).
    const tube = this.ctx.bridge.drillThroughHole(center, normal, radius, HOLE_SEGMENTS);
    if (tube > 0) {
      debugLog(`[Hole] Drilled through R=${radius.toFixed(2)} → ${tube} tube quads`);
      Toast.success('관통 구멍을 뚫었습니다');
      this.ctx.syncMesh();
      return;
    }

    // 2) Fallback — 2D face hole (ring-with-hole on the single host face).
    const faceId = this.ctx.bridge.punchHole(center, normal, radius, HOLE_SEGMENTS);
    if (faceId < 0) {
      Toast.fromBridgeError(
        this.ctx.bridge,
        '구멍 뚫기 실패 — 면 경계 안에서 다시 시도하세요',
      );
      return;
    }
    debugLog(`[Hole] Punched 2D face hole R=${radius.toFixed(2)} → ring face ${faceId}`);
    Toast.success('면 구멍을 뚫었습니다');
    this.ctx.syncMesh();
  }

  // ═══════════════════════════════════════════════════
  //  Drawing Plane Ray Intersection (mirrors DrawCircleTool)
  // ═══════════════════════════════════════════════════

  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3 || !this.holeCenter) return null;

    const rawPt = this.ctx.get3DPoint(e);
    const snapped = this.ctx.getSnappedPoint(e, rawPt);
    if (snapped) {
      return this.projectOntoPlane(snapped);
    }
    const ray = this.ctx.getRay(e);
    const target = new THREE.Vector3();
    const hit = ray.ray.intersectPlane(this.drawPlane3, target);
    if (!hit) return null;
    if (target.distanceTo(this.holeCenter) > MAX_DRAW_DISTANCE) return null;
    return target;
  }

  private projectOntoPlane(point: THREE.Vector3): THREE.Vector3 {
    if (!this.drawPlane3) return point.clone();
    const projected = point.clone();
    const dist = this.drawPlane3.distanceToPoint(projected);
    projected.addScaledVector(this.drawPlane3.normal, -dist);
    return projected;
  }

  // ═══════════════════════════════════════════════════
  //  Preview Rendering (mirrors DrawCircleTool, hole-red accent)
  // ═══════════════════════════════════════════════════

  private updatePreview(center: THREE.Vector3, radius: number): void {
    this.removePreview();
    if (!this.plane) return;

    const n = this.plane.normal;
    const r = this.plane.right;
    const u = this.plane.up;
    const segments = HOLE_SEGMENTS;

    const points: THREE.Vector3[] = [];
    for (let i = 0; i <= segments; i++) {
      const angle = (i / segments) * Math.PI * 2;
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      points.push(
        center.clone()
          .addScaledVector(r, cos * radius)
          .addScaledVector(u, sin * radius)
          .addScaledVector(n, 0.5),
      );
    }
    const lineGeo = new THREE.BufferGeometry().setFromPoints(points);
    const lineMat = new THREE.LineBasicMaterial({ color: HOLE_COLOR, linewidth: 1 });
    this.circlePreview = new THREE.Line(lineGeo, lineMat);
    this.circlePreview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.circlePreview);

    const fillGeo = new THREE.CircleGeometry(radius, segments);
    const fillMat = new THREE.MeshBasicMaterial({
      color: HOLE_COLOR,
      transparent: true,
      opacity: 0.18,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    this.circleFill = new THREE.Mesh(fillGeo, fillMat);
    const defaultNormal = new THREE.Vector3(0, 0, 1);
    const quat = new THREE.Quaternion().setFromUnitVectors(defaultNormal, n);
    this.circleFill.quaternion.copy(quat);
    this.circleFill.position.copy(center.clone().addScaledVector(n, 0.5));
    this.circleFill.renderOrder = 998;
    this.ctx.viewport.scene.add(this.circleFill);
  }

  private removePreview(): void {
    if (this.circlePreview) {
      this.ctx.viewport.scene.remove(this.circlePreview);
      this.circlePreview.geometry.dispose();
      (this.circlePreview.material as THREE.Material).dispose();
      this.circlePreview = null;
    }
    if (this.circleFill) {
      this.ctx.viewport.scene.remove(this.circleFill);
      this.circleFill.geometry.dispose();
      (this.circleFill.material as THREE.Material).dispose();
      this.circleFill = null;
    }
  }
}
