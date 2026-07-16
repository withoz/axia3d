import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { t, setLocale, getLocale } from './index';
import { EN } from './en';

describe('ADR-294 — t()', () => {
  beforeEach(() => setLocale('ko'));
  afterEach(() => setLocale('ko'));

  it('is the identity function in Korean — the key IS the string (D2)', () => {
    // The reason there is no ko.ts, and the reason migration is a wrap.
    expect(t('아무 문구나')).toBe('아무 문구나');
    expect(t('곡면은 직접 밀 수 없습니다 — 곡면 위에 원을 그린 뒤 그 면을 미세요'))
      .toContain('곡면');
  });

  it('translates when the locale is English', () => {
    setLocale('en');
    expect(t('그 면을 찾을 수 없습니다 — 다시 선택해 주세요'))
      .toBe('That face no longer exists — please select it again.');
  });

  it('falls back to Korean for an untranslated key — never a key name (L-294-3)', () => {
    setLocale('en');
    expect(t('아직 번역 안 된 문구')).toBe('아직 번역 안 된 문구');
  });

  it('fills {name} placeholders, in both locales (D3)', () => {
    expect(t('두께 {limit}mm 에서 멈췄습니다', { limit: 1000 }))
      .toBe('두께 1000mm 에서 멈췄습니다');
    // repeated + numeric + string params
    expect(t('{a} 와 {b} 와 {a}', { a: 'x', b: 2 })).toBe('x 와 2 와 x');
  });

  it('leaves an unfilled placeholder alone rather than blanking it', () => {
    // A missing param is a bug in the caller; showing {limit} makes it visible.
    // Blanking it would produce "두께 mm 에서" and hide the mistake.
    expect(t('두께 {limit}mm', {})).toBe('두께 {limit}mm');
  });

  it('resolves the locale at CALL time, not at import (L-294-5)', () => {
    // Why module-scope constants must become getters (D6): if t() bound the
    // locale at module load, this would not switch.
    const key = '그 면을 찾을 수 없습니다 — 다시 선택해 주세요';
    setLocale('ko');
    const ko = t(key);
    setLocale('en');
    const en = t(key);
    expect(ko).not.toBe(en);
    expect(getLocale()).toBe('en');
  });

  it('persists the choice', () => {
    setLocale('en');
    expect(localStorage.getItem('axia:locale')).toBe('en');
  });
});

describe('ADR-294 — en.ts hygiene', () => {
  it('every key is Korean — an English key means someone invented a name', () => {
    // D2: the key is the Korean SOURCE TEXT. A key without Hangul is either a
    // key-name (wrong layer) or an already-English string (nothing to do).
    for (const key of Object.keys(EN)) {
      expect(key, `"${key}" is not Korean source text`).toMatch(/[가-힣]/);
    }
  });

  it('no value is still Korean — that entry would be doing nothing', () => {
    for (const [key, value] of Object.entries(EN)) {
      expect(value, `"${key}" is not actually translated`).not.toMatch(/[가-힣]/);
    }
  });

  it('placeholders survive translation (L-294-4)', () => {
    const ph = /\{(\w+)\}/g;
    for (const [key, value] of Object.entries(EN)) {
      const inKey = [...key.matchAll(ph)].map((m) => m[1]).sort();
      const inValue = [...value.matchAll(ph)].map((m) => m[1]).sort();
      expect(inValue, `"${key}" changed its {placeholders}`).toEqual(inKey);
    }
  });

  it('no orphan: every entry is still referenced in the source', () => {
    // The honest cost of source-as-key (D2): editing the Korean orphans its
    // English silently. This finds the orphan.
    const files = [
      'src/bridge/humanizeEngineError.ts',
    ];
    const src = files
      .map((f) => readFileSync(resolve(process.cwd(), f), 'utf8'))
      .join('\n');
    const missing = Object.keys(EN).filter((k) => !src.includes(k));
    expect(missing, 'en.ts entries whose Korean no longer exists in the source')
      .toEqual([]);
  });

  it('every t() call in a migrated file has an English entry', () => {
    // The other direction: a wrapped string with no translation renders Korean,
    // which is fine — but for a file the batch claims to have DONE, silence is
    // an omission, not a fallback.
    const src = readFileSync(
      resolve(process.cwd(), 'src/bridge/humanizeEngineError.ts'), 'utf8',
    );
    const keys = [...src.matchAll(/\bt\(\s*'((?:[^'\\]|\\.)*)'/g)].map((m) => m[1]);
    expect(keys.length, 'the slice must actually be wrapped').toBeGreaterThan(5);
    const untranslated = keys.filter((k) => /[가-힣]/.test(k) && !(k in EN));
    expect(untranslated, 'wrapped but missing from en.ts').toEqual([]);
  });
});
