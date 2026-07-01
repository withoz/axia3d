/**
 * ADR-186 A3/B6-2b — FreeformOverlapSettings regression coverage.
 * production default ON (구조 D1=(b) gated+flip — 겹치는 freeform self-loop
 * 가 smooth lens 로 자동 split). engine default 는 OFF. FaceRederiveSettings
 * 패턴 1:1 답습.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('FreeformOverlapSettings', () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it('default ON (production — freeform overlap → smooth lens split)', async () => {
    const m = await import('./FreeformOverlapSettings');
    expect(m.getFreeformOverlap()).toBe(true);
  });

  it('localStorage "true" → ON (matches default)', async () => {
    localStorage.setItem('axia:freeform-overlap-on-draw', 'true');
    const m = await import('./FreeformOverlapSettings');
    expect(m.getFreeformOverlap()).toBe(true);
  });

  it('localStorage "false" → OFF (explicit OFF preference 보존)', async () => {
    localStorage.setItem('axia:freeform-overlap-on-draw', 'false');
    const m = await import('./FreeformOverlapSettings');
    expect(m.getFreeformOverlap()).toBe(false);
  });

  it('setFreeformOverlap persists to localStorage', async () => {
    const m = await import('./FreeformOverlapSettings');
    m.setFreeformOverlap(false);
    expect(localStorage.getItem('axia:freeform-overlap-on-draw')).toBe('false');
    m.setFreeformOverlap(true);
    expect(localStorage.getItem('axia:freeform-overlap-on-draw')).toBe('true');
  });

  it('onFreeformOverlapChange fires on actual change only', async () => {
    const m = await import('./FreeformOverlapSettings');
    const cb = vi.fn();
    const unsubscribe = m.onFreeformOverlapChange(cb);
    m.setFreeformOverlap(false);
    expect(cb).toHaveBeenCalledTimes(1);
    expect(cb).toHaveBeenCalledWith(false);
    m.setFreeformOverlap(false); // no-op
    expect(cb).toHaveBeenCalledTimes(1);
    m.setFreeformOverlap(true);
    expect(cb).toHaveBeenCalledTimes(2);
    unsubscribe();
    m.setFreeformOverlap(false);
    expect(cb).toHaveBeenCalledTimes(2);
  });
});
