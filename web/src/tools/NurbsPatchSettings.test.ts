import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  getNurbsPatchMode,
  setNurbsPatchMode,
  onNurbsPatchModeChange,
  type NurbsPatchMode,
} from './NurbsPatchSettings';

describe('NurbsPatchSettings (ADR-231)', () => {
  beforeEach(() => {
    setNurbsPatchMode('bezier');
    try {
      localStorage.removeItem('axia:nurbs-patch-mode');
    } catch {
      /* ignore */
    }
  });

  it('defaults to bezier (preserves current MVP behavior)', () => {
    expect(getNurbsPatchMode()).toBe('bezier');
  });

  it('setNurbsPatchMode switches mode + persists to localStorage', () => {
    setNurbsPatchMode('vault');
    expect(getNurbsPatchMode()).toBe('vault');
    expect(localStorage.getItem('axia:nurbs-patch-mode')).toBe('vault');
    setNurbsPatchMode('bezier');
    expect(getNurbsPatchMode()).toBe('bezier');
  });

  it('is idempotent (no listener fire on same value)', () => {
    setNurbsPatchMode('vault');
    const cb = vi.fn();
    const off = onNurbsPatchModeChange(cb);
    setNurbsPatchMode('vault');
    expect(cb).not.toHaveBeenCalled();
    off();
  });

  it('onNurbsPatchModeChange fires on change + unsubscribe works', () => {
    const seen: NurbsPatchMode[] = [];
    const off = onNurbsPatchModeChange((m) => seen.push(m));
    setNurbsPatchMode('vault');
    setNurbsPatchMode('bezier');
    expect(seen).toEqual(['vault', 'bezier']);
    off();
    setNurbsPatchMode('vault');
    expect(seen).toEqual(['vault', 'bezier']);
  });
});
