import { describe, it, expect, beforeEach, vi } from 'vitest';
import { OffsetSessionManager } from './OffsetSessionManager';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn(), debugWarn: vi.fn() }));

function mockToolContext() {
  return {
    bridge: {
      pushPull: vi.fn().mockReturnValue(true),
      undo: vi.fn(),
      facesCentroid: vi.fn().mockReturnValue(new Float32Array([0, 0, 0])),
      getFaceNormal: vi.fn().mockReturnValue(new Float32Array([0, 1, 0])),
      engine: {
        push_pull_smooth_group_seamless: vi.fn().mockReturnValue(true),
      },
    },
    viewport: {
      scene: { add: vi.fn(), remove: vi.fn() },
    },
    syncMesh: vi.fn(),
    units: {
      format: vi.fn().mockReturnValue('10.0 mm'),
    },
  } as any;
}

describe('OffsetSessionManager', () => {
  let ctx: ReturnType<typeof mockToolContext>;
  let manager: OffsetSessionManager;

  beforeEach(() => {
    ctx = mockToolContext();
    manager = new OffsetSessionManager(ctx);
  });

  describe('start', () => {
    it('starts session with single face', () => {
      expect(manager.start([5])).toBe(true);
      expect(manager.isActive()).toBe(true);
    });

    it('starts session with multiple faces (smooth group)', () => {
      expect(manager.start([1, 2, 3])).toBe(true);
      expect(manager.isActive()).toBe(true);
    });

    it('returns false for empty faceIds', () => {
      expect(manager.start([])).toBe(false);
      expect(manager.isActive()).toBe(false);
    });

    it('returns false when session already active', () => {
      manager.start([1]);
      expect(manager.start([2])).toBe(false);
    });
  });

  describe('isActive', () => {
    it('false when no session', () => {
      expect(manager.isActive()).toBe(false);
    });

    it('true after start', () => {
      manager.start([1]);
      expect(manager.isActive()).toBe(true);
    });
  });

  describe('getCurrentDistance', () => {
    it('returns 0 when no session', () => {
      expect(manager.getCurrentDistance()).toBe(0);
    });
  });

  describe('setParam', () => {
    it('does nothing when no session', () => {
      manager.setParam(50); // should not throw
      expect(manager.getCurrentDistance()).toBe(0);
    });
  });

  describe('confirm', () => {
    it('returns false when no session', () => {
      expect(manager.confirm()).toBe(false);
    });
  });

  describe('cancel', () => {
    it('does nothing when no session', () => {
      // ⚠ Was assertion-free ("should not throw"). Deleting cancel()'s own
      // early return left it green, so it held nothing. Name the state instead.
      expect(manager.isActive(), 'premise: no session').toBe(false);
      expect(() => manager.cancel()).not.toThrow();
      expect(manager.isActive(), 'and cancel leaves it inactive').toBe(false);
      expect(manager.getSession()).toBeNull();
      // ⚠ The state assertions above are NOT enough - they hold whether or
      // not the guard exists, because the session was already null. What the
      // guard actually buys is that cancel() does not reach bridge.undo() and
      // roll back somebody else's operation. Mutation-checked: delete
      // `if (!this.session) return;` and THIS line fails.
      expect(ctx.bridge.undo, 'cancel with no session must not undo anything')
        .not.toHaveBeenCalled();
    });
  });

  describe('getSession', () => {
    it('returns null when no session', () => {
      expect(manager.getSession()).toBeNull();
    });

    it('returns session with correct target type', () => {
      manager.start([1, 2]);
      const session = manager.getSession();
      expect(session).not.toBeNull();
      expect(session!.target.type).toBe('smooth_group');
      expect(session!.target.faceIds).toEqual([1, 2]);
    });

    it('single face creates single type', () => {
      manager.start([5]);
      const session = manager.getSession();
      expect(session!.target.type).toBe('single');
    });

    it('session has correct initial values', () => {
      manager.start([1]);
      const session = manager.getSession();
      expect(session!.input.distance).toBe(0);
      expect(session!.ui.active).toBe(true);
      expect(session!.ui.mode).toBe('preview');
      expect(session!.snap.enabled).toBe(true);
    });
  });

  describe('dispose', () => {
    it('does not throw when no session', () => {
      expect(() => manager.dispose()).not.toThrow();
    });
  });
});
