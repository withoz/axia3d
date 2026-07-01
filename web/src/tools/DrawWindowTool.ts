/**
 * Draw Window Tool — punch an axis-aligned rectangular opening (a window) into
 * an existing coplanar face (e.g. a wall). Reuses the atomic stable-id punch
 * path (`bridge.punchRectHole`).
 * (24-tool toolbar — Window, 2026-06-09)
 *
 * Flow (a constrained, rectangular variant of DrawHoleTool):
 *   1st click → must land ON a face → capture corner A + the face plane/normal
 *   mouse move → ray ∩ face plane → preview the rectangle (A → cursor)
 *   2nd click → corner B → `bridge.punchRectHole(A, B, normal)` → ring-with-hole
 *   Escape → cancel
 *
 * Like DrawHoleTool the host face is resolved fresh by the engine at commit time
 * (no stale face id). The opening is the bounding box of the two corners in the
 * host face's in-plane basis — the same `basis(n)` is replicated here so the
 * preview matches the actual punch.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const MAX_DRAW_DISTANCE = 50000;
const WINDOW_COLOR = 0x4dabf7;
const MIN_SIDE_MM = 1;

export class DrawWindowTool implements ITool {
  readonly name = 'window';

  private ctx: ToolContext;
  private cornerA: THREE.Vector3 | null = null;
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private preview: THREE.Line | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawWindowTool] Activated');
    Toast.info('개구부(창/문)를 낼 면 위를 클릭 — 바닥까지 끌면 자동으로 문', 3000);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.cornerA) {
      // First click must be ON a face (a window needs a host wall).
      if (!point) return;
      const plane = this.ctx.getDrawPlane(e);
      if (!plane.onFace) {
        Toast.warning('창은 기존 면 위에 내야 합니다 — 면을 클릭하세요');
        return;
      }
      this.plane = plane;
      this.cornerA = point.clone();
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        plane.normal, this.cornerA,
      );
      this.ctx.snap.setReferencePoint(point);
    } else {
      const planePoint = this.getPointOnDrawPlane(e);
      if (!planePoint || !this.plane) {
        this.cleanup();
        return;
      }
      this.commitWindow(planePoint);
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.cornerA || !this.plane) {
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
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(_value: number): void {
    // Window is defined by two free clicks (no numeric entry in this MVP).
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
  //  Rectangle (engine basis replica) + commit
  // ═══════════════════════════════════════════════════

  /** In-plane basis (e1, e2) — mirrors `Mesh::punch_rect_hole`'s `basis(n)`. */
  private faceBasis(n: THREE.Vector3): { e1: THREE.Vector3; e2: THREE.Vector3 } {
    let t = new THREE.Vector3(1, 0, 0);
    if (t.clone().cross(n).lengthSq() < 1e-6) t = new THREE.Vector3(0, 1, 0);
    const e1 = t.clone().addScaledVector(n, -t.dot(n)).normalize();
    const e2 = n.clone().cross(e1).normalize();
    return { e1, e2 };
  }

  /** World corners of the axis-aligned (to the face basis) rect through A and B. */
  private rectCorners(a: THREE.Vector3, b: THREE.Vector3): THREE.Vector3[] | null {
    if (!this.plane) return null;
    const n = this.plane.normal.clone().normalize();
    const { e1, e2 } = this.faceBasis(n);
    const au = a.dot(e1), av = a.dot(e2);
    const bu = b.dot(e1), bv = b.dot(e2);
    const umin = Math.min(au, bu), umax = Math.max(au, bu);
    const vmin = Math.min(av, bv), vmax = Math.max(av, bv);
    if (umax - umin < MIN_SIDE_MM || vmax - vmin < MIN_SIDE_MM) return null;
    // A sits on the face plane: A = e1·au + e2·av + n·(A·n). Reuse n·(A·n) offset.
    const off = n.clone().multiplyScalar(a.dot(n));
    const mk = (u: number, v: number) =>
      e1.clone().multiplyScalar(u).addScaledVector(e2, v).add(off);
    return [mk(umin, vmin), mk(umax, vmin), mk(umax, vmax), mk(umin, vmax)];
  }

  private commitWindow(b: THREE.Vector3): void {
    if (!this.cornerA || !this.plane) return;
    const a = this.cornerA;
    if (!this.rectCorners(a, b)) {
      Toast.warning('개구부가 너무 작습니다 — 더 크게 끌어 주세요');
      return;
    }
    const n = this.plane.normal;

    // 0) DOOR (floor-reaching notch) — ADR-262 β-3. Try first: the engine
    //    auto-detects a door (opening bottom in the lower 15% of the wall) and
    //    snaps it to the wall floor → a U-notch (open bottom). A higher opening
    //    (a window) is rejected EARLY (no mutation; the WASM wrapper restores —
    //    ADR-190 P0.2) returning ≤ 0, so we fall through to the window path on a
    //    clean mesh. Same graceful fall-through for a non-vertical face / sheet /
    //    missing export.
    const jambs = this.ctx.bridge.cutWallDoorOpening(
      [a.x, a.y, a.z],
      [b.x, b.y, b.z],
      [n.x, n.y, n.z],
    );
    if (jambs > 0) {
      debugLog(`[Window] Cut door notch → ${jambs} jambs`);
      Toast.success('문(door)을 냈습니다');
      this.ctx.syncMesh();
      return;
    }

    // 1) Through-window (solid) — ADR-249. Succeeds on a closed solid (the
    //    −normal ray reaches an anti-parallel opposite wall); returns the tube-
    //    quad count (> 0). On a single sheet face there is no opposite wall →
    //    `drillRectThroughHole` rolls back its own snapshot (ADR-190 P0.2) and
    //    returns −1, so we fall back to a 2D face window on the clean mesh. The
    //    same fallback covers an engine without the export (graceful −1).
    const tube = this.ctx.bridge.drillRectThroughHole(
      [a.x, a.y, a.z],
      [b.x, b.y, b.z],
      [n.x, n.y, n.z],
    );
    if (tube > 0) {
      debugLog(`[Window] Drilled rect through → ${tube} tube quads`);
      Toast.success('관통 창을 냈습니다');
      this.ctx.syncMesh();
      return;
    }

    // 2) Fallback — 2D face window (ring-with-hole on the single host face).
    const faceId = this.ctx.bridge.punchRectHole(
      [a.x, a.y, a.z],
      [b.x, b.y, b.z],
      [n.x, n.y, n.z],
    );
    if (faceId < 0) {
      Toast.fromBridgeError(
        this.ctx.bridge,
        '창 내기 실패 — 면 경계 안에서 다시 시도하세요',
      );
      return;
    }
    debugLog(`[Window] Punched rect on face → ring face ${faceId}`);
    Toast.success('창을 냈습니다');
    this.ctx.syncMesh();
  }

  // ═══════════════════════════════════════════════════
  //  Preview + plane ray intersection (mirrors DrawHoleTool)
  // ═══════════════════════════════════════════════════

  private updatePreview(b: THREE.Vector3): void {
    this.removePreview();
    if (!this.cornerA || !this.plane) return;
    const corners = this.rectCorners(this.cornerA, b);
    if (!corners) return;
    const n = this.plane.normal;
    const pts = corners.map((p) => p.clone().addScaledVector(n, 0.5)); // lift to avoid z-fight
    pts.push(pts[0].clone());
    const geo = new THREE.BufferGeometry().setFromPoints(pts);
    const mat = new THREE.LineBasicMaterial({ color: WINDOW_COLOR, linewidth: 2 });
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
    if (!this.drawPlane3 || !this.cornerA) return null;
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
    if (target.distanceTo(this.cornerA) > MAX_DRAW_DISTANCE) return null;
    return target;
  }
}
