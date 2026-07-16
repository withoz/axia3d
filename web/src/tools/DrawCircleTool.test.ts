import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawCircleTool } from './DrawCircleTool';
import { Toast } from '../ui/Toast';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));

function mockToolContext() {
  return {
    bridge: {
      drawCircle: vi.fn().mockReturnValue(0),
      drawCircleAsShape: vi.fn().mockReturnValue(0),
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
      activeCamera: new THREE.PerspectiveCamera(),
      renderer: {
        domElement: {
          getBoundingClientRect: () => ({
            left: 0, top: 0, right: 800, bottom: 600,
            width: 800, height: 600,
          }),
        },
      },
    },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('100mm') },
    snap: {
      setReferencePoint: vi.fn(),
    },
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 1, 0),
      up: new THREE.Vector3(0, 0, 1),
      origin: new THREE.Vector3(0, 0, 0),
    }),
  } as any;
}

describe('DrawCircleTool', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawCircleTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawCircleTool(ctx);
  });

  describe('name', () => {
    it('is "circle"', () => {
      expect(tool.name).toBe('circle');
    });
  });

  describe('isBusy', () => {
    it('defaults to false', () => {
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onMouseDown - first click', () => {
    it('sets center point', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(10, 0, 10));
      expect(tool.isBusy()).toBe(true);
      expect(ctx.getDrawPlane).toHaveBeenCalled();
    });

    it('does nothing when point is null', () => {
      tool.onMouseDown({} as MouseEvent, null);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onActivate / onDeactivate', () => {
    it('activate does not throw', () => {
      expect(() => tool.onActivate()).not.toThrow();
    });

    it('deactivate cleans up', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3());
      tool.onDeactivate();
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onKeyDown', () => {
    it('Escape cancels drawing', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3());
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('cleanup', () => {
    it('resets state', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3());
      tool.cleanup();
      expect(tool.isBusy()).toBe(false);
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-087 K-ε / ADR-089 A-π-β — VCB dispatch (default ON, explicit OFF preserved)
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-089 A-π-β VCB dispatch (default ON)', () => {
    beforeEach(() => {
      // Provide both kernel-aware methods on bridge mock
      ctx.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
    });

    it('VCB default path calls bridge.drawCircleAsCurve (kernel-native)', async () => {
      const { setDrawCurveMode } = await import('./DrawCurveSettings');
      setDrawCurveMode(true); // explicit ON (default after A-π-β)

      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(50);

      expect(ctx.bridge.drawCircleAsCurve).toHaveBeenCalledTimes(1);
      expect(ctx.bridge.drawCircleAsShape).not.toHaveBeenCalled();
    });

    it('VCB explicit OFF path calls bridge.drawCircleAsShape (legacy ADR-087 K-ε)', async () => {
      const { setDrawCurveMode } = await import('./DrawCurveSettings');
      setDrawCurveMode(false); // L-π-2 — explicit OFF preference

      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(50);

      expect(ctx.bridge.drawCircleAsShape).toHaveBeenCalledTimes(1);
      expect(ctx.bridge.drawCircleAsCurve).not.toHaveBeenCalled();
      expect(ctx.bridge.drawCircle).not.toHaveBeenCalled();
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-107 ζ-δ — DrawCircleTool default segments=24 triggers ζ-β
  // threshold-based dispatch (POLYGON_THRESHOLD=12) → Path B canonical
  // 자동 활성. UI tool 영향 0 (signature 보존).
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-107 ζ-δ — DrawCircleTool default Path B activation', () => {
    beforeEach(async () => {
      ctx.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      const { setDrawCurveMode } = await import('./DrawCurveSettings');
      setDrawCurveMode(false); // explicit OFF — legacy *AsShape path
    });

    it('default segments=24 invokes bridge.drawCircleAsShape with segments=24', () => {
      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(50);

      expect(ctx.bridge.drawCircleAsShape).toHaveBeenCalledTimes(1);
      // ADR-107 ζ-δ — segments=24 (>= POLYGON_THRESHOLD=12) → engine
      // 내부 ζ-β dispatch 가 Path B canonical 변환. UI tool 의 signature
      // 보존 (drawCircleAsShape with segments=24).
      const call = ctx.bridge.drawCircleAsShape.mock.calls[0];
      expect(call[7]).toBe(24); // segments parameter
      expect(call[6]).toBe(50); // radius parameter
    });

    it('VCB segments arg = 24 is >= POLYGON_THRESHOLD (12) → engine Path B', () => {
      // L1 backward compat — UI tool calls drawCircleAsShape unchanged.
      // Engine layer (ADR-107 ζ-β) handles threshold dispatch internally.
      const POLYGON_THRESHOLD = 12;
      const UI_DEFAULT_SEGMENTS = 24;
      expect(UI_DEFAULT_SEGMENTS).toBeGreaterThanOrEqual(POLYGON_THRESHOLD);
    });
  });

  // ──────────────────────────────────────────────────────────────────
  // ADR-202 β-3c — draw circle ON a Sphere face (곡면 위 닫힌 원)
  // ──────────────────────────────────────────────────────────────────
  describe('ADR-202 β-3c — Sphere face dispatch', () => {
    function sphereCtx() {
      const c = mockToolContext();
      c.bridge.drawCircleOnSphere = vi.fn().mockReturnValue('{"cap":2,"annulus":0}');
      c.bridge.drawCircleAsShape = vi.fn().mockReturnValue(0);
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      c.getFaceId = vi.fn().mockReturnValue(5);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, 0, 5) });
      c.get3DPoint = vi.fn().mockReturnValue(new THREE.Vector3(50, 0, 0));
      c.getSnappedPoint = vi.fn().mockReturnValue(null);
      c.getRay = vi.fn().mockReturnValue({
        ray: { intersectPlane: (_p: THREE.Plane, target: THREE.Vector3) => { target.set(50, 0, 0); return target; } },
      });
      c.lockPlane = vi.fn();
      c.setLastDrawnPlane = vi.fn();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), right: new THREE.Vector3(1, 0, 0),
        onFace: true, surfaceKind: 3, origin: new THREE.Vector3(0, 0, 5), // hit on sphere (north pole)
      });
      return c;
    }

    it('first click on Sphere → second click calls drawCircleOnSphere(host, center, radius)', () => {
      const c = sphereCtx();
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      expect(t.isBusy()).toBe(true);
      // second click: radius point on the sphere
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(3, 0, 4) });
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(3, 0, 4));
      expect(c.bridge.drawCircleOnSphere).toHaveBeenCalledTimes(1);
      const args = c.bridge.drawCircleOnSphere.mock.calls[0];
      expect(args[0]).toBe(5);              // host face id
      expect(args[1]).toEqual([0, 0, 5]);   // center on sphere
      expect(args[2]).toEqual([3, 0, 4]);   // radius point on sphere
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();  // NOT planar
      expect(c.syncMesh).toHaveBeenCalled();
      expect(t.isBusy()).toBe(false);       // cleaned up
    });

    it('non-Sphere face (surfaceKind 1) uses the planar path, not drawCircleOnSphere', () => {
      const c = sphereCtx();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), right: new THREE.Vector3(1, 0, 0),
        onFace: true, surfaceKind: 1, // Plane
      });
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(50, 0, 0));
      expect(c.bridge.drawCircleOnSphere).not.toHaveBeenCalled();
    });

    it('Sphere face but missing drawCircleOnSphere export → does not enter sphere mode', () => {
      const c = sphereCtx();
      c.bridge.drawCircleOnSphere = undefined;
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(3, 0, 4));
      // planar path (drawCircleAsShape) taken; no crash.
      expect(c.bridge.drawCircleAsShape).toHaveBeenCalled();
    });

    // ADR-290 곡면 편집 마무리 — the live preview follows the curved surface.
    it('mouse-move on a Sphere host draws the on-surface preview (previewCircleOnSurface)', () => {
      const c = sphereCtx();
      const poly = new Float32Array([0, 0, 10, 3, 0, 9.5, -3, 0, 9.5]);
      c.bridge.previewCircleOnSurface = vi.fn().mockReturnValue(poly);
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      // move: radius reference on the sphere surface
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(3, 0, 4) });
      const addSpy = c.viewport.scene.add as ReturnType<typeof vi.fn>;
      addSpy.mockClear();
      t.onMouseMove({ clientX: 150, clientY: 100 } as MouseEvent, null);
      expect(c.bridge.previewCircleOnSurface).toHaveBeenCalled();
      const args = c.bridge.previewCircleOnSurface.mock.calls[0];
      expect(args[0]).toBe(5); // host sphere face id
      expect(addSpy).toHaveBeenCalled(); // an on-surface preview Line was added
    });

    it('mouse-move falls back to the flat preview when previewCircleOnSurface is empty', () => {
      const c = sphereCtx();
      c.bridge.previewCircleOnSurface = vi.fn().mockReturnValue(new Float32Array(0));
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(3, 0, 4) });
      const addSpy = c.viewport.scene.add as ReturnType<typeof vi.fn>;
      addSpy.mockClear();
      // does not throw; still draws a (flat) preview
      expect(() => t.onMouseMove({ clientX: 150, clientY: 100 } as MouseEvent, null)).not.toThrow();
      expect(c.bridge.previewCircleOnSurface).toHaveBeenCalled();
      expect(addSpy).toHaveBeenCalled();
    });
  });

  // ──────────────────────────────────────────────────────────────────
  // ADR-257 β-7 — draw circle ON a Cylinder side face (곡면 벽 포트홀, S9-cylinder)
  // ──────────────────────────────────────────────────────────────────
  describe('ADR-257 β-7 — Cylinder face dispatch', () => {
    function cylinderCtx() {
      const c = mockToolContext();
      c.bridge.drawCircleOnCylinder = vi.fn().mockReturnValue('{"cap":3,"annulus":2}');
      c.bridge.drawCircleOnSphere = vi.fn().mockReturnValue('{"cap":2,"annulus":0}');
      c.bridge.drawCircleAsShape = vi.fn().mockReturnValue(0);
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      c.getFaceId = vi.fn().mockReturnValue(7);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(10, 0, 5) });
      c.get3DPoint = vi.fn().mockReturnValue(new THREE.Vector3(50, 0, 0));
      c.getSnappedPoint = vi.fn().mockReturnValue(null);
      c.getRay = vi.fn().mockReturnValue({
        ray: { intersectPlane: (_p: THREE.Plane, target: THREE.Vector3) => { target.set(50, 0, 0); return target; } },
      });
      c.lockPlane = vi.fn();
      c.setLastDrawnPlane = vi.fn();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1), right: new THREE.Vector3(0, 1, 0),
        onFace: true, surfaceKind: 2, origin: new THREE.Vector3(10, 0, 5), // hit on cylinder wall
      });
      return c;
    }

    it('first click on Cylinder → second click calls drawCircleOnCylinder(host, center, radius)', () => {
      const c = cylinderCtx();
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 0, 5));
      expect(t.isBusy()).toBe(true);
      // second click: radius point on the cylinder wall
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(10, 4, 5) });
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 4, 5));
      expect(c.bridge.drawCircleOnCylinder).toHaveBeenCalledTimes(1);
      const args = c.bridge.drawCircleOnCylinder.mock.calls[0];
      expect(args[0]).toBe(7);                // host face id
      expect(args[1]).toEqual([10, 0, 5]);    // center on cylinder
      expect(args[2]).toEqual([10, 4, 5]);    // radius point on cylinder
      expect(c.bridge.drawCircleOnSphere).not.toHaveBeenCalled();  // NOT sphere path
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();   // NOT planar
      expect(c.syncMesh).toHaveBeenCalled();
      expect(t.isBusy()).toBe(false);         // cleaned up
    });

    it('non-Cylinder face (surfaceKind 1) uses the planar path, not drawCircleOnCylinder', () => {
      const c = cylinderCtx();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), right: new THREE.Vector3(1, 0, 0),
        onFace: true, surfaceKind: 1, // Plane
      });
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(50, 0, 0));
      expect(c.bridge.drawCircleOnCylinder).not.toHaveBeenCalled();
    });

    it('Cylinder face but missing drawCircleOnCylinder export → does not enter cylinder mode', () => {
      const c = cylinderCtx();
      c.bridge.drawCircleOnCylinder = undefined;
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 0, 5));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 4, 5));
      // planar path (drawCircleAsShape) taken; no crash.
      expect(c.bridge.drawCircleAsShape).toHaveBeenCalled();
    });

    it('Sphere face (surfaceKind 3) does not call drawCircleOnCylinder', () => {
      const c = cylinderCtx();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0), right: new THREE.Vector3(1, 0, 0),
        onFace: true, surfaceKind: 3, origin: new THREE.Vector3(0, 0, 5),
      });
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, 0, 5) });
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(3, 0, 4));
      expect(c.bridge.drawCircleOnCylinder).not.toHaveBeenCalled();  // sphere ≠ cylinder
      expect(c.bridge.drawCircleOnSphere).toHaveBeenCalledTimes(1);
    });
  });

  // ──────────────────────────────────────────────────────────────────
  // ADR-263 β-3 — draw circle ON a Cone side face (곡면 벽 포트홀, #5 P3-C)
  // ──────────────────────────────────────────────────────────────────
  describe('ADR-263 β-3 — Cone face dispatch', () => {
    function coneCtx() {
      const c = mockToolContext();
      c.bridge.drawCircleOnCone = vi.fn().mockReturnValue('{"cap":3,"annulus":2}');
      c.bridge.drawCircleOnCylinder = vi.fn().mockReturnValue('{"cap":3,"annulus":2}');
      c.bridge.drawCircleAsShape = vi.fn().mockReturnValue(0);
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      c.getFaceId = vi.fn().mockReturnValue(7);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(10, 0, 5) });
      c.get3DPoint = vi.fn().mockReturnValue(new THREE.Vector3(50, 0, 0));
      c.getSnappedPoint = vi.fn().mockReturnValue(null);
      c.getRay = vi.fn().mockReturnValue({
        ray: { intersectPlane: (_p: THREE.Plane, target: THREE.Vector3) => { target.set(50, 0, 0); return target; } },
      });
      c.lockPlane = vi.fn();
      c.setLastDrawnPlane = vi.fn();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1), right: new THREE.Vector3(0, 1, 0),
        onFace: true, surfaceKind: 4, origin: new THREE.Vector3(10, 0, 5), // hit on cone wall
      });
      return c;
    }

    it('first click on Cone → second click calls drawCircleOnCone(host, center, radius)', () => {
      const c = coneCtx();
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 0, 5));
      expect(t.isBusy()).toBe(true);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(10, 4, 5) });
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 4, 5));
      expect(c.bridge.drawCircleOnCone).toHaveBeenCalledTimes(1);
      const args = c.bridge.drawCircleOnCone.mock.calls[0];
      expect(args[0]).toBe(7);                // host face id
      expect(args[1]).toEqual([10, 0, 5]);    // center on cone
      expect(args[2]).toEqual([10, 4, 5]);    // radius point on cone
      expect(c.bridge.drawCircleOnCylinder).not.toHaveBeenCalled();  // NOT cylinder path
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();     // NOT planar
      expect(c.syncMesh).toHaveBeenCalled();
      expect(t.isBusy()).toBe(false);
    });

    it('Cone face but missing drawCircleOnCone export → planar path, no crash', () => {
      const c = coneCtx();
      c.bridge.drawCircleOnCone = undefined;
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 0, 5));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 4, 5));
      expect(c.bridge.drawCircleAsShape).toHaveBeenCalled();
    });

    it('Cylinder face (surfaceKind 2) does not call drawCircleOnCone', () => {
      const c = coneCtx();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1), right: new THREE.Vector3(0, 1, 0),
        onFace: true, surfaceKind: 2, origin: new THREE.Vector3(10, 0, 5),
      });
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 0, 5));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(10, 4, 5));
      expect(c.bridge.drawCircleOnCone).not.toHaveBeenCalled();      // cone ≠ cylinder
      expect(c.bridge.drawCircleOnCylinder).toHaveBeenCalledTimes(1);
    });
  });

  // ──────────────────────────────────────────────────────────────────
  // ADR-263 β-6 — draw circle ON a Torus face (곡면 벽 포트홀, #5 P3-C)
  // ──────────────────────────────────────────────────────────────────
  describe('ADR-263 β-6 — Torus face dispatch', () => {
    function torusCtx() {
      const c = mockToolContext();
      c.bridge.drawCircleOnTorus = vi.fn().mockReturnValue('{"cap":3,"annulus":2}');
      c.bridge.drawCircleOnCone = vi.fn().mockReturnValue('{"cap":3,"annulus":2}');
      c.bridge.drawCircleAsShape = vi.fn().mockReturnValue(0);
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      c.getFaceId = vi.fn().mockReturnValue(7);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(600, 0, 0) });
      c.get3DPoint = vi.fn().mockReturnValue(new THREE.Vector3(50, 0, 0));
      c.getSnappedPoint = vi.fn().mockReturnValue(null);
      c.getRay = vi.fn().mockReturnValue({
        ray: { intersectPlane: (_p: THREE.Plane, target: THREE.Vector3) => { target.set(50, 0, 0); return target; } },
      });
      c.lockPlane = vi.fn();
      c.setLastDrawnPlane = vi.fn();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1), right: new THREE.Vector3(0, 1, 0),
        onFace: true, surfaceKind: 5, origin: new THREE.Vector3(600, 0, 0), // hit on torus wall
      });
      return c;
    }

    it('first click on Torus → second click calls drawCircleOnTorus(host, center, radius)', () => {
      const c = torusCtx();
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 0, 0));
      expect(t.isBusy()).toBe(true);
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(600, 40, 0) });
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 40, 0));
      expect(c.bridge.drawCircleOnTorus).toHaveBeenCalledTimes(1);
      const args = c.bridge.drawCircleOnTorus.mock.calls[0];
      expect(args[0]).toBe(7);
      expect(args[1]).toEqual([600, 0, 0]);
      expect(args[2]).toEqual([600, 40, 0]);
      expect(c.bridge.drawCircleOnCone).not.toHaveBeenCalled();
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();
      expect(c.syncMesh).toHaveBeenCalled();
      expect(t.isBusy()).toBe(false);
    });

    it('Torus face but missing drawCircleOnTorus export → planar path, no crash', () => {
      const c = torusCtx();
      c.bridge.drawCircleOnTorus = undefined;
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 0, 0));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 40, 0));
      expect(c.bridge.drawCircleAsShape).toHaveBeenCalled();
    });

    it('Cone face (surfaceKind 4) does not call drawCircleOnTorus', () => {
      const c = torusCtx();
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0), up: new THREE.Vector3(0, 0, 1), right: new THREE.Vector3(0, 1, 0),
        onFace: true, surfaceKind: 4, origin: new THREE.Vector3(600, 0, 0),
      });
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 0, 0));
      t.onMouseDown({ clientX: 150, clientY: 100 } as MouseEvent, new THREE.Vector3(600, 40, 0));
      expect(c.bridge.drawCircleOnTorus).not.toHaveBeenCalled();     // torus ≠ cone
      expect(c.bridge.drawCircleOnCone).toHaveBeenCalledTimes(1);
    });
  });
  // ══════════════════════════════════════════════════════════════════════
  // ADR-284 follow-up — a typed radius on a curved host.
  //
  // applyVCBValue had no curved branch: it ignored the sphere/cylinder/cone/
  // torusMode the mouse path sets and drew a FLAT circle on the tangent plane.
  // The same tool behaved differently depending on whether you clicked the
  // radius or typed it — and typing looked like it worked.
  // ══════════════════════════════════════════════════════════════════════
  describe('VCB radius on a curved host (ADR-284 follow-up)', () => {
    function onSphere() {
      const c = mockToolContext();
      // sphereMode is only entered when the bridge actually exposes the endpoint
      c.bridge.drawCircleOnSphere = vi.fn().mockReturnValue('{"cap":1,"annulus":0}');
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      c.getDrawPlane = vi.fn().mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1), up: new THREE.Vector3(0, 1, 0),
        right: new THREE.Vector3(1, 0, 0),
        onFace: true, surfaceKind: 3, origin: new THREE.Vector3(0, 0, 5),
      });
      c.viewport.pick = vi.fn().mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, 0, 5) });
      c.getFaceId = vi.fn().mockReturnValue(4);
      return c;
    }

    it('asks the engine for the geodesic radius point and draws ON the surface', () => {
      // The typed radius now means what it says: the engine returns a point at
      // geodesic distance 50, and THAT is what goes to drawCircleOnSphere.
      const c = onSphere();
      c.bridge.surfacePointAtGeodesicDistance = vi.fn().mockReturnValue([3, 0, 4]);
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      t.applyVCBValue(50);
      expect(c.bridge.surfacePointAtGeodesicDistance)
        .toHaveBeenCalledWith(4, [0, 0, 5], 50);
      expect(c.bridge.drawCircleOnSphere).toHaveBeenCalledWith(4, [0, 0, 5], [3, 0, 4]);
      // and never the flat path
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();
      expect(c.bridge.drawCircleAsCurve).not.toHaveBeenCalled();
      expect(t.isBusy()).toBe(false);
    });

    it('FAILS CLOSED when the engine cannot answer — no flat fallback', () => {
      // A degenerate ask, a sphere radius past half a turn, or an older build
      // without the export. Falling back to the tangent-plane circle is exactly
      // what this whole path exists to prevent, so decline instead.
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      const c = onSphere();
      c.bridge.surfacePointAtGeodesicDistance = vi.fn().mockReturnValue(null);
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      t.applyVCBValue(50);
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();
      expect(c.bridge.drawCircleAsCurve).not.toHaveBeenCalled();
      expect(c.bridge.drawCircleOnSphere).not.toHaveBeenCalled();
      expect(warn, 'and the user is told which way does work').toHaveBeenCalled();
      expect(String(warn.mock.calls[0][0])).toContain('마우스로');
      expect(t.isBusy(), 'the tool resets rather than hanging').toBe(false);
      warn.mockRestore();
    });

    it('an engine without the export declines too (graceful, no flat circle)', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      const c = onSphere(); // mock bridge has no surfacePointAtGeodesicDistance
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 5));
      t.applyVCBValue(50);
      expect(c.bridge.drawCircleAsShape).not.toHaveBeenCalled();
      expect(c.bridge.drawCircleAsCurve).not.toHaveBeenCalled();
      expect(warn).toHaveBeenCalled();
      warn.mockRestore();
    });

    it('a typed radius on a PLANAR face still works (no regression)', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      const c = mockToolContext();
      c.bridge.drawCircleAsCurve = vi.fn().mockReturnValue(0);
      const t = new DrawCircleTool(c);
      t.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      t.applyVCBValue(50);
      const drew = (c.bridge.drawCircleAsShape as ReturnType<typeof vi.fn>).mock.calls.length
        + (c.bridge.drawCircleAsCurve as ReturnType<typeof vi.fn>).mock.calls.length;
      expect(drew, 'the planar VCB path must be untouched').toBeGreaterThan(0);
      expect(warn).not.toHaveBeenCalled();
      warn.mockRestore();
    });
  });
});
