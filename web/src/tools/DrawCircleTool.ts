/**
 * Draw Circle Tool — Supports drawing on any plane (ground, face, Z-axis wall, etc.)
 *
 * Flow:
 *   1st click → detect drawing plane (face normal or ground) + set center
 *   mouse move → ray ∩ drawing plane → preview circle
 *   2nd click → ray ∩ drawing plane → commit circle to engine
 *
 * After the first click establishes a plane, ALL subsequent mouse positions
 * are computed by intersecting the camera ray with that plane. This ensures
 * the radius point always lies on the drawing plane regardless of where the
 * mouse is pointing in 3D space.
 */

import * as THREE from 'three';
import { ITool, ToolContext, DrawPlaneInfo } from './ITool';
import { debugLog } from '../utils/debug';
import { getDrawCurveMode } from './DrawCurveSettings';
import { Toast } from '../ui/Toast';
import { humanizeEngineError } from '../bridge/humanizeEngineError';

/** Max distance from center to prevent runaway geometry when ray grazes the plane */
const MAX_DRAW_DISTANCE = 50000;

export class DrawCircleTool implements ITool {
  readonly name = 'circle';

  private ctx: ToolContext;
  private circleCenter: THREE.Vector3 | null = null;
  private circlePreview: THREE.Line | null = null;
  private circleFill: THREE.Mesh | null = null;

  // Drawing plane (detected at first click)
  private plane: DrawPlaneInfo | null = null;
  private drawPlane3: THREE.Plane | null = null; // Three.js Plane for ray intersection

  // ADR-202 β-3c — Sphere face draw-on-surface mode (S9 곡면 위 닫힌 원).
  private sphereMode = false;
  private sphereHostFace = -1;

  // ADR-257 β-7 — Cylinder side-face draw-on-surface mode (S9-cylinder 곡면 벽 포트홀).
  private cylinderMode = false;
  private cylinderHostFace = -1;
  // ADR-263 β-3 — Cone wall circle sketching (surfaceKind===4).
  private coneMode = false;
  private coneHostFace = -1;
  // ADR-263 β-6 — Torus wall circle sketching (surfaceKind===5).
  private torusMode = false;
  private torusHostFace = -1;

  constructor(ctx: ToolContext) {
    this.ctx = ctx;
  }

  onActivate(): void {
    debugLog('[DrawCircleTool] Activated');
  }

  onDeactivate(): void {
    this.cleanup();
  }

  onMouseDown(e: MouseEvent, point: THREE.Vector3 | null): void {
    if (!this.circleCenter) {
      // ═══ First click: detect drawing plane + set center ═══
      if (!point) return;
      this.plane = this.ctx.getDrawPlane(e);

      // ADR-202 β-3c — first click on a Sphere face → draw the circle ON the
      // sphere (the engine projects center/radius onto the sphere + splits the
      // face into cap + annulus). Capture the host face + the surface hit point.
      this.sphereMode = false;
      this.sphereHostFace = -1;
      if (this.plane.surfaceKind === 3 && this.plane.origin
          && typeof this.ctx.bridge.drawCircleOnSphere === 'function') {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        if (hit && hit.faceIndex != null) {
          const fid = this.ctx.getFaceId(hit.faceIndex);
          if (fid >= 0) {
            this.sphereMode = true;
            this.sphereHostFace = fid;
          }
        }
      }

      // ADR-257 β-7 — first click on a Cylinder side face (surfaceKind===2) →
      // draw the geodesic porthole ON the wall (engine builds the geodesic
      // circle + splits the face into cap + remainder). Mirror of the sphere
      // branch above.
      this.cylinderMode = false;
      this.cylinderHostFace = -1;
      if (this.plane.surfaceKind === 2 && this.plane.origin
          && typeof this.ctx.bridge.drawCircleOnCylinder === 'function') {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        if (hit && hit.faceIndex != null) {
          const fid = this.ctx.getFaceId(hit.faceIndex);
          if (fid >= 0) {
            this.cylinderMode = true;
            this.cylinderHostFace = fid;
          }
        }
      }

      // ADR-263 β-3 — first click on a Cone side face (surfaceKind===4) →
      // draw the geodesic porthole ON the wall. Mirror of the cylinder branch.
      this.coneMode = false;
      this.coneHostFace = -1;
      if (this.plane.surfaceKind === 4 && this.plane.origin
          && typeof this.ctx.bridge.drawCircleOnCone === 'function') {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        if (hit && hit.faceIndex != null) {
          const fid = this.ctx.getFaceId(hit.faceIndex);
          if (fid >= 0) {
            this.coneMode = true;
            this.coneHostFace = fid;
          }
        }
      }

      // ADR-263 β-6 — first click on a Torus face (surfaceKind===5) → draw the
      // porthole ON the wall. Mirror of the cone branch.
      this.torusMode = false;
      this.torusHostFace = -1;
      if (this.plane.surfaceKind === 5 && this.plane.origin
          && typeof this.ctx.bridge.drawCircleOnTorus === 'function') {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        if (hit && hit.faceIndex != null) {
          const fid = this.ctx.getFaceId(hit.faceIndex);
          if (fid >= 0) {
            this.torusMode = true;
            this.torusHostFace = fid;
          }
        }
      }

      this.circleCenter = point.clone();
      // ADR-202 β-3c / ADR-257 β-7 — surface mode uses the exact raycast hit
      // point on the sphere/cylinder (the engine projects it onto the surface).
      if ((this.sphereMode || this.cylinderMode || this.coneMode || this.torusMode) && this.plane.origin) {
        this.circleCenter = this.plane.origin.clone();
      }

      // 2026-04-28 — 바닥면 (default cardinal plane) 에서 z/y/x 좌표 정확히 0.
      //   Mouse picking 의 ray-plane intersection ε 오차 흡수. (곡면 mode 제외)
      if (!this.sphereMode && !this.cylinderMode && !this.coneMode && !this.torusMode && !this.plane.onFace) {
        const n = this.plane.normal;
        if (Math.abs(n.x) > 0.999) this.circleCenter.x = 0;
        else if (Math.abs(n.y) > 0.999) this.circleCenter.y = 0;
        else if (Math.abs(n.z) > 0.999) this.circleCenter.z = 0;
      }

      // Build Three.js Plane from normal + coplanar point for future ray intersections
      this.drawPlane3 = new THREE.Plane().setFromNormalAndCoplanarPoint(
        this.plane.normal, this.circleCenter,
      );

      this.ctx.snap.setReferencePoint(point);

      // ADR-166 β-2 — first_click plane lock (idempotent: no-op when
      // already locked, L-166-2). Cross-tool 유지 활성화.
      this.ctx.lockPlane?.({
        origin: this.circleCenter,
        normal: this.plane.normal,
        up: this.plane.up,
        source: 'first_click',
      });
    } else {
      // ═══ Second click ═══
      // ADR-202 β-3c — Sphere mode → draw the closed circle ON the sphere.
      if (this.sphereMode && this.sphereHostFace >= 0 && this.circleCenter) {
        // radius point: prefer the sphere surface hit; else the tangent-plane
        // point (the engine projects it onto the sphere either way).
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        const radiusPt = (hit && hit.point) ? hit.point : this.getPointOnDrawPlane(e);
        if (radiusPt) {
          this.ctx.bridge.drawCircleOnSphere(
            this.sphereHostFace,
            [this.circleCenter.x, this.circleCenter.y, this.circleCenter.z],
            [radiusPt.x, radiusPt.y, radiusPt.z],
          );
          debugLog(`[Circle/Sphere] host=${this.sphereHostFace} drawn on sphere`);
          this.ctx.syncMesh();
        }
        this.cleanup();
        return;
      }

      // ADR-257 β-7 — Cylinder mode → draw the geodesic porthole ON the wall.
      if (this.cylinderMode && this.cylinderHostFace >= 0 && this.circleCenter) {
        // radius point: prefer the cylinder surface hit; else the tangent-plane
        // point (the engine projects it onto the cylinder either way).
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        const radiusPt = (hit && hit.point) ? hit.point : this.getPointOnDrawPlane(e);
        if (radiusPt) {
          this.ctx.bridge.drawCircleOnCylinder(
            this.cylinderHostFace,
            [this.circleCenter.x, this.circleCenter.y, this.circleCenter.z],
            [radiusPt.x, radiusPt.y, radiusPt.z],
          );
          debugLog(`[Circle/Cylinder] host=${this.cylinderHostFace} drawn on cylinder`);
          this.ctx.syncMesh();
        }
        this.cleanup();
        return;
      }

      // ADR-263 β-3 — Cone mode → draw the geodesic porthole ON the wall.
      if (this.coneMode && this.coneHostFace >= 0 && this.circleCenter) {
        // radius point: prefer the cone surface hit; else the tangent-plane
        // point (the engine projects it onto the cone either way).
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        const radiusPt = (hit && hit.point) ? hit.point : this.getPointOnDrawPlane(e);
        if (radiusPt) {
          this.ctx.bridge.drawCircleOnCone(
            this.coneHostFace,
            [this.circleCenter.x, this.circleCenter.y, this.circleCenter.z],
            [radiusPt.x, radiusPt.y, radiusPt.z],
          );
          debugLog(`[Circle/Cone] host=${this.coneHostFace} drawn on cone`);
          this.ctx.syncMesh();
        }
        this.cleanup();
        return;
      }

      // ADR-263 β-6 — Torus mode → draw the porthole ON the wall.
      if (this.torusMode && this.torusHostFace >= 0 && this.circleCenter) {
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        const radiusPt = (hit && hit.point) ? hit.point : this.getPointOnDrawPlane(e);
        if (radiusPt) {
          this.ctx.bridge.drawCircleOnTorus(
            this.torusHostFace,
            [this.circleCenter.x, this.circleCenter.y, this.circleCenter.z],
            [radiusPt.x, radiusPt.y, radiusPt.z],
          );
          debugLog(`[Circle/Torus] host=${this.torusHostFace} drawn on torus`);
          this.ctx.syncMesh();
        }
        this.cleanup();
        return;
      }

      // ═══ Planar: intersect ray with drawing plane → create circle ═══
      const planePoint = this.getPointOnDrawPlane(e);
      if (!planePoint || !this.plane) {
        this.cleanup();
        return;
      }

      const radius = this.circleCenter.distanceTo(planePoint);
      if (radius > 1) {
        const n = this.plane.normal;
        // ADR-089 A-λ-β — DrawCurveSettings flag check.
        // Curve mode (opt-in): kernel-native closed-curve face
        // (1 vert + 1 self-loop edge with AnalyticCurve::Circle).
        // Legacy mode (default): 24-segment polygon Shape (ADR-087 K-ε).
        if (getDrawCurveMode()) {
          this.ctx.bridge.drawCircleAsCurve(
            this.circleCenter.x, this.circleCenter.y, this.circleCenter.z,
            n.x, n.y, n.z,
            radius,
          );
          debugLog(`[Circle/Curve] Kernel-native R=${radius.toFixed(2)} on plane (${n.x.toFixed(2)},${n.y.toFixed(2)},${n.z.toFixed(2)})`);
        } else {
          this.ctx.bridge.drawCircleAsShape(
            this.circleCenter.x, this.circleCenter.y, this.circleCenter.z,
            n.x, n.y, n.z,
            radius, 24,
          );
          debugLog(`[Circle] Created on plane (${n.x.toFixed(2)},${n.y.toFixed(2)},${n.z.toFixed(2)}): R=${radius.toFixed(2)}`);
        }
        // ADR-164 β-2 — Sticky last drawn plane (Q1=a face 합성 성공 후).
        this.ctx.setLastDrawnPlane?.({
          origin: this.circleCenter,
          normal: n,
          up: this.plane.up,
          source: 'view',
        });
        this.ctx.syncMesh();
      }
      this.cleanup();
    }
  }

  onMouseMove(e: MouseEvent, _point: THREE.Vector3 | null): void {
    if (!this.circleCenter || !this.plane) {
      this.removePreview();
      return;
    }

    // Always use drawing plane intersection (not raw 3D point)
    const planePoint = this.getPointOnDrawPlane(e);
    if (!planePoint) {
      this.removePreview();
      return;
    }

    const radius = this.circleCenter.distanceTo(planePoint);
    if (radius > 0.1) {
      // ADR-290 곡면 편집 마무리 — on a curved host (Sphere/Cylinder/Cone/Torus),
      // draw the on-surface preview (follows the surface) instead of the flat
      // tangent-plane circle. Fall back to the flat preview if unavailable.
      const hostFace = this.activeCurvedHostFace();
      let drewCurved = false;
      if (hostFace >= 0) {
        // Prefer the surface hit for the radius reference (engine projects
        // onto the surface either way, matching the commit path).
        const hit = this.ctx.viewport.pick(e.clientX, e.clientY);
        const radiusPt = (hit && hit.point) ? hit.point : planePoint;
        drewCurved = this.updateCurvedPreview(this.circleCenter, radiusPt, hostFace);
      }
      if (!drewCurved) this.updatePreview(this.circleCenter, radius);

      // Dimension label: from center to current point on plane
      this.ctx.dimLabel.update(this.ctx.viewport.activeCamera, [
        { from: this.circleCenter.clone(), to: planePoint, text: 'R ' + this.ctx.units.format(radius), color: '#da77f2' },
      ]);
    }
  }

  onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      this.cleanup();
    }
  }

  applyVCBValue(value: number): void {
    if (!this.circleCenter) return;

    // ADR-284 follow-up — a typed radius on a curved host.
    //
    // The mouse path takes a POINT, so "50" can only be honoured by placing the
    // radius point whose GEODESIC distance is 50. Offsetting 50 in the tangent
    // plane is not that: it lands ~2% short at r=200/d=50 and ~7% at d=100 — a
    // quietly wrong dimension in a tool whose whole point is being more precise
    // than SketchUp. This path used to fall through and draw a flat circle
    // regardless, then (7c6e4c2) declined rather than approximate. The engine
    // now answers it exactly, so ask.
    //
    // Still fail-closed: if the engine cannot answer (degenerate ask, a sphere
    // radius past half a turn, an older build without the export), decline as
    // before rather than fall back to the flat circle this is here to prevent.
    const curvedHost = this.sphereMode ? this.sphereHostFace
      : this.cylinderMode ? this.cylinderHostFace
        : this.coneMode ? this.coneHostFace
          : this.torusMode ? this.torusHostFace
            : -1;
    if (curvedHost >= 0) {
      const c = this.circleCenter;
      const rp = this.ctx.bridge.surfacePointAtGeodesicDistance?.(
        curvedHost, [c.x, c.y, c.z], value,
      ) ?? null;
      if (!rp) {
        Toast.warning('이 곡면에는 그 반지름으로 원을 그릴 수 없습니다 — 마우스로 지정해 주세요', 3500);
        this.cleanup();
        return;
      }
      const draw = this.sphereMode ? this.ctx.bridge.drawCircleOnSphere
        : this.cylinderMode ? this.ctx.bridge.drawCircleOnCylinder
          : this.coneMode ? this.ctx.bridge.drawCircleOnCone
            : this.ctx.bridge.drawCircleOnTorus;
      const res = draw?.call(this.ctx.bridge, curvedHost, [c.x, c.y, c.z], rp);
      if (!res || res.includes('"error"')) {
        Toast.warning(
          humanizeEngineError(this.ctx.bridge.lastError())
            || '이 곡면에는 원을 그릴 수 없습니다',
          3500,
        );
      } else {
        debugLog(`[VCB/Circle/Curved] host=${curvedHost} geodesic R=${value}`);
      }
      this.ctx.syncMesh();
      this.cleanup();
      return;
    }

    // ADR-103-δ-1 (Z-up): fallback plane = XY ground (Z=0), normal +Z.
    const plane = this.plane || {
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: false,
    };

    const n = plane.normal;
    // ADR-089 A-λ-β — DrawCurveSettings flag check (VCB path).
    if (getDrawCurveMode()) {
      this.ctx.bridge.drawCircleAsCurve(
        this.circleCenter.x, this.circleCenter.y, this.circleCenter.z,
        n.x, n.y, n.z,
        value,
      );
      debugLog(`[VCB/Circle/Curve] Kernel-native R=${value} on plane (${n.x.toFixed(2)},${n.y.toFixed(2)},${n.z.toFixed(2)})`);
    } else {
      this.ctx.bridge.drawCircleAsShape(
        this.circleCenter.x, this.circleCenter.y, this.circleCenter.z,
        n.x, n.y, n.z,
        value, 24,
      );
      debugLog(`[VCB/Circle] R=${value} on plane (${n.x.toFixed(2)},${n.y.toFixed(2)},${n.z.toFixed(2)})`);
    }
    this.cleanup();
    this.ctx.syncMesh();
  }

  isBusy(): boolean {
    return this.circleCenter !== null;
  }

  cleanup(): void {
    this.circleCenter = null;
    this.plane = null;
    this.drawPlane3 = null;
    this.sphereMode = false;
    this.sphereHostFace = -1;
    this.cylinderMode = false;
    this.cylinderHostFace = -1;
    this.coneMode = false;
    this.coneHostFace = -1;
    this.torusMode = false;
    this.torusHostFace = -1;
    this.removePreview();
    this.ctx.dimLabel.clear();
    this.ctx.snap.setReferencePoint(null);
  }

  // ═══════════════════════════════════════════════════
  //  Drawing Plane Ray Intersection
  // ═══════════════════════════════════════════════════

  /**
   * Get a point on the drawing plane by intersecting the camera ray with it.
   * Returns null if the ray is nearly parallel to the plane (grazing angle)
   * or if the intersection is too far from the center point.
   */
  private getPointOnDrawPlane(e: MouseEvent): THREE.Vector3 | null {
    if (!this.drawPlane3 || !this.circleCenter) return null;

    // First check snap — if there's a snap point, project it onto the plane
    const rawPt = this.ctx.get3DPoint(e);
    const snapped = this.ctx.getSnappedPoint(e, rawPt);
    let result: THREE.Vector3 | null = null;
    if (snapped) {
      result = this.projectOntoPlane(snapped);
    } else {
      // No snap — intersect camera ray with drawing plane
      const ray = this.ctx.getRay(e);
      const target = new THREE.Vector3();
      const hit = ray.ray.intersectPlane(this.drawPlane3, target);
      if (!hit) return null;
      const dist = target.distanceTo(this.circleCenter);
      if (dist > MAX_DRAW_DISTANCE) return null;
      result = target;
    }
    if (!result) return null;

    // 2026-04-29 — 사용자 요청: 바닥면 cardinal plane 에서 normal-axis 좌표를
    //   circleCenter 의 같은 좌표 (정확히 0) 로 강제. f32 ray-plane intersection
    //   ε 오차 차단.
    if (this.plane && !this.plane.onFace) {
      const n = this.plane.normal;
      if (Math.abs(n.x) > 0.999) result.x = this.circleCenter.x;
      else if (Math.abs(n.y) > 0.999) result.y = this.circleCenter.y;
      else if (Math.abs(n.z) > 0.999) result.z = this.circleCenter.z;
    }
    return result;
  }

  /**
   * Project a 3D point onto the drawing plane (along the plane normal).
   */
  private projectOntoPlane(point: THREE.Vector3): THREE.Vector3 {
    if (!this.drawPlane3) return point.clone();
    const projected = point.clone();
    const dist = this.drawPlane3.distanceToPoint(projected);
    projected.addScaledVector(this.drawPlane3.normal, -dist);
    return projected;
  }

  // ═══════════════════════════════════════════════════
  //  Preview Rendering
  // ═══════════════════════════════════════════════════

  /** ADR-290 — the active curved host face id (Sphere/Cylinder/Cone/Torus), or -1. */
  private activeCurvedHostFace(): number {
    if (this.sphereMode && this.sphereHostFace >= 0) return this.sphereHostFace;
    if (this.cylinderMode && this.cylinderHostFace >= 0) return this.cylinderHostFace;
    if (this.coneMode && this.coneHostFace >= 0) return this.coneHostFace;
    if (this.torusMode && this.torusHostFace >= 0) return this.torusHostFace;
    return -1;
  }

  /** ADR-290 — draw the on-surface circle preview (follows the curved host) via
   *  the read-only engine query. Returns false (preview left cleared) when the
   *  engine has no on-surface polyline, so the caller can fall back to flat. */
  private updateCurvedPreview(
    center: THREE.Vector3,
    radiusPt: THREE.Vector3,
    hostFace: number,
  ): boolean {
    this.removePreview();
    if (typeof this.ctx.bridge.previewCircleOnSurface !== 'function') return false;
    const poly = this.ctx.bridge.previewCircleOnSurface(
      hostFace,
      [center.x, center.y, center.z],
      [radiusPt.x, radiusPt.y, radiusPt.z],
    );
    if (!poly || poly.length < 9) return false;
    const points: THREE.Vector3[] = [];
    for (let i = 0; i + 2 < poly.length; i += 3) {
      points.push(new THREE.Vector3(poly[i], poly[i + 1], poly[i + 2]));
    }
    if (points.length < 3) return false;
    // ensure a visually closed loop
    points.push(points[0].clone());
    const lineGeo = new THREE.BufferGeometry().setFromPoints(points);
    const lineMat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 1 });
    this.circlePreview = new THREE.Line(lineGeo, lineMat);
    this.circlePreview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.circlePreview);
    return true;
  }

  private updatePreview(center: THREE.Vector3, radius: number): void {
    this.removePreview();
    if (!this.plane) return;

    const n = this.plane.normal;
    const r = this.plane.right;
    const u = this.plane.up;
    const segments = 48;

    // ── Circle outline on the detected plane ──
    const points: THREE.Vector3[] = [];
    for (let i = 0; i <= segments; i++) {
      const angle = (i / segments) * Math.PI * 2;
      const cos = Math.cos(angle);
      const sin = Math.sin(angle);
      points.push(
        center.clone()
          .addScaledVector(r, cos * radius)
          .addScaledVector(u, sin * radius)
          .addScaledVector(n, 0.5),
      );
    }
    const lineGeo = new THREE.BufferGeometry().setFromPoints(points);
    const lineMat = new THREE.LineBasicMaterial({ color: 0xda77f2, linewidth: 1 });
    this.circlePreview = new THREE.Line(lineGeo, lineMat);
    this.circlePreview.renderOrder = 999;
    this.ctx.viewport.scene.add(this.circlePreview);

    // ── Semi-transparent fill ──
    const fillGeo = new THREE.CircleGeometry(radius, segments);
    const fillMat = new THREE.MeshBasicMaterial({
      color: 0xda77f2,
      transparent: true,
      opacity: 0.15,
      side: THREE.DoubleSide,
      depthWrite: false,
    });
    this.circleFill = new THREE.Mesh(fillGeo, fillMat);

    // Rotate CircleGeometry (default normal = +Z) to match drawing plane normal
    const defaultNormal = new THREE.Vector3(0, 0, 1);
    const quat = new THREE.Quaternion().setFromUnitVectors(defaultNormal, n);
    this.circleFill.quaternion.copy(quat);

    // Offset slightly along normal to prevent z-fighting
    const offset = center.clone().addScaledVector(n, 0.5);
    this.circleFill.position.copy(offset);
    this.circleFill.renderOrder = 998;
    this.ctx.viewport.scene.add(this.circleFill);
  }

  private removePreview(): void {
    if (this.circlePreview) {
      this.ctx.viewport.scene.remove(this.circlePreview);
      this.circlePreview.geometry.dispose();
      (this.circlePreview.material as THREE.Material).dispose();
      this.circlePreview = null;
    }
    if (this.circleFill) {
      this.ctx.viewport.scene.remove(this.circleFill);
      this.circleFill.geometry.dispose();
      (this.circleFill.material as THREE.Material).dispose();
      this.circleFill = null;
    }
  }
}
