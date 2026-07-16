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
import { debugLog, debugWarn } from '../utils/debug';
import { Toast } from '../ui/Toast';
import { humanizeEngineError } from '../bridge/humanizeEngineError';

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

  // ADR-284 follow-up — curved host captured on the first click. Ellipse was in
  // ADR-284's own audit table ("Rect / Polygon / Ellipse") but dropped from the
  // fix without a word in the closure, leaving it the worst case in the matrix:
  // a CLOSED shape (exactly what the engine handles) on surfaces that all work,
  // drawn flat on the tangent plane with no split, no error and no toast —
  // strictly worse than Line, which at least says why it declines.
  private curvedKind: 'cylinder' | 'sphere' | 'cone' | 'torus' | null = null;
  private curvedHostFace = -1;

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
      // ADR-284 follow-up — a curved host: capture it now, exactly as
      // DrawPolygonTool does. getDrawPlane gives the TANGENT plane for
      // surfaceKind >= 2, which is why an unwired tool draws a flat shape that
      // touches the surface at one point and floats off it everywhere else.
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
    // curvedKind / curvedHostFace are deliberately NOT reset here: the first
    // click re-derives both before anything can read them, so a reset in
    // cleanup would be redundant — and mutation-testing confirmed no test can
    // tell the difference, which is the tell for a guard that isn't earning its
    // place. The "does not leak the host" regression pins the behaviour.
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  /**
   * ADR-284 follow-up — the ellipse's world verts, sampled in the tangent plane
   * for the engine to project onto the host surface (mirrors
   * DrawPolygonTool.ngonWorldVerts). 48 samples matches the chord fidelity the
   * other closed curved shapes use.
   */
  private ellipseWorldVerts(
    center: THREE.Vector3, refDir: THREE.Vector3, rx: number, ry: number, normal: THREE.Vector3,
  ): Array<[number, number, number]> {
    const minorDir = normal.clone().cross(refDir).normalize();
    const out: Array<[number, number, number]> = [];
    const N = 48;
    for (let i = 0; i < N; i++) {
      const t = (i / N) * Math.PI * 2;
      const p = center.clone()
        .addScaledVector(refDir, rx * Math.cos(t))
        .addScaledVector(minorDir, ry * Math.sin(t));
      out.push([p.x, p.y, p.z]);
    }
    return out;
  }

  private commitEllipse(refDir: THREE.Vector3, rx: number, ry: number): void {
    if (!this.center || !this.plane) { this.cleanup(); return; }
    if (rx < 1 || ry < 1) { this.cleanup(); return; }
    const n = this.plane.normal;

    // ADR-284 follow-up — curved host: project + split instead of laying a flat
    // ellipse on the tangent plane. This lives in commitEllipse, which the VCB
    // path also calls, so typing the radii behaves the same as clicking them —
    // DrawCircleTool's applyVCBValue skips its own curved branch and silently
    // draws flat, and that split-brain is not worth reproducing here.
    if (this.curvedKind && this.curvedHostFace >= 0
        && typeof this.ctx.bridge.drawPolylineOnCurved === 'function') {
      const verts = this.ellipseWorldVerts(this.center, refDir, rx, ry, n);
      const res = this.ctx.bridge.drawPolylineOnCurved(
        this.curvedKind, this.curvedHostFace, verts, true,
      );
      if (!res || res.includes('"error"')) {
        debugWarn(`[Ellipse] curved split on ${this.curvedKind} failed: ${res}`);
        Toast.warning(
          humanizeEngineError(this.ctx.bridge.lastError())
            || '이 곡면에는 타원을 그릴 수 없습니다',
          3500,
        );
      } else {
        debugLog(`[Ellipse] curved split on ${this.curvedKind} host=${this.curvedHostFace} rx=${rx.toFixed(2)} ry=${ry.toFixed(2)}`);
      }
      this.ctx.syncMesh();
      this.cleanup();
      return;
    }

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
