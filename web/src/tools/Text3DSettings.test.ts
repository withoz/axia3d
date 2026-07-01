import { describe, it, expect, beforeEach, vi } from 'vitest';
import {
  getText3DMode,
  setText3DMode,
  onText3DModeChange,
  type Text3DMode,
} from './Text3DSettings';

describe('Text3DSettings (ADR-228)', () => {
  beforeEach(() => {
    // reset to default between tests
    setText3DMode('extruded');
    try {
      localStorage.removeItem('axia:text3d-mode');
    } catch {
      /* ignore */
    }
  });

  it('defaults to extruded', () => {
    expect(getText3DMode()).toBe('extruded');
  });

  it('setText3DMode switches mode + persists to localStorage', () => {
    setText3DMode('sprite');
    expect(getText3DMode()).toBe('sprite');
    expect(localStorage.getItem('axia:text3d-mode')).toBe('sprite');
    setText3DMode('extruded');
    expect(getText3DMode()).toBe('extruded');
    expect(localStorage.getItem('axia:text3d-mode')).toBe('extruded');
  });

  it('setText3DMode is idempotent (no listener fire on same value)', () => {
    setText3DMode('sprite');
    const cb = vi.fn();
    const off = onText3DModeChange(cb);
    setText3DMode('sprite'); // same → no fire
    expect(cb).not.toHaveBeenCalled();
    off();
  });

  it('onText3DModeChange fires on change + unsubscribe works', () => {
    const seen: Text3DMode[] = [];
    const off = onText3DModeChange((m) => seen.push(m));
    setText3DMode('sprite');
    setText3DMode('extruded');
    expect(seen).toEqual(['sprite', 'extruded']);
    off();
    setText3DMode('sprite');
    expect(seen).toEqual(['sprite', 'extruded']); // no further pushes after off()
  });

  it('only accepts valid modes (type-guarded)', () => {
    setText3DMode('sprite');
    // @ts-expect-error — invalid mode rejected at compile time
    const bad: Text3DMode = 'invalid';
    expect(bad).toBeDefined(); // runtime: TS guards the union; nothing to assert beyond compile
    expect(getText3DMode()).toBe('sprite');
  });
});
