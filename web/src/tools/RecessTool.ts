/**
 * RecessTool — 3D pocket recess (offset inset + inward push).
 *
 * Workflow: click a solid face → enter "여유(inset), 깊이(depth)" in the VCB
 * (e.g. `20 100`) → the face boundary is inset by `inset` and the inner region
 * is pushed into the solid by `depth`, forming a pocket (floor + walls) with a
 * coplanar ring left flush at the surface. Delegates to
 * `bridge.createRecess(faceId, inset, depth)` which is guarded by the engine's
 * closure-preserving + self-intersection gate.
 *
 * Modelled on OffsetTool (single-value face op); RecessTool needs TWO values,
 * routed through `applyVCBValue(inset, depth)`.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { debugLog } from '../utils/debug';
import { Toast } from '../ui/Toast';

export class RecessTool implements ITool {
  readonly name = 'recess';

  private ctx: ToolContext;
  private phase: 0 | 1 = 0; // 0 = awaiting face pick, 1 = face picked, awaiting VCB
  private faceId: number = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    // Pre-pick a single already-selected face so the user can go straight to
    // entering values (mirrors the ergonomics of the transform/offset tools).
    const selected = this.ctx.getSelectedFaces();
    if (selected.length === 1) {
      this.faceId = selected[0];
      this.phase = 1;
      this.promptValues();
      debugLog('[RecessTool] Activated with pre-selected face', this.faceId);
    } else {
      this.phase = 0;
      this.faceId = -1;
      Toast.info('홈파기: 면을 클릭하세요.', 2000);
      debugLog('[RecessTool] Activated; awaiting face pick');
    }
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (this.phase !== 0) {
      // Already have a face — wait for VCB input (or ESC / re-pick below).
      const picked = this.pickFace(_e);
      if (picked >= 0 && picked !== this.faceId) {
        this.faceId = picked;
        this.ctx.selection.handleClick(picked, false, false);
        this.promptValues();
        debugLog('[Recess] Re-picked face', picked);
      }
      return;
    }
    const picked = this.pickFace(_e);
    if (picked >= 0) {
      this.faceId = picked;
      this.phase = 1;
      this.ctx.selection.handleClick(picked, false, false);
      this.promptValues();
      debugLog('[Recess] Phase 1: faceId=', picked);
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
      Toast.info('홈파기 취소됨', 1500);
    }
  }

  /** VCB delivers (inset, depth). Both required and positive. */
  applyVCBValue(inset: number, depth?: number): void {
    if (this.phase !== 1 || this.faceId < 0) {
      Toast.warning('먼저 면을 클릭하세요.', 2000);
      return;
    }
    if (depth === undefined || !Number.isFinite(inset) || !Number.isFinite(depth)) {
      Toast.warning('홈파기는 두 값이 필요합니다 — "여유 깊이" (예: 20 100).', 3000);
      return;
    }
    if (inset <= 0 || depth <= 0) {
      Toast.warning('여유(inset)와 깊이(depth)는 0보다 커야 합니다.', 3000);
      return;
    }

    const result = this.ctx.bridge.createRecess(this.faceId, inset, depth);
    if (result && result.ok) {
      debugLog(
        '[Recess] Applied: inset=', inset, 'depth=', depth,
        'pocketFace=', result.pocketFace, 'walls=', result.wallFaces?.length,
      );
      this.ctx.syncMesh();
    }
    // On failure the bridge already surfaced a Toast (fail-loud).
    this.ctx.dimLabel.clear();
    this.cleanup();
  }

  isBusy(): boolean {
    return this.phase > 0;
  }

  cleanup(): void {
    this.phase = 0;
    this.faceId = -1;
    this.ctx.selection.clearSelection();
    this.ctx.dimLabel.clear();
  }

  // ── helpers ──────────────────────────────────────────────────────────

  private pickFace(e: MouseEvent): number {
    const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
    if (hit && hit.faceIndex != null && hit.faceIndex >= 0) {
      return this.ctx.getFaceId(hit.faceIndex);
    }
    const selected = this.ctx.getSelectedFaces();
    return selected.length === 1 ? selected[0] : -1;
  }

  private promptValues(): void {
    Toast.info('홈파기: VCB에 "여유 깊이" 입력 (예: 20 100). ESC 로 취소.', 3000);
  }
}
