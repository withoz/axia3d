/**
 * Draw Freehand Tool — 마우스 드래그로 곡선 그리기 (Phase I3, 2026-04-20).
 *
 * Flow:
 *   mousedown → plane detect + start collecting points
 *   mousemove (drag) → append point + preview
 *   mouseup → RDP simplify → Catmull-Rom smoothing → tessellate → DCEL
 *
 * SketchUp "자유손 (Freehand)" 도구 대응.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';
import { freehandFromPoints, tessellateCurve } from '../curves/Curve';
import { getCurveRegistry } from '../curves/CurveRegistry';

/** 연속 점 사이 최소 거리 (mm) — 너무 촘촘한 샘플링 방지 */
const MIN_SAMPLE_DISTANCE = 0.5;

export class DrawFreehandTool implements ITool {
  readonly name = 'freehand';

  private ctx: ToolContext;
  private drawing = false;
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null;
  private rawPoints: THREE.Vector3[] = [];
  private previewLine: THREE.Line | null = null;
  // ADR-284 β-3 — curved-surface draw: a CLOSED freehand loop ON a curved face.
  private curvedKind: 'cylinder' | 'cone' | 'torus' | 'sphere' | null = null;
  private curvedHostFace = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawFreehandTool] Activated — 드래그로 선 그리기, 놓으면 smoothing');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.drawing || !point) return;
    this.plane = this.ctx.getDrawPlane(e);
    this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
      this.plane.normal, point,
    );
    this.drawing = true;
    this.rawPoints = [point.clone()];
    // ADR-284 β-3 — first click on a curved face → a CLOSED freehand loop will
    // be projected + split onto the surface. Capture host + kind.
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
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.drawing) return;
    const p = this.getPointOnDrawPlane(e);
    if (!p) return;
    // 너무 가까운 연속 점은 건너뜀
    const last = this.rawPoints[this.rawPoints.length - 1];
    if (last && p.distanceTo(last) < MIN_SAMPLE_DISTANCE) return;
    this.rawPoints.push(p);
    this.updatePreview();
  }

  onMouseUp(_e: MouseEvent): void {
    if (!this.drawing) return;
    this.commitFreehand();
    this.cleanup();
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    }
  }

  applyVCBValue(_value: number): void {
    // Freehand는 VCB 입력 없음
  }

  isBusy(): boolean {
    return this.drawing;
  }

  cleanup(): void {
    this.drawing = false;
    this.plane = null;
    this.drawPlane3 = null;
    this.rawPoints = [];
    this.curvedKind = null;
    this.curvedHostFace = -1;
    this.removePreview();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Commit: raw points → curve → tessellation → DCEL
  // ═══════════════════════════════════════════════════

  private commitFreehand(): void {
    if (this.rawPoints.length < 2) {
      debugLog('[Freehand] too few points');
      return;
    }

    // Curve 생성 (자동 RDP simplify + Catmull-Rom smoothing)
    const curve = freehandFromPoints(
      this.rawPoints,
      /* simplifyTolerance */ 1.0,
      /* segments */ 0,
      /* closed */ false,
    );
    getCurveRegistry().add(curve);

    // ADR-012 §3 BatchCommand — N 회 crossing 대신 1 회.
    //   퇴화 방지 필터 후 평탄화 배열로 한 번에 전송.
    const pts = tessellateCurve(curve);
    const filtered: Array<{ x: number; y: number; z: number }> = [];
    for (const p of pts) {
      if (filtered.length === 0 ||
          p.distanceTo(filtered[filtered.length - 1] as any) >= 0.1) {
        filtered.push(p);
      }
    }
    // ADR-284 β-3 — curved-surface path: a CLOSED freehand loop on a curved
    // face is projected + split (cap + remainder). "Closed" = the stroke's ends
    // are near each other (≤ 20% of its bbox diagonal). Open strokes on a curved
    // face fall through (planar wire; on-surface open lines are β-4).
    if (this.curvedKind && this.curvedHostFace >= 0 && filtered.length >= 3) {
      const first = this.rawPoints[0];
      const lastRaw = this.rawPoints[this.rawPoints.length - 1];
      const bb = new THREE.Box3().setFromPoints(this.rawPoints);
      const diag = bb.getSize(new THREE.Vector3()).length();
      const closed = first.distanceTo(lastRaw) <= Math.max(diag * 0.2, 1);
      const pts: Array<[number, number, number]> = filtered.map((p) => [p.x, p.y, p.z]);
      if (closed && typeof this.ctx.bridge.drawPolylineOnCurved === 'function') {
        const res = this.ctx.bridge.drawPolylineOnCurved(this.curvedKind, this.curvedHostFace, pts, true);
        if (!res || res.includes('"error"')) {
          // eslint-disable-next-line no-console
          console.warn(`[Freehand] curved split on ${this.curvedKind} failed: ${res}`);
        } else {
          debugLog(`[Freehand] curved closed loop split on ${this.curvedKind} host=${this.curvedHostFace}`);
        }
        this.ctx.syncMesh();
        return;
      }
      // ADR-284 β-4-3/β-4-4 — OPEN stroke on a Sphere OR Cone face → rim-to-rim
      // seam split (both are self-loop rim faces with a rim-sharing twin).
      // (Cylinder/torus open = multi-rim, deferred; those fall through to a
      // planar wire. A straight 2-click line is degenerate — ADR-284 §β-4-1 —
      // so this is the freehand/bezier path.)
      if (!closed && (this.curvedKind === 'sphere' || this.curvedKind === 'cone')
          && typeof this.ctx.bridge.drawOpenSeamOnCurved === 'function') {
        const res = this.ctx.bridge.drawOpenSeamOnCurved(this.curvedHostFace, pts);
        if (!res || res.includes('"error"')) {
          // eslint-disable-next-line no-console
          console.warn(`[Freehand] open seam on ${this.curvedKind} failed: ${res}`);
        } else {
          debugLog(`[Freehand] open seam split on ${this.curvedKind} host=${this.curvedHostFace}`);
        }
        this.ctx.syncMesh();
        return;
      }
    }

    let edgeCount = 0;
    if (filtered.length >= 2) {
      const flat = new Float64Array(filtered.length * 3);
      for (let i = 0; i < filtered.length; i++) {
        flat[i * 3]     = filtered[i].x;
        flat[i * 3 + 1] = filtered[i].y;
        flat[i * 3 + 2] = filtered[i].z;
      }
      // ADR-087 K-ε — kernel-aware drawPolylineAsShape only path. 닫힌
      // loop 합성 시 face 에 Plane 자동 attach (plane.normal hint).
      const n = this.plane?.normal;
      this.ctx.bridge.drawPolylineAsShape(
        flat,
        n ? { x: n.x, y: n.y, z: n.z } : undefined,
      );
      edgeCount = filtered.length - 1;

      // ADR-164 β-2 — Sticky last drawn plane (drawPolylineAsShape 호출
      // 후 — closed loop 시 face 합성, open 시 wire 만. Q1=a 정합 위해
      // plane 정보가 있을 때만 저장. Open vs closed branch 구분은 engine
      // 책임, 본 hook 은 plane intent 만 기록).
      if (this.plane) {
        this.ctx.setLastDrawnPlane?.({
          origin: filtered[0] as THREE.Vector3,
          normal: this.plane.normal,
          up: this.plane.up,
          source: 'view',
        });
      }
    }

    this.ctx.syncMesh();
    debugLog(
      `[Freehand] raw=${this.rawPoints.length} tessellated=${pts.length} edges=${edgeCount}`
    );
  }

  // ═══════════════════════════════════════════════════
  //  Preview
  // ═══════════════════════════════════════════════════

  private updatePreview(): void {
    this.removePreview();
    if (this.rawPoints.length < 2) return;
    const geo = new THREE.BufferGeometry().setFromPoints(this.rawPoints);
    const mat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 2 });
    this.previewLine = new THREE.Line(geo, mat);
    this.previewLine.renderOrder = 999;
    this.ctx.viewport.scene.add(this.previewLine);
  }

  private removePreview(): void {
    if (this.previewLine) {
      this.ctx.viewport.scene.remove(this.previewLine);
      (this.previewLine.geometry as THREE.BufferGeometry).dispose();
      (this.previewLine.material as THREE.Material).dispose();
      this.previewLine = null;
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
