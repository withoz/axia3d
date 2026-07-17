import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import * as THREE from 'three';
import { PushPullTool } from './PushPullTool';
import { setLocale } from '../i18n';
import { Toast } from '../ui/Toast';
import { setExtrudeMode, setExtrudeDistNeg } from './ExtrudeModeSettings';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn(), debugWarn: vi.fn() }));

function mockToolContext() {
  return {
    bridge: {
      pushPull: vi.fn().mockReturnValue(true),
      createSolidExtrude: vi.fn().mockReturnValue(true),
      createSolidExtrudeTapered: vi.fn().mockReturnValue(true),
      createSolidExtrudeCone: vi.fn().mockReturnValue(true),
      createSolidExtrudeBidirectional: vi.fn().mockReturnValue(true),
      facesCentroid: vi.fn().mockReturnValue(new THREE.Vector3(0, 5, 0)),
      getFaceNormal: vi.fn().mockReturnValue(new Float32Array([0, 1, 0])),
      // ADR-193 — live Push/Pull session methods.
      beginLiveExtrude: vi.fn().mockReturnValue(42),
      updateLiveExtrude: vi.fn().mockReturnValue(true),
      commitLiveExtrude: vi.fn().mockReturnValue(true),
      cancelLiveExtrude: vi.fn().mockReturnValue(true),
      isLiveExtrudeActive: vi.fn().mockReturnValue(false),
      lastError: vi.fn().mockReturnValue(''),
      engine: {
        push_pull_smooth_group_seamless: vi.fn().mockReturnValue(true),
      },
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
      activeCamera: new THREE.PerspectiveCamera(),
      pick: vi.fn().mockReturnValue(null),
      renderer: {
        domElement: {
          getBoundingClientRect: () => ({
            left: 0, top: 0, width: 800, height: 600,
          }),
        },
      },
    },
    selection: {
      handleClick: vi.fn(),
      clearSelection: vi.fn(),
      getSmoothGroup: vi.fn().mockReturnValue([]),
      selectFaces: vi.fn(),
    },
    syncMesh: vi.fn(),
    dimLabel: { update: vi.fn(), clear: vi.fn() },
    snap: {
      findAlignedDistance: vi.fn().mockReturnValue(null),
    },
    snapVisual: {
      update: vi.fn(),
      clear: vi.fn(),
    },
    units: { format: vi.fn().mockReturnValue('10.0 mm') },
    getFaceId: vi.fn().mockReturnValue(-1),
    getSelectedFaces: vi.fn().mockReturnValue([]),
    extractFaceBoundary: vi.fn().mockReturnValue([]),
  } as any;
}

describe('PushPullTool', () => {
  // jsdom's navigator.language is 'en-US'; these assert Korean copy.
  beforeEach(() => setLocale('ko'));

  let ctx: ReturnType<typeof mockToolContext>;
  let tool: PushPullTool;

  beforeEach(() => {
    ctx = mockToolContext();
    tool = new PushPullTool(ctx);
  });

  describe('name', () => {
    it('is "pushpull"', () => {
      expect(tool.name).toBe('pushpull');
    });
  });

  describe('isBusy', () => {
    it('defaults to false', () => {
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('onActivate / onDeactivate', () => {
    it('activate does not throw', () => {
      expect(() => tool.onActivate()).not.toThrow();
    });

    it('deactivate cleans up', () => {
      tool.onDeactivate();
      expect(tool.isBusy()).toBe(false);
      expect(ctx.dimLabel.clear).toHaveBeenCalled();
    });
  });

  describe('onMouseDown - phase 1 (face selection)', () => {
    it('does nothing when no face hit', () => {
      ctx.viewport.pick.mockReturnValue(null);
      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(false);
    });

    it('selects face from viewport pick', () => {
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 2,
        point: new THREE.Vector3(10, 5, 10),
      });
      ctx.getFaceId.mockReturnValue(5);

      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(true);
      expect(ctx.selection.handleClick).toHaveBeenCalledWith(5, false, false);
    });

    it('falls back to selected face when pick misses', () => {
      ctx.viewport.pick.mockReturnValue(null);
      ctx.getSelectedFaces.mockReturnValue([3]);
      ctx.bridge.facesCentroid.mockReturnValue(new THREE.Vector3(0, 10, 0));

      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(true);
    });

    it('detects smooth group', () => {
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 2,
        point: new THREE.Vector3(0, 0, 0),
      });
      ctx.getFaceId.mockReturnValue(5);
      ctx.selection.getSmoothGroup.mockReturnValue([5, 6, 7]);

      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(true);
    });
  });

  describe('onKeyDown', () => {
    it('Escape cancels active push/pull', () => {
      // Start a session
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 0,
        point: new THREE.Vector3(0, 0, 0),
      });
      ctx.getFaceId.mockReturnValue(1);
      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);
      expect(tool.isBusy()).toBe(true);

      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });

    it('does nothing when not active', () => {
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(tool.isBusy()).toBe(false);
    });
  });

  describe('applyVCBValue', () => {
    it('applies push/pull via VCB on selected face', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);

      tool.applyVCBValue(100);

      // ADR-087 K-ε — kernel-aware createSolidExtrude only path.
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 100);
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('does nothing when no face selected', () => {
      ctx.getSelectedFaces.mockReturnValue([]);

      tool.applyVCBValue(100);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
    });

    it('cleans up after VCB apply', () => {
      ctx.getSelectedFaces.mockReturnValue([3]);
      tool.applyVCBValue(50);
      expect(tool.isBusy()).toBe(false);
      expect(ctx.dimLabel.clear).toHaveBeenCalled();
    });
  });

  // ADR-259 β-3 — tapered (draft) extrude via VCB "거리,각도".
  describe('ADR-259 β-3 taper VCB (거리,각도)', () => {
    it('distance + angle routes to createSolidExtrudeTapered, NOT createSolidExtrude', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(100, 15);
      expect(ctx.bridge.createSolidExtrudeTapered).toHaveBeenCalledWith(5, 100, 15);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('no angle → straight createSolidExtrude (existing path unchanged)', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(100);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 100);
      expect(ctx.bridge.createSolidExtrudeTapered).not.toHaveBeenCalled();
    });

    it('taperDeg 0 → straight path (0° = no draft)', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(100, 0);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 100);
      expect(ctx.bridge.createSolidExtrudeTapered).not.toHaveBeenCalled();
    });

    it('smooth group + taper → Toast.warning, neither extrude (taper is flat-profile only)', () => {
      const warnSpy = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).isSmoothGroup = true;
      tool.applyVCBValue(100, 15);
      expect(warnSpy).toHaveBeenCalled();
      expect(ctx.bridge.createSolidExtrudeTapered).not.toHaveBeenCalled();
      warnSpy.mockRestore();
    });
  });

  // ADR-260 β-3 — circle → cone/frustum via VCB "거리,비율%" (topScale 3rd arg).
  describe('ADR-260 β-3 cone VCB (거리,비율%)', () => {
    it('distance + topScale routes to createSolidExtrudeCone, NOT extrude/taper', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(800, undefined, 0.4); // 거리 800, top 40%
      expect(ctx.bridge.createSolidExtrudeCone).toHaveBeenCalledWith(5, 800, 0.4);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
      expect(ctx.bridge.createSolidExtrudeTapered).not.toHaveBeenCalled();
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('topScale 0 (apex cone) routes to createSolidExtrudeCone', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(800, undefined, 0); // apex
      expect(ctx.bridge.createSolidExtrudeCone).toHaveBeenCalledWith(5, 800, 0);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
    });

    it('no topScale → straight createSolidExtrude (existing path unchanged)', () => {
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(800);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 800);
      expect(ctx.bridge.createSolidExtrudeCone).not.toHaveBeenCalled();
    });

    it('smooth group + cone → Toast.warning, no cone extrude (flat-circle only)', () => {
      const warnSpy = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).isSmoothGroup = true;
      tool.applyVCBValue(800, undefined, 0.4);
      expect(warnSpy).toHaveBeenCalled();
      expect(ctx.bridge.createSolidExtrudeCone).not.toHaveBeenCalled();
      warnSpy.mockRestore();
    });
  });

  // ADR-261 β-3 — bidirectional / two-sided extrude via the ExtrudeMode toggle.
  describe('ADR-261 β-3 bidirectional ExtrudeMode', () => {
    afterEach(() => {
      // reset module-level mode so it doesn't leak into other tests
      setExtrudeMode('oneway');
      setExtrudeDistNeg(0);
    });

    it('symmetric mode: plain distance routes to createSolidExtrudeBidirectional (dp, dp)', () => {
      setExtrudeMode('symmetric');
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(300);
      expect(ctx.bridge.createSolidExtrudeBidirectional).toHaveBeenCalledWith(5, 300, 300);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('twosided mode: (dp, distNeg) from settings', () => {
      setExtrudeMode('twosided');
      setExtrudeDistNeg(200);
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(800);
      expect(ctx.bridge.createSolidExtrudeBidirectional).toHaveBeenCalledWith(5, 800, 200);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
    });

    it('oneway (default): plain distance stays one-way createSolidExtrude', () => {
      // mode is 'oneway' (reset by afterEach / default)
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(300);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 300);
      expect(ctx.bridge.createSolidExtrudeBidirectional).not.toHaveBeenCalled();
    });

    it('comma input (taper) takes priority over bidirectional mode', () => {
      setExtrudeMode('symmetric');
      ctx.getSelectedFaces.mockReturnValue([5]);
      tool.applyVCBValue(100, 15); // taper — has taperDeg arg
      expect(ctx.bridge.createSolidExtrudeTapered).toHaveBeenCalledWith(5, 100, 15);
      expect(ctx.bridge.createSolidExtrudeBidirectional).not.toHaveBeenCalled();
    });

    it('smooth group + bidir → Toast.warning, no bidir extrude (flat-profile only)', () => {
      setExtrudeMode('symmetric');
      const warnSpy = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (tool as any).isSmoothGroup = true;
      tool.applyVCBValue(300);
      expect(warnSpy).toHaveBeenCalled();
      expect(ctx.bridge.createSolidExtrudeBidirectional).not.toHaveBeenCalled();
      warnSpy.mockRestore();
    });
  });

  describe('cleanup', () => {
    it('resets all state', () => {
      // Start a session
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 0,
        point: new THREE.Vector3(0, 0, 0),
      });
      ctx.getFaceId.mockReturnValue(1);
      tool.onMouseDown({ clientX: 100, clientY: 100 } as MouseEvent, null);

      tool.cleanup();
      expect(tool.isBusy()).toBe(false);
      expect(ctx.selection.clearSelection).toHaveBeenCalled();
      expect(ctx.dimLabel.clear).toHaveBeenCalled();
    });
  });

  describe('onMouseMove', () => {
    it('does nothing when not active', () => {
      tool.onMouseMove({ clientX: 200, clientY: 200 } as MouseEvent, null);
      // Should not throw
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-087 K-ε — kernel-aware createSolidExtrude only path (Q3 fallback
  // to push_pull is now Rust-side, not exposed to TS).
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-087 K-ε kernel-aware dispatch', () => {
    it('always calls bridge.createSolidExtrude (single face)', () => {
      ctx.getSelectedFaces.mockReturnValue([7]);
      tool.applyVCBValue(150);

      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledTimes(1);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(7, 150);
      expect(ctx.bridge.pushPull).not.toHaveBeenCalled();
    });

    it('smooth group fallback → createSolidExtrude per-face', () => {
      // Force smooth-group fallback path: seamless returns false.
      ctx.bridge.engine.push_pull_smooth_group_seamless.mockReturnValue(false);

      ctx.getSelectedFaces.mockReturnValue([3]);
      tool.applyVCBValue(50);

      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledTimes(1);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(3, 50);
      expect(ctx.bridge.pushPull).not.toHaveBeenCalled();
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-193 — Live Push/Pull (direct manipulation) for a single planar face
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-193 live Push/Pull', () => {
    // Phase-1 select a SINGLE planar face. The pick point is far below the
    // origin so the (mocked) ray∩plane math in ppRayDist yields a large
    // signed distance (> MIN_COMMIT_DIST) — letting the live session begin
    // deterministically in the headless three mock.
    function startSingleFaceDrag() {
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 0,
        point: new THREE.Vector3(0, -1000, 0),
      });
      ctx.getFaceId.mockReturnValue(5);
      ctx.bridge.getFaceNormal.mockReturnValue(new Float32Array([0, 1, 0]));
      ctx.selection.getSmoothGroup.mockReturnValue([]); // single face
      tool.onMouseDown({ clientX: 400, clientY: 300 } as MouseEvent, null);
    }

    it('begins a live session on the first move past threshold (no ghost)', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null);
      expect(ctx.bridge.beginLiveExtrude).toHaveBeenCalledWith(5, expect.any(Number));
      expect(ctx.syncMesh).toHaveBeenCalled();
    });

    it('updates the live session on subsequent moves', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null); // begin
      tool.onMouseMove({ clientX: 400, clientY: 100 } as MouseEvent, null); // update
      expect(ctx.bridge.updateLiveExtrude).toHaveBeenCalled();
    });

    it('commits the live session on the second click (single Undo)', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null); // begin
      tool.onMouseDown({ clientX: 400, clientY: 200 } as MouseEvent, null); // Phase 2
      expect(ctx.bridge.commitLiveExtrude).toHaveBeenCalled();
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    // Press-drag-release (SketchUp/Fusion): holding the button + dragging then
    // releasing commits on mouseUP (previously there was no onMouseUp so a drag
    // did nothing — "커서 드래그는 안됨").
    it('press-drag-release commits on mouseUp (held button)', () => {
      startSingleFaceDrag();
      // button held (buttons:1) + moved past the px threshold ⇒ live begin + drag
      tool.onMouseMove({ clientX: 400, clientY: 200, buttons: 1 } as MouseEvent, null);
      expect(ctx.bridge.beginLiveExtrude).toHaveBeenCalled();
      tool.onMouseUp({ clientX: 400, clientY: 200 } as MouseEvent);
      expect(ctx.bridge.commitLiveExtrude).toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    // A plain click with no drag must NOT commit on release — click-move-click
    // still waits for the second click (both gestures coexist).
    it('a plain click (no drag) does not commit on mouseUp', () => {
      startSingleFaceDrag(); // Phase 1 mousedown at (400,300)
      tool.onMouseUp({ clientX: 400, clientY: 300 } as MouseEvent); // released in place
      expect(ctx.bridge.commitLiveExtrude).not.toHaveBeenCalled();
      expect(tool.isBusy()).toBe(true); // still armed for the second click
    });

    // A button-up move (the middle of click-move-click) must NOT arm the drag
    // commit — only a held-button (buttons&1) move does.
    it('a button-up move does not arm the drag-release commit', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 100, buttons: 0 } as MouseEvent, null);
      tool.onMouseUp({ clientX: 400, clientY: 100 } as MouseEvent);
      expect(ctx.bridge.commitLiveExtrude).not.toHaveBeenCalled();
      expect(tool.isBusy()).toBe(true);
    });

    // ADR-252 — a rect drawn on a wall (sheet source) captures the wall
    // thickness on pick (for the pocket↔through ghost color) and never begins a
    // live extrude — the drag previews a ghost box and the commit carves.
    it('sheet source captures wall thickness and does not begin a live extrude', () => {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, 100, 0) });
      ctx.getFaceId.mockReturnValue(7);
      ctx.bridge.getFaceNormal.mockReturnValue(new Float32Array([0, 1, 0]));
      ctx.selection.getSmoothGroup.mockReturnValue([]);
      ctx.bridge.faceHasLargerCoplanarContainer = vi.fn().mockReturnValue(true);
      ctx.bridge.wallThicknessFromSourceFace = vi.fn().mockReturnValue(300);
      ctx.extractFaceBoundary = vi.fn().mockReturnValue([
        new THREE.Vector3(-50, 100, -50), new THREE.Vector3(50, 100, -50),
        new THREE.Vector3(50, 100, 50), new THREE.Vector3(-50, 100, 50),
      ]);
      tool.onMouseDown({ clientX: 400, clientY: 300 } as MouseEvent, null);
      expect(ctx.bridge.wallThicknessFromSourceFace).toHaveBeenCalledWith(7);
      // dragging (held button) previews a ghost — never a live extrude.
      tool.onMouseMove({ clientX: 400, clientY: 420, buttons: 1 } as MouseEvent, null);
      expect(ctx.bridge.beginLiveExtrude).not.toHaveBeenCalled();
    });

    it('ESC cancels the live session (rollback)', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null); // begin
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      expect(ctx.bridge.cancelLiveExtrude).toHaveBeenCalled();
      expect(tool.isBusy()).toBe(false);
    });

    it('VCB commits the live session at the typed value', () => {
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null); // begin
      tool.applyVCBValue(123);
      expect(ctx.bridge.updateLiveExtrude).toHaveBeenCalledWith(123);
      expect(ctx.bridge.commitLiveExtrude).toHaveBeenCalled();
    });

    it('smooth group does NOT use the live session (keeps the ghost)', () => {
      ctx.viewport.pick.mockReturnValue({
        faceIndex: 0,
        point: new THREE.Vector3(0, -1000, 0),
      });
      ctx.getFaceId.mockReturnValue(5);
      ctx.selection.getSmoothGroup.mockReturnValue([5, 6, 7]); // smooth group
      tool.onMouseDown({ clientX: 400, clientY: 300 } as MouseEvent, null);
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null);
      expect(ctx.bridge.beginLiveExtrude).not.toHaveBeenCalled();
    });

    it('falls back to the legacy commit when beginLiveExtrude is unavailable', () => {
      ctx.bridge.beginLiveExtrude.mockReturnValue(null);
      startSingleFaceDrag();
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null); // begin fails → ghost
      tool.onMouseDown({ clientX: 400, clientY: 200 } as MouseEvent, null); // Phase 2
      expect(ctx.bridge.commitLiveExtrude).not.toHaveBeenCalled();
      // legacy commit path used createSolidExtrude on the seed face
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, expect.any(Number));
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-190 Phase 3 — repeat last distance (double-click), SketchUp parity.
  // `lastPPDist` was written by all four commit paths but READ by nothing —
  // a dead cache. These lock the wiring AND its guards: the feature may only
  // ADD behaviour, never divert a normal click-move-click.
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-190 Phase 3 — repeat last (double-click)', () => {
    /** Phase-1 pick a single planar face (mirrors startSingleFaceDrag). */
    function pickFace() {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, -1000, 0) });
      ctx.getFaceId.mockReturnValue(5);
      ctx.bridge.getFaceNormal.mockReturnValue(new Float32Array([0, 1, 0]));
      ctx.selection.getSmoothGroup.mockReturnValue([]);
      tool.onMouseDown({ clientX: 400, clientY: 300 } as MouseEvent, null);
    }
    /** Commit once via VCB so lastPPDist is seeded by a REAL commit path. */
    function seedLastDist(v: number) {
      pickFace();
      tool.applyVCBValue(v);
      vi.clearAllMocks();
    }

    /**
     * Whether the repeat block RAN — its Toast is the only observable signal.
     * Asserting on the committed distance alone is not enough: with a live
     * session the commit takes no argument (`commitLiveExtrude()`), so a
     * mis-fire there would be invisible in the bridge calls.
     */
    function repeatFired(spy: ReturnType<typeof vi.spyOn>): boolean {
      return spy.mock.calls.some((c) => String(c[0]).includes('직전 거리 반복'));
    }

    it('double-click re-applies the last committed distance', () => {
      const info = vi.spyOn(Toast, 'info').mockImplementation(() => {});
      seedLastDist(42);
      pickFace();                                   // 1st click of the double
      // 2nd click, cursor unmoved → no live session → legacy commit at 42
      tool.onMouseDown({ clientX: 400, clientY: 300, detail: 2 } as MouseEvent, null);
      expect(repeatFired(info)).toBe(true);
      expect(ctx.bridge.createSolidExtrude).toHaveBeenCalledWith(5, 42);
      info.mockRestore();
    });

    it('does NOT fire without a prior commit (lastPPDist still 0)', () => {
      const info = vi.spyOn(Toast, 'info').mockImplementation(() => {});
      pickFace();                                   // no seed
      tool.onMouseDown({ clientX: 400, clientY: 300, detail: 2 } as MouseEvent, null);
      expect(repeatFired(info), 'nothing to repeat → must not announce one').toBe(false);
      info.mockRestore();
    });

    it('does NOT fire on a single click (detail 1), even with the cursor unmoved', () => {
      const info = vi.spyOn(Toast, 'info').mockImplementation(() => {});
      seedLastDist(42);
      pickFace();
      // same geometry as the repeat case — ONLY detail differs, so this pins
      // the detail guard rather than being shadowed by the drag guard
      tool.onMouseDown({ clientX: 400, clientY: 300, detail: 1 } as MouseEvent, null);
      expect(repeatFired(info)).toBe(false);
      expect(ctx.bridge.createSolidExtrude).not.toHaveBeenCalledWith(5, 42);
      info.mockRestore();
    });

    it('does NOT override a distance already in flight, even on detail 2', () => {
      const info = vi.spyOn(Toast, 'info').mockImplementation(() => {});
      seedLastDist(42);
      pickFace();
      // every onMouseMove ends in `currentDragDist = dist`, so a moved cursor
      // (drag or align) makes the guard decline regardless of detail
      tool.onMouseMove({ clientX: 400, clientY: 200 } as MouseEvent, null);
      tool.onMouseDown({ clientX: 400, clientY: 200, detail: 2 } as MouseEvent, null);
      expect(repeatFired(info), 'a live drag distance must win').toBe(false);
      info.mockRestore();
    });
  });

  // ════════════════════════════════════════════════════════════════════════
  // ADR-190 Phase 3 — break the silence around the ADR-196 inward clamp.
  // Measured (ADR-293 §5): a 2000×1000×1000 box pushed −1500 collapses to
  // 0.001mm thick, createSolidExtrude returns true, the mesh is watertight,
  // and nothing is said. The clamp stays; the silence goes.
  // ════════════════════════════════════════════════════════════════════════
  describe('ADR-190 Phase 3 — inward clamp is announced', () => {
    /** Arm Push/Pull on a single planar face. Does NOT touch moveOnlyMaxInward,
     *  so a test may install its own probe for it. */
    function armFace() {
      ctx.viewport.pick.mockReturnValue({ faceIndex: 0, point: new THREE.Vector3(0, -1000, 0) });
      ctx.getFaceId.mockReturnValue(5);
      ctx.bridge.getFaceNormal.mockReturnValue(new Float32Array([0, 1, 0]));
      ctx.selection.getSmoothGroup.mockReturnValue([]);
      tool.onMouseDown({ clientX: 400, clientY: 300 } as MouseEvent, null);
    }
    function pickFaceThenCommit(pushDist: number, thickness: number) {
      ctx.bridge.moveOnlyMaxInward = vi.fn().mockReturnValue(thickness);
      armFace();
      tool.applyVCBValue(pushDist);
    }
    const clampWarned = (spy: ReturnType<typeof vi.spyOn>) =>
      spy.mock.calls.some((c) => String(c[0]).includes('에서 멈췄습니다'));

    it('warns when an inward push exceeds the solid thickness', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      pickFaceThenCommit(-1500, 1000);
      expect(clampWarned(warn), 'a silently clamped push must be explained').toBe(true);
      expect(warn.mock.calls[0][0]).toContain('1000.0mm');   // the actual limit
      warn.mockRestore();
    });

    it('stays quiet for an inward push within the thickness', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      pickFaceThenCommit(-300, 1000);
      expect(clampWarned(warn)).toBe(false);
      warn.mockRestore();
    });

    it('stays quiet at exactly the thickness (the clamp has nothing to cut)', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      pickFaceThenCommit(-1000, 1000);
      expect(clampWarned(warn), 'boundary must not false-positive').toBe(false);
      warn.mockRestore();
    });

    it('stays quiet for an outward push, and for a flat profile (-1 = unclamped)', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      pickFaceThenCommit(1500, 1000);          // outward
      expect(clampWarned(warn)).toBe(false);
      tool.onKeyDown({ key: 'Escape' } as KeyboardEvent);
      pickFaceThenCommit(-1500, -1);           // flat/open profile → no walls
      expect(clampWarned(warn), 'an unclamped profile must not be warned about').toBe(false);
      warn.mockRestore();
    });

    it('measures the limit BEFORE the commit (afterwards the evidence is gone)', () => {
      const warn = vi.spyOn(Toast, 'warning').mockImplementation(() => {});
      const order: string[] = [];
      ctx.bridge.moveOnlyMaxInward = vi.fn(() => { order.push('measure'); return 1000; });
      ctx.bridge.createSolidExtrude = vi.fn(() => { order.push('commit'); return true; });
      armFace();                       // must not clobber the probes above
      tool.applyVCBValue(-1500);
      // both must actually have run — a bare index comparison would pass with
      // `measure` missing entirely (-1 < 0), which is how this test first
      // shipped vacuous
      expect(order, 'both the measurement and the commit must run').toEqual(['measure', 'commit']);
      warn.mockRestore();
    });
  });
});
