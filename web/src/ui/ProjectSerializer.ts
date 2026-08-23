/**
 * Project Serializer — .xia project file Save / Load
 *
 * Extracted from main.ts (lines 1231-1385).
 * Handles project export (snapshot + fallback) and import with camera/style restoration.
 */

import { WasmBridge } from '../bridge/WasmBridge';
import { Viewport } from '../viewport/Viewport';
import { ToolManager } from '../tools/ToolManagerRefactored';
import { UnitSystem } from '../units/UnitSystem';
import { debugLog } from '../utils/debug';
import { t } from '../i18n';

export interface ProjectSerializerDeps {
  bridge: WasmBridge;
  viewport: Viewport;
  toolManager: ToolManager;
  units: UnitSystem;
  /** 바이너리 .xia 를 읽는 유일한 파서. 없으면 그 형식은 거절합니다 —
   *  기존 호출자를 깨지 않으려고 optional 로 둡니다. */
  fileManager?: {
    loadFromArrayBuffer(data: Uint8Array, fileName?: string): Promise<boolean>;
  };
}

/** Uint8Array → base64 문자열 */
function toBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

/** base64 → Uint8Array */
function fromBase64(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export interface ProjectSerializerAPI {
  saveProject: () => void;
  openProject: () => void;
}

/** FileManager.createAxiaFile 이 앞 4바이트에 쓰는 값 ('AXIA', little-endian). */
const AXIA_MAGIC = 0x41584941;
/** 같은 상수를 파일 앞 4글자로 — little-endian 이라 디스크에는 'AIXA' 로 적힙니다
    (코드 주석의 'AXIA' 와 다릅니다). 손으로 적지 않고 상수에서 유도합니다. */
const AXIA_MAGIC_PREFIX = String.fromCharCode(
  AXIA_MAGIC & 0xff,
  (AXIA_MAGIC >>> 8) & 0xff,
  (AXIA_MAGIC >>> 16) & 0xff,
  (AXIA_MAGIC >>> 24) & 0xff,
);

export function initProjectSerializer(deps: ProjectSerializerDeps): ProjectSerializerAPI {
  const { bridge, viewport, toolManager, units } = deps;

  /**
   * ADR-078 P-3 L1 — Save sync (push).
   *
   * Push SelectionManager.groupTags → Scene.boolean_group_tags via
   * WasmBridge BEFORE exportSnapshot. Idempotent order:
   *   1. clearBooleanGroupTags (drop any stale state from prior session)
   *   2. setBooleanGroupTag(groupA, 'A') if non-empty
   *   3. setBooleanGroupTag(groupB, 'B') if non-empty
   *
   * If both groups empty: clear-only (no set calls). Persistence layer
   * truth source = SelectionManager (UI runtime).
   *
   * Bypasses SelectionManager.setGroupTag (selection-bound). The bridge
   * has no selection constraint — Rust layer is a simple HashMap insert.
   */
  const pushGroupTagsToBridge = () => {
    bridge.clearBooleanGroupTags();
    const sel: any = (toolManager as any)?.selection;
    if (!sel) return;
    const groupA: number[] = sel.getGroupA?.() ?? [];
    const groupB: number[] = sel.getGroupB?.() ?? [];
    if (groupA.length > 0) bridge.setBooleanGroupTag(groupA, 'A');
    if (groupB.length > 0) bridge.setBooleanGroupTag(groupB, 'B');
  };

  /**
   * ADR-078 P-3 L2 — Load sync (pull).
   *
   * Pull Scene.boolean_group_tags → SelectionManager.groupTags via
   * WasmBridge AFTER importSnapshot + syncMesh (face IDs stable).
   * One restoreGroupTags call → one notifyChange emit → one V-2
   * outline rebuild.
   *
   * SelectionManager.restoreGroupTags handles policy (P-3 L3):
   * groupTags fully replaced + selection ∪ (A ∪ B) + 1 notifyChange.
   */
  const pullGroupTagsFromBridge = () => {
    const sel: any = (toolManager as any)?.selection;
    if (!sel || typeof sel.restoreGroupTags !== 'function') return;
    const groupA = bridge.getBooleanGroupAFaces();
    const groupB = bridge.getBooleanGroupBFaces();
    sel.restoreGroupTags(groupA, groupB);
  };

  /** WASM export 불가 시 fallback: 메시 버퍼를 직접 저장 */
  const saveFallback = () => {
    const buffers = bridge.getMeshBuffers();
    const edgeLines = bridge.getEdgeLines();

    const project = {
      format: 'xia',
      version: '1.0.0-fallback',
      engine: 'AXiA 3D',
      created: new Date().toISOString(),
      units: {
        unit: units.unit,
        precision: units.precision,
      },
      camera: viewport.getCameraState(),
      style: viewport.getStyleSettings(),
      buffers: buffers ? {
        positions: Array.from(buffers.positions),
        normals: Array.from(buffers.normals),
        indices: Array.from(buffers.indices),
        faceMap: Array.from(buffers.faceMap),
      } : null,
      edgeLines: edgeLines ? Array.from(edgeLines) : null,
    };

    const json = JSON.stringify(project);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);

    const a = document.createElement('a');
    a.href = url;
    a.download = `AXiA_Project_${new Date().toISOString().slice(0, 10)}.xia`;
    a.click();
    URL.revokeObjectURL(url);
    debugLog('[Save] Fallback project saved:', json.length, 'bytes');
  };

  /** .xia 프로젝트 파일 저장 */
  /** ADR-078 P-3 참고 — 스냅샷을 들인 뒤 뷰와 그룹을 되살립니다.
   *
   * JSON 경로와 바이너리 경로가 **같은** 사후 작업을 하도록 한곳에 둡니다.
   * 나뉘어 있으면 한쪽만 고쳐져 조용히 어긋납니다 — 바이너리 파일이 열리는데
   * 화면은 그대로인 식으로. */
  const restoreViewAfterImport = () => {
    toolManager.syncMesh();
    // 면 ID 가 안정된 뒤에 당겨야 합니다.
    pullGroupTagsFromBridge();
    // 그룹 자체는 Boolean 그룹 태그와 다른 것입니다. 스냅샷이 엔진에는
    // 되살리지만 로컬 맵은 비어 있어서, 다시 연 프로젝트의 그룹을 UI 가 보지
    // 못했습니다. 양쪽 optional 접근 — 열기 전체가 하나의 try/catch 안이라
    // 여기서 던지면 units·camera·styles 까지 건너뛰고 "불러오기 실패"가 됩니다.
    const engineGroups = bridge.getAllGroups?.() ?? [];
    if (engineGroups.length > 0) {
      toolManager.selection.syncGroupsFromWasm?.(
        engineGroups.map((g) => ({ id: g.id, faceIds: g.faceIds })),
      );
      debugLog(`[Open] ${engineGroups.length} group(s) restored`);
    }
  };
  const saveProject = () => {
    // ADR-078 P-3 — push group tags to bridge BEFORE exportSnapshot.
    pushGroupTagsToBridge();

    const snapshot = bridge.exportSnapshot();
    if (!snapshot) {
      console.warn('[Save] WASM export_snapshot not available (WASM rebuild needed)');
      saveFallback();
      return;
    }

    const project = {
      format: 'xia',
      version: '1.0.0',
      engine: 'AXiA 3D',
      created: new Date().toISOString(),
      units: {
        unit: units.unit,
        precision: units.precision,
      },
      camera: viewport.getCameraState(),
      style: viewport.getStyleSettings(),
      mesh: toBase64(snapshot),
    };

    const json = JSON.stringify(project, null, 2);
    const blob = new Blob([json], { type: 'application/json' });
    const url = URL.createObjectURL(blob);

    const a = document.createElement('a');
    a.href = url;
    a.download = `AXiA_Project_${new Date().toISOString().slice(0, 10)}.xia`;
    a.click();
    URL.revokeObjectURL(url);
    debugLog('[Save] Project saved:', json.length, 'bytes');
  };

  /** .xia 프로젝트 파일 열기 */
  const openProject = () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.xia';
    input.style.display = 'none';
    document.body.appendChild(input);

    // Cleanup helper — removes DOM element and listeners exactly once
    let cleaned = false;
    const cleanup = () => {
      if (cleaned) return;
      cleaned = true;
      input.removeEventListener('change', onChange);
      input.removeEventListener('cancel', onCancel);
      if (input.parentNode) input.parentNode.removeChild(input);
    };

    const onChange = async () => {
      const file = input.files?.[0];
      cleanup();

      if (!file) return;

      try {
        // 두 저장 경로가 서로 다른 포맷을 씁니다 (2026-08-23 측정):
        //   메뉴 "저장"              -> JSON
        //   "다른 이름으로 저장"      -> [magic]['AXIA'][version][len][meta][snapshot]
        // 열기는 JSON 만 읽어서, 후자로 저장한 파일은 이 앱에서 다시 열리지
        // 않았습니다. 앞 4바이트를 보고 갈라 둘 다 받습니다.
        // ⚠ Sniff from TEXT, not bytes. `openProject` has always read via
        // `file.text()`, and ProjectSerializer.test.ts supplies exactly that
        // one reader on its fixtures — switching the first read to
        // `arrayBuffer()` broke three of its tests with
        // "file.arrayBuffer is not a function". The magic is four ASCII bytes,
        // so the text prefix carries it, and bytes are only fetched once we
        // know we need them.
        const text = await file.text();
        if (text.startsWith(AXIA_MAGIC_PREFIX)) {
          const fm = deps.fileManager;
          if (!fm) {
            alert(t('이 파일은 바이너리 .xia 입니다 — 열 수 없습니다.'));
            return;
          }
          // FileManager 가 materials / curves / 파일명 복원 + importSnapshot 까지
          // 합니다. 뷰포트 갱신과 그룹 복원은 그쪽이 하지 않으므로 여기서.
          const bytes = new Uint8Array(await file.arrayBuffer());
          const ok = await fm.loadFromArrayBuffer(bytes, file.name);
          if (!ok) {
            alert(t('파일을 불러오는데 실패했습니다.'));
            return;
          }
          restoreViewAfterImport();
          debugLog('[Open] binary .xia restored');
          return;
        }
        const project = JSON.parse(text);

        if (project.format !== 'xia') {
          alert(t('올바른 .xia 파일이 아닙니다.'));
          return;
        }

        // 메시 복원
        if (project.mesh) {
          const data = fromBase64(project.mesh);
          const ok = bridge.importSnapshot(data);
          if (ok) {
            restoreViewAfterImport();
            debugLog('[Open] Mesh restored from snapshot');
          } else {
            console.error('[Open] importSnapshot failed');
          }
        }

        // 단위 복원
        if (project.units) {
          units.unit = project.units.unit;
          if (project.units.precision !== undefined) {
            units.precision = project.units.precision;
          }
        }

        // 카메라 복원
        if (project.camera) {
          viewport.setCameraState(project.camera);
        }

        // 스타일 복원
        if (project.style) {
          const s = project.style;
          viewport.updateBackground(s.bgMode, s.bgSkyColor, s.bgGroundColor, s.bgMidColor);
          if (s.frontColor !== undefined) viewport.setFaceColors(s.frontColor, s.backColor);
          if (s.edgeColor !== undefined) viewport.setEdgeStyle({ color: s.edgeColor, visible: s.edgeVisible });
          if (s.gridVisible !== undefined) viewport.setGridVisible(s.gridVisible);
          if (s.axisVisible !== undefined) viewport.setAxisVisible(s.axisVisible);
        }

        debugLog('[Open] Project loaded:', file.name);
      } catch (e) {
        console.error('[Open] Failed to load project:', e);
        alert(t('파일을 불러오는데 실패했습니다.'));
      }
    };

    const onCancel = () => {
      cleanup();
    };

    input.addEventListener('change', onChange);
    input.addEventListener('cancel', onCancel);
    input.click();
  };

  return { saveProject, openProject };
}
