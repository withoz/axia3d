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
  // Renders ActionCatalog at t(action.label) — its 210 catalog strings are
  // covered by CapabilityExplorerPanel.test.ts, which renders the tree in
  // English and looks for Hangul. Only its own chrome is literal.
  { file: 'src/ui/CapabilityExplorerPanel.ts', minLiteralKeys: 3 },  // batch 4
  // batch 3b — the surfaces you touch while modelling
  { file: 'src/ui/VCB.ts', minLiteralKeys: 3 },
  { file: 'src/ui/StatusBar.ts', minLiteralKeys: 5 },
  { file: 'src/ui/XiaInspector.ts', minLiteralKeys: 12 },
  // batch 5 — what the tools say back. Every Korean literal in these is
  // checked, not just the t('literal') calls, so a Toast wrapped but not
  // translated (or not wrapped at all) fails.
  { file: 'src/tools/DrawLineTool.ts', minLiteralKeys: 8 },
  { file: 'src/tools/PushPullTool.ts', minLiteralKeys: 7 },
  { file: 'src/tools/MoveTool.ts', minLiteralKeys: 4 },
  { file: 'src/tools/RotateTool.ts', minLiteralKeys: 6 },
  { file: 'src/tools/ScaleTool.ts', minLiteralKeys: 3 },
  { file: 'src/tools/CopyTool.ts', minLiteralKeys: 3 },
  { file: 'src/tools/GroupTool.ts', minLiteralKeys: 7 },
  { file: 'src/tools/FilletTool.ts', minLiteralKeys: 2 },
  { file: 'src/tools/ChamferTool.ts', minLiteralKeys: 2 },
  { file: 'src/tools/JoinTool.ts', minLiteralKeys: 3 },
  { file: 'src/tools/TrimTool.ts', minLiteralKeys: 3 },
  { file: 'src/tools/ExtendTool.ts', minLiteralKeys: 5 },
  { file: 'src/tools/BoxTool.ts', minLiteralKeys: 4 },
  { file: 'src/tools/RecessTool.ts', minLiteralKeys: 6 },
  { file: 'src/tools/OffsetTool.ts', minLiteralKeys: 2 },
  { file: 'src/tools/EraseTool.ts', minLiteralKeys: 1 },
  { file: 'src/tools/DrawCircleTool.ts', minLiteralKeys: 1 },
  // batch 6 — what a menu or right-click action says back
  { file: 'src/ui/MenuBar.ts', minLiteralKeys: 13 },
  { file: 'src/ui/ContextMenu.ts', minLiteralKeys: 7 },
  { file: 'src/ui/BooleanHandler.ts', minLiteralKeys: 2 },
  { file: 'src/tools/actions/MergeActions.ts', minLiteralKeys: 9 },
  { file: 'src/ui/KeyboardShortcuts.ts', minLiteralKeys: 4 },
  // batch 7 — the action dispatcher. Biggest single file: most of what a menu
  // item or a command-palette entry actually runs ends up reporting here.
  { file: 'src/tools/ToolManagerRefactored.ts', minLiteralKeys: 70 },
];
const MIGRATED_PATHS = MIGRATED_FILES.map((m) => m.file);

/**
 * The source spells `\n`; the string the code passes holds a real newline. So a
 * guard that reads source text has to convert before comparing — the same rule
 * as L-294-12 for `&#9633;` → `□`: **the key is what the runtime holds, not what
 * the source spells.** Without this, every multi-line string looks untranslated
 * AND its entry looks orphaned, from one cause.
 */
function asRuntimeString(sourceLiteral: string): string {
  return sourceLiteral
    .replace(/\\n/g, '\n')
    .replace(/\\t/g, '\t')
    .replace(/\\'/g, "'")
    .replace(/\\\\/g, '\\');
}

/** The inverse — for searching source text for a key that holds real newlines. */
function asSourceLiteral(runtime: string): string {
  return runtime.replace(/\\/g, '\\\\').replace(/\n/g, '\\n').replace(/\t/g, '\\t');
}

const count = (s: string, ch: string) => s.split(ch).length - 1;

/**
 * Drop every `${…}` from a template body, counting braces so a nested object
 * literal doesn't end the match early. A flat /\$\{[^}]*\}/ stops at the first
 * `}`, which cut `${t('{tier} 작업: {label}', { … })}` after `{tier}` and left
 * the Korean looking un-wrapped — a false positive on already-correct code.
 */
function stripInterpolations(v: string): string {
  let out = '';
  for (let i = 0; i < v.length; i++) {
    if (v[i] === '$' && v[i + 1] === '{') {
      let depth = 1;
      i += 2;
      for (; i < v.length && depth > 0; i++) {
        if (v[i] === '{') depth++;
        else if (v[i] === '}') depth--;
      }
      i--;
      continue;
    }
    out += v[i];
  }
  return out;
}

/**
 * Files whose Korean IS a key but which never call t() — the panels translate
 * them at render (batch 4). They are translation sources all the same, so the
 * orphan guard must search them or every catalog entry looks orphaned.
 *
 * The catalogs deliberately do not import t(): @axia/action-catalog is a
 * zero-dependency data package, and reaching into web/src/i18n from it would
 * invert the layering (ADR-294 §3 batch 4).
 */
const TRANSLATION_SOURCES = [
  '../packages/axia-action-catalog/src/catalog.ts',
  'src/commands/AxiaCommands.ts',
];

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

describe('ADR-294 D10 — material names are data, not keys', () => {
  // 사용자 결재: "재질은 사용자가 직접 임의 입력도 가능하기때문에 재질은
  // 사용자 입력에 맞춥니다."
  //
  // These sit right next to panel chrome and look exactly like something a
  // batch should sweep up, so the decision is enforced rather than documented.
  // Measured reasons: MaterialLibrary.addCustom takes any name a user types
  // (Quick Color mints one per use), and FileManager persists getCustom() into
  // metadata.materials and restores by name. A name that round-trips through a
  // file is data — translating it would make a material's name depend on the
  // language it was saved in.
  it('no built-in material name is an en.ts key', () => {
    const src = readFileSync(
      resolve(process.cwd(), 'src/materials/MaterialLibrary.ts'), 'utf8',
    );
    const names = [...src.matchAll(/^\s*name:\s*'([^']*)',/gm)]
      .map((m) => m[1])
      .filter((n) => /[가-힣]/.test(n));
    expect(names.length, 'the built-in materials must actually be there')
      .toBeGreaterThan(10);
    expect(names.filter((n) => n in EN), 'a material name leaked into en.ts')
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
    const ts = [...MIGRATED_PATHS, ...TRANSLATION_SOURCES]
      .map((f) => readFileSync(resolve(process.cwd(), f), 'utf8'));
    const src = [...ts, ...koreanTextNodes(), ...readIndexHtml().attrs].join('\n');
    const missing = Object.keys(EN)
      .filter((k) => !src.includes(k) && !src.includes(asSourceLiteral(k)));
    expect(missing, 'en.ts entries whose Korean no longer exists in the source')
      .toEqual([]);
  });

  /**
   * Korean that is deliberately NOT a translation key, with the reason. Any
   * other Korean literal in a migrated file must have an entry — see below.
   */
  const NOT_KEYS = new Set([
    '면 #$1',                        // a regex REPLACEMENT template, not a string
    '부피 무결성 위반으로 취소됨',    // a matcher for engine output, not output
    '한국어',                        // the language button: names stay in their own language
  ]);

  it.each(MIGRATED_FILES)('every Korean literal in $file has an English entry', ({ file }) => {
    // Stronger than the t()-call guard below, and the reason it exists: that
    // one only sees `t('literal')`, so it is blind to `t(vcbLabels[tool])`,
    // `t(next ? A : B)` and every string in a data table. Measured when this
    // landed: it immediately found 5 user-facing strings in the Capability
    // Explorer that the t()-call guard had passed over.
    const src = readFileSync(resolve(process.cwd(), file), 'utf8');
    const missing: string[] = [];
    let debugDepth = 0;
    for (const line of src.split(/\r?\n/)) {
      const trimmed = line.trim();
      if (trimmed.startsWith('//') || trimmed.startsWith('*') || trimmed.startsWith('/*')) continue;
      // Debug output is not copy — it goes to the console, behind a flag, and
      // is written for whoever is reading the code, not for a user. The call
      // can open on one line and hold its string on the next, so follow the
      // paren depth rather than testing each line in isolation.
      const dbg = /\b(debugLog|debugWarn|console)\s*[.(]/.exec(line);
      if (dbg) {
        const tail = line.slice(dbg.index);
        debugDepth = Math.max(0, count(tail, '(') - count(tail, ')'));
        continue;
      }
      if (debugDepth > 0) {
        debugDepth = Math.max(0, debugDepth + count(line, '(') - count(line, ')'));
        continue;
      }
      for (const m of line.matchAll(/'([^']*)'/g)) {
        const v = asRuntimeString(m[1]);
        if (/[가-힣]/.test(v) && !(v in EN) && !NOT_KEYS.has(v)) missing.push(v);
      }
      // Backticks too. This was a real hole, and an expensive one: the guard
      // saw only single quotes, so `${n}개 면 …` templates sailed straight
      // through — 187 of them, sitting in files this ledger called done. A
      // template holding Korean is either already t(`…`) or it still needs
      // converting to t('…{n}…', { n }).
      for (const m of line.matchAll(/`([^`]*)`/g)) {
        const v = m[1];
        if (!/[가-힣]/.test(v)) continue;
        if (/\bt\(\s*`/.test(line.slice(0, (m.index ?? 0) + 1))) continue;
        // Korean only inside ${…} is a nested t() call, not this template's copy
        if (!/[가-힣]/.test(stripInterpolations(v))) continue;
        missing.push(v);
      }
    }
    expect(missing, `Korean in ${file} with no en.ts entry`).toEqual([]);
  });

  it.each(MIGRATED_FILES)('every t() call in $file has an English entry', ({ file, minLiteralKeys }) => {
    // The other direction: a wrapped string with no translation renders Korean,
    // which is fine — but for a file the batch claims to have DONE, silence is
    // an omission, not a fallback.
    const src = readFileSync(resolve(process.cwd(), file), 'utf8');
    const keys = [...src.matchAll(/\bt\(\s*'((?:[^'\\]|\\.)*)'/g)].map((m) => m[1]);
    expect(keys.length, `${file} must actually be wrapped`).toBeGreaterThanOrEqual(minLiteralKeys);
    const untranslated = keys.map(asRuntimeString).filter((k) => /[가-힣]/.test(k) && !(k in EN));
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
