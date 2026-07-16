import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
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

  it('honours an EMPTY translation instead of falling back (L-294-10)', () => {
    // A sentence split across DOM nodes reorders in English:
    //   재질을 부여하면 이 객체는 <strong>XIA</strong>로 승격됩니다
    //   Assigning a material promotes this object to <strong>XIA</strong>
    // The trailing fragment has no English counterpart, so '' is the correct
    // translation — and `(table[key] || key)` fell back to Korean, rendering
    // the sentence half-translated.
    setLocale('en');
    expect(t('로 승격됩니다')).toBe('');
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
    // A t() evaluated per render follows setLocale. (Module-scope constants do
    // not — see the D6 block below for what that does and does not cost.)
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

describe('ADR-294 D6 — module-scope t() under reload semantics', () => {
  afterEach(() => {
    localStorage.removeItem('axia:locale');
    vi.resetModules();
  });

  // D6 as first drafted said module-scope constants "must become getters".
  // That would force every command/menu catalog to be restructured — the
  // expensive half of the bulk migration. These two measure whether it is
  // actually true, rather than asserting it from the spec.
  //
  // It is not, GIVEN reload-on-switch: ES modules evaluate depth-first, so
  // i18n/index.ts's body (and its detect()) finishes before any importing
  // module's body starts. A module-scope t() therefore already sees the
  // persisted locale.
  //
  // The Korean case is the load-bearing one: jsdom reports
  // navigator.language = 'en-US', so only 'ko' proves the persisted choice
  // beat the browser default rather than coinciding with it.
  it('a module-scope t() sees the persisted locale at import — ko over an en browser', async () => {
    localStorage.setItem('axia:locale', 'ko');
    vi.resetModules();
    const mod = await import('./__fixtures__/moduleScope');
    expect(mod.LABEL).toBe('그 면을 찾을 수 없습니다 — 다시 선택해 주세요');
  });

  it('…and English when that is what is persisted', async () => {
    localStorage.setItem('axia:locale', 'en');
    vi.resetModules();
    const mod = await import('./__fixtures__/moduleScope');
    expect(mod.LABEL).toBe('That face no longer exists — please select it again.');
  });
});

/**
 * Files a batch has claimed (ADR-294 §3). This list is the batch ledger: both
 * guards below read it, so adding a file here without translating it fails,
 * and translating one without listing it leaves its entries looking orphaned.
 */
const MIGRATED_FILES: { file: string; minLiteralKeys: number }[] = [
  { file: 'src/bridge/humanizeEngineError.ts', minLiteralKeys: 6 },  // batch 1
  { file: 'src/units/SettingsPanel.ts', minLiteralKeys: 25 },        // batch 3
  // Translates at render — t(sec.title) / t(r.description) — so only the two
  // modal-chrome strings are literals. Its 75 SECTIONS strings are covered by
  // ShortcutHelpModal.test.ts, which renders the sheet and looks for Hangul.
  { file: 'src/ui/ShortcutHelpModal.ts', minLiteralKeys: 2 },        // batch 3
];
const MIGRATED_PATHS = MIGRATED_FILES.map((m) => m.file);

/**
 * index.html holds the app's chrome as static markup, so it is a translation
 * source like any .ts file. It must be PARSED, not read as text: the markup
 * writes `&#9633; 직사각형` where the DOM — and therefore the key — has
 * '□ 직사각형'. Parsing also mirrors what translateDom actually walks.
 */
function readIndexHtml(): { attrs: string[] } {
  const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8');
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const attrs: string[] = [];
  for (const el of doc.querySelectorAll('[title], [placeholder], [aria-label]')) {
    for (const a of ['title', 'placeholder', 'aria-label']) {
      const v = el.getAttribute(a);
      if (v?.trim()) attrs.push(v.trim());
    }
  }
  return { attrs };
}

/** Korean text nodes exactly as translateDom will see them. */
function koreanTextNodes(): string[] {
  const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8');
  const doc = new DOMParser().parseFromString(html, 'text/html');
  // same exclusion translateDom makes — CSS/JS text is not user-facing copy,
  // and their Korean comments would otherwise be mistaken for chrome
  doc.querySelectorAll('script, style, noscript, textarea').forEach((el) => el.remove());
  const out: string[] = [];
  const walk = (node: Node) => {
    if (node.nodeType === 3) {
      const v = (node.nodeValue ?? '').trim();
      if (v && /[가-힣]/.test(v)) out.push(v);
      return;
    }
    node.childNodes.forEach(walk);
  };
  walk(doc.body);
  return out;
}

describe('ADR-294 D8 — the static chrome is fully translated', () => {
  // The drift guard for batch 2: add Korean markup to index.html without an
  // en.ts entry and this fails. Without it, new chrome would silently render
  // Korean inside an otherwise-English UI — the exact mixed state the design
  // is trying to avoid. Measured at the time of writing: 344 Korean text
  // nodes (306 unique) and 44 Korean tooltips.
  it('every Korean text node in index.html has an English entry', () => {
    const nodes = koreanTextNodes();
    expect(nodes.length, 'the chrome must actually be there — a parse failure would pass vacuously')
      .toBeGreaterThan(300);
    const missing = [...new Set(nodes)].filter((k) => !(k in EN));
    expect(missing, 'Korean markup with no en.ts entry').toEqual([]);
  });

  it('every Korean title/placeholder/aria-label has an English entry', () => {
    const ko = [...new Set(readIndexHtml().attrs.filter((a) => /[가-힣]/.test(a)))];
    expect(ko.length, 'the tooltips must actually be there').toBeGreaterThan(40);
    expect(ko.filter((k) => !(k in EN)), 'Korean tooltip with no en.ts entry')
      .toEqual([]);
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
    const ts = MIGRATED_PATHS.map((f) => readFileSync(resolve(process.cwd(), f), 'utf8'));
    const src = [...ts, ...koreanTextNodes(), ...readIndexHtml().attrs].join('\n');
    const missing = Object.keys(EN).filter((k) => !src.includes(k));
    expect(missing, 'en.ts entries whose Korean no longer exists in the source')
      .toEqual([]);
  });

  it.each(MIGRATED_FILES)('every t() call in $file has an English entry', ({ file, minLiteralKeys }) => {
    // The other direction: a wrapped string with no translation renders Korean,
    // which is fine — but for a file the batch claims to have DONE, silence is
    // an omission, not a fallback.
    const src = readFileSync(resolve(process.cwd(), file), 'utf8');
    const keys = [...src.matchAll(/\bt\(\s*'((?:[^'\\]|\\.)*)'/g)].map((m) => m[1]);
    expect(keys.length, `${file} must actually be wrapped`).toBeGreaterThanOrEqual(minLiteralKeys);
    const untranslated = keys.filter((k) => /[가-힣]/.test(k) && !(k in EN));
    expect(untranslated, `wrapped in ${file} but missing from en.ts`).toEqual([]);
  });

  it('no migrated file still has a bare Korean literal in its markup', () => {
    // The gap the t()-call guard cannot see: a string that was never wrapped at
    // all. Scoped to innerHTML/textContent assignments, which is where a panel's
    // copy lives — code-level Korean (comments, console) is out of scope.
    for (const { file } of MIGRATED_FILES) {
      const src = readFileSync(resolve(process.cwd(), file), 'utf8');
      for (const m of src.matchAll(/(innerHTML|textContent)\s*=\s*`([\s\S]*?)`/g)) {
        // strip every ${...} — a wrapped string lives inside one
        const bare = m[2].replace(/\$\{[\s\S]*?\}/g, '');
        expect(/[가-힣]/.test(bare) ? `${file}: ${bare.match(/[^\n]*[가-힣][^\n]*/)?.[0]?.trim()}` : '',
          'unwrapped Korean in a template the batch claims to have done').toBe('');
      }
    }
  });
});
