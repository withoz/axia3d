import { describe, it, expect, beforeEach } from 'vitest';
import { humanizeEngineError } from './humanizeEngineError';
import { setLocale, t } from '../i18n';

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
  // ADR-267 gate on the curved sketch-split — truncated; the real one lists
  // every damaged edge (~3000 chars for a 55-violation cylinder).
  integrityGateCurved:
    '부피 무결성 위반으로 취소됨 (curved sketch): ✗ volume integrity violations:   invariants: 55 violation(s)     - edge EdgeId(25): shared by 3 active faces (non-manifold)     - edge EdgeId(50): shared by 3 active faces (non-manifold)',
} as const;

describe('humanizeEngineError', () => {
  // ADR-294 — these assert Korean, so pin the locale. jsdom reports
  // navigator.language = 'en-US', which the detector honours; without this
  // the suite would silently test the English table instead.
  beforeEach(() => setLocale('ko'));

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

  it('collapses the integrity gate wall-of-EdgeIds into one actionable line', () => {
    const out = humanizeEngineError(RAW.integrityGateCurved);
    expect(out).not.toContain('EdgeId');
    expect(out).not.toContain('non-manifold');
    expect(out.length, 'a Toast is not a console dump').toBeLessThan(80);
    expect(out).toContain('겹칩니다');            // why it was refused
    expect(out).toContain('모델은 그대로');        // the gate rolled back — say so
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
  it('speaks English when the locale is English (ADR-294)', () => {
    setLocale('en');
    const out = humanizeEngineError(RAW.cylinderSidePush);
    expect(out).toContain('draw a circle on it first');
    expect(out, 'no Korean must leak through').not.toMatch(/[가-힣]/);
    setLocale('ko');
  });

  it('an untranslated string falls back to Korean, never to a key name', () => {
    setLocale('en');
    // not in en.ts — the Korean IS the key, so this must render Korean
    expect(t('아직 번역되지 않은 문구')).toBe('아직 번역되지 않은 문구');
    setLocale('ko');
  });
});
