/**
 * Draw NURBS Surface Tool — create a bulged tensor-product patch face from a
 * 2-click rectangle (24-tool toolbar — NURBS surface).
 *
 * Flow:
 *   1st click → detect drawing plane + first corner A
 *   mouse move → ray ∩ plane → preview rectangle (the patch's flat boundary)
 *   2nd click → opposite corner C → build a 4×4 control grid + commit
 *   Escape    → cancel
 *
 * MVP scope: the 2-click rectangle defines the patch footprint on the draw
 * plane. A 4×4 (bicubic) control grid spans the rectangle; the 4 interior
 * control points are raised along the plane normal by a default bulge, giving
 * a smooth "pillow" dome with flat edges. The engine
 * (`bridge.createBezierPatch` → `Mesh::create_bezier_patch`) creates ONE
 * kernel-native face carrying the `AnalyticSurface::BezierPatch` — the render
 * pipeline (ADR-038 P23) tessellates the full bulged surface, and downstream
 * kernel-aware ops see the analytic patch (meta-principle #14).
 *
 * VCB: after the first click, type a number → a square patch of that side.
 *
 * A Bezier patch is the simplest, robust NURBS-class surface (uniform weights,
 * clamped knots implicit).
 *
 * ADR-231 — patch mode toggle (NurbsPatchSettings, default 'bezier'):
 *   'bezier' → the bicubic bulge above (createBezierPatch).
 *   'vault'  → an EXACT rational half-cylinder vault via createNurbsSurface
 *              (degree-2 rational arc × degree-1 extrude, weights [1,1/√2,1,
 *              1/√2,1]) — a true circular cross-section that a uniform Bezier
 *              cannot represent. Per-control-point weights / a draggable
 *              control net remain a future enhancement.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';
import { getNurbsPatchMode } from './NurbsPatchSettings';

/** Max distance from the first corner to prevent runaway geometry on grazing rays. */
const MAX_DRAW_DISTANCE = 50000;
/** Below this footprint (mm) the patch is too small to be meaningful. */
const MIN_SIZE = 1;
/** Interior-control-point rise, as a fraction of the smaller footprint side. */
const BULGE_FRACTION = 0.3;
/** Control grid resolution (bicubic = 4×4). */
const GRID_N = 4;

export class DrawNurbsTool implements ITool {
  readonly name = 'nurbs';

  private ctx: ToolContext;
  private cornerA: THREE.Vector3 | null = null;
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawNurbsTool] Activated — click two opposite corners for a NURBS patch');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.cornerA) {
      // ═══ First click: detect drawing plane + first corner ═══
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);
      this.cornerA = point.clone();

      // Cardinal ground plane → snap the normal-axis coordinate exactly to the
      // click's value (ADR-026 P12 spirit — absorb ray-plane ε on the floor).
      if (!this.plane.onFace) {
        const n = this.plane.normal;
        if (Math.abs(n.x) > 0.999) this.cornerA.x = point.x;
        else if (Math.abs(n.y) > 0.999) this.cornerA.y = point.y;
        else if (Math.abs(n.z) > 0.999) this.cornerA.z = point.z;
      }

      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        this.plane.normal, this.cornerA,
      );
      this.ctx.snap.setReferencePoint(this.cornerA);
    } else {
      // ═══ Second click: opposite corner → build + commit the patch ═══
      const cornerC = this.getPointOnDrawPlane(e);
      if (!cornerC || !this.plane) {
        this.cleanup();
        return;
      }
      this.commit(cornerC);
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.cornerA || !this.plane) {
      this.removePreview();
      return;
    }
    const cornerC = this.getPointOnDrawPlane(e);
    if (!cornerC) {
      this.removePreview();
      return;
    }
    this.updatePreview(cornerC);

    const { duLen, dvLen } = this.localExtent(cornerC);
    this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
      {
        from: this.cornerA.clone(),
        to: cornerC,
        text: `${this.ctx.units.format(Math.abs(duLen))} × ${this.ctx.units.format(Math.abs(dvLen))}`,
        color: '#63e6be',
      },
    ]);
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(value: number): void {
    // After the first click, type a number → a square patch of that side.
    if (!this.cornerA || !this.plane) return;
    if (!Number.isFinite(value) || value < MIN_SIZE) return;
    const r = this.plane.right;
    const u = this.plane.up;
    const cornerC = this.cornerA.clone()
      .addScaledVector(r, value)
      .addScaledVector(u, value);
    this.commit(cornerC);
    this.cleanup();
  }

  isBusy(): boolean {
    return this.cornerA !== null;
  }

  cleanup(): void {
    this.cornerA = null;
    this.plane = null;
    this.drawPlane3 = null;
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Control grid + commit
  // ═══════════════════════════════════════════════════

  /** Footprint extent of corner C relative to A in the plane's (right, up) basis. */
  private localExtent(cornerC: THREE.Vector3): { duLen: number; dvLen: number } {
    const d = cornerC.clone().sub(this.cornerA!);
    return { duLen: d.dot(this.plane!.right), dvLen: d.dot(this.plane!.up) };
  }

  private commit(cornerC: THREE.Vector3): void {
    if (!this.cornerA || !this.plane) return;
    const { duLen, dvLen } = this.localExtent(cornerC);
    if (Math.abs(duLen) < MIN_SIZE || Math.abs(dvLen) < MIN_SIZE) {
      debugLog('[NURBS] patch footprint too small — skipped');
      return;
    }

    // ADR-231 — mode dispatch: 'vault' = exact rational half-cylinder
    // (createNurbsSurface), 'bezier' = uniform bicubic bulge (default).
    if (getNurbsPatchMode() === 'vault') {
      const v = this.buildVaultGrid(duLen, dvLen);
      const faces = this.ctx.bridge.createNurbsSurface(
        v.controlPts, v.uCount, v.vCount, v.weights, v.uKnots, v.vKnots, v.degreeU, v.degreeV,
      );
      if (faces.length === 0) {
        debugLog('[NURBS] createNurbsSurface returned no face (engine rejected input)');
        return;
      }
      this.ctx.syncMesh();
      debugLog(
        `[NURBS] rational vault (half-cylinder) ${duLen.toFixed(1)}×${dvLen.toFixed(1)} → face ${faces[0]}`,
      );
      return;
    }

    const flat = this.buildControlGrid(duLen, dvLen);
    const faces = this.ctx.bridge.createBezierPatch(flat, GRID_N, GRID_N);
    if (faces.length === 0) {
      debugLog('[NURBS] createBezierPatch returned no face (engine rejected input)');
      return;
    }
    this.ctx.syncMesh();
    debugLog(
      `[NURBS] ${GRID_N}×${GRID_N} Bezier patch ${duLen.toFixed(1)}×${dvLen.toFixed(1)} → face ${faces[0]}`,
    );
  }

  /**
   * ADR-231 — exact rational half-cylinder vault control net. The footprint
   * WIDTH (along `right`, `duLen`) is the semicircle diameter (radius = |duLen|/2,
   * peak height = radius along `normal`); the LENGTH (along `up`, `dvLen`) is the
   * linear extrude. Cross-section = the canonical 2-span rational semicircle
   * (5 control points, weights [1, 1/√2, 1, 1/√2, 1], knots [0,0,0,.5,.5,1,1,1])
   * → an EXACT circular arc (not a Bezier approximation). 5×2 grid, degree (2,1).
   */
  private buildVaultGrid(duLen: number, dvLen: number): {
    controlPts: number[]; weights: number[];
    uCount: number; vCount: number; uKnots: number[]; vKnots: number[];
    degreeU: number; degreeV: number;
  } {
    const a = this.cornerA!;
    const r3 = this.plane!.right;
    const u3 = this.plane!.up;
    const n3 = this.plane!.normal;
    const r = Math.abs(duLen) / 2;     // semicircle radius = half the width; peak = r
    const w = Math.SQRT1_2;            // 1/√2 — 90° rational-arc middle weight
    // 5-CP semicircle in (right, normal): arcs UP along `normal` by radius r,
    // diameter along `right` tracking the signed footprint width.
    const arc = [
      { x: 0,         z: 0, wt: 1 }, // near edge (corner A)
      { x: 0,         z: r, wt: w }, // corner CP
      { x: duLen / 2, z: r, wt: 1 }, // peak
      { x: duLen,     z: r, wt: w }, // corner CP
      { x: duLen,     z: 0, wt: 1 }, // far edge
    ];
    const controlPts: number[] = [];
    const weights: number[] = [];
    for (const ap of arc) {
      for (let j = 0; j < 2; j++) {
        const vlen = j === 0 ? 0 : dvLen;
        const p = a.clone()
          .addScaledVector(r3, ap.x)
          .addScaledVector(n3, ap.z)
          .addScaledVector(u3, vlen);
        controlPts.push(p.x, p.y, p.z);
        weights.push(ap.wt);
      }
    }
    return {
      controlPts, weights,
      uCount: 5, vCount: 2,
      uKnots: [0, 0, 0, 0.5, 0.5, 1, 1, 1],
      vKnots: [0, 0, 1, 1],
      degreeU: 2, degreeV: 1,
    };
  }

  /**
   * 4×4 row-major control grid spanning the footprint, with the 4 interior
   * control points raised along the plane normal (boundary stays flat → flat
   * edges, bulged center). Returned as a flat `[x,y,z, …]` array.
   */
  private buildControlGrid(duLen: number, dvLen: number): number[] {
    const a = this.cornerA!;
    const r = this.plane!.right;
    const u = this.plane!.up;
    const n = this.plane!.normal;
    const bulge = Math.min(Math.abs(duLen), Math.abs(dvLen)) * BULGE_FRACTION;

    const flat: number[] = [];
    for (let i = 0; i < GRID_N; i++) {
      for (let j = 0; j < GRID_N; j++) {
        const s = i / (GRID_N - 1);
        const t = j / (GRID_N - 1);
        const interior = i > 0 && i < GRID_N - 1 && j > 0 && j < GRID_N - 1;
        const h = interior ? bulge : 0;
        const p = a.clone()
          .addScaledVector(r, s * duLen)
          .addScaledVector(u, t * dvLen)
          .addScaledVector(n, h);
        flat.push(p.x, p.y, p.z);
      }
    }
    return flat;
  }

  // ═══════════════════════════════════════════════════
  //  Preview (flat rectangle footprint on the draw plane)
  // ═══════════════════════════════════════════════════

  private updatePreview(cornerC: THREE.Vector3): void {
    this.removePreview();
    if (!this.cornerA || !this.plane) return;
    const { duLen, dvLen } = this.localExtent(cornerC);
    const a = this.cornerA;
    const r = this.plane.right;
    const u = this.plane.up;
    const b = a.clone().addScaledVector(r, duLen);
    const c = a.clone().addScaledVector(r, duLen).addScaledVector(u, dvLen);
    const d = a.clone().addScaledVector(u, dvLen);
    const pts = [a.clone(), b, c, d, a.clone()];
    const geo = new THREE.BufferGeometry().setFromPoints(pts);
    const mat = new THREE.LineBasicMaterial({ color: 0x63e6be, linewidth: 2 });
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
  //  Drawing-plane ray intersection (mirror DrawCircleTool)
  // ═══════════════════════════════════════════════════

  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3 || !this.cornerA) return null;
    const rawPt = this.ctx.get3DPoint(e);
    const snapped = this.ctx.getSnappedPoint(e, rawPt);
    let result: THREE.Vector3 | null = null;
    if (snapped) {
      const projected = snapped.clone();
      const dist = this.drawPlane3.distanceToPoint(projected);
      projected.addScaledVector(this.drawPlane3.normal, -dist);
      result = projected;
    } else {
      const ray = this.ctx.getRay(e);
      const target = new THREE.Vector3();
      const hit = ray.ray.intersectPlane(this.drawPlane3, target);
      if (!hit) return null;
      if (target.distanceTo(this.cornerA) > MAX_DRAW_DISTANCE) return null;
      result = target;
    }
    if (!result) return null;
    // Floor cardinal plane → pin the normal-axis coordinate to corner A's.
    if (this.plane && !this.plane.onFace) {
      const n = this.plane.normal;
      if (Math.abs(n.x) > 0.999) result.x = this.cornerA.x;
      else if (Math.abs(n.y) > 0.999) result.y = this.cornerA.y;
      else if (Math.abs(n.z) > 0.999) result.z = this.cornerA.z;
    }
    return result;
  }
}
