/**
 * Draw Text3D Tool — place 3D text labels (ADR-228).
 *
 * onActivate() prompts for the string (localStorage default, DrawPolygonTool
 * pattern). Each click places that string at the cursor (continuous, like
 * DrawPointTool; Esc / tool-switch ends). Mode (Text3DSettings):
 *   'extruded' → true 3D TextGeometry mesh (Latin; Korean auto-falls-back to sprite)
 *   'sprite'   → canvas billboard label (any string incl. Korean)
 *
 * render-only Reference (메타-원칙 #2) — added to a scene-root overlay group
 * (Viewport.addTextObject), NOT injected into the engine DCEL. The geometry
 * builder (FontLoader/TextGeometry/font) is lazy-imported so the initial bundle
 * is untouched (ADR-035 P20.C #2).
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { getText3DMode } from './Text3DSettings';

const STORAGE_KEY = 'axia:text3d:last';

export class DrawText3DTool implements ITool {
  readonly name = 'text3d';
  readonly wantsSnap = true;

  private ctx: ToolContext;
  private text = '';

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    let prev = 'Text';
    try {
      prev = localStorage.getItem(STORAGE_KEY) || 'Text';
    } catch {
      /* private mode */
    }
    const input =
      typeof window !== 'undefined' && typeof window.prompt === 'function'
        ? window.prompt('3D 텍스트 내용:', prev)
        : prev;
    if (input == null || input.trim() === '') {
      this.text = '';
      Toast.info('3D 텍스트 취소됨 (다시 도구를 선택해 내용 입력)', 1800);
      return;
    }
    this.text = input;
    try {
      localStorage.setItem(STORAGE_KEY, input);
    } catch {
      /* ignore */
    }
    const mode = getText3DMode();
    Toast.info(
      `3D 텍스트 "${input}" — 클릭하여 배치 (${mode === 'extruded' ? '압출' : '스프라이트'} 모드, 연속, Esc 종료)`,
      4000,
    );
  }

  onDeactivate(): void {
    this.text = '';
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.text) {
      Toast.warning('텍스트 도구를 다시 선택해 내용을 입력하세요', 2000);
      return;
    }
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint(e, raw) ?? raw ?? point;
    if (!pt) {
      Toast.warning('텍스트를 배치할 위치를 클릭하세요', 1800);
      return;
    }
    const plane = this.ctx.getDrawPlane(e);
    void this._place(this.text, pt.clone(), plane);
  }

  private async _place(
    text: string,
    pos: THREE.Vector3,
    plane: ReturnType<ToolContext['getDrawPlane']> | null,
  ): Promise<void> {
    try {
      const builder = await import('./Text3DBuilder');
      const mode = getText3DMode();
      let obj: THREE.Object3D | null = null;
      if (mode === 'extruded') {
        obj = builder.buildExtrudedText(text);
        if (!obj) {
          // Latin font lacks these glyphs (e.g. Korean) → graceful sprite fallback.
          obj = builder.buildSpriteText(text);
          Toast.info('이 글자는 3D 폰트에 없어 스프라이트 라벨로 표시했습니다', 3000);
        }
      } else {
        obj = builder.buildSpriteText(text);
      }
      if (!obj) {
        Toast.warning('텍스트 생성 실패', 2000);
        return;
      }
      obj.position.copy(pos);
      // Extruded text orients to the draw plane (local XYZ → right/up/normal);
      // sprites are billboards (camera-facing) so they need no orientation.
      if (obj.name === 'text3d-extruded' && plane) {
        const m = new THREE.Matrix4().makeBasis(plane.right, plane.up, plane.normal);
        obj.quaternion.setFromRotationMatrix(m);
      }
      this.ctx.viewport.addTextObject(obj);
      debugLog(
        `[Text3D] "${text}" (${mode}) @ (${pos.x.toFixed(1)}, ${pos.y.toFixed(1)}, ${pos.z.toFixed(1)})`,
      );
    } catch (err) {
      Toast.warning('3D 텍스트 모듈 로드 실패', 2500);
      debugLog('[Text3D] build error', err);
    }
  }

  onKeyDown(_e: KeyboardEvent): void {
    // Esc handled by ToolManager (switches to select)
  }

  isBusy(): boolean {
    return false; // each click independent (continuous placement)
  }

  cleanup(): void {
    this.text = '';
  }
}
