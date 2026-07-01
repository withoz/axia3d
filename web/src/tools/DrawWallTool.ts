/**
 * Draw Wall Tool — a parametric wall from a baseline (architectural).
 * (24-tool toolbar — Wall, 2026-06-08)
 *
 * Flow:
 *   1st click → baseline start P0 + plane detect + lock (ADR-166)
 *   2nd click → baseline end P1 → wall
 *   Escape    → cancel
 *
 * Builds a footprint rectangle (length × thickness) along the baseline on the
 * lock plane, then extrudes it up the plane normal by the wall height — a
 * box-like wall. Defaults: thickness 20 mm, height 250 mm; type a number (VCB)
 * after the first click to override the height.
 *
 * Engine APIs: `bridge.drawRectAsShape` → `getShapeFaceIds` →
 * `bridge.createSolidExtrude(faceId, height)` (ADR-079 / ADR-087). The
 * footprint is drawn in empty space, so its face id is stable (no
 * auto-synthesis churn).
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

const DEFAULT_THICKNESS_MM = 20;
const DEFAULT_HEIGHT_MM = 250;
const MIN_LENGTH_MM = 1;

export class DrawWallTool implements ITool {
  readonly name = 'wall';

  private ctx: ToolContext;
  private start: THREE.Vector3 | null = null;
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;
  private thickness = DEFAULT_THICKNESS_MM;
  private height = DEFAULT_HEIGHT_MM;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawWallTool] Activated — click baseline start → end (type a number for height)');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.start) {
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(this.plane.normal, point);
      this.start = point.clone();
      this.ctx.snap.setReferencePoint(point);
      this.ctx.lockPlane?.({
        origin: point, normal: this.plane.normal, up: this.plane.up, source: 'first_click',
      });
      return;
    }
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    const len = p.distanceTo(this.start);
    if (len < MIN_LENGTH_MM) {
      debugLog('[Wall] baseline too short — skipped');
      this.cleanup();
      return;
    }
    this.commit(p);
    this.cleanup();
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.start || !this.plane) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    this.updatePreview(p);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(value: number): void {
    // Type a number after the first click → wall height (mm).
    if (this.start && Number.isFinite(value) && value > 0) {
      this.height = value;
      debugLog(`[Wall] height = ${value} mm`);
    }
  }

  isBusy(): boolean {
    return this.start !== null;
  }

  cleanup(): void {
    this.start = null;
    this.plane = null;
    this.drawPlane3 = null;
    this.height = DEFAULT_HEIGHT_MM;
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Commit — footprint rect → extrude up
  // ═══════════════════════════════════════════════════

  private commit(end: THREE.Vector3): void {
    if (!this.start || !this.plane) return;
    const start = this.start;
    const mid = start.clone().add(end).multiplyScalar(0.5);
    const dir = end.clone().sub(start).normalize(); // baseline direction
    const len = end.distanceTo(start);
    const n = this.plane.normal;

    // footprint: width = length along the baseline (up = dir), height = thickness
    const shapeId = this.ctx.bridge.drawRectAsShape(
      mid.x, mid.y, mid.z,
      n.x, n.y, n.z,
      dir.x, dir.y, dir.z,
      len, this.thickness,
    );
    this.ctx.syncMesh();
    if (typeof shapeId !== 'number' || shapeId < 0) {
      debugLog('[Wall] footprint drawRectAsShape failed');
      return;
    }
    const faces = this.ctx.bridge.getShapeFaceIds(shapeId);
    if (faces.length === 0) {
      debugLog('[Wall] footprint has no face');
      return;
    }
    const ok = this.ctx.bridge.createSolidExtrude(faces[0], this.height);
    this.ctx.syncMesh();
    debugLog(`[Wall] len=${len.toFixed(1)} thick=${this.thickness} height=${this.height} → extrude=${ok}`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview — the footprint outline along the baseline
  // ═══════════════════════════════════════════════════

  private updatePreview(end: THREE.Vector3): void {
    this.removePreview();
    if (!this.start || !this.plane) return;
    const start = this.start;
    const dir = end.clone().sub(start);
    const len = dir.length();
    if (len < MIN_LENGTH_MM) return;
    dir.normalize();
    const n = this.plane.normal.clone().normalize();
    const side = n.clone().cross(dir).normalize().multiplyScalar(this.thickness * 0.5);
    const a = start.clone().add(side);
    const b = end.clone().add(side);
    const c = end.clone().sub(side);
    const d = start.clone().sub(side);
    const geo = new THREE.BufferGeometry().setFromPoints([a, b, c, d, a]);
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
