/**
 * Draw Sweep Tool — sweep a circular profile along a drawn path (pipe / tube).
 * (24-tool toolbar — Sweep, 2026-06-08)
 *
 * Flow:
 *   1st click           → path P0 + plane detect + lock (ADR-166)
 *   2nd+ click          → add a path point
 *   double-click / Enter → finish the path → sweep (≥ 2 points)
 *   Escape              → cancel
 *
 * Profile = a circle (radius default 5 mm, VCB-adjustable: type a number) in
 * its local XY plane. The engine (`Mesh::sweep`) orients the profile
 * perpendicular to the path tangent at each path point (`up_ref` fallback
 * handles tangents parallel to world-up), producing a circular tube. The path
 * is drawn on the first-click plane (z = 0 ground by default — LOCKED #63 /
 * ADR-175).
 *
 * Engine API: `bridge.sweepProfileAlongPath(profileFlat, pathFlat, closed=true)`
 * → `Mesh::sweep` (operations/sweep.rs), single undo transaction.
 *
 * MVP scope: planar path + circular profile. Arbitrary profile (extracted from
 * a selected face) and a true 3D path are natural future enhancements.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';

const DEFAULT_RADIUS_MM = 5;
const MIN_RADIUS_MM = 0.1;
const PROFILE_SEGMENTS = 24;
const TWO_PI = Math.PI * 2;

export class DrawSweepTool implements ITool {
  readonly name = 'sweep';

  private ctx: ToolContext;
  private path: THREE.Vector3[] = [];
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;
  private radius = DEFAULT_RADIUS_MM;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawSweepTool] Activated — click path points, Enter / double-click to sweep');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    // Double-click finishes the path. The first click of the pair already
    // pushed its point, so just commit here (no duplicate point).
    if (e.detail >= 2) {
      if (this.path.length >= 2) {
        this.commit();
        this.cleanup();
      }
      return;
    }

    if (this.path.length === 0) {
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(this.plane.normal, point);
      this.path.push(point.clone());
      this.ctx.snap.setReferencePoint(point);
      this.ctx.lockPlane?.({
        origin: point, normal: this.plane.normal, up: this.plane.up, source: 'first_click',
      });
      return;
    }

    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    // Skip a coincident point (accidental repeat / double-click first hit).
    const last = this.path[this.path.length - 1];
    if (p.distanceTo(last) < MIN_RADIUS_MM) return;
    this.path.push(p.clone());
    this.ctx.snap.setReferencePoint(p);
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.path.length === 0 || !this.plane) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    this.updatePreview(p);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
      return;
    }
    if (e.key === 'Enter') {
      if (this.path.length >= 2) {
        this.commit();
        this.cleanup();
      }
    }
  }

  applyVCBValue(value: number): void {
    // Type a number → set the profile radius (mm).
    if (Number.isFinite(value) && value >= MIN_RADIUS_MM) {
      this.radius = value;
      debugLog(`[Sweep] profile radius = ${value} mm`);
    }
  }

  isBusy(): boolean {
    return this.path.length > 0;
  }

  cleanup(): void {
    this.path = [];
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Profile + commit
  // ═══════════════════════════════════════════════════

  /** Circular cross-section in the profile's local XY plane (z = 0). */
  private circleProfile(): number[] {
    const r = Math.max(MIN_RADIUS_MM, this.radius);
    const out: number[] = [];
    for (let i = 0; i < PROFILE_SEGMENTS; i++) {
      const a = (i / PROFILE_SEGMENTS) * TWO_PI;
      out.push(r * Math.cos(a), r * Math.sin(a), 0);
    }
    return out;
  }

  private commit(): void {
    if (this.path.length < 2) return;
    const pathFlat: number[] = [];
    for (const p of this.path) pathFlat.push(p.x, p.y, p.z);
    const profileFlat = this.circleProfile();
    const faces = this.ctx.bridge.sweepProfileAlongPath(profileFlat, pathFlat, true);
    this.ctx.syncMesh();
    debugLog(
      `[Sweep] ${this.path.length}-pt path × r${this.radius} circle → ${faces.length} faces`,
    );
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(cursor: THREE.Vector3): void {
    this.removePreview();
    const pts = [...this.path, cursor];
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
  //  Plane ray intersection (mirror DrawPie / Spline / RotRect)
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
