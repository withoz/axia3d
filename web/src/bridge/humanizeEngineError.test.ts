import { describe, it, expect } from 'vitest';
import { humanizeEngineError } from './humanizeEngineError';

/**
 * ADR-190 Phase 3 — engine error humanization.
 *
 * Every RAW fixture below was captured from the real engine through the
 * production bridge (Playwright + WASM), not invented — a made-up string would
 * pin a mapping that never fires. Driving the actual failure is the only way to
 * know what a user really sees.
 */
const RAW = {
  cylinderSidePush: 'Face needs at least 3 verts',
  steepTaper:
    'create_solid: not yet supported — tapered extrude v1 supports (Plane, AllLinear) only (ADR-259 D2) (Q3 fallback to legacy push_pull)',
  coneScaleOne:
    'create_solid: not yet supported — cone extrude: top_scale ≥ 1 is a cylinder — use straight Extrude (ADR-260 D2 [0,1)) (Q3 fallback to legacy push_pull)',
  staleFace: 'create_solid: face FaceId(3) not found or inactive',
  curvedPocketOnPlane: 'curved pocket: cap must be a Cylinder/Sphere/Cone/Torus-surface face',
} as const;

describe('humanizeEngineError', () => {
  it('turns the cylinder-side rejection into what to do instead', () => {
    const out = humanizeEngineError(RAW.cylinderSidePush);
    expect(out).not.toContain('verts');
    expect(out).toContain('곡면');
    expect(out).toContain('원을 그린 뒤');   // the actual way through
  });

  it('strips ADR refs, internal enum names and the Q3 fallback note from taper', () => {
    const out = humanizeEngineError(RAW.steepTaper);
    expect(out).not.toContain('ADR-259');
    expect(out).not.toContain('AllLinear');
    expect(out).not.toContain('push_pull');
    expect(out).toContain('테이퍼');
  });

  it('keeps the useful part of the cone message and drops the noise', () => {
    const out = humanizeEngineError(RAW.coneScaleOne);
    expect(out).not.toContain('top_scale');
    expect(out).not.toContain('ADR-260');
    expect(out).toContain('100%');
  });

  it('does not show FaceId(...) internals', () => {
    const out = humanizeEngineError(RAW.staleFace);
    expect(out).not.toContain('FaceId');
    expect(out).not.toContain('create_solid');
  });

  it('rewrites FaceId(N) on the pass-through path too', () => {
    // RAW.staleFace is matched by an explicit mapping and never reaches
    // stripInternals, so the FaceId rule only fires for UNMAPPED messages —
    // which is exactly where a raw id would otherwise surface.
    const out = humanizeEngineError('merge: FaceId(12) has no coplanar neighbour');
    expect(out).not.toContain('FaceId(12)');
    expect(out).toContain('면 #12');
  });

  it('drops cap/surface-face jargon from the curved ops', () => {
    const out = humanizeEngineError(RAW.curvedPocketOnPlane);
    expect(out).not.toContain('cap');
    expect(out).toContain('곡면');
  });

  it('passes UNKNOWN messages through — noise stripped, meaning kept', () => {
    // a whitelist that swallowed these would trade one silence for another
    const out = humanizeEngineError(
      'create_solid: something nobody mapped yet (ADR-999 X1) (Q3 fallback to legacy push_pull)',
    );
    expect(out).toContain('something nobody mapped yet');
    expect(out).not.toContain('ADR-999');
    expect(out).not.toContain('Q3 fallback');
    expect(out).not.toContain('create_solid:');
  });

  it('is empty for an empty error (callers fall back to their own text)', () => {
    expect(humanizeEngineError('')).toBe('');
    expect(humanizeEngineError('   ')).toBe('');
  });

  it('every mapped message is free of engine vocabulary', () => {
    // one sweep so a future mapping cannot quietly reintroduce leakage
    const leaks = [/ADR-\d+/, /FaceId\(/, /push_pull/, /AllLinear|AllCircular/, /top_scale/, /Q3 fallback/];
    for (const raw of Object.values(RAW)) {
      const out = humanizeEngineError(raw);
      for (const leak of leaks) {
        expect(out, `"${raw}" → "${out}" still leaks ${leak}`).not.toMatch(leak);
      }
    }
  });
});
