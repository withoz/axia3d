import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import {
  stashSceneForLocaleSwitch,
  takeStashedScene,
  clearStashedScene,
  MAX_SNAPSHOT_BYTES,
} from './localeSwitchScene';

/**
 * These carry someone's drawing across a reload. A bug here does not throw —
 * it silently hands back the wrong bytes, or nothing, and the work is gone.
 * So the round-trip is checked at a size where the encoding actually has to
 * work, not just at four bytes.
 */
describe('localeSwitchScene', () => {
  beforeEach(() => sessionStorage.clear());
  afterEach(() => {
    sessionStorage.clear();
    vi.restoreAllMocks();
  });

  describe('round-trip', () => {
    it('returns exactly the bytes it was given', () => {
      const scene = new Uint8Array([0, 1, 127, 128, 255, 42]);
      expect(stashSceneForLocaleSwitch(scene)).toBe(true);
      expect(takeStashedScene()).toEqual(scene);
    });

    it('survives a snapshot larger than one base64 chunk', () => {
      // CHUNK is 0x8000. Anything under that never exercises the loop, and
      // `String.fromCharCode(...bytes)` on a whole real snapshot would blow
      // the call stack — which is the reason the loop exists.
      const scene = new Uint8Array(0x8000 * 3 + 17);
      for (let i = 0; i < scene.length; i++) scene[i] = i % 256;
      expect(stashSceneForLocaleSwitch(scene)).toBe(true);
      const back = takeStashedScene();
      expect(back).toEqual(scene);
    });

    it('preserves high bytes (a snapshot is not text)', () => {
      // bincode output is arbitrary binary; a latin1/utf8 mixup would mangle
      // everything above 0x7F and only show up as a corrupt scene on boot.
      const scene = new Uint8Array(256);
      for (let i = 0; i < 256; i++) scene[i] = i;
      stashSceneForLocaleSwitch(scene);
      expect(takeStashedScene()).toEqual(scene);
    });
  });

  describe('take consumes', () => {
    it('gives the scene back exactly once', () => {
      stashSceneForLocaleSwitch(new Uint8Array([9]));
      expect(takeStashedScene()).toEqual(new Uint8Array([9]));
      // A second restore would re-apply a stale scene over whatever the user
      // has drawn since — worse than the bug this file fixes.
      expect(takeStashedScene()).toBeNull();
    });

    it('returns null when nothing was stashed', () => {
      expect(takeStashedScene()).toBeNull();
    });
  });

  describe('refuses rather than pretends', () => {
    it('rejects an empty snapshot', () => {
      expect(stashSceneForLocaleSwitch(new Uint8Array(0))).toBe(false);
    });

    it('rejects a null snapshot', () => {
      expect(stashSceneForLocaleSwitch(null)).toBe(false);
    });

    it('rejects past the size ceiling instead of throwing on quota', () => {
      expect(stashSceneForLocaleSwitch(new Uint8Array(MAX_SNAPSHOT_BYTES + 1))).toBe(false);
      // and stores nothing, so a later take does not resurrect a partial write
      expect(takeStashedScene()).toBeNull();
    });

    it('accepts exactly at the ceiling', () => {
      // Off-by-one at the boundary would reject scenes that fit fine.
      expect(stashSceneForLocaleSwitch(new Uint8Array(MAX_SNAPSHOT_BYTES))).toBe(true);
    });

    it('reports false when storage refuses (quota, private mode)', () => {
      vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
        throw new DOMException('quota', 'QuotaExceededError');
      });
      expect(stashSceneForLocaleSwitch(new Uint8Array([1, 2, 3]))).toBe(false);
    });
  });

  describe('boot is never blocked', () => {
    it('returns null on a corrupt payload rather than throwing', () => {
      // Whatever produced this, an exception here would take main.ts down
      // before the app renders. An empty canvas beats a blank screen.
      sessionStorage.setItem('axia:locale-switch-scene', 'not%%base64%%');
      expect(() => takeStashedScene()).not.toThrow();
      expect(takeStashedScene()).toBeNull();
    });

    it('returns null when storage reads throw', () => {
      vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
        throw new DOMException('denied', 'SecurityError');
      });
      expect(takeStashedScene()).toBeNull();
    });

    it('clears a corrupt payload so it cannot be retried forever', () => {
      sessionStorage.setItem('axia:locale-switch-scene', 'not%%base64%%');
      takeStashedScene();
      expect(sessionStorage.getItem('axia:locale-switch-scene')).toBeNull();
    });
  });

  describe('clearStashedScene', () => {
    it('drops what was parked', () => {
      stashSceneForLocaleSwitch(new Uint8Array([1]));
      clearStashedScene();
      expect(takeStashedScene()).toBeNull();
    });

    it('is safe when storage is unavailable', () => {
      vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => {
        throw new DOMException('denied', 'SecurityError');
      });
      expect(() => clearStashedScene()).not.toThrow();
    });
  });
});
