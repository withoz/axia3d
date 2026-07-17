/**
 * BoundaryTool test — ADR-148 β-4 verification.
 *
 * Tests the TS UI tool integration of `bridge.boundaryFromPoint`:
 *   1. Click → bridge dispatch + syncMesh + Toast.success
 *   2. Engine throw → Toast.error with humanized Korean message
 *   3. humanizeBoundaryError translates all 4 BoundaryError variants
 *
 * Cross-link:
 *   - ADR-148 §2.4 (BoundaryError 4 variants Toast 한국어 매핑)
 *   - LOCKED #44 (Complete Meaning per Merge)
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setLocale } from '../i18n';
import * as THREE from 'three';
import { BoundaryTool, humanizeBoundaryError } from './BoundaryTool';
import type { ToolContext } from './ITool';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn() }));
vi.mock('../ui/Toast', () => ({
  Toast: {
    success: vi.fn(),
    error: vi.fn(),
    warning: vi.fn(),
    info: vi.fn(),
  },
}));

function mockCtx(): ToolContext {
  return {
    bridge: {
      boundaryFromPoint: vi.fn(() => 42),
    } as any,
    syncMesh: vi.fn(),
  } as unknown as ToolContext;
}

describe('BoundaryTool (ADR-148 β-4)', () => {
  // jsdom's navigator.language is 'en-US'; these assert Korean copy.
  beforeEach(() => setLocale('ko'));

  let ctx: ToolContext;
  let tool: BoundaryTool;

  beforeEach(() => {
    vi.clearAllMocks();
    ctx = mockCtx();
    tool = new BoundaryTool(ctx);
  });

  describe('click dispatch', () => {
    it('click at valid point dispatches to bridge.boundaryFromPoint with Z=0 plane', async () => {
      const { Toast } = await import('../ui/Toast');
      const point = new THREE.Vector3(5, 5, 0);
      tool.onMouseDown({} as MouseEvent, point);

      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        5, 5, 0,  // point xyz
        0, 0, 1,  // normal (Z-up canonical, LOCKED #63)
        0,        // plane dist
        1000,     // DEFAULT_SEARCH_RADIUS_MM
      );
      expect(ctx.syncMesh).toHaveBeenCalledTimes(1);
      expect(Toast.success).toHaveBeenCalledWith('Boundary 면이 생성되었습니다');
      expect(Toast.error).not.toHaveBeenCalled();
    });

    it('ADR-175/178 — face-aware: synthesizes on the hovered face plane (z=200)', () => {
      // getDrawPlane resolves the hit face's plane; the click pt lies on it.
      (ctx as any).getDrawPlane = vi.fn(() => ({
        normal: new THREE.Vector3(0, 0, 1),
        up: new THREE.Vector3(0, 1, 0),
        right: new THREE.Vector3(1, 0, 0),
        onFace: true,
      }));
      const point = new THREE.Vector3(5, 5, 200); // on a box top face at z=200
      tool.onMouseDown({} as MouseEvent, point);

      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        5, 5, 200, // point xyz
        0, 0, 1,   // face normal
        200,       // plane dist = normal · pt (NOT hardcoded 0)
        1000,
      );
    });

    it('ADR-175/178 — face-aware: non-cardinal +X face passes its normal', () => {
      (ctx as any).getDrawPlane = vi.fn(() => ({
        normal: new THREE.Vector3(1, 0, 0),
        up: new THREE.Vector3(0, 0, 1),
        right: new THREE.Vector3(0, 1, 0),
        onFace: true,
      }));
      const point = new THREE.Vector3(100, 5, 30); // on a +X face at x=100
      tool.onMouseDown({} as MouseEvent, point);

      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        100, 5, 30,
        1, 0, 0, // +X normal
        100,     // dist = normal · pt = 100
        1000,
      );
    });

    it('null point shows warning and does not dispatch', async () => {
      const { Toast } = await import('../ui/Toast');
      tool.onMouseDown({} as MouseEvent, null);

      expect(ctx.bridge.boundaryFromPoint).not.toHaveBeenCalled();
      expect(ctx.syncMesh).not.toHaveBeenCalled();
      expect(Toast.warning).toHaveBeenCalledWith(
        expect.stringContaining('유효한 평면 위 위치를 클릭'),
      );
    });

    it('engine throw → Toast.error with humanized Korean message (NoEnclosingCycle)', async () => {
      const { Toast } = await import('../ui/Toast');
      (ctx.bridge.boundaryFromPoint as any) = vi.fn(() => {
        throw new Error('boundaryFromPoint: NoEnclosingCycle');
      });
      const point = new THREE.Vector3(15, 5, 0);
      tool.onMouseDown({} as MouseEvent, point);

      expect(Toast.error).toHaveBeenCalledWith(
        expect.stringContaining('이 영역을 둘러싼 boundary 가 없습니다'),
      );
      expect(ctx.syncMesh).not.toHaveBeenCalled();
    });
  });

  describe('ADR-148 §5 — auto plane inference', () => {
    it('asks the geometry for the plane before falling back to the draw plane', async () => {
      const { Toast } = await import('../ui/Toast');
      const auto = vi.fn(() => 7);
      (ctx.bridge as any).boundaryFromPointAutoPlane = auto;

      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 5, 0));

      // No plane arguments: that is the point. Boundary runs where there is no
      // face to hit, so the draw-plane cascade cannot know the plane and a
      // loop at z=100 would fall through to Z=0.
      expect(auto).toHaveBeenCalledWith(5, 5, 0, 1000);
      expect(ctx.bridge.boundaryFromPoint).not.toHaveBeenCalled();
      expect(Toast.success).toHaveBeenCalledWith('Boundary 면이 생성되었습니다');
    });

    it('falls back to the draw plane when inference declines', async () => {
      const { Toast } = await import('../ui/Toast');
      // Ambiguous / not planar — the engine refuses rather than guessing
      // (메타-원칙 #5 / #16). The user's lock or sticky plane is still a
      // stated intent, so it is the honest second answer.
      (ctx.bridge as any).boundaryFromPointAutoPlane = vi.fn(() => {
        throw new Error('boundaryFromPointAutoPlane: NoEnclosingCycle');
      });

      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 5, 0));

      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        5, 5, 0, 0, 0, 1, 0, 1000,
      );
      expect(Toast.success).toHaveBeenCalledWith('Boundary 면이 생성되었습니다');
      expect(Toast.error).not.toHaveBeenCalled();
    });

    it('reports the draw-plane error when both paths decline', async () => {
      const { Toast } = await import('../ui/Toast');
      (ctx.bridge as any).boundaryFromPointAutoPlane = vi.fn(() => {
        throw new Error('boundaryFromPointAutoPlane: NoEnclosingCycle');
      });
      (ctx.bridge.boundaryFromPoint as any).mockImplementation(() => {
        throw new Error('boundaryFromPoint: NoOrphanEdgesInRadius (radius 1000.0mm)');
      });

      tool.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 5, 0));

      // The LAST error, not the first: what the user needs is why the plane
      // they are actually on did not work.
      expect(Toast.error).toHaveBeenCalledWith(
        expect.stringContaining('주변에 boundary 후보가 없습니다'),
      );
      expect(ctx.syncMesh).not.toHaveBeenCalled();
    });
  });

  describe('humanizeBoundaryError translations', () => {
    it('PointNotOnPlane includes distance value', () => {
      const msg = humanizeBoundaryError(
        'boundaryFromPoint: PointNotOnPlane (distance 10.000mm)',
      );
      expect(msg).toContain('10.000');
      expect(msg).toContain('평면 위가 아닙니다');
    });

    it('NoOrphanEdgesInRadius includes radius value', () => {
      const msg = humanizeBoundaryError(
        'boundaryFromPoint: NoOrphanEdgesInRadius (radius 1000.0mm)',
      );
      expect(msg).toContain('1000.0');
      expect(msg).toContain('boundary 후보가 없습니다');
    });

    it('NoEnclosingCycle returns canonical Korean message', () => {
      const msg = humanizeBoundaryError('boundaryFromPoint: NoEnclosingCycle');
      expect(msg).toBe('이 영역을 둘러싼 boundary 가 없습니다');
    });

    it('CycleAlreadyFaced returns canonical Korean message', () => {
      const msg = humanizeBoundaryError(
        'boundaryFromPoint: CycleAlreadyFaced (face 7)',
      );
      expect(msg).toContain('이미 면이 있습니다');
    });
  });

  // ════════════════════════════════════════════════════════════════════
  // ADR-170 β-3 — normalizeDrawInput SSOT migration verification
  // ════════════════════════════════════════════════════════════════════
  describe('ADR-170 β-3 — normalizeDrawInput SSOT routing', () => {
    it('calls ctx.normalizeDrawInput when available (SSOT routing)', () => {
      const normalizeFn = vi.fn((pt: THREE.Vector3) => ({ point: pt.clone() }));
      const ctxWithNormalize = {
        ...ctx,
        normalizeDrawInput: normalizeFn,
      } as unknown as ToolContext;
      const toolWithSSOT = new BoundaryTool(ctxWithNormalize);

      const point = new THREE.Vector3(5, 5, 0);
      toolWithSSOT.onMouseDown({} as MouseEvent, point);

      expect(normalizeFn).toHaveBeenCalledTimes(1);
      expect(normalizeFn).toHaveBeenCalledWith(point);
    });

    it('uses normalized point (not raw) for bridge.boundaryFromPoint call', () => {
      // Normalize returns a different point (simulating cardinal force / projection)
      const normalizedPoint = new THREE.Vector3(5.0, 5.0, 0); // exactly 0 z
      const ctxWithNormalize = {
        ...ctx,
        normalizeDrawInput: vi.fn(() => ({ point: normalizedPoint })),
      } as unknown as ToolContext;
      const toolWithSSOT = new BoundaryTool(ctxWithNormalize);

      const rawPoint = new THREE.Vector3(5.0, 5.0, 0.000001); // drift z
      toolWithSSOT.onMouseDown({} as MouseEvent, rawPoint);

      // bridge should receive normalized (exact 0), not raw (0.000001)
      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        5.0, 5.0, 0,  // normalized point (z = exact 0)
        0, 0, 1,
        0,
        1000,
      );
    });

    it('skipReason=DegenerateBelowEpsilon → Toast.warning + skip dispatch', async () => {
      const { Toast } = await import('../ui/Toast');
      const ctxWithSkip = {
        ...ctx,
        normalizeDrawInput: vi.fn(() => ({
          point: new THREE.Vector3(0, 0, 0),
          skipReason: 'DegenerateBelowEpsilon' as const,
        })),
      } as unknown as ToolContext;
      const toolWithSSOT = new BoundaryTool(ctxWithSkip);

      toolWithSSOT.onMouseDown({} as MouseEvent, new THREE.Vector3(0, 0, 0));

      expect(ctx.bridge.boundaryFromPoint).not.toHaveBeenCalled();
      expect(ctx.syncMesh).not.toHaveBeenCalled();
      expect(Toast.warning).toHaveBeenCalledWith(
        expect.stringContaining('너무 작은 영역'),
      );
    });

    it('graceful fallback when ctx.normalizeDrawInput absent (L-170-6 backward compat)', () => {
      // ctx (default from beforeEach) has NO normalizeDrawInput
      const point = new THREE.Vector3(5, 5, 0);
      tool.onMouseDown({} as MouseEvent, point);

      // Should still dispatch with raw point (backward compat)
      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledWith(
        5, 5, 0,
        0, 0, 1,
        0,
        1000,
      );
      expect(ctx.syncMesh).toHaveBeenCalledTimes(1);
    });

    it('no skipReason → normal dispatch flow continues', async () => {
      const { Toast } = await import('../ui/Toast');
      const ctxWithNormalize = {
        ...ctx,
        normalizeDrawInput: vi.fn((pt: THREE.Vector3) => ({
          point: pt.clone(),
          // No skipReason — normal flow
        })),
      } as unknown as ToolContext;
      const toolWithSSOT = new BoundaryTool(ctxWithNormalize);

      toolWithSSOT.onMouseDown({} as MouseEvent, new THREE.Vector3(5, 5, 0));

      expect(ctx.bridge.boundaryFromPoint).toHaveBeenCalledTimes(1);
      expect(Toast.success).toHaveBeenCalled();
    });
  });

  describe('lifecycle', () => {
    it('isBusy always returns false (single-click tool)', () => {
      expect(tool.isBusy()).toBe(false);
    });

    it('onActivate shows info Toast', async () => {
      const { Toast } = await import('../ui/Toast');
      tool.onActivate();
      expect(Toast.info).toHaveBeenCalledWith(
        expect.stringContaining('Boundary 도구'),
      );
    });
  });
});
