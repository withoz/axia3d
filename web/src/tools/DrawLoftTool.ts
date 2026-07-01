/**
 * Draw Loft Tool — loft a stack of circular cross-sections into a vase shell.
 * (24-tool toolbar — Loft, 2026-06-08)
 *
 * Flow:
 *   1st click → center P0 + plane detect + lock (ADR-166)
 *   2nd click → base radius R = |P1 - P0| → loft
 *   Escape    → cancel
 *
 * Builds four circular sections perpendicular to the lock-plane normal at
 * fractions of the height (H = 2.5·R) with varying radius [R, 1.4R, 0.5R,
 * 0.75R] — a bulged-then-necked silhouette no primitive can produce, which
 * showcases the loft's varying-section blend. The engine (`Mesh::loft`)
 * stitches consecutive rings into a surface (open shell — no end caps).
 *
 * Engine API: `bridge.loftSections(sectionsFlat, sectionSize, closed=true)`
 * → `Mesh::loft` (operations/loft.rs), single undo transaction.
 *
 * MVP scope: a parametric vase profile. Arbitrary user-defined sections
 * (multiple drawn/selected profiles) are a natural future enhancement.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

const MIN_RADIUS_MM = 0.1;
const PROFILE_SEGMENTS = 24;
const HEIGHT_PER_RADIUS = 2.5;
const TWO_PI = Math.PI * 2;

/** Vase silhouette: [height fraction, radius scale]. */
const VASE_SECTIONS: ReadonlyArray<readonly [number, number]> = [
  [0.0, 1.0],   // base
  [0.4, 1.4],   // bulge
  [0.75, 0.5],  // neck
  [1.0, 0.75],  // lip
];

export class DrawLoftTool implements ITool {
  readonly name = 'loft';

  private ctx: ToolContext;
  private points: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawLoftTool] Activated — center → radius → lofted vase');
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
    if (this.points.length === 2) {
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

  applyVCBValue(value: number): void {
    // Type a radius (mm) after the first click → loft at that radius.
    if (this.points.length === 1 && Number.isFinite(value) && value >= MIN_RADIUS_MM && this.plane) {
      const n = this.plane.normal.clone().normalize();
      const { u } = this.planeBasis(n);
      this.points.push(this.points[0].clone().addScaledVector(u, value));
      this.commit();
      this.cleanup();
    }
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
  //  Sections + commit
  // ═══════════════════════════════════════════════════

  /** In-plane orthonormal basis (u, v) perpendicular to `n`. */
  private planeBasis(n: THREE.Vector3): { u: THREE.Vector3; v: THREE.Vector3 } {
    const u = Math.abs(n.x) > 0.9 ? new THREE.Vector3(0, 1, 0) : new THREE.Vector3(1, 0, 0);
    u.addScaledVector(n, -u.dot(n)).normalize();
    const v = n.clone().cross(u).normalize();
    return { u, v };
  }

  private commit(): void {
    if (this.points.length !== 2 || !this.plane) return;
    const c = this.points[0];
    const R = this.points[1].distanceTo(c);
    if (R < MIN_RADIUS_MM) {
      debugLog('[Loft] radius too small — skipped');
      return;
    }
    const n = this.plane.normal.clone().normalize();
    const { u, v } = this.planeBasis(n);
    const H = R * HEIGHT_PER_RADIUS;

    const sectionsFlat: number[] = [];
    for (const [hf, rs] of VASE_SECTIONS) {
      const center = c.clone().addScaledVector(n, H * hf);
      const r = R * rs;
      for (let i = 0; i < PROFILE_SEGMENTS; i++) {
        const a = (i / PROFILE_SEGMENTS) * TWO_PI;
        const cosA = Math.cos(a), sinA = Math.sin(a);
        sectionsFlat.push(
          center.x + r * (cosA * u.x + sinA * v.x),
          center.y + r * (cosA * u.y + sinA * v.y),
          center.z + r * (cosA * u.z + sinA * v.z),
        );
      }
    }
    const faces = this.ctx.bridge.loftSections(sectionsFlat, PROFILE_SEGMENTS, true);
    this.ctx.syncMesh();
    debugLog(`[Loft] ${VASE_SECTIONS.length} sections × ${PROFILE_SEGMENTS} pts (r${R.toFixed(1)}) → ${faces.length} faces`);
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    if (!this.plane) return;
    const c = this.points[0];
    const R = cursor.distanceTo(c);
    const n = this.plane.normal.clone().normalize();
    const { u, v } = this.planeBasis(n);
    const pts: THREE.Vector3[] = [];
    for (let i = 0; i <= PROFILE_SEGMENTS; i++) {
      const a = (i / PROFILE_SEGMENTS) * TWO_PI;
      pts.push(new THREE.Vector3(
        c.x + R * (Math.cos(a) * u.x + Math.sin(a) * v.x),
        c.y + R * (Math.cos(a) * u.y + Math.sin(a) * v.y),
        c.z + R * (Math.cos(a) * u.z + Math.sin(a) * v.z),
      ));
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
  //  Plane ray intersection (mirror DrawSweep / Pie)
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
