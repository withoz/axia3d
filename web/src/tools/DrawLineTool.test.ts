import { describe, it, expect, beforeEach, vi } from 'vitest';
import * as THREE from 'three';
import { DrawLineTool } from './DrawLineTool';
import { Toast } from '../ui/Toast';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: { info: vi.fn(), warning: vi.fn(), error: vi.fn(), show: vi.fn() },
}));

function mockToolContext() {
  return {
    bridge: {
      drawLine: vi.fn().mockReturnValue(0),
      drawLineAsShape: vi.fn().mockReturnValue(0),
      faceCount: vi.fn().mockReturnValue(0),
      splitFaceByLine: vi.fn().mockReturnValue('{"faces":[10,11],"verts":[5],"edges":1}'),
      pointInFace: vi.fn().mockReturnValue(false),
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
      activeCamera: new THREE.PerspectiveCamera(),
      renderer: {
        domElement: {
          getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
        },
      },
      pick: vi.fn().mockReturnValue(null),
      container: { getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }) },
    },
    selection: {
      clearSelection: vi.fn(),
      selectFaces: vi.fn(),
    },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    units: { format: vi.fn().mockReturnValue('100mm') },
    snap: {
      setReferencePoint: vi.fn(),
      getSnap: vi.fn().mockReturnValue(null),
      saveSnapConfig: vi.fn().mockReturnValue(new Set()),
      restoreSnapConfig: vi.fn(),
      applyFaceCreationPreset: vi.fn(),
      findNearestEndpoint: vi.fn().mockReturnValue(null),
    },
    snapVisual: { update: vi.fn(), clear: vi.fn() },
    clearAxisGuide: vi.fn(),
    updateAxisGuide: vi.fn(),
    getSelectedFaces: vi.fn().mockReturnValue([]),
    getFaceId: vi.fn().mockReturnValue(-1),
    get3DPoint: vi.fn(),
    getGroundPoint: vi.fn(),
    getSnappedPoint: vi.fn().mockReturnValue(null),
    getAxisInferredPoint: vi.fn().mockReturnValue(null),
    axisLock: null as string | null,
    inferredAxis: 'free' as string | null,
    faceMap: new Uint32Array([0, 1, 2]),
    // ADR-140 ε-1: DrawLineTool now uses ctx.getDrawPlane SSOT for face-hit
    // drawing plane. Default returns legacy-equivalent DCEL fallback (no
    // surface-aware origin). Tests override .mockReturnValue as needed.
    getDrawPlane: vi.fn().mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: false,
    }),
  } as any;
}

describe('DrawLineTool', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawLineTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new DrawLineTool(ctx);
  });

  describe('name', () => {
    it('is "line"', () => {
      expect(tool.name).toBe('line');
    });
  });

  describe('state machine', () => {
    it('starts in Idle', () => {
      expect(tool.isBusy()).toBe(false);
    });

    it('onActivate transitions Idle → Armed', () => {
      tool.onActivate();
      expect(tool.isBusy()).toBe(false); // Armed is not busy
    });

    it('first click transitions Armed → Drawing', () => {
      tool.onActivate(); // → Armed
      // Simulate click with button=0
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 0));
      expect(tool.isBusy()).toBe(true); // Drawing
    });

    it('Escape from Armed → Idle', () => {
      tool.onActivate();
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });

    it('Escape from Drawing → Idle', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3());
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('면을 따라 그리기 (drawing along the solid)', () => {
    /** Make the two clicks land on two different faces, with the second one
     *  picking a point on the solid that is NOT on the first click's plane. */
    function twoFaces(secondSurfacePoint: THREE.Vector3) {
      let call = 0;
      ctx.getFaceId = vi.fn(() => (call++ === 0 ? 7 : 9));
      ctx.viewport.pick = vi.fn(() => ({
        faceIndex: 0,
        point: secondSurfacePoint.clone(),
        face: { normal: new THREE.Vector3(0, 0, 1) },
        object: null,
      }));
      ctx.bridge.drawLineAlongSurface = vi.fn().mockReturnValue(3);
      ctx.bridge.lastError = vi.fn().mockReturnValue('');
    }

    it('follows the solid when Alt is held on the ending click', () => {
      const onWall = new THREE.Vector3(200, 100, 50);
      twoFaces(onWall);
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown(
        { button: 0, altKey: true } as MouseEvent,
        new THREE.Vector3(200, 100, 100),
      );

      // The point sent is the one picked ON the wall, not the one the state
      // machine received (which was projected onto the top face's plane).
      expect(ctx.bridge.drawLineAlongSurface).toHaveBeenCalledWith(
        100, 100, 100, 200, 100, 50,
      );
      expect(ctx.bridge.drawLineAsShape).not.toHaveBeenCalled();
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    /// 사용자 (2026-08-04): `기본은 연장으로`.
    ///
    /// Without the modifier this used to happen by itself whenever the cursor
    /// landed on another face — so a line meant to leave the shape and carry on
    /// bent back onto it as soon as a wall got under the pointer. The cursor
    /// decided; now the user does.
    it('stays on the plane it started on when Alt is not held', () => {
      twoFaces(new THREE.Vector3(200, 100, 50));
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(200, 100, 100));

      expect(ctx.bridge.drawLineAlongSurface).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });

    /// A held Alt belongs to the click it was held on, not to the tool. The
    /// next line, drawn without it, is an ordinary one.
    it('the ask does not carry over to the next line', () => {
      twoFaces(new THREE.Vector3(200, 100, 50));
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown(
        { button: 0, altKey: true } as MouseEvent,
        new THREE.Vector3(200, 100, 100),
      );
      expect(ctx.bridge.drawLineAlongSurface).toHaveBeenCalledTimes(1);

      (ctx.bridge.drawLineAlongSurface as ReturnType<typeof vi.fn>).mockClear();
      (ctx.bridge.drawLineAsShape as ReturnType<typeof vi.fn>).mockClear();
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(300, 100, 100));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(400, 100, 100));
      expect(ctx.bridge.drawLineAlongSurface).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });

    it('leaves same-face drawing on the ordinary path', () => {
      ctx.getFaceId = vi.fn().mockReturnValue(7); // both clicks, one face
      ctx.viewport.pick = vi.fn(() => ({
        faceIndex: 0,
        point: new THREE.Vector3(150, 100, 100),
        face: { normal: new THREE.Vector3(0, 0, 1) },
        object: null,
      }));
      ctx.bridge.drawLineAlongSurface = vi.fn().mockReturnValue(3);
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(150, 100, 100));

      expect(ctx.bridge.drawLineAlongSurface).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });

    it('falls back to the ordinary line when the engine cannot follow', () => {
      twoFaces(new THREE.Vector3(100, 100, 0));
      ctx.bridge.drawLineAlongSurface = vi.fn().mockReturnValue(-1);
      ctx.bridge.lastError = vi.fn().mockReturnValue('두 점이 맞닿지 않은 면 위에 있습니다');
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown(
        { button: 0, altKey: true } as MouseEvent,
        new THREE.Vector3(100, 100, 0),
      );

      expect(ctx.bridge.drawLineAlongSurface).toHaveBeenCalled();
      // Refused, so what the user could already do still happens.
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
      // And the engine's reason reaches the user. Asserting the text catches a
      // wrong accessor name, which `?? ''` would otherwise swallow.
      expect(Toast.info).toHaveBeenCalledWith(
        expect.stringContaining('맞닿지 않은'), expect.anything(),
      );
    });

    it('does nothing new when the engine has no such export', () => {
      twoFaces(new THREE.Vector3(200, 100, 50));
      delete (ctx.bridge as Record<string, unknown>).drawLineAlongSurface;
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 100, 100));
      tool.onMouseDown(
        { button: 0, altKey: true } as MouseEvent,
        new THREE.Vector3(200, 100, 100),
      );
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });
  });

  describe('line creation', () => {
    it('second click creates line via bridge', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 0, 0));

      // ADR-103-δ-1 (Z-up): default plane normal = +Z (XY ground).
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledWith(0, 0, 0, 100, 0, 0, 0, 0, 1);
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('continuous mode: stays in Drawing after confirm', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 0, 0));
      // After confirm, should be back in Drawing (continuous)
      expect(tool.isBusy()).toBe(true);
    });

    it('ignores very short lines (< 1 unit)', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0.5, 0, 0));
      expect(ctx.bridge.drawLineAsShape).not.toHaveBeenCalled();
    });
  });

  describe('onMouseMove', () => {
    it('does nothing when not in Drawing state', () => {
      tool.onActivate(); // Armed
      tool.onMouseMove({} as MouseEvent, new THREE.Vector3(50, 0, 0));
      // No preview updates in Armed state
    });
  });

  describe('right click', () => {
    it('cancels drawing', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3());
      expect(tool.isBusy()).toBe(true);

      tool.onMouseDown({ button: 2 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('applyVCBValue', () => {
    it('creates line along x axis by default', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.applyVCBValue(500);
      // ADR-103-δ-1 (Z-up): default plane normal = +Z.
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledWith(0, 0, 0, 500, 0, 0, 0, 0, 1);
    });

    it('does nothing when not in Drawing state', () => {
      tool.applyVCBValue(500);
      expect(ctx.bridge.drawLineAsShape).not.toHaveBeenCalled();
    });
  });

  describe('face split', () => {
    // 사용자 결재 (a) 2026-06-05 — continuous polyline on faces. A same-face
    // segment no longer auto-splits-and-stops; it draws the edge via the
    // kernel-aware drawLineAsShape path and the chain CONTINUES. Faces derive
    // from closed boundaries via the engine rederive (ADR-186).
    it('same-face segment uses drawLineAsShape and continues (no split-stop)', () => {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 2, point: new THREE.Vector3(50, 0, 50) });
      ctx.getFaceId.mockReturnValue(7);

      tool.onActivate(); // → Armed
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      // Now in Drawing, startFaceId=7
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));
      // endFaceId=7 (same face) → NO per-segment split; drawLineAsShape + continue.

      expect(ctx.bridge.splitFaceByLine).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledWith(10, 0, 10, 90, 0, 90, 0, 0, 1);
      // Continuous: still Drawing (chain not stopped).
      expect(tool.isBusy()).toBe(true);
      expect(tool.getStateName()).toBe('Drawing');
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('continuous on face: consecutive same-face segments all use drawLineAsShape', () => {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 2 });
      ctx.getFaceId.mockReturnValue(7);

      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));

      // Two committed segments, both via drawLineAsShape; chain continues.
      expect(ctx.bridge.splitFaceByLine).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledTimes(2);
      expect(tool.isBusy()).toBe(true);
      expect(tool.getStateName()).toBe('Drawing');
    });

    it('falls back to drawLine when splitFaceByLine throws', () => {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 2 });
      ctx.getFaceId.mockReturnValue(7);
      ctx.bridge.splitFaceByLine.mockImplementation(() => { throw new Error('not available'); });

      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));

      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledWith(10, 0, 10, 90, 0, 90, 0, 0, 1);
    });

    it('uses regular drawLine when start and end are on different faces', () => {
      let callCount = 0;
      ctx.viewport.pick.mockReturnValue({ faceIndex: 2 });
      ctx.getFaceId.mockImplementation(() => {
        callCount++;
        return callCount === 1 ? 7 : 8; // Different face IDs
      });

      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));

      expect(ctx.bridge.splitFaceByLine).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });

    it('uses regular drawLine when clicking on empty space (no face)', () => {
      ctx.viewport.pick.mockReturnValue(null); // No face hit

      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));

      expect(ctx.bridge.splitFaceByLine).not.toHaveBeenCalled();
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    });

    it('continues (stays Drawing) after a same-face segment — continuous polyline on face', () => {
      // 사용자 결재 (a) — a same-face segment no longer stops continuous drawing.
      // Only an explicit loop-close ends the chain (→ Armed). Mid-segments
      // (incl. edge-to-edge splits) keep the chain in Drawing.
      ctx.viewport.pick.mockReturnValue({ faceIndex: 2 });
      ctx.getFaceId.mockReturnValue(7);

      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(10, 0, 10));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(90, 0, 90));

      expect(tool.isBusy()).toBe(true); // Drawing (continuous), not Armed
      expect(tool.getStateName()).toBe('Drawing');
    });
  });

  describe('cleanup', () => {
    it('transitions to Idle', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3());
      tool.cleanup();
      expect(tool.isBusy()).toBe(false);
    });
  });

  // ADR-047 P32 — Chain self-touch prevention.
  // The DrawLineTool exposes its pending chain points (excluding chainStart)
  // so SnapManager can drop them from endpoint-snap candidates.
  describe('getExcludedSnapPoints (ADR-047 P32)', () => {
    it('returns empty when no chain is active', () => {
      expect(tool.getExcludedSnapPoints()).toEqual([]);
    });

    it('returns empty for a fresh chain with only chainStart', () => {
      tool.onActivate();
      // First click → chainStart set, chainPoints = [start]. No mid yet.
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      expect(tool.getExcludedSnapPoints()).toEqual([]);
    });

    it('excludes mid-waypoints but NOT chainStart after multiple clicks', () => {
      tool.onActivate();
      // P0 = chainStart
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      // P1 = first mid (commits the segment P0→P1, chainPoints becomes [P0, P1])
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 0, 0));
      // P2 = second mid → chainPoints [P0, P1, P2]
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 0, 100));

      const excluded = tool.getExcludedSnapPoints();

      // chainStart (P0) must NOT be excluded — needed for loop-close.
      const includesStart = excluded.some(p => p.distanceTo(new THREE.Vector3(0, 0, 0)) < 1e-3);
      expect(includesStart).toBe(false);

      // P1 and P2 (mid waypoints) MUST be excluded.
      const includesP1 = excluded.some(p => p.distanceTo(new THREE.Vector3(100, 0, 0)) < 1e-3);
      const includesP2 = excluded.some(p => p.distanceTo(new THREE.Vector3(100, 0, 100)) < 1e-3);
      expect(includesP1).toBe(true);
      expect(includesP2).toBe(true);
    });

    it('returns clones (mutating result must not affect chain state)', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(50, 0, 0));

      const excluded1 = tool.getExcludedSnapPoints();
      if (excluded1.length > 0) excluded1[0].set(9999, 9999, 9999);
      const excluded2 = tool.getExcludedSnapPoints();

      // Second call must yield original positions, untouched by mutation above.
      if (excluded2.length > 0) {
        expect(excluded2[0].x).not.toBe(9999);
      }
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-087 K-ε — kernel-aware drawLineAsShape only path.
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-087 K-ε kernel-aware dispatch', () => {
    it('always calls bridge.drawLineAsShape (Plane attach on face path)', () => {
      tool.onActivate();
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(0, 0, 0));
      tool.onMouseDown({ button: 0 } as MouseEvent, new THREE.Vector3(100, 0, 0));

      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledTimes(1);
      expect(ctx.bridge.drawLine).not.toHaveBeenCalled();
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-140 ε-1 — DrawLineTool surface-aware drawing plane integration
  // (140-δ getDrawPlane SSOT 통합 — face-hit branch 가 ctx.getDrawPlane
  //  결과 사용. Cylinder/Sphere surface 위 사용자 click 의 정확한 tangent
  //  plane chord substitute 회피.)
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-140 ε-1 — surface-aware drawing plane integration', () => {
    // Mock event helper — same coords as elsewhere in this file
    function mockEvent(): MouseEvent {
      return { clientX: 100, clientY: 100 } as MouseEvent;
    }

    // Build a face hit with world point + face normal (Three.js shape).
    // Note: hit.object intentionally omitted to skip the matrixWorld transform
    // path in the defensive fallback branch (Three.js mock has no Matrix4).
    // This is sound because:
    //   - tests 1, 2 use dp.onFace:true → defensive branch never entered
    //   - test 3 uses dp.onFace:false → defensive branch entered, but
    //     `if (hit.object && hit.object.matrixWorld)` evaluates false →
    //     skips matrixWorld transform → worldNormal = hit.face.normal directly
    function mockFaceHit(point: { x: number; y: number; z: number }, normal: { x: number; y: number; z: number }) {
      return {
        faceIndex: 0,
        point: new THREE.Vector3(point.x, point.y, point.z),
        face: { normal: new THREE.Vector3(normal.x, normal.y, normal.z) },
      };
    }

    it('uses ctx.getDrawPlane SSOT when face is hit (Plane kind ≤ 1 — DCEL legacy equivalent)', () => {
      ctx.viewport.pick.mockReturnValue(mockFaceHit({ x: 1, y: 2, z: 0 }, { x: 0, y: 0, z: 1 }));
      ctx.getDrawPlane.mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1),  // DCEL face normal (Plane)
        up: new THREE.Vector3(0, 1, 0),
        right: new THREE.Vector3(1, 0, 0),
        onFace: true,
        surfaceKind: 1,  // Plane — no surface-aware origin
      });

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).establishDrawingPlane(mockEvent());

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const plane = (tool as any).drawingPlane as THREE.Plane;
      expect(plane).toBeDefined();
      // SSOT dispatch consulted exactly once for face-hit branch
      expect(ctx.getDrawPlane).toHaveBeenCalledTimes(1);
      // Plane normal = DCEL face normal (Z-up)
      expect(plane.normal.z).toBeCloseTo(1, 6);
      // Legacy fallback origin = hit.point (no surface-aware origin for kind ≤ 1)
      // Plane equation: n·x + d = 0 → for normal=(0,0,1), origin=(1,2,0): d = -(0*1 + 0*2 + 1*0) = 0
      expect(plane.constant).toBeCloseTo(0, 6);
    });

    it('uses surface-aware tangent plane when getDrawPlane returns origin (Cylinder kind ≥ 2)', () => {
      // Hit on Cylinder surface at world (5, 0, 0) — radial normal +X
      ctx.viewport.pick.mockReturnValue(mockFaceHit({ x: 5, y: 0, z: 0 }, { x: 0, y: 0, z: 1 }));
      ctx.getDrawPlane.mockReturnValue({
        normal: new THREE.Vector3(1, 0, 0),  // Surface-aware radial normal
        up: new THREE.Vector3(0, 0, 1),
        right: new THREE.Vector3(0, 1, 0),
        onFace: true,
        origin: new THREE.Vector3(5, 0, 0),  // ADR-140 δ surface-aware tangent origin
        surfaceKind: 2,  // Cylinder
      });

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).establishDrawingPlane(mockEvent());

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const plane = (tool as any).drawingPlane as THREE.Plane;
      expect(plane).toBeDefined();
      // Surface-aware normal (radial)
      expect(plane.normal.x).toBeCloseTo(1, 6);
      // Surface-aware origin (hit point P on cylinder)
      // Plane equation: n·x + d = 0 → for normal=(1,0,0), origin=(5,0,0): d = -5
      expect(plane.constant).toBeCloseTo(-5, 6);
    });

    it('falls back to legacy hit.face.normal when getDrawPlane returns onFace=false (defensive)', () => {
      // Pathological case: viewport hit OK, but getDrawPlane fails to recognize face
      // (e.g. mid-syncMesh stale state, no axia FaceId). Should not throw.
      ctx.viewport.pick.mockReturnValue(mockFaceHit({ x: 0, y: 0, z: 0 }, { x: 0, y: 1, z: 0 }));
      ctx.getDrawPlane.mockReturnValue({
        normal: new THREE.Vector3(0, 0, 1),  // Default ground plane (Z-up)
        up: new THREE.Vector3(0, 1, 0),
        right: new THREE.Vector3(1, 0, 0),
        onFace: false,  // ← getDrawPlane failed to recognize face
      });

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).establishDrawingPlane(mockEvent());

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const plane = (tool as any).drawingPlane as THREE.Plane;
      expect(plane).toBeDefined();
      // Defensive: legacy hit.face.normal used (matrixWorld is identity in mock → +Y)
      expect(plane.normal.y).toBeCloseTo(1, 6);
    });

    it('does not call ctx.getDrawPlane in no-face workplane fallback (existing path unchanged)', () => {
      // Empty space click (pick returns null) — should use workplane logic only
      ctx.viewport.pick.mockReturnValue(null);
      ctx.get3DPoint.mockReturnValue(new THREE.Vector3(0, 0, 0));

      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).establishDrawingPlane(mockEvent());

      // ADR-140 ε-1 face-hit branch NOT entered → getDrawPlane never called
      expect(ctx.getDrawPlane).not.toHaveBeenCalled();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const plane = (tool as any).drawingPlane as THREE.Plane;
      expect(plane).toBeDefined();
    });
  });
});

/**
 * C-3 — a line across a cone is a SEAM, so its clicks are collected and handed
 * over whole rather than committed one at a time.
 *
 * Two points cannot divide a curved face: a great circle through two rim
 * points IS the rim (LOCKED #104, settled). Three is the floor, and the engine
 * enforces it. What was missing was only that DrawLine committed each segment
 * as it went and so never had three points to hand over.
 *
 * The cone, and not the sphere. `split_curved_face_by_open_seam` trims a
 * CIRCULAR rim, which the cone's base is; the sphere this app builds is the
 * axis-native one, a single face with a meridian seam, and it is declined —
 * measured both ways in `an_open_seam_on_the_shapes_the_app_builds.rs`. The
 * ADR-284 coverage that reads as sphere support builds its host with
 * `create_sphere_kernel_native`, the two-hemisphere form the app stopped
 * making.
 */
describe('DrawLineTool — a seam on a curved face (C-3)', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let tool: DrawLineTool;

  /** A click that the tool will read as landing on face 7 of a curved host. */
  const ev = (alt = false) => ({ button: 0, clientX: 100, clientY: 100, altKey: alt } as MouseEvent);

  /** Where the next pick will say the cursor landed ON the host. */
  let surfaceAt = new THREE.Vector3();

  function onCurved(kind: 2 | 3 | 4 | 5) {
    ctx.getDrawPlane.mockReturnValue({
      normal: new THREE.Vector3(0, 0, 1),
      up: new THREE.Vector3(0, 1, 0),
      right: new THREE.Vector3(1, 0, 0),
      onFace: true,
      surfaceKind: kind,
      origin: new THREE.Vector3(0, 0, 0),
    });
    ctx.viewport.pick.mockImplementation(() => ({
      point: surfaceAt.clone(),
      face: { normal: new THREE.Vector3(0, 0, 1) },
      faceIndex: 3,
    }));
    ctx.getFaceId.mockReturnValue(7);
  }

  /** Click where the cursor is over `v` on the surface. The point the state
   *  machine gets is projected onto the locked plane, which is exactly why
   *  the seam reads the pick instead. */
  function clickOn(v: THREE.Vector3, alt = false) {
    surfaceAt = v;
    tool.onMouseDown(ev(alt), v.clone());
  }

  beforeEach(() => {
    ctx = mockToolContext();
    ctx.bridge.drawOpenSeamOnCurved = vi.fn().mockReturnValue('{"a":11,"b":12}');
    tool = new DrawLineTool(ctx);
    tool.onActivate();
  });

  it('collects the clicks instead of drawing each segment, then hands over one seam', () => {
    onCurved(4); // cone
    clickOn(new THREE.Vector3(-10, 0, 0)); // rim
    clickOn(new THREE.Vector3(0, 0, 10));  // over the pole
    clickOn(new THREE.Vector3(10, 0, 0));  // the far rim

    // Nothing was drawn segment by segment — that is what "collects" means.
    expect(ctx.bridge.drawLineAsShape).not.toHaveBeenCalled();

    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(ctx.bridge.drawOpenSeamOnCurved).toHaveBeenCalledTimes(1);
    const [host, pts] = ctx.bridge.drawOpenSeamOnCurved.mock.calls[0];
    expect(host).toBe(7);
    expect(pts.length).toBe(3);
    expect(pts[0]).toEqual([-10, 0, 0]);
    expect(pts[2]).toEqual([10, 0, 0]);
  });

  /**
   * A sphere looks like it should qualify and does not. The seam path trims a
   * CIRCULAR rim, and the sphere this app builds is the axis-native one —
   * poles and a meridian seam, a single face. Measured over the pole and
   * meridian-to-meridian, declined both times, mesh untouched both times
   * (`an_open_seam_on_the_shapes_the_app_builds.rs`). Collecting there would
   * promise a split that cannot happen.
   */
  it('a sphere is not a seam host in this app', () => {
    onCurved(3);
    clickOn(new THREE.Vector3(-10, 0, 0));
    clickOn(new THREE.Vector3(0, 0, 10));
    clickOn(new THREE.Vector3(10, 0, 0));
    expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(ctx.bridge.drawOpenSeamOnCurved).not.toHaveBeenCalled();
  });

  /**
   * The engine rejects an open stroke on a cylinder or a torus — they have
   * more than one rim — so the tool must not collect there. Collecting and
   * then failing would swallow the user's line.
   */
  it('a cylinder and a torus are drawn, not collected', () => {
    for (const kind of [2, 5] as const) {
      ctx.bridge.drawLineAsShape.mockClear();
      ctx.bridge.drawOpenSeamOnCurved.mockClear();
      onCurved(kind);
      tool.onActivate();
      clickOn(new THREE.Vector3(-10, 0, 0));
      clickOn(new THREE.Vector3(0, 0, 10));
      expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(ctx.bridge.drawOpenSeamOnCurved).not.toHaveBeenCalled();
    }
  });

  /** Alt means "flat on the tangent plane" here as it does in DrawBezierTool. */
  it('Alt draws flat instead of collecting a seam', () => {
    onCurved(4);
    clickOn(new THREE.Vector3(-10, 0, 0), true);
    clickOn(new THREE.Vector3(0, 0, 10), true);
    expect(ctx.bridge.drawLineAsShape).toHaveBeenCalled();
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(ctx.bridge.drawOpenSeamOnCurved).not.toHaveBeenCalled();
  });

  /**
   * A chain that ends before it is a seam must not vanish. Collecting is
   * reversible: what was withheld is drawn as the ordinary segments it looked
   * like. Written because suppressing a commit is the easy half and undoing
   * the suppression is the half that gets forgotten.
   */
  it('a chain too short to be a seam is drawn as plain lines instead of lost', () => {
    onCurved(4);
    clickOn(new THREE.Vector3(-10, 0, 0));
    clickOn(new THREE.Vector3(10, 0, 0));
    expect(ctx.bridge.drawLineAsShape).not.toHaveBeenCalled(); // withheld so far

    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(ctx.bridge.drawOpenSeamOnCurved).not.toHaveBeenCalled();
    expect(ctx.bridge.drawLineAsShape).toHaveBeenCalledTimes(1); // and given back
    const call = ctx.bridge.drawLineAsShape.mock.calls[0];
    expect(call.slice(0, 6)).toEqual([-10, 0, 0, 10, 0, 0]);
  });

  it('says so when the seam does not reach the far rim', () => {
    onCurved(4);
    ctx.bridge.drawOpenSeamOnCurved.mockReturnValue('{"error":"no rim"}');
    clickOn(new THREE.Vector3(-10, 0, 0));
    clickOn(new THREE.Vector3(0, 0, 10));
    clickOn(new THREE.Vector3(2, 0, 9));
    tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
    expect(Toast.warning).toHaveBeenCalled();
  });

  /** Switching tools ends the chain the same way Escape does. */
  it('leaving the tool hands over the seam rather than dropping it', () => {
    onCurved(4);
    clickOn(new THREE.Vector3(-10, 0, 0));
    clickOn(new THREE.Vector3(0, 0, 10));
    clickOn(new THREE.Vector3(10, 0, 0));
    tool.onDeactivate();
    expect(ctx.bridge.drawOpenSeamOnCurved).toHaveBeenCalledTimes(1);
  });
});
