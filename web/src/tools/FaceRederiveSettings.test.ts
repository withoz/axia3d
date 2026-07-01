/**
 * ADR-186 (A) — FaceRederiveSettings regression coverage.
 * production default ON (사용자 결재 2026-06-02 "내가 그리면 안됨" — UI 마우스
 * draw 도 analytic rederive containment 경로 사용). engine default 는 OFF.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('FaceRederiveSettings', () => {
  beforeEach(() => {
    vi.resetModules();
    localStorage.clear();
  });

  it('default ON (production — UI 마우스 draw 도 analytic rederive)', async () => {
    const m = await import('./FaceRederiveSettings');
    expect(m.getFaceRederive()).toBe(true);
  });

  it('localStorage "true" → ON (matches default)', async () => {
    localStorage.setItem('axia:face-rederive-on-draw', 'true');
    const m = await import('./FaceRederiveSettings');
    expect(m.getFaceRederive()).toBe(true);
  });

  it('localStorage "false" → OFF (explicit OFF preference 보존)', async () => {
    localStorage.setItem('axia:face-rederive-on-draw', 'false');
    const m = await import('./FaceRederiveSettings');
    expect(m.getFaceRederive()).toBe(false);
  });

  it('setFaceRederive persists to localStorage', async () => {
    const m = await import('./FaceRederiveSettings');
    m.setFaceRederive(false);
    expect(localStorage.getItem('axia:face-rederive-on-draw')).toBe('false');
    m.setFaceRederive(true);
    expect(localStorage.getItem('axia:face-rederive-on-draw')).toBe('true');
  });

  it('onFaceRederiveChange fires on actual change only', async () => {
    const m = await import('./FaceRederiveSettings');
    const cb = vi.fn();
    const unsubscribe = m.onFaceRederiveChange(cb);
    m.setFaceRederive(false);
    expect(cb).toHaveBeenCalledTimes(1);
    expect(cb).toHaveBeenCalledWith(false);
    m.setFaceRederive(false); // no-op
    expect(cb).toHaveBeenCalledTimes(1);
    m.setFaceRederive(true);
    expect(cb).toHaveBeenCalledTimes(2);
    unsubscribe();
    m.setFaceRederive(false);
    expect(cb).toHaveBeenCalledTimes(2);
  });
});
