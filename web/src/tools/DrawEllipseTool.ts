/**
 * Draw Ellipse Tool — kernel-native ellipse on any plane (ground, face, wall).
 *
 * Flow (3-click, ADR-206):
 *   1st click → detect drawing plane (face normal or ground) + set center
 *   2nd click → in-plane direction from center = major axis (ref_dir) + radius_x
 *   3rd click → perpendicular distance = minor radius (radius_y) → commit
 *
 * Always kernel-native (`drawEllipseAsCurve` → 1 anchor + 1 self-loop exact-
 * ellipse NURBS edge + 1 Plane face). There is no polygon-Shape legacy for
 * ellipses (unlike circles). Mirrors DrawCircleTool's plane/snap/cardinal logic.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

/** Max distance from center to prevent runaway geometry when the ray grazes the plane. */
const MAX_DRAW_DISTANCE = 50000;

export class DrawEllipseTool implements ITool {
  readonly name = 'ellipse';

  private ctx: ToolContext;
  private center: THREE.Vector3 | null = null;
  private refDir: THREE.Vector3 | null = null; // in-plane major-axis direction
  private radiusX = 0;                          // semi-axis along refDir
  private preview: THREE.Line | null = null;

  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawEllipseTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.center) {
      // ═══ Click 1: detect drawing plane + set center ═══
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.center = point.clone();

      // Cardinal ground plane → force normal-axis coord to exactly 0
      // (absorbs ray-plane intersection ε). Skip on a solid face.
      if (!this.plane.onFace) {
        const n = this.plane.normal;
        if (Math.abs(n.x) > 0.999) this.center.x = 0;
        else if (Math.abs(n.y) > 0.999) this.center.y = 0;
        else if (Math.abs(n.z) > 0.999) this.center.z = 0;
      }

      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        this.plane.normal, this.center,
      );
      this.ctx.snap.setReferencePoint(this.center);

      // ADR-166 β-2 — first_click plane lock (idempotent).
      this.ctx.lockPlane?.({
        origin: this.center,
        normal: this.plane.normal,
        up: this.plane.up,
        source: 'first_click',
      });
    } else if (!this.refDir) {
      // ═══ Click 2: major axis direction (ref_dir) + radius_x ═══
      const planePoint = this.getPointOnDrawPlane(e);
      if (!planePoint) return;
      const delta = planePoint.clone().sub(this.center);
      const rx = delta.length();
      if (rx < 1) return; // too small — keep waiting
      this.refDir = delta.normalize();
      this.radiusX = rx;
    } else {
      // ═══ Click 3: minor radius (radius_y) → commit ═══
      const planePoint = this.getPointOnDrawPlane(e);
      if (!planePoint || !this.plane) { this.cleanup(); return; }
      const minorDir = this.plane.normal.clone().cross(this.refDir).normalize();
      const ry = Math.abs(planePoint.clone().sub(this.center).dot(minorDir));
      this.commitEllipse(this.refDir, this.radiusX, ry);
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.center || !this.plane) { this.removePreview(); return; }
    const planePoint = this.getPointOnDrawPlane(e);
    if (!planePoint) { this.removePreview(); return; }

    if (!this.refDir) {
      // Stage 2 preview: major-axis line center → cursor + radius label.
      const rx = this.center.distanceTo(planePoint);
      this.updateLinePreview(this.center, planePoint);
      this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
        { from: this.center.clone(), to: planePoint, text: 'A ' + this.ctx.units.format(rx), color: '#da77f2' },
      ]);
    } else {
      // Stage 3 preview: ellipse with rx fixed + ry from cursor.
      const minorDir = this.plane.normal.clone().cross(this.refDir).normalize();
      const ry = Math.abs(planePoint.clone().sub(this.center).dot(minorDir));
      this.updateEllipsePreview(this.center, this.refDir, minorDir, this.radiusX, Math.max(ry, 0.1));
      this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
        { from: this.center.clone(), to: planePoint, text: 'B ' + this.ctx.units.format(ry), color: '#da77f2' },
      ]);
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  /** VCB: stage 2 → radius_x (ref_dir = plane.right); stage 3 → radius_y → commit. */
  applyVCBValue(value: number): void {
    if (!this.center || !this.plane || value <= 0) return;
    if (!this.refDir) {
      // Stage 2 via VCB: axis-aligned major along plane.right.
      this.refDir = this.plane.right.clone().normalize();
      this.radiusX = value;
    } else {
      // Stage 3 via VCB: radius_y → commit.
      this.commitEllipse(this.refDir, this.radiusX, value);
    }
  }

  isBusy(): boolean {
    return this.center !== null;
  }

  cleanup(): void {
    this.center = null;
    this.refDir = null;
    this.radiusX = 0;
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  private commitEllipse(refDir: THREE.Vector3, rx: number, ry: number): void {
    if (!this.center || !this.plane) { this.cleanup(); return; }
    if (rx < 1 || ry < 1) { this.cleanup(); return; }
    const n = this.plane.normal;
    this.ctx.bridge.drawEllipseAsCurve?.(
      this.center.x, this.center.y, this.center.z,
      refDir.x, refDir.y, refDir.z,
      n.x, n.y, n.z,
      rx, ry,
    );
    debugLog(`[Ellipse] rx=${rx.toFixed(2)} ry=${ry.toFixed(2)} on plane (${n.x.toFixed(2)},${n.y.toFixed(2)},${n.z.toFixed(2)})`);
    this.ctx.setLastDrawnPlane?.({
      origin: this.center, normal: n, up: this.plane.up, source: 'view',
    });
    this.ctx.syncMesh();
    this.cleanup();
  }

  // ═══════════════════════════════════════════════════
  //  Drawing Plane Ray Intersection (mirrors DrawCircleTool)
  // ═══════════════════════════════════════════════════

  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3 || !this.center) return null;
    const rawPt = this.ctx.get3DPoint(e);
    const snapped = this.ctx.getSnappedPoint(e, rawPt);
    let result: THREE.Vector3 | null = null;
    if (snapped) {
      result = this.projectOntoPlane(snapped);
    } else {
      const ray = this.ctx.getRay(e);
      const target = new THREE.Vector3();
      const hit = ray.ray.intersectPlane(this.drawPlane3, target);
      if (!hit) return null;
      if (target.distanceTo(this.center) > MAX_DRAW_DISTANCE) return null;
      result = target;
    }
    if (!result) return null;
    // Cardinal ground plane → force normal-axis coord to the center's (exactly 0).
    if (this.plane && !this.plane.onFace) {
      const n = this.plane.normal;
      if (Math.abs(n.x) > 0.999) result.x = this.center.x;
      else if (Math.abs(n.y) > 0.999) result.y = this.center.y;
      else if (Math.abs(n.z) > 0.999) result.z = this.center.z;
    }
    return result;
  }

  private projectOntoPlane(point: THREE.Vector3): THREE.Vector3 {
    if (!this.drawPlane3) return point.clone();
    const projected = point.clone();
    const dist = this.drawPlane3.distanceToPoint(projected);
    projected.addScaledVector(this.drawPlane3.normal, -dist);
    return projected;
  }

  // ═══════════════════════════════════════════════════
  //  Preview Rendering
  // ═══════════════════════════════════════════════════

  private updateLinePreview(a: THREE.Vector3, b: THREE.Vector3): void {
    this.removePreview();
    const geo = new THREE.BufferGeometry().setFromPoints([a.clone(), b.clone()]);
    const mat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 1 });
    this.preview = new THREE.Line(geo, mat);
    this.preview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.preview);
  }

  private updateEllipsePreview(
    center: THREE.Vector3, major: THREE.Vector3, minor: THREE.Vector3,
    rx: number, ry: number,
  ): void {
    this.removePreview();
    const n = this.plane ? this.plane.normal : new THREE.Vector3(0, 0, 1);
    const segments = 64;
    const points: THREE.Vector3[] = [];
    for (let i = 0; i <= segments; i++) {
      const t = (i / segments) * Math.PI * 2;
      points.push(
        center.clone()
          .addScaledVector(major, Math.cos(t) * rx)
          .addScaledVector(minor, Math.sin(t) * ry)
          .addScaledVector(n, 0.5),
      );
    }
    const geo = new THREE.BufferGeometry().setFromPoints(points);
    const mat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 1 });
    this.preview = new THREE.Line(geo, mat);
    this.preview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.preview);
  }

  private removePreview(): void {
    if (this.preview) {
      this.ctx.viewport.scene.remove(this.preview);
      this.preview.geometry.dispose();
      (this.preview.material as THREE.Material).dispose();
      this.preview = null;
    }
  }
}
