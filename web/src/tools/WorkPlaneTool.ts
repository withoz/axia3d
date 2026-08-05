/**
 * Work Plane — three clicks define the plane everything is then drawn on.
 *
 * Measured 2026-08-05: the engine has always been free of direction. A rect and
 * a circle drawn on the normal (0.577, 0.577, 0.577) come out with zero
 * invariant violations. `enterSketch` has always accepted any (origin, normal,
 * up, right), and both `getDrawPlane` and `get3DPoint` consult the sketch before
 * a face hit — so a plane set this way governs every tool.
 *
 * What did not exist was a way for the user to NAME one. The entries were XZ,
 * XY, YZ, a selected face and auto: three cardinal planes plus whatever face
 * happens to be there. An oblique plane in open space could not be reached at
 * all. Three points can express any plane, so that is what this asks for.
 *
 * The points come through the ordinary snap path, which is the point of using
 * clicks rather than typed numbers: "the plane through those two corners and
 * that vertex" is a thing you can say by pointing at them.
 */

import * as THREE from 'three';
import { ITool, ToolContext } from './ITool';
import { Toast } from '../ui/Toast';
import { debugLog } from '../utils/debug';
import { t } from '../i18n';

/** Below this the three points are too close to a line to give a normal. */
const MIN_NORMAL = 1e-6;

export class WorkPlaneTool implements ITool {
  readonly name = 'workplane';
  readonly wantsSnap = true;

  private ctx: ToolContext;
  private pts: THREE.Vector3[] = [];
  /** The dots dropped so far, so the user can see what has been picked. */
  private marks: THREE.Object3D[] = [];

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[WorkPlaneTool] Activated');
    this.reset();
    Toast.info(t('작업 평면: 세 점을 클릭하세요 (1/3)'), 3000);
  }

  onDeactivate(): void {
    this.reset();
  }

  isBusy(): boolean {
    return this.pts.length > 0;
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    const raw = this.ctx.get3DPoint(e);
    const pt = this.ctx.getSnappedPoint?.(e, raw) ?? raw ?? point;
    if (!pt) return;

    // A repeat of a point already taken says nothing new, and three of them
    // would give no plane at all — say so rather than counting it.
    if (this.pts.some((q) => q.distanceTo(pt) < 1e-6)) {
      Toast.warning(t('같은 점입니다 — 다른 위치를 클릭하세요'), 1800);
      return;
    }

    this.pts.push(pt.clone());
    this.mark(pt);

    if (this.pts.length < 3) {
      Toast.info(t('작업 평면: 세 점을 클릭하세요 ({n}/3)', { n: String(this.pts.length + 1) }), 2000);
      return;
    }
    this.commit();
  }

  onMouseMove(): void { /* nothing to preview until the third point */ }
  onMouseUp(): void { /* click-based */ }

  onKeyDown(e: KeyboardEvent): boolean {
    if (e.key === 'Escape' && this.pts.length > 0) {
      this.reset();
      Toast.info(t('작업 평면 취소'), 1500);
      return true;
    }
    return false;
  }

  cancel(): void {
    this.reset();
  }

  private commit(): void {
    const [a, b, c] = this.pts;
    const normal = new THREE.Vector3()
      .crossVectors(new THREE.Vector3().subVectors(b, a), new THREE.Vector3().subVectors(c, a));
    if (normal.lengthSq() < MIN_NORMAL) {
      // Collinear — no plane. Keep the first two (they are still useful) and
      // ask again for a third off the line.
      this.pts.pop();
      this.marks.pop();
      Toast.warning(t('세 점이 한 직선 위에 있습니다 — 다른 점을 클릭하세요'), 2500);
      return;
    }
    normal.normalize();

    // `up` is world-up laid onto the plane, so a wall reads upright and a floor
    // keeps its north; when the plane IS horizontal that projection vanishes,
    // and the first edge is the only direction the user has actually given.
    const worldUp = new THREE.Vector3(0, 0, 1);
    let up = worldUp.clone().sub(normal.clone().multiplyScalar(worldUp.dot(normal)));
    if (up.lengthSq() < 1e-9) {
      up = new THREE.Vector3().subVectors(b, a).normalize();
    } else {
      up.normalize();
    }
    const right = new THREE.Vector3().crossVectors(up, normal).normalize();

    if (typeof this.ctx.enterSketch !== 'function') {
      Toast.error(t('작업 평면을 시작할 수 없습니다'), 2500);
      this.reset();
      return;
    }
    this.ctx.enterSketch({
      label: t('작업 평면 (3점)'),
      origin: a.clone(),
      normal,
      up,
      right,
    });
    debugLog(
      `[WorkPlaneTool] plane normal=(${normal.x.toFixed(3)}, ${normal.y.toFixed(3)}, ${normal.z.toFixed(3)})`,
    );
    Toast.info(t('✏️ 작업 평면 설정됨 — 모든 드로잉이 이 평면에 고정됩니다.'), 4000);
    this.reset();
  }

  /** A dot where a point was taken — three clicks with no feedback is a guess. */
  private mark(p: THREE.Vector3): void {
    const scene = (this.ctx.viewport as unknown as { scene?: THREE.Scene }).scene;
    if (!scene) return;
    try {
      const g = new THREE.SphereGeometry(2, 8, 6);
      const m = new THREE.MeshBasicMaterial({ color: 0xffaa33, depthTest: false });
      const dot = new THREE.Mesh(g, m);
      dot.position.copy(p);
      dot.renderOrder = 1002;
      scene.add(dot);
      this.marks.push(dot);
    } catch {
      // A viewport without a real scene (tests) simply gets no marks.
    }
  }

  private reset(): void {
    const scene = (this.ctx.viewport as unknown as { scene?: { remove: (o: THREE.Object3D) => void } }).scene;
    for (const m of this.marks) {
      scene?.remove(m);
      const mesh = m as THREE.Mesh;
      mesh.geometry?.dispose?.();
      (mesh.material as THREE.Material | undefined)?.dispose?.();
    }
    this.marks = [];
    this.pts = [];
  }
}
