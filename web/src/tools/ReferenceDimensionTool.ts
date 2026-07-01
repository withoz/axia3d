/**
 * Reference Dimension Tool — create a persistent, READ-ONLY dimension (ADR-218).
 *
 * A reference dimension MEASURES but never drives: it is the same engine
 * constraint kind (Distance / Angle / Radius) carrying `value = None`, so the
 * solver ignores it. The DimensionManager shows it parenthesised and
 * non-editable (CAD convention).
 *
 * A SINGLE tool dispatches by what the first click hits (Option A):
 *   • a vertex (tight pick)        → reference LINEAR  (wait for a 2nd vertex)
 *   • a Circle/Arc edge            → reference RADIAL  (instant)
 *   • a straight edge              → reference ANGULAR (wait for a 2nd edge)
 *
 * Reuses addReferenceDistance/Angle/Radius (Pattern-12) — no new geometry kernel.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { pickClickedEdge } from './edgePick';

const PICK_TOL = 2.0; // mm — snapped point must sit on a vertex for LINEAR mode

function dirOf(p0: [number, number, number], p1: [number, number, number]): [number, number, number] {
  const d: [number, number, number] = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
  const len = Math.hypot(d[0], d[1], d[2]) || 1;
  return [d[0] / len, d[1] / len, d[2] / len];
}

export class ReferenceDimensionTool implements ITool {
  readonly name = 'reference-dimension';
  readonly wantsSnap = true;

  private ctx: ToolContext;
  private mode: 'idle' | 'linear' | 'angular' = 'idle';
  private v1 = -1;
  private edge1 = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[ReferenceDimensionTool] Activated');
    Toast.info('참조 치수(읽기전용): 정점→정점 / 원·호 / 엣지→엣지 클릭', 3800);
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (this.mode === 'linear') { this.finishLinear(e, point); return; }
    if (this.mode === 'angular') { this.finishAngular(e); return; }

    // idle → dispatch on what's under the cursor. Vertex (tight) wins so that
    // clicking precisely on a corner starts a linear measure; clicking an edge
    // body goes radial/angular.
    const vid = this.pickVertex(e, point);
    if (vid >= 0) {
      this.mode = 'linear';
      this.v1 = vid;
      Toast.info('둘째 정점을 클릭하세요 (Esc 취소)', 2500);
      return;
    }

    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) {
      Toast.warning('정점·원/호·엣지 위를 클릭하세요', 2000);
      return;
    }
    const radius = this.ctx.bridge.edgeCurveRadius(picked.edgeId);
    if (radius > 0) {
      // Circle/Arc → instant reference radial.
      const verts = this.ctx.bridge.getEdgeEndpoints(picked.edgeId);
      if (verts.length < 1) return;
      const id = this.ctx.bridge.addReferenceRadius(verts[0]);
      this.report(id, `참조 반지름 (R${this.ctx.units.format(radius)})`);
      return;
    }
    // Straight edge → start reference angular.
    this.mode = 'angular';
    this.edge1 = picked.edgeId;
    Toast.info('둘째 엣지를 클릭하세요 (Esc 취소)', 2500);
  }

  private finishLinear(e: MouseEvent, point: THREE.Vector3 | null): void {
    const vid = this.pickVertex(e, point);
    if (vid < 0) { Toast.warning('치수를 잴 정점 위를 클릭하세요', 2000); return; }
    if (vid === this.v1) { Toast.warning('서로 다른 정점을 선택하세요', 2000); return; }
    const pa = this.ctx.bridge.getVertexPos(this.v1);
    const pb = this.ctx.bridge.getVertexPos(vid);
    if (!pa || !pb) { this.cleanup(); return; }
    const dist = Math.hypot(pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]);
    if (dist < 1e-6) { Toast.warning('두 정점이 같은 위치입니다', 2000); this.cleanup(); return; }
    const id = this.ctx.bridge.addReferenceDistance(this.v1, vid);
    this.report(id, `참조 거리 (${this.ctx.units.format(dist)})`);
    this.cleanup();
  }

  private finishAngular(e: MouseEvent): void {
    const picked = pickClickedEdge(this.ctx, e);
    if (!picked) { Toast.warning('둘째 엣지를 클릭하세요', 2000); return; }
    if (picked.edgeId === this.edge1) { Toast.warning('서로 다른 엣지를 선택하세요', 2000); return; }
    const eA = this.ctx.bridge.getEdgeEndpoints(this.edge1);
    const eB = this.ctx.bridge.getEdgeEndpoints(picked.edgeId);
    if (eA.length < 2 || eB.length < 2) { this.cleanup(); return; }
    const pa0 = this.ctx.bridge.getVertexPos(eA[0]); const pa1 = this.ctx.bridge.getVertexPos(eA[1]);
    const pb0 = this.ctx.bridge.getVertexPos(eB[0]); const pb1 = this.ctx.bridge.getVertexPos(eB[1]);
    if (!pa0 || !pa1 || !pb0 || !pb1) { this.cleanup(); return; }
    const dA = dirOf(pa0, pa1); const dB = dirOf(pb0, pb1);
    const dot = Math.max(-1, Math.min(1, dA[0] * dB[0] + dA[1] * dB[1] + dA[2] * dB[2]));
    const angle = Math.acos(dot);
    if (angle < 1e-3 || angle > Math.PI - 1e-3) {
      Toast.warning('평행/일직선 엣지는 각도 치수 불가', 2200); this.cleanup(); return;
    }
    const id = this.ctx.bridge.addReferenceAngle(eA[0], eA[1], eB[0], eB[1]);
    this.report(id, `참조 각도 (${((angle * 180) / Math.PI).toFixed(1)}°)`);
    this.cleanup();
  }

  /** Snap the click and resolve it to a vertex id (tight), or -1. */
  private pickVertex(e: MouseEvent, point: THREE.Vector3 | null): number {
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint(e, raw) ?? raw ?? point;
    if (!pt) return -1;
    return this.ctx.bridge.findVertexIdAt?.(pt.x, pt.y, pt.z, PICK_TOL) ?? -1;
  }

  private report(id: number, label: string): void {
    if (id > 0) {
      this.ctx.syncMesh();
      Toast.info(`${label} — 읽기전용`, 2500);
      debugLog(`[ReferenceDimension] constraint ${id}: ${label}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '참조 치수 생성 실패');
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  isBusy(): boolean {
    return this.mode !== 'idle';
  }

  cleanup(): void {
    this.mode = 'idle';
    this.v1 = -1;
    this.edge1 = -1;
  }
}
