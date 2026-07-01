/**
 * Mirror Tool — reflect the selected faces across a world plane, interactively
 * (ADR-209). The one-shot `mirror-x/y/z` actions still exist; this tool adds a
 * *mode* with a live mirror-plane indicator + axis keys + repeat + Esc.
 *
 * Flow:
 *   select faces → enter Mirror tool → a translucent mirror plane shows where the
 *   reflection lands → X / Y / Z switches the plane → click (or Enter) commits a
 *   reflected copy (mirrorFaces) → the tool stays active for repeated mirrors → Esc.
 *
 * Engine + WASM + bridge (mirrorFaces) already exist; this is UI-only interactive
 * polish (Pattern-12).
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';

type Axis = 'x' | 'y' | 'z';

export class MirrorTool implements ITool {
  readonly name = 'mirror';

  private ctx: ToolContext;
  private axis: Axis = 'x';
  private planeVisual: THREE.Mesh | null = null;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    this.updatePlaneVisual();
    Toast.info('미러 평면 위 X/Y/Z 키로 축 선택 → 클릭(또는 Enter)으로 반사, Esc 종료', 3500);
    debugLog('[MirrorTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    this.commit();
  }

  onMouseMove(_e: MouseEvent, _point: THREE.Vector3 | null): void {
    // static mirror-plane indicator (no per-move work)
  }

  onKeyDown(e: KeyboardEvent): void {
    const k = e.key.toLowerCase();
    if (k === 'escape') { this.cleanup(); return; }
    if (k === 'enter') { this.commit(); return; }
    if (k === 'x' || k === 'y' || k === 'z') {
      this.axis = k;
      this.updatePlaneVisual();
    }
  }

  isBusy(): boolean {
    // Mirror is a stateless mode (commit per click); never blocks tool switching.
    return false;
  }

  cleanup(): void {
    this.removePlaneVisual();
  }

  private commit(): void {
    const faces = this.ctx.getSelectedFaces();
    const edges = this.ctx.selection.getSelectedEdges();
    if (faces.length === 0 && edges.length === 0) {
      Toast.warning('미러링할 면 또는 엣지를 먼저 선택하세요', 2000);
      return;
    }
    const [nx, ny, nz] = this.axis === 'x' ? [1, 0, 0] : this.axis === 'y' ? [0, 1, 0] : [0, 0, 1];
    const plane = this.axis === 'x' ? 'YZ' : this.axis === 'y' ? 'XZ' : 'XY';
    // Faces take precedence; fall back to a wire-edge mirror (ADR-214).
    const out = faces.length > 0
      ? this.ctx.bridge.mirrorFaces(faces, 0, 0, 0, nx, ny, nz)
      : this.ctx.bridge.mirrorEdges(edges, 0, 0, 0, nx, ny, nz);
    if (out.length > 0) {
      this.ctx.syncMesh();
      const kind = faces.length > 0 ? '면' : '엣지';
      const n = faces.length > 0 ? faces.length : edges.length;
      Toast.info(`${n}개 ${kind}를 ${plane} 평면 기준 미러링 (${out.length}개 생성)`, 2000);
      debugLog(`[Mirror] ${out.length} mirrored ${faces.length > 0 ? 'faces' : 'edges'} across ${plane}`);
    } else {
      Toast.fromBridgeError(this.ctx.bridge, '미러링 실패');
    }
  }

  // ═══════════════════════════════════════════════════
  //  Live mirror-plane indicator
  // ═══════════════════════════════════════════════════

  private updatePlaneVisual(): void {
    this.removePlaneVisual();
    // World mirror plane: x=0 (YZ) for axis x, etc. Normal = axis.
    const normal = new THREE.Vector3(
      this.axis === 'x' ? 1 : 0,
      this.axis === 'y' ? 1 : 0,
      this.axis === 'z' ? 1 : 0,
    );
    const color = this.axis === 'x' ? 0xff6b6b : this.axis === 'y' ? 0x51cf66 : 0x5b9bd5;
    const geo = new THREE.PlaneGeometry(4000, 4000);
    const mat = new THREE.MeshBasicMaterial({
      color, transparent: true, opacity: 0.12, side: THREE.DoubleSide, depthWrite: false,
    });
    const mesh = new THREE.Mesh(geo, mat);
    // PlaneGeometry default normal = +Z → rotate to the mirror plane normal.
    const quat = new THREE.Quaternion().setFromUnitVectors(new THREE.Vector3(0, 0, 1), normal);
    mesh.quaternion.copy(quat);
    mesh.renderOrder = 997;
    this.planeVisual = mesh;
    this.ctx.viewport.scene.add(mesh);
  }

  private removePlaneVisual(): void {
    if (this.planeVisual) {
      this.ctx.viewport.scene.remove(this.planeVisual);
      this.planeVisual.geometry.dispose();
      (this.planeVisual.material as THREE.Material).dispose();
      this.planeVisual = null;
    }
  }
}
