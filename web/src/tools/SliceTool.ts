/**
 * Slice Tool — Plane-cut a closed volume into two volumes.
 *
 * Workflow:
 *   1. Select a face of the target volume (or any face of the volume).
 *      The volume = the XIA owning the selected face. All faces of that
 *      XIA are passed to the slice operation.
 *   2. Activate Slice tool (menu / keyboard / action).
 *   3. Click 1 — first point on cutting plane.
 *   4. Click 2 — second point. Together with click 1 these define a line
 *      on the plane.
 *   5. Click 3 — third (non-collinear) point. The three points fully
 *      define the cutting plane.
 *      Alternative quick mode: pressing ENTER / SPACE after click 2
 *      finishes with a VERTICAL plane (normal perpendicular to both
 *      points and world-up axis) — common case for architectural cuts.
 *
 * Esc cancels at any time.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { debugLog } from '../utils/debug';
import { Toast } from '../ui/Toast';
import { t } from '../i18n';

type Phase = 'idle' | 'awaiting_p2' | 'awaiting_p3';

const PREVIEW_COLOR = 0xff8c00;       // orange — distinct from blue draw previews
const PREVIEW_OUTLINE = 0xc25500;
const PLANE_PATCH_SIZE = 5000;        // mm — 5m square preview patch

/**
 * ADR-197 β-3-n — curved knife mode.
 *   'slice' = split into two volumes (default, matches the polygonal SliceTool);
 *   'above' / 'below' = TRIM (keep one side of a horizontal cut, remove the other).
 * For a curved Path B solid + a HORIZONTAL plane these route to the surface-
 * preserving `cutCurvedByZPlane`; otherwise the polygonal slice runs ('slice' only).
 */
type CutMode = 'slice' | 'above' | 'below';
const CUT_MODE_KO: Record<CutMode, string> = {
  slice: '쪼개기 (2 볼륨)',
  above: '트림 — 위쪽 유지',
  below: '트림 — 아래쪽 유지',
};

export class SliceTool implements ITool {
  readonly name = 'slice';

  private ctx: ToolContext;
  private phase: Phase = 'idle';
  private p1: THREE.Vector3 | null = null;
  private p2: THREE.Vector3 | null = null;
  private cutMode: CutMode = 'slice';

  // Captured volume face ids at activation time.
  private volumeFaceIds: number[] = [];

  // Preview meshes
  private linePreview: THREE.Line | null = null;
  private planePatch: THREE.Mesh | null = null;
  private planeOutline: THREE.LineLoop | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  isBusy(): boolean { return this.phase !== 'idle'; }

  onActivate(): void {
    debugLog('[SliceTool] activated');
    // Capture the volume from the current selection.
    const selected = this.ctx.selection.getSelectedFaces();
    if (selected.length === 0) {
      Toast.warning(t('Slice: 자를 볼륨의 면을 먼저 선택하세요'), 4000);
      this.phase = 'idle';
      return;
    }
    // Volume = all faces of the XIA owning the first selected face.
    const bridge = this.ctx.bridge;
    const xiaIds = new Set<number>();
    for (const fid of selected) {
      const xid = bridge.engine?.get_xia_for_face?.(fid);
      if (xid !== undefined && xid >= 0) xiaIds.add(xid);
    }
    if (xiaIds.size === 0) {
      Toast.error(t('Slice: 선택된 면에 소속 볼륨(XIA)이 없습니다'));
      this.phase = 'idle';
      return;
    }
    if (xiaIds.size > 1) {
      Toast.warning(t('Slice: 한 번에 하나의 볼륨만 자를 수 있습니다 — 단일 솔리드의 면을 선택하세요'), 5000);
      this.phase = 'idle';
      return;
    }
    const xiaId = [...xiaIds][0];
    // Fetch the XIA's face_ids via bridge.
    const xiaFaces = bridge.engine?.getXiaFaceIds?.(xiaId);
    if (!xiaFaces || xiaFaces.length === 0) {
      Toast.error(t('Slice: XIA {xiaId}에 면이 없습니다', { xiaId }));
      this.phase = 'idle';
      return;
    }
    this.volumeFaceIds = Array.from(xiaFaces);
    debugLog(`[SliceTool] target volume: XIA ${xiaId}, ${this.volumeFaceIds.length} faces`);
    Toast.info(
      t('Slice [{mode}]: 평면 3점 클릭 — 또는 한 점 클릭 후 H=수평 절단.\n', { mode: t(CUT_MODE_KO[this.cutMode]) }) +
      t('M=모드 전환 (쪼개기/트림), Esc 취소'),
      5000,
    );
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!point) return;
    if (this.phase === 'idle') {
      this.p1 = point.clone();
      this.phase = 'awaiting_p2';
      debugLog('[Slice] click 1', this.p1.toArray());
    } else if (this.phase === 'awaiting_p2') {
      if (!this.p1) return;
      if (this.p1.distanceTo(point) < 1.0) {
        Toast.warning(t('두 번째 점은 첫 번째와 다른 위치여야 합니다'));
        return;
      }
      this.p2 = point.clone();
      this.phase = 'awaiting_p3';
      debugLog('[Slice] click 2', this.p2.toArray());
    } else if (this.phase === 'awaiting_p3') {
      if (!this.p1 || !this.p2) return;
      const p3 = point.clone();
      // Reject if collinear with p1-p2.
      const d12 = new THREE.Vector3().subVectors(this.p2, this.p1);
      const d13 = new THREE.Vector3().subVectors(p3, this.p1);
      const cross = new THREE.Vector3().crossVectors(d12, d13);
      if (cross.lengthSq() < 1e-6) {
        Toast.warning(t('세 점이 일직선 — 다른 위치를 클릭하세요'));
        return;
      }
      this.commit(this.p1, this.p2, p3);
    }
  }

  onMouseMove(_e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!point) return;
    if (this.phase === 'awaiting_p2' && this.p1) {
      this.updateLinePreview(this.p1, point);
    } else if (this.phase === 'awaiting_p3' && this.p1 && this.p2) {
      // Compute plane from p1, p2, point and show patch.
      const d12 = new THREE.Vector3().subVectors(this.p2, this.p1);
      const d13 = new THREE.Vector3().subVectors(point, this.p1);
      const normal = new THREE.Vector3().crossVectors(d12, d13);
      if (normal.lengthSq() < 1e-6) {
        this.clearPlanePatch();
        return;
      }
      normal.normalize();
      this.updatePlanePatch(this.p1, this.p2, point, normal);
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      Toast.info(t('Slice 취소'));
      this.cleanup();
      return;
    }
    // M — cycle cut mode (쪼개기 → 위쪽 트림 → 아래쪽 트림). 곡면 솔리드 + 수평 절단에서만 트림 적용.
    if (e.key === 'm' || e.key === 'M') {
      this.cutMode = this.cutMode === 'slice' ? 'above' : this.cutMode === 'above' ? 'below' : 'slice';
      Toast.info(t('Slice 모드: {mode}', { mode: t(CUT_MODE_KO[this.cutMode]) }), 2500);
      e.preventDefault();
      return;
    }
    // H — horizontal cut through the first point (the natural curved-knife gesture):
    // click a point at the desired height, then H → a horizontal Z-plane there.
    if ((e.key === 'h' || e.key === 'H') && this.p1 &&
        (this.phase === 'awaiting_p2' || this.phase === 'awaiting_p3')) {
      e.preventDefault();
      this.commitWithNormal(this.p1, new THREE.Vector3(0, 0, 1));
      return;
    }
    if (this.phase === 'awaiting_p3' && (e.key === 'Enter' || e.key === ' ')) {
      // Quick mode: vertical plane through p1-p2 with world-up normal direction.
      if (!this.p1 || !this.p2) return;
      const d12 = new THREE.Vector3().subVectors(this.p2, this.p1);
      const up = new THREE.Vector3(0, 1, 0);
      const normal = new THREE.Vector3().crossVectors(d12, up);
      if (normal.lengthSq() < 1e-6) {
        Toast.warning(t('수직 평면 모드: p1-p2가 세로축과 평행 — 세 번째 점을 클릭하세요'));
        return;
      }
      normal.normalize();
      // Run slice directly with this plane.
      e.preventDefault();
      this.commitWithNormal(this.p1, normal);
    }
  }

  // ── Commit ──────────────────────────────────────────────────────

  private commit(p1: THREE.Vector3, p2: THREE.Vector3, p3: THREE.Vector3): void {
    const d12 = new THREE.Vector3().subVectors(p2, p1);
    const d13 = new THREE.Vector3().subVectors(p3, p1);
    const normal = new THREE.Vector3().crossVectors(d12, d13).normalize();
    this.commitWithNormal(p1, normal);
  }

  private commitWithNormal(origin: THREE.Vector3, normal: THREE.Vector3): void {
    const bridge = this.ctx.bridge;

    // Hotfix (2026-05-28) — pre-check empty volume face set.
    // Root cause: SliceTool.onActivate() 가 face 미선택 시 Toast.warning +
    // phase='idle' 만 set 하고 종료 — 그러나 onMouseDown('idle') 이 phase 를
    // 'awaiting_p2' 로 전이시킴. 사용자 3 클릭 → commit 도달 → engine
    // "empty face set" error.
    //
    // 사용자 facing: "솔리드(volume) 가 없습니다" 한국어 안내 +
    // 도구 자동 비활성 → 다음 클릭 무시.
    if (this.volumeFaceIds.length === 0) {
      Toast.error(
        t('⚠️ Slice 불가 — 솔리드(volume) 가 선택되지 않았습니다.\n') +
        '먼저 돌출/잘라내기(Extrude/Cut) 로 입체를 만들고, 그 면을 선택한 후 Slice 도구 사용',
        7000,
      );
      debugLog('[Slice] empty volumeFaceIds — hotfix pre-check 차단');
      this.cleanup();
      return;
    }

    const fids = new Uint32Array(this.volumeFaceIds);

    // ── ADR-197 β-3-n — CURVED knife: a HORIZONTAL plane (normal ‖ ±Z) on a
    // curved Path B solid routes to the surface-preserving cut. `routed:false`
    // (the volume is not a single analytic primitive) → fall through to the
    // polygonal slice below. Non-horizontal planes stay polygonal (MVP).
    if (Math.abs(normal.z) > 0.999 && typeof bridge.engine?.cutCurvedByZPlane === 'function') {
      const cjson = bridge.engine.cutCurvedByZPlane(fids, origin.z, this.cutMode);
      let cres: { ok: boolean; routed?: boolean; resultFaces?: number[]; newXia?: number; error?: string };
      try { cres = JSON.parse(cjson); } catch { cres = { ok: false, error: 'parse' }; }
      if (cres.ok && cres.routed) {
        bridge.markDirty();
        this.ctx.syncMesh();
        const n = cres.resultFaces?.length ?? 0;
        // ADR-197 #Track3 — slice splits into 2 volumes; the lower half gets a new XIA.
        const xiaNote = (this.cutMode === 'slice' && typeof cres.newXia === 'number' && cres.newXia >= 0)
          ? t(' — 아래쪽 새 볼륨 (XIA {newXia})', { newXia: cres.newXia })
          : '';
        const msg = this.cutMode === 'slice'
          ? t('곡면 쪼개기 완료 — 2개 볼륨, 곡면 보존{xiaNote} (면 {n}개)', { xiaNote, n })
          : t('곡면 트림 완료 — {side} 유지, 곡면 보존 (면 {n}개)', { side: this.cutMode === 'above' ? '위쪽' : '아래쪽', n });
        Toast.success(msg, 3500);
        debugLog(`[Slice] curved cut (${this.cutMode}) → ${n} faces`);
        this.cleanup();
        return;
      }
      if (cres.ok && cres.routed === false) {
        debugLog('[Slice] not a curved primitive — falling back to polygonal slice');
        // fall through to the polygonal path below.
      } else if (!cres.ok) {
        Toast.error(t('곡면 절단 실패: {error}', { error: this.translateEngineError(cres.error ?? '알 수 없는 오류') }), 7000);
        debugLog('[Slice] curved cut error:', cres.error);
        this.cleanup();
        return;
      }
    }

    // ── ADR-205 γ-wire-ui — an ARBITRARY (oblique / axial) plane in TRIM mode on
    // a curved Path B primitive routes to the surface-preserving trim: a CYLINDER
    // (β-2 elliptic / β-4 axial-flat / local-frame, by the plane-vs-axis angle) or a
    // TORUS (β-2-torus annular halfspace). The horizontal branch above already
    // returned for ‖Z planes; `routed:false` (not a curved primitive) → fall through
    // to the polygonal slice. Trim only (curved arbitrary-plane slice = γ-2).
    if (this.cutMode !== 'slice' && typeof bridge.engine?.trimCurvedByPlane === 'function') {
      const keepN = this.cutMode === 'above' ? normal : normal.clone().negate();
      const tjson = bridge.engine.trimCurvedByPlane(
        fids, origin.x, origin.y, origin.z, keepN.x, keepN.y, keepN.z,
      );
      let tres: { ok: boolean; routed?: boolean; resultFaces?: number[]; error?: string };
      try { tres = JSON.parse(tjson); } catch { tres = { ok: false, error: 'parse' }; }
      if (tres.ok && tres.routed) {
        bridge.markDirty();
        this.ctx.syncMesh();
        const n = tres.resultFaces?.length ?? 0;
        Toast.success(t('곡면 트림 완료 — 임의 평면, 곡면 보존 (면 {n}개)', { n }), 3500);
        debugLog(`[Slice] curved arbitrary-plane trim (${this.cutMode}) → ${n} faces`);
        this.cleanup();
        return;
      }
      if (tres.ok && tres.routed === false) {
        debugLog('[Slice] not a curved primitive — falling back to polygonal slice');
        // fall through to the polygonal path below.
      } else if (!tres.ok) {
        Toast.error(t('곡면 트림 실패: {error}', { error: this.translateEngineError(tres.error ?? '알 수 없는 오류') }), 7000);
        debugLog('[Slice] curved trim error:', tres.error);
        this.cleanup();
        return;
      }
    }

    // ── Polygonal slice (non-curved volume, or non-horizontal plane). Only the
    // 'slice' (2-volume) semantics is supported here; trim is curved-only.
    if (!bridge.engine?.sliceVolumeByPlane) {
      Toast.error(t('Slice: WASM 엔진에 sliceVolumeByPlane 함수가 없습니다 (rebuild 필요)'));
      this.cleanup();
      return;
    }
    if (this.cutMode !== 'slice') {
      // ADR-241 Phase 1 C5 — polygonal TRIM (keep one half). 'above' = +normal
      // side (consistent with trimCurvedByPlane). Falls back to a 2-volume
      // slice on a legacy build lacking the endpoint.
      if (typeof bridge.engine?.trimVolumeByPlane === 'function') {
        const keepAbove = this.cutMode === 'above';
        const tjson = bridge.engine.trimVolumeByPlane(
          fids,
          origin.x, origin.y, origin.z,
          normal.x, normal.y, normal.z,
          keepAbove,
        );
        let tres: { ok: boolean; totalFaces?: number; error?: string };
        try {
          tres = JSON.parse(tjson);
        } catch {
          Toast.error(t('Trim: 응답 파싱 실패'));
          this.cleanup();
          return;
        }
        if (tres.ok) {
          Toast.success(
            t('트림 완료 — {keepAbove} 유지 (면 {tres}개)', { keepAbove: keepAbove ? t('위쪽(+법선)') : t('아래쪽(−법선)'), tres: tres.totalFaces ?? '?' }),
            3000,
          );
          bridge.markDirty();
          this.ctx.syncMesh();
          this.cleanup();
          return;
        }
        Toast.error(t('트림 실패: {error}', { error: this.translateEngineError(tres.error ?? '알 수 없는 오류') }), 7000);
        debugLog('[Slice] polygonal trim error:', tres.error);
        this.cleanup();
        return;
      }
      Toast.warning(t('트림 미지원 빌드 — 쪼개기(2 볼륨)로 대체합니다.'), 4000);
      // proceed as a normal 2-volume slice.
    }
    const json = bridge.engine.sliceVolumeByPlane(
      fids,
      origin.x, origin.y, origin.z,
      normal.x, normal.y, normal.z,
    );
    let result: { ok: boolean; newXia?: number; error?: string };
    try {
      result = JSON.parse(json);
    } catch {
      Toast.error(t('Slice: 응답 파싱 실패'));
      this.cleanup();
      return;
    }
    if (!result.ok) {
      // Engine error 한국어 mapping (사용자 시연 evidence 2026-05-28).
      const engineErr = result.error ?? '알 수 없는 오류';
      const userMsg = this.translateEngineError(engineErr);
      Toast.error(t('Slice 실패: {userMsg}', { userMsg }), 7000);
      debugLog('[Slice] error:', engineErr);
      this.cleanup();
      return;
    }
    Toast.success(t('Slice 완료 — 위쪽은 원본 볼륨에 유지, 아래쪽은 새 볼륨 (XIA {newXia})', { newXia: result.newXia ?? '?' }), 3000);
    bridge.markDirty();
    this.ctx.syncMesh();
    this.cleanup();
  }

  /**
   * Engine error 메시지 한국어 사용자 facing translation.
   *
   * Hotfix (2026-05-28) — production user reporting trigger
   * (`empty face set` 영어 메시지가 사용자 인지 어려움).
   */
  private translateEngineError(engineErr: string): string {
    if (engineErr.includes('empty face set')) {
      return t('솔리드(volume) 가 없습니다. 돌출/잘라내기(Extrude/Cut) 로 입체 먼저 만들기');
    }
    if (engineErr.includes('span multiple XIAs')) {
      return t('여러 볼륨 동시 자르기 불가 — 단일 솔리드의 면을 선택하세요');
    }
    if (engineErr.includes('has no owning XIA')) {
      return t('선택된 면에 소속 볼륨이 없습니다 (Sheet face — 돌출/잘라내기 필요)');
    }
    if (engineErr.includes('cannot determine source XIA')) {
      return t('소속 볼륨을 결정할 수 없습니다');
    }
    // Default fallback — engine 메시지 그대로
    return engineErr;
  }

  // ── Preview helpers ─────────────────────────────────────────────

  private updateLinePreview(a: THREE.Vector3, b: THREE.Vector3): void {
    const verts = new Float32Array([a.x, a.y, a.z, b.x, b.y, b.z]);
    if (!this.linePreview) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(verts, 3));
      const mat = new THREE.LineBasicMaterial({ color: PREVIEW_OUTLINE, depthTest: false });
      this.linePreview = new THREE.Line(geo, mat);
      this.linePreview.renderOrder = 1000;
      this.ctx.viewport.scene.add(this.linePreview);
    } else {
      const attr = this.linePreview.geometry.getAttribute('position') as THREE.BufferAttribute;
      (attr.array as Float32Array).set(verts);
      attr.needsUpdate = true;
    }
  }

  private updatePlanePatch(
    p1: THREE.Vector3,
    p2: THREE.Vector3,
    p3: THREE.Vector3,
    normal: THREE.Vector3,
  ): void {
    // Centroid.
    const c = p1.clone().add(p2).add(p3).multiplyScalar(1 / 3);
    // Build orthonormal basis on plane.
    const u = new THREE.Vector3().subVectors(p2, p1).normalize();
    const v = new THREE.Vector3().crossVectors(normal, u).normalize();
    const r = PLANE_PATCH_SIZE * 0.5;
    const corners = [
      c.clone().addScaledVector(u, -r).addScaledVector(v, -r),
      c.clone().addScaledVector(u,  r).addScaledVector(v, -r),
      c.clone().addScaledVector(u,  r).addScaledVector(v,  r),
      c.clone().addScaledVector(u, -r).addScaledVector(v,  r),
    ];
    const verts = new Float32Array(12);
    for (let i = 0; i < 4; ++i) {
      verts[i * 3] = corners[i].x;
      verts[i * 3 + 1] = corners[i].y;
      verts[i * 3 + 2] = corners[i].z;
    }
    const indices = new Uint16Array([0, 1, 2, 0, 2, 3]);
    if (!this.planePatch) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute('position', new THREE.BufferAttribute(verts, 3));
      geo.setIndex(new THREE.BufferAttribute(indices, 1));
      const mat = new THREE.MeshBasicMaterial({
        color: PREVIEW_COLOR,
        transparent: true,
        opacity: 0.18,
        side: THREE.DoubleSide,
        depthWrite: false,
      });
      this.planePatch = new THREE.Mesh(geo, mat);
      this.ctx.viewport.scene.add(this.planePatch);

      const og = new THREE.BufferGeometry();
      og.setAttribute('position', new THREE.BufferAttribute(verts, 3));
      const om = new THREE.LineBasicMaterial({ color: PREVIEW_OUTLINE, depthTest: false });
      this.planeOutline = new THREE.LineLoop(og, om);
      this.planeOutline.renderOrder = 1000;
      this.ctx.viewport.scene.add(this.planeOutline);
    } else {
      const a1 = this.planePatch.geometry.getAttribute('position') as THREE.BufferAttribute;
      (a1.array as Float32Array).set(verts);
      a1.needsUpdate = true;
      const a2 = this.planeOutline!.geometry.getAttribute('position') as THREE.BufferAttribute;
      (a2.array as Float32Array).set(verts);
      a2.needsUpdate = true;
    }
  }

  private clearPlanePatch(): void {
    if (this.planePatch) {
      this.ctx.viewport.scene.remove(this.planePatch);
      this.planePatch.geometry.dispose();
      (this.planePatch.material as THREE.Material).dispose();
      this.planePatch = null;
    }
    if (this.planeOutline) {
      this.ctx.viewport.scene.remove(this.planeOutline);
      this.planeOutline.geometry.dispose();
      (this.planeOutline.material as THREE.Material).dispose();
      this.planeOutline = null;
    }
  }

  cleanup(): void {
    this.phase = 'idle';
    this.p1 = null;
    this.p2 = null;
    this.volumeFaceIds = [];
    if (this.linePreview) {
      this.ctx.viewport.scene.remove(this.linePreview);
      this.linePreview.geometry.dispose();
      (this.linePreview.material as THREE.Material).dispose();
      this.linePreview = null;
    }
    this.clearPlanePatch();
  }
}
