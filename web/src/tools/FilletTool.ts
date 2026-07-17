/**
 * Fillet Tool — interactive edge-fillet mode (ADR-209). The one-shot fillet-edge
 * action still exists; this tool adds a persistent mode: select edge(s), type a
 * radius in the VCB (or click to reuse the last), commit, repeat.
 *
 * Engine + WASM + bridge (filletEdge) already exist → UI-only (Pattern-12).
 * A geometric ghost preview of the rounded fillet is deferred (the fillet result
 * is new swept geometry, not a simple transform).
 */

import * as THREE from 'three';
import { t } from '../i18n';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

const LS_KEY = 'axia:fillet:radius';
const SEGMENTS = 8;

export class FilletTool implements ITool {
  readonly name = 'fillet';

  private ctx: ToolContext;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    Toast.info(t('둥글릴 엣지를 선택하고 반지름을 입력하세요 (또는 클릭 = 마지막 값), Esc 종료'), 3500);
    debugLog('[FilletTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    this.commit(this.lastRadius());
  }

  onMouseMove(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    // no geometric ghost preview (deferred)
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') this.cleanup();
  }

  applyVCBValue(value: number): void {
    if (value > 0) this.commit(value);
  }

  isBusy(): boolean {
    return false;
  }

  cleanup(): void {
    // stateless
  }

  private lastRadius(): number {
    const v = Number(localStorage.getItem(LS_KEY) ?? '5');
    return Number.isFinite(v) && v > 0 ? v : 5;
  }

  private commit(radius: number): void {
    const edges = this.ctx.selection.getSelectedEdges();
    if (edges.length === 0) {
      Toast.warning(t('둥글릴 엣지를 먼저 선택하세요'), 2000);
      return;
    }
    let ok = 0;
    let firstErr = false;
    for (const eid of edges) {
      const n = this.ctx.bridge.filletEdge(eid, radius, SEGMENTS);
      if (n >= 0) ok++; else firstErr = true;
    }
    if (ok > 0) {
      try { localStorage.setItem(LS_KEY, String(radius)); } catch { /* ignore */ }
      this.ctx.syncMesh();
      Toast.info(t('엣지 필렛 완료 ({ok}개 · 반지름 {radius}mm)', { ok, radius }), 2000);
      debugLog(`[Fillet] ${ok}/${edges.length} edges, radius=${radius}`);
    } else if (firstErr) {
      Toast.fromBridgeError(this.ctx.bridge, t('필렛 실패 (3-way corner 등은 미지원)'));
    }
  }
}
