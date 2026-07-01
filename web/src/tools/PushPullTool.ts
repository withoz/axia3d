/**
 * Push/Pull Tool — SketchUp style extrude (click → move → click)
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { debugLog, debugWarn } from '../utils/debug';
import { Toast } from '../ui/Toast';
import { getExtrudeMode, getExtrudeDistNeg } from './ExtrudeModeSettings';

export class PushPullTool implements ITool {
  readonly name = 'pushpull';

  private ctx: ToolContext;
  private ppFaceId: number = -1;
  private ppStartX: number = 0;
  private ppStartY: number = 0;
  private ppActive: boolean = false;
  private ppNormal: THREE.Vector3 = new THREE.Vector3(0, 1, 0);
  private ppGhost: THREE.Group | null = null;
  private ppHitPoint: THREE.Vector3 = new THREE.Vector3();
  private ppFaceVerts: THREE.Vector3[] = [];
  /** smooth group 전체의 face별 boundary (고스트 프리뷰에서 모든 면 표시용) */
  private ppAllFaceVerts: THREE.Vector3[][] = [];
  private lastPPDist: number = 0;
  /** align-to-geometry 발동 시 저장되는 현재 드래그 거리 (Phase 2 클릭 commit용) */
  private currentDragDist: number = 0;

  // ═══ ADR-193 — Live Push/Pull (direct manipulation) ═══
  // Single planar face uses a live engine session (real geometry extrudes as
  // you move — no ghost). Smooth groups keep the legacy ghost path (curved /
  // multi-face migration is a follow-up). `liveActive` is true once the engine
  // session has begun (on the first move past MIN_COMMIT_DIST).
  private liveActive: boolean = false;
  /** Top FaceId of the live preview (for reference; -1 when no session). */
  private liveTopFace: number = -1;
  /** A failed beginLiveExtrude is not retried every move (avoid error spam). */
  private liveBeginFailed: boolean = false;
  /** ADR-252 — the picked face is a coplanar Shape sheet (not part of a volume,
   *  e.g. a rect drawn on a wall). An INWARD push carves a blind pocket (not a
   *  new box); no live preview (commit decides pocket vs boss). */
  private isSheetSource: boolean = false;

  /** 최소 유효 거리 (mm) — 이보다 작으면 무시 (프리뷰 확정용 threshold) */
  private static readonly MIN_COMMIT_DIST = 0.5;

  // ═══ 곡면 그룹 Push/Pull ═══
  private smoothGroupFaces: number[] = [];  // 곡면 그룹의 모든 faceId
  private isSmoothGroup: boolean = false;   // 곡면 그룹 모드 여부

  // ═══ Pooled/reusable objects (avoid GC pressure in hot paths) ═══
  private static readonly _mouse = new THREE.Vector2();
  private static readonly _ray = new THREE.Raycaster();
  private static readonly _camRight = new THREE.Vector3();
  private static readonly _camUp = new THREE.Vector3();
  private static readonly _planeNormal = new THREE.Vector3();
  private static readonly _intersection = new THREE.Vector3();
  private static readonly _plane = new THREE.Plane();
  private static readonly _mouseNdc = new THREE.Vector2();
  private static readonly _projTmp = new THREE.Vector3();

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[PushPullTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.ppActive) {
      // Phase 1: select face (first click)
      const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
      let rustFaceId = -1;
      let hitPoint: THREE.Vector3 | null = null;

      if (hit && hit.faceIndex != null && hit.faceIndex >= 0) {
        rustFaceId = this.ctx.getFaceId(hit.faceIndex);
        hitPoint = hit.point ? hit.point.clone() : null;
      }

      // Fallback to already-selected face
      if (rustFaceId < 0) {
        const selected = this.ctx.getSelectedFaces();
        if (selected.length === 1) {
          rustFaceId = selected[0];
          const centroid = this.ctx.bridge.facesCentroid(selected);
          if (centroid) hitPoint = centroid;
        }
      }

      if (rustFaceId >= 0 && hitPoint) {
        // ── Bug E fix: 법선이 degenerate면 Push/Pull 시작 거부 ──
        const normalArr = this.ctx.bridge.getFaceNormal(rustFaceId);
        if (!normalArr ||
            (normalArr[0] === 0 && normalArr[1] === 0 && normalArr[2] === 0)) {
          debugWarn('[PP] Invalid face normal for faceId=', rustFaceId);
          Toast.error('이 면의 법선을 계산할 수 없습니다 (degenerate)');
          return;
        }
        this.ppNormal = new THREE.Vector3(normalArr[0], normalArr[1], normalArr[2]);

        // ADR-007 Rev 2 — Sheet 의 normal 은 임의 winding 산물이므로
        //   사용자가 클릭한 측에서 보았을 때 "drag-outward = 카메라 쪽"
        //   직관을 유지하도록 normal 방향을 카메라 위치 기반으로 보정.
        //   Wall 은 외부=Front 로 well-defined 이므로 보정 안 함.
        if (this.ctx.bridge.isFaceInVolume?.(rustFaceId) === false) {
          const cam = this.ctx.viewport.activeCamera;
          const toCamera = new THREE.Vector3()
            .subVectors(cam.position, hitPoint)
            .normalize();
          if (toCamera.dot(this.ppNormal) < 0) {
            this.ppNormal.negate();
            debugLog('[PP] Sheet detected — flipped normal to face camera');
          }
        }

        // ADR-252 — pocket candidate: the face is a coplanar profile contained in
        //   a LARGER wall (e.g. a rect drawn on a wall). An INWARD push carves a
        //   blind pocket (not a new box); the live preview is suppressed and the
        //   commit dispatches to carvePocketFromSourceFace.
        this.isSheetSource =
          this.ctx.bridge.faceHasLargerCoplanarContainer?.(rustFaceId) ?? false;

        this.ppFaceId = rustFaceId;
        this.ppStartX = e.clientX;
        this.ppStartY = e.clientY;
        this.ppActive = true;

        // ── Bug D fix: 사용자가 이미 여러 면을 선택했으면 그 선택을 존중 ──
        // 단, 모든 선택면이 클릭한 면과 같은 smooth group일 때만 그룹 Push/Pull로 간주.
        // 그렇지 않으면 단일 면 Push/Pull (seed만).
        const manualSelected = this.ctx.getSelectedFaces();
        if (manualSelected.length > 1 && manualSelected.includes(rustFaceId)) {
          this.smoothGroupFaces = [...manualSelected];
          this.isSmoothGroup = true;
          debugLog('[PP] Phase 1: using manual selection of', manualSelected.length, 'faces');
        } else {
          // 자동 smooth group 감지 (법선 각도 기반)
          this.smoothGroupFaces = this.ctx.selection.getSmoothGroup(rustFaceId);
          this.isSmoothGroup = this.smoothGroupFaces.length > 1;
        }

        this.ppHitPoint = hitPoint;
        // ADR-193 — single planar face goes live (no ghost; the real solid
        // extrudes on the first move). Smooth groups keep the legacy ghost
        // preview (curved / multi-face live is a follow-up).
        this.liveActive = false;
        this.liveTopFace = -1;
        this.liveBeginFailed = false;
        if (this.isSmoothGroup) {
          this.createPPGhost(rustFaceId, hitPoint);
        } else {
          // Capture boundary verts for the dimension-label anchor only.
          this.ppFaceVerts = this.ctx.extractFaceBoundary(rustFaceId);
        }

        // ── Bug G fix: smooth group은 전체 face 선택 표시 (seed만 X) ──
        if (this.isSmoothGroup) {
          this.ctx.selection.selectFaces(this.smoothGroupFaces);
        } else {
          this.ctx.selection.handleClick(rustFaceId, false, false);
        }

        if (this.isSmoothGroup) {
          debugLog('[PP] Phase 1: SMOOTH GROUP selected,', this.smoothGroupFaces.length, 'faces, seed=', rustFaceId);
        } else {
          debugLog('[PP] Phase 1: face selected, faceId=', rustFaceId,
            'normal=', this.ppNormal.toArray().map(v => v.toFixed(3)));
        }
      }
    } else {
      // Phase 2: confirm distance (second click)
      // align 스냅이 발동됐다면 currentDragDist가 그 값을 담고 있음
      const dist = this.currentDragDist !== 0 ? this.currentDragDist : this.ppRayDist(e);
      debugLog('[PP] Phase 2: confirm dist=', dist.toFixed(2));

      // ── ADR-252 — POCKET carve ──
      // A coplanar profile on a larger wall (isSheetSource), pushed INWARD
      // (dist < 0), becomes a blind POCKET (not a new box). The profile is
      // consumed; the wall gets a recessed floor + side walls. carve returns -1
      // if it isn't a valid pocket (mesh restored) → normal extrude path. Live is
      // suppressed for pocket candidates, but cancel defensively just in case.
      if (this.isSheetSource && dist < 0 && Math.abs(dist) >= PushPullTool.MIN_COMMIT_DIST) {
        if (this.liveActive) {
          this.ctx.bridge.cancelLiveExtrude();
          this.liveActive = false;
          this.liveTopFace = -1;
        }
        const walls = this.ctx.bridge.carvePocketFromSourceFace(this.ppFaceId, -dist);
        if (walls > 0) {
          debugLog(`[PP] Pocket carved → ${walls} side walls (depth ${(-dist).toFixed(1)})`);
          Toast.success('포켓(pocket)을 파냈습니다');
          this.ctx.syncMesh();
          this.cleanup();
          return;
        }
        debugWarn('[PP] carvePocket declined — falling back to extrude:', this.ctx.bridge.lastError());
      }

      // ADR-261 — bidirectional / two-sided mode: cancel the one-way live
      // preview and commit a two-sided solid (commit-only v1; live preview is
      // one-way, mode applied here). Flat-profile only (commitBidirectional
      // Toasts + no-ops for smooth groups).
      if (getExtrudeMode() !== 'oneway' && Math.abs(dist) >= PushPullTool.MIN_COMMIT_DIST) {
        if (this.liveActive) {
          this.ctx.bridge.cancelLiveExtrude();
          this.liveActive = false;
          this.liveTopFace = -1;
        }
        this.commitBidirectional(dist);
        this.cleanup();
        return;
      }

      // ADR-193 — single-face live session: commit the already-real preview.
      if (this.liveActive) {
        const ok = this.ctx.bridge.commitLiveExtrude();
        if (ok) {
          this.lastPPDist = dist;
        } else {
          const err = this.ctx.bridge.lastError();
          Toast.error(err ? `돌출/잘라내기 실패: ${err}` : '돌출/잘라내기가 실행되지 않았습니다', 3500);
        }
        this.ctx.syncMesh();
        this.liveActive = false;
        this.liveTopFace = -1;
        this.cleanup();
        return;
      }

      // No live session yet (smooth group, or a single-face double-click with
      // no movement) → legacy commit path.
      if (Math.abs(dist) >= PushPullTool.MIN_COMMIT_DIST) {
        this.commitPushPull(dist);
      } else if (Math.abs(dist) > 0.001) {
        // Bug C fix: 0 < |dist| < 0.5mm 일 때 조용히 실패하지 않고 피드백
        Toast.warning(`돌출/잘라내기 거리가 너무 짧습니다 (최소 ${PushPullTool.MIN_COMMIT_DIST}mm)`, 2500);
      }
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.ppActive) return;

    let dist = this.ppRayDist(e);
    let isAligned = false;
    let alignedTargetType: 'vertex' | 'edge' | 'face' | null = null;

    // ── Align-to-geometry (v1): 단일 면만 지원, smooth group은 비활성 ──
    if (!this.isSmoothGroup) {
      const aligned = this.ctx.snap.findAlignedDistance(
        e.clientX, e.clientY,
        this.ctx.viewport.activeCamera,
        this.ctx.viewport.renderer.domElement,
        this.ppFaceId,
        this.ppHitPoint,
        this.ppNormal,
      );
      if (aligned) {
        dist = aligned.dist;
        isAligned = true;
        alignedTargetType = aligned.targetType;
        // 타겟에 snap marker 표시
        const s = aligned.target.clone().project(this.ctx.viewport.activeCamera);
        const rect = this.ctx.viewport.renderer.domElement.getBoundingClientRect();
        const screenPos = new THREE.Vector2(
          (s.x * 0.5 + 0.5) * rect.width + rect.left,
          (-s.y * 0.5 + 0.5) * rect.height + rect.top,
        );
        const markerType = aligned.targetType === 'vertex' ? 'endpoint'
                         : aligned.targetType === 'edge' ? 'nearest'
                         : 'onFace';
        this.ctx.snapVisual.update({
          type: markerType,
          position: aligned.target,
          screenPos,
        }, this.ctx.viewport.activeCamera);
      } else {
        this.ctx.snapVisual.clear();
      }
    }

    this.currentDragDist = dist;

    // ADR-193 — single planar face: live real-geometry direct manipulation.
    //   - first move past MIN: beginLiveExtrude (real preview extrude)
    //   - subsequent moves: updateLiveExtrude (slide the top cap)
    // Smooth groups keep the legacy translucent ghost.
    if (this.isSmoothGroup) {
      this.updatePPGhost(dist);
    } else if (this.isSheetSource) {
      // ADR-252 — pocket candidate (profile on a larger wall): no live preview.
      //   The commit decides inward → blind pocket carve, outward → boss/extrude.
      //   Dimension label only (shown below).
    } else if (this.liveActive) {
      this.ctx.bridge.updateLiveExtrude(dist);
      this.ctx.syncMesh();
    } else if (!this.liveBeginFailed && Math.abs(dist) >= PushPullTool.MIN_COMMIT_DIST) {
      const top = this.ctx.bridge.beginLiveExtrude(this.ppFaceId, dist);
      if (top !== null) {
        this.liveActive = true;
        this.liveTopFace = top;
        this.ctx.syncMesh();
      } else {
        // begin failed (e.g. unsupported build / engine reject) — don't retry
        // every move; fall back to a ghost so the user still gets feedback.
        this.liveBeginFailed = true;
        debugWarn('[PP] beginLiveExtrude failed, falling back to ghost:', this.ctx.bridge.lastError());
        this.createPPGhost(this.ppFaceId, this.ppHitPoint);
        this.updatePPGhost(dist);
      }
    } else if (this.liveBeginFailed && this.ppGhost) {
      this.updatePPGhost(dist);
    }

    // Show dimension
    if (this.ppFaceVerts.length >= 2 && Math.abs(dist) > 0.001) {
      const absDist = Math.abs(dist);
      const sign = dist >= 0 ? '' : '-';
      const alignPrefix = isAligned ? (alignedTargetType === 'face' ? '⊡ ' : alignedTargetType === 'edge' ? '／ ' : '■ ') : '';
      const text = alignPrefix + sign + this.ctx.units.format(absDist);
      const labelColor = isAligned ? '#66ff99' : '#ffd43b';
      // 저장: dim label 렌더에서 사용하도록
      const _labelColor = labelColor; void _labelColor;
      const offset = this.ppNormal.clone().multiplyScalar(dist);

      // Find closest vertex to mouse
      const canvasRect = this.ctx.viewport.renderer.domElement.getBoundingClientRect();
      const mouseNdc = PushPullTool._mouseNdc;
      mouseNdc.set(
        ((e.clientX - canvasRect.left) / canvasRect.width) * 2 - 1,
        -((e.clientY - canvasRect.top) / canvasRect.height) * 2 + 1,
      );
      let bestIdx = 0;
      let bestScreenDist = Infinity;
      const projTmp = PushPullTool._projTmp;
      for (let i = 0; i < this.ppFaceVerts.length; i++) {
        projTmp.copy(this.ppFaceVerts[i]).project(this.ctx.viewport.activeCamera);
        const dx = projTmp.x - mouseNdc.x;
        const dy = projTmp.y - mouseNdc.y;
        const sd = Math.sqrt(dx * dx + dy * dy);
        if (sd < bestScreenDist) {
          bestScreenDist = sd;
          bestIdx = i;
        }
      }

      const edgeFrom = this.ppFaceVerts[bestIdx].clone();
      const edgeTo = edgeFrom.clone().add(offset);

      this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
        { from: edgeFrom, to: edgeTo, text, color: isAligned ? '#66ff99' : '#ffd43b' },
      ]);
    } else {
      this.ctx.dimLabel.clear();
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
      return;
    }
    // ADR-007 Rev 2 Phase B-1 — Tab key flips push direction.
    //   Useful when the auto camera-based detection (sheet) chose the
    //   "wrong" side, or when the user wants to override the cached
    //   normal on a wall face. Updates ghost preview live.
    if (e.key === 'Tab' && this.ppActive) {
      e.preventDefault();
      this.ppNormal.negate();
      // ADR-193 — for a live session, flipping direction mid-drag means
      // rolling back the current preview; the next move re-begins in the new
      // direction (ppRayDist's sign flips with the negated ppNormal).
      if (this.liveActive) {
        this.ctx.bridge.cancelLiveExtrude();
        this.ctx.syncMesh();
        this.liveActive = false;
        this.liveTopFace = -1;
        this.liveBeginFailed = false;
      } else {
        // Re-render ghost with the new direction so the user sees it
        // flip instantly (carries over current drag distance if any).
        const dist = this.currentDragDist !== 0 ? this.currentDragDist : 0;
        this.updatePPGhost(dist);
      }
      Toast.info(`방향 반전 (Tab) — normal=(${this.ppNormal.x.toFixed(2)}, ${this.ppNormal.y.toFixed(2)}, ${this.ppNormal.z.toFixed(2)})`, 1500);
      debugLog('[PP] Tab pressed — normal flipped, new=', this.ppNormal.toArray());
    }
  }

  applyVCBValue(value: number, taperDeg?: number, topScale?: number): void {
    // ADR-261 β-3 — bidirectional / two-sided mode via the ExtrudeMode toggle
    // (NOT a VCB arg). Only a PLAIN distance (no comma → no taperDeg/topScale)
    // routes here; an explicit `거리,각도` (taper) / `거리,비율%` (cone) takes
    // priority. Cancels any one-way live preview, then commits two-sided.
    if (
      getExtrudeMode() !== 'oneway'
      && taperDeg === undefined
      && topScale === undefined
    ) {
      if (this.liveActive) {
        this.ctx.bridge.cancelLiveExtrude();
        this.liveActive = false;
        this.liveTopFace = -1;
      }
      if (this.ppFaceId < 0 && !this.isSmoothGroup) {
        const sel = this.ctx.getSelectedFaces();
        if (sel.length >= 1) this.ppFaceId = sel[0];
      }
      this.commitBidirectional(value);
      this.cleanup();
      return;
    }

    // ADR-193 — live session active: snap the preview to the typed value and
    // commit it (one clean Undo).
    if (this.liveActive) {
      this.ctx.bridge.updateLiveExtrude(value);
      const ok = this.ctx.bridge.commitLiveExtrude();
      if (ok) {
        this.lastPPDist = value;
      } else {
        const err = this.ctx.bridge.lastError();
        Toast.error(err ? `돌출/잘라내기 실패: ${err}` : '돌출/잘라내기가 실행되지 않았습니다', 3500);
      }
      this.ctx.syncMesh();
      this.liveActive = false;
      this.liveTopFace = -1;
      this.cleanup();
      return;
    }

    // ADR-260 β-3 — circle → cone/frustum via VCB "거리,비율%" (topScale set).
    // Commit-only (no live drag cone in v1). Seeds the face from the current
    // selection if Push/Pull wasn't already armed. topScale ∈ [0,1): 0 = apex.
    if (topScale !== undefined && Number.isFinite(topScale) && topScale >= 0) {
      if (this.ppFaceId < 0 && !this.isSmoothGroup) {
        const sel = this.ctx.getSelectedFaces();
        if (sel.length >= 1) this.ppFaceId = sel[0];
      }
      this.commitCone(value, topScale);
      this.cleanup();
      return;
    }

    // ADR-259 β-3 — tapered (draft) extrude via VCB "거리,각도" (taperDeg !== 0).
    // Commit-only (no live drag taper in v1). Seeds the face from the current
    // selection if Push/Pull wasn't already armed.
    if (taperDeg !== undefined && Number.isFinite(taperDeg) && Math.abs(taperDeg) > 1e-9) {
      if (this.ppFaceId < 0 && !this.isSmoothGroup) {
        const sel = this.ctx.getSelectedFaces();
        if (sel.length >= 1) this.ppFaceId = sel[0];
      }
      this.commitTaper(value, taperDeg);
      this.cleanup();
      return;
    }

    // Bug B fix: VCB 입력도 drag 경로와 동일하게 commitPushPull 사용
    // (곡면 그룹은 seamless, 단일 면은 pushPull, 둘 다 fallback 포함)
    if (this.ppFaceId < 0 && !this.isSmoothGroup) {
      // ppActive 진입 전 VCB 입력: 선택된 면으로 seed
      const sel = this.ctx.getSelectedFaces();
      if (sel.length >= 1) {
        this.ppFaceId = sel[0];
      }
    }
    if (this.ppFaceId >= 0 || this.isSmoothGroup) {
      this.commitPushPull(value);
    }
    this.cleanup();
  }

  /**
   * Push/Pull 커밋 — drag / VCB 공통 경로
   * - 곡면 그룹: seamless 우선, 실패/미지원 시 per-face fallback (Bug F)
   * - 단일 면: pushPull
   */
  private commitPushPull(dist: number): void {
    if (this.isSmoothGroup && this.smoothGroupFaces.length > 1) {
      const faceArray = new Uint32Array(this.smoothGroupFaces);
      const seamlessFn = this.ctx.bridge.engine?.push_pull_smooth_group_seamless;
      let ok = false;
      if (typeof seamlessFn === 'function') {
        ok = seamlessFn.call(this.ctx.bridge.engine, faceArray, dist) ?? false;
      }
      debugLog('[PP] Smooth group seamless:', ok ? 'OK' : 'FAILED/UNAVAILABLE',
        'faces=', this.smoothGroupFaces.length, 'dist=', dist.toFixed(2));

      if (ok) {
        this.lastPPDist = dist;
        this.ctx.syncMesh();
        return;
      }

      // Bug F fix: seamless 미지원 또는 실패 → per-face fallback
      // ADR-087 K-ε — kernel-aware createSolidExtrude only path.
      // Scene-level Q3 fallback (NotYetSupported → push_pull) 은 Rust
      // 측 exec_create_solid 가 자동 처리 — 사용자 facing 거동 동일.
      let successCount = 0;
      for (const fid of this.smoothGroupFaces) {
        const ok = this.ctx.bridge.createSolidExtrude(fid, dist);
        if (ok) successCount++;
      }
      if (successCount > 0) {
        debugLog('[PP] Fallback per-face:', successCount, '/', this.smoothGroupFaces.length);
        this.lastPPDist = dist;
        this.ctx.syncMesh();
      } else {
        const err = this.ctx.bridge.lastError();
        Toast.error(err ? `곡면 돌출/잘라내기 실패: ${err}` : '돌출/잘라내기가 실행되지 않았습니다', 3500);
      }
    } else {
      const faceId = this.ppFaceId >= 0 ? this.ppFaceId : this.ctx.getSelectedFaces()[0];
      if (faceId < 0) return;
      // ADR-087 K-ε — kernel-aware createSolidExtrude only path. Scene-level
      // Q3 fallback (NotYetSupported → push_pull) 은 Rust exec_create_solid
      // 가 자동 처리.
      const success = this.ctx.bridge.createSolidExtrude(faceId, dist);
      debugLog('[PP] result=', success, 'dist=', dist.toFixed(2));
      if (success) {
        this.lastPPDist = dist;
        this.ctx.syncMesh();
      } else {
        const err = this.ctx.bridge.lastError();
        Toast.error(err ? `돌출/잘라내기 실패: ${err}` : '돌출/잘라내기가 실행되지 않았습니다', 3500);
      }
    }
  }

  /**
   * ADR-259 β-3 — Tapered (draft) extrude commit (VCB "거리,각도" only; no live
   * drag taper in v1). A single FLAT profile face → frustum. Smooth-group /
   * multi-face taper is not a flat-profile op → Toast + no-op. On engine reject
   * (offset collapse / self-intersect / solid-face) the bridge wrapper already
   * surfaces `lastError()` as a Toast (D5 fail-closed) — never a silent straight
   * extrude.
   */
  private commitTaper(dist: number, taperDeg: number): void {
    if (this.isSmoothGroup) {
      Toast.warning('테이퍼(draft) 돌출은 단일 평면 프로파일만 지원합니다 (곡면/그룹 미지원)', 4000);
      return;
    }
    const faceId = this.ppFaceId >= 0 ? this.ppFaceId : this.ctx.getSelectedFaces()[0];
    if (faceId === undefined || faceId < 0) return;
    const ok = this.ctx.bridge.createSolidExtrudeTapered(faceId, dist, taperDeg);
    debugLog('[PP] taper result=', ok, 'dist=', dist.toFixed(2), 'taper=', taperDeg.toFixed(2));
    if (ok) {
      this.lastPPDist = dist;
      this.ctx.syncMesh();
    }
    // On failure the bridge wrapper already Toasted lastError (D5 fail-closed).
  }

  /**
   * ADR-260 β-3 — Circle → cone / frustum commit (VCB "거리,비율%" only; no live
   * drag cone in v1). A single FLAT circle profile → cone (`topScale = 0`) or
   * frustum (`0 < topScale < 1`). Smooth-group / multi-face is not a flat-circle
   * op → Toast + no-op. On engine reject (`topScale ≥ 1` / `< 0` / non-circle /
   * solid-face) the bridge wrapper already surfaces `lastError()` as a Toast
   * (D5 fail-closed) — never a silent straight cylinder.
   */
  private commitCone(dist: number, topScale: number): void {
    if (this.isSmoothGroup) {
      Toast.warning('콘(cone) 돌출은 단일 평면 원 프로파일만 지원합니다 (곡면/그룹 미지원)', 4000);
      return;
    }
    const faceId = this.ppFaceId >= 0 ? this.ppFaceId : this.ctx.getSelectedFaces()[0];
    if (faceId === undefined || faceId < 0) return;
    const ok = this.ctx.bridge.createSolidExtrudeCone(faceId, dist, topScale);
    debugLog('[PP] cone result=', ok, 'dist=', dist.toFixed(2), 'topScale=', topScale.toFixed(3));
    if (ok) {
      this.lastPPDist = dist;
      this.ctx.syncMesh();
    }
    // On failure the bridge wrapper already Toasted lastError (D5 fail-closed).
  }

  /**
   * ADR-261 β-3 — Bidirectional / two-sided extrude commit (ExtrudeMode toggle:
   * symmetric / twosided). `dist` = the +normal extent (drag/VCB magnitude);
   * symmetric → `(dp, dp)`, twosided → `(dp, distNeg)` from the settings. A
   * single FLAT profile (Plane, AllLinear|AllCircular) → two-sided solid spanning
   * `[−distNeg, +dp]`. Smooth-group / multi-face is not a flat-profile op →
   * Toast + no-op. On engine reject (negative / zero-sum / non-Plane / solid-face)
   * the bridge wrapper surfaces `lastError()` (D5 fail-closed) — never silent.
   */
  private commitBidirectional(dist: number): void {
    if (this.isSmoothGroup) {
      Toast.warning('양방향 돌출은 단일 평면 프로파일만 지원합니다 (곡면/그룹 미지원)', 4000);
      return;
    }
    const faceId = this.ppFaceId >= 0 ? this.ppFaceId : this.ctx.getSelectedFaces()[0];
    if (faceId === undefined || faceId < 0) return;
    const mode = getExtrudeMode();
    const dp = Math.abs(dist);
    const distNeg = mode === 'symmetric' ? dp : getExtrudeDistNeg();
    const ok = this.ctx.bridge.createSolidExtrudeBidirectional(faceId, dp, distNeg);
    debugLog('[PP] bidir result=', ok, 'mode=', mode, 'dp=', dp.toFixed(2), 'distNeg=', distNeg.toFixed(2));
    if (ok) {
      this.lastPPDist = dist;
      this.ctx.syncMesh();
    }
    // On failure the bridge wrapper already Toasted lastError (D5 fail-closed).
  }

  isBusy(): boolean {
    return this.ppActive;
  }

  cleanup(): void {
    // ADR-193 — roll back an un-committed live preview (ESC / tool switch /
    // deactivate mid-drag). commitLiveExtrude() already cleared liveActive on
    // the commit path, so this only fires for genuine cancels.
    if (this.liveActive) {
      this.ctx.bridge.cancelLiveExtrude();
      this.ctx.syncMesh();
    }
    this.liveActive = false;
    this.liveTopFace = -1;
    this.liveBeginFailed = false;
    this.isSheetSource = false;
    this.ppActive = false;
    this.ppFaceId = -1;
    this.smoothGroupFaces = [];
    this.isSmoothGroup = false;
    this.currentDragDist = 0;
    this.removePPGhost();
    this.ctx.selection.clearSelection();
    this.ctx.dimLabel.clear();
    this.ctx.snapVisual.clear();
  }

  private createPPGhost(faceId: number, _hitPoint: THREE.Vector3): void {
    this.removePPGhost();
    this.ppFaceVerts = this.ctx.extractFaceBoundary(faceId);
    if (this.ppFaceVerts.length < 3) return;

    // Bug A fix: smooth group 전체의 boundary 수집
    // (seed 외의 면은 ghost에 포함되지만 치수 라벨 anchor는 seed 유지)
    if (this.isSmoothGroup && this.smoothGroupFaces.length > 1) {
      this.ppAllFaceVerts = this.smoothGroupFaces
        .map(fid => this.ctx.extractFaceBoundary(fid))
        .filter(v => v.length >= 3);
    } else {
      this.ppAllFaceVerts = [this.ppFaceVerts];
    }

    this.ppGhost = new THREE.Group();
    this.ppGhost.renderOrder = 999;
    this.ctx.viewport.scene.add(this.ppGhost);
    this.rebuildPPGhost(0);
  }

  private rebuildPPGhost(dist: number): void {
    if (!this.ppGhost || this.ppFaceVerts.length < 3) return;

    while (this.ppGhost.children.length > 0) {
      const child = this.ppGhost.children[0];
      this.ppGhost.remove(child);
      if (child instanceof THREE.Mesh || child instanceof THREE.LineSegments) {
        child.geometry.dispose();
        if (child.material instanceof THREE.Material) child.material.dispose();
      }
    }

    if (Math.abs(dist) < 0.001) return;

    const offset = this.ppNormal.clone().multiplyScalar(dist);

    // Bug A fix: smooth group의 모든 face 각각 ghost로 렌더
    // (단일 면일 때는 ppAllFaceVerts.length === 1)
    const allLinePositions: number[] = [];

    for (const verts of this.ppAllFaceVerts) {
      if (verts.length < 3) continue;
      const offsetVerts = verts.map(v => v.clone().add(offset));
      const n = verts.length;

      // Top face (per-face BufferGeometry, fan triangulation)
      const topGeo = new THREE.BufferGeometry();
      topGeo.setAttribute('position', new THREE.BufferAttribute(
        new Float32Array(offsetVerts.flatMap(v => [v.x, v.y, v.z])), 3));
      const localIdx: number[] = [];
      for (let i = 1; i < n - 1; i++) localIdx.push(0, i, i + 1);
      topGeo.setIndex(localIdx);
      topGeo.computeVertexNormals();
      const topMesh = new THREE.Mesh(topGeo, new THREE.MeshBasicMaterial({
        color: 0x5b9bd5, side: THREE.FrontSide,
        transparent: true, opacity: 0.3,
        depthWrite: false,
      }));
      topMesh.renderOrder = 999;
      this.ppGhost.add(topMesh);

      // Wall quads per boundary edge
      const wallGeo = new THREE.BufferGeometry();
      const wallPos: number[] = [];
      const wallIdx: number[] = [];
      let wi = 0;
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        const a = verts[i], b = verts[j], c = offsetVerts[j], d = offsetVerts[i];
        wallPos.push(a.x, a.y, a.z, b.x, b.y, b.z, c.x, c.y, c.z, d.x, d.y, d.z);
        wallIdx.push(wi, wi + 1, wi + 2, wi, wi + 2, wi + 3);
        wi += 4;
      }
      wallGeo.setAttribute('position', new THREE.BufferAttribute(new Float32Array(wallPos), 3));
      wallGeo.setIndex(wallIdx);
      wallGeo.computeVertexNormals();
      const wallMesh = new THREE.Mesh(wallGeo, new THREE.MeshBasicMaterial({
        color: 0x5b9bd5, side: THREE.FrontSide,
        transparent: true, opacity: 0.2,
        depthWrite: false,
      }));
      wallMesh.renderOrder = 998;
      this.ppGhost.add(wallMesh);

      // Boundary lines (top + vertical)
      for (let i = 0; i < n; i++) {
        const j = (i + 1) % n;
        allLinePositions.push(offsetVerts[i].x, offsetVerts[i].y, offsetVerts[i].z);
        allLinePositions.push(offsetVerts[j].x, offsetVerts[j].y, offsetVerts[j].z);
      }
      for (let i = 0; i < n; i++) {
        allLinePositions.push(verts[i].x, verts[i].y, verts[i].z);
        allLinePositions.push(offsetVerts[i].x, offsetVerts[i].y, offsetVerts[i].z);
      }
    }

    // 모든 face의 outline을 통합된 LineSegments 하나로
    if (allLinePositions.length > 0) {
      const lineGeo = new THREE.BufferGeometry();
      lineGeo.setAttribute('position', new THREE.BufferAttribute(
        new Float32Array(allLinePositions), 3));
      const lineSegs = new THREE.LineSegments(lineGeo, new THREE.LineBasicMaterial({
        color: 0x2a6cb8, depthTest: false,
      }));
      lineSegs.renderOrder = 1000;
      this.ppGhost.add(lineSegs);
    }
  }

  private updatePPGhost(dist: number): void {
    this.rebuildPPGhost(dist);
  }

  private removePPGhost(): void {
    if (this.ppGhost) {
      while (this.ppGhost.children.length > 0) {
        const child = this.ppGhost.children[0];
        this.ppGhost.remove(child);
        if (child instanceof THREE.Mesh || child instanceof THREE.LineSegments) {
          child.geometry.dispose();
          if (child.material instanceof THREE.Material) child.material.dispose();
        }
      }
      this.ctx.viewport.scene.remove(this.ppGhost);
      this.ppGhost = null;
    }
    this.ppFaceVerts = [];
  }

  private ppRayDist(e: MouseEvent): number {
    const canvas = this.ctx.viewport.renderer.domElement;
    const rect = canvas.getBoundingClientRect();

    // Reuse pooled objects to avoid GC pressure
    const mouse = PushPullTool._mouse;
    mouse.set(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    const ray = PushPullTool._ray;
    ray.setFromCamera(mouse, this.ctx.viewport.activeCamera);

    const camRight = PushPullTool._camRight;
    camRight.setFromMatrixColumn(this.ctx.viewport.activeCamera.matrixWorld, 0).normalize();

    const planeNormal = PushPullTool._planeNormal;
    planeNormal.crossVectors(this.ppNormal, camRight).normalize();
    if (planeNormal.length() < 0.001) {
      const camUp = PushPullTool._camUp;
      camUp.setFromMatrixColumn(this.ctx.viewport.activeCamera.matrixWorld, 1).normalize();
      planeNormal.crossVectors(this.ppNormal, camUp).normalize();
    }

    const plane = PushPullTool._plane;
    plane.setFromNormalAndCoplanarPoint(planeNormal, this.ppHitPoint);
    const intersection = PushPullTool._intersection;
    const hit = ray.ray.intersectPlane(plane, intersection);

    if (!hit) return 0;

    const diff = intersection.sub(this.ppHitPoint);
    return diff.dot(this.ppNormal);
  }
}
