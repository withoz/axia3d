import { describe, it, expect, beforeEach } from 'vitest';
import {
  getCylinderSegments,
  setCylinderSegments,
  onCylinderSegmentsChange,
  CYLINDER_SEGMENTS_DEFAULT,
  CYLINDER_SEGMENTS_MIN,
  CYLINDER_SEGMENTS_MAX,
} from './CylinderSegmentsSettings';

describe('CylinderSegmentsSettings', () => {
  beforeEach(() => {
    setCylinderSegments(CYLINDER_SEGMENTS_DEFAULT);
    try { localStorage.removeItem('axia:cylinder:segments'); } catch { /* ignore */ }
  });

  it('default is 16', () => {
    expect(CYLINDER_SEGMENTS_DEFAULT).toBe(16);
    expect(getCylinderSegments()).toBe(16);
  });

  it('sets and reads a valid value + persists to localStorage', () => {
    setCylinderSegments(48);
    expect(getCylinderSegments()).toBe(48);
    expect(localStorage.getItem('axia:cylinder:segments')).toBe('48');
  });

  it('clamps below min and above max', () => {
    setCylinderSegments(1);
    expect(getCylinderSegments()).toBe(CYLINDER_SEGMENTS_MIN);
    setCylinderSegments(9999);
    expect(getCylinderSegments()).toBe(CYLINDER_SEGMENTS_MAX);
  });

  it('rounds to an integer + ignores non-finite', () => {
    setCylinderSegments(31.7);
    expect(getCylinderSegments()).toBe(32);
    setCylinderSegments(Number.NaN);
    expect(getCylinderSegments()).toBe(32); // unchanged
  });

  it('notifies listeners on change only', () => {
    setCylinderSegments(24);
    let seen = -1;
    const off = onCylinderSegmentsChange((v) => { seen = v; });
    setCylinderSegments(24); // no change → no notify
    expect(seen).toBe(-1);
    setCylinderSegments(64);
    expect(seen).toBe(64);
    off();
    setCylinderSegments(12);
    expect(seen).toBe(64); // detached
  });
});
