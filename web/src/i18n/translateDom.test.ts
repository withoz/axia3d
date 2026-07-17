import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { translateDom } from './translateDom';
import { setLocale } from './index';

/**
 * ADR-294 D8 — the boot pass over index.html's static chrome.
 *
 * Fixtures use real keys from en.ts, so these break if the entry is renamed
 * away — invented keys would test the walker against nothing.
 */
describe('translateDom', () => {
  let root: HTMLElement;

  beforeEach(() => {
    root = document.createElement('div');
    document.body.appendChild(root);
    setLocale('en');
  });

  afterEach(() => {
    root.remove();
    setLocale('ko');
  });

  it('is a no-op in Korean — the key IS the Korean, so there is nothing to do', () => {
    setLocale('ko');
    root.innerHTML = '<div>새로 만들기</div>';
    const r = translateDom(root);
    expect(r).toEqual({ texts: 0, attrs: 0, untranslated: [] });
    expect(root.textContent).toBe('새로 만들기');
  });

  it('translates a text node', () => {
    root.innerHTML = '<div class="menu-action">새로 만들기</div>';
    expect(translateDom(root).texts).toBe(1);
    expect(root.textContent).toBe('New');
  });

  it('leaves the shortcut span alone — it translates text NODES, not textContent', () => {
    // The real menu row shape. Writing textContent back would fuse the label
    // and the shortcut into one unkeyable string AND destroy the span.
    root.innerHTML = '<div class="menu-action">새로 만들기<span class="mk">Ctrl+N</span></div>';
    translateDom(root);
    expect(root.querySelector('.mk')?.textContent, 'the span must survive').toBe('Ctrl+N');
    expect(root.firstElementChild?.firstChild?.nodeValue).toBe('New');
  });

  it('translates title / placeholder / aria-label', () => {
    root.innerHTML =
      '<button title="메뉴"></button><input placeholder="이름 입력..." />';
    expect(translateDom(root).attrs).toBe(2);
    expect(root.querySelector('button')?.getAttribute('title')).toBe('Menu');
    expect(root.querySelector('input')?.getAttribute('placeholder')).toBe('Enter a name…');
  });

  it('reports Korean with no entry rather than silently skipping it', () => {
    root.innerHTML = '<div>이건 번역이 없는 새 문구</div>';
    const r = translateDom(root);
    expect(r.texts).toBe(0);
    expect(r.untranslated).toEqual(['이건 번역이 없는 새 문구']);
    expect(root.textContent, 'and it still renders — Korean, not blank')
      .toBe('이건 번역이 없는 새 문구');
  });

  it('does not touch script, style or textarea text', () => {
    // The fixtures are artificial on purpose, and worth explaining. A real
    // <style> body is ONE text node — '/* 드롭다운 메뉴 */ .x { color: red }'
    // — which never equals a key, so it would survive even without the filter;
    // a test using one would pass either way and prove nothing. Making the
    // body exactly a key is what makes the filter observable.
    //
    // The rule it pins is real: <textarea> holds USER TEXT, and translating
    // that would rewrite what someone typed.
    root.innerHTML = '<style>저장</style><script>저장</script><textarea>저장</textarea>';
    expect(translateDom(root).texts).toBe(0);
    expect(root.querySelector('style')?.textContent).toBe('저장');
    expect(root.querySelector('script')?.textContent).toBe('저장');
    expect(root.querySelector('textarea')?.textContent, 'user text is not copy').toBe('저장');
  });

  it('keeps the whitespace around a label', () => {
    // Markup indents its rows; collapsing that changes inline layout.
    root.innerHTML = '<div>\n      저장\n    </div>';
    translateDom(root);
    expect(root.firstElementChild?.firstChild?.nodeValue).toBe('\n      Save\n    ');
  });

  it('renders a split sentence as one English sentence', () => {
    // 재질을 부여하면 이 객체는 <strong>XIA (특성)</strong>로 승격됩니다
    // English reorders, so the trailing fragment translates to ''.
    root.innerHTML =
      '<div>재질을 부여하면 이 객체는 <strong>XIA (특성)</strong>로 승격됩니다</div>';
    translateDom(root);
    expect(root.textContent?.replace(/\s+/g, ' ').trim())
      .toBe('Assigning a material promotes this object to XIA (property)');
  });

  it('does not re-translate on a second pass', () => {
    root.innerHTML = '<div>저장</div>';
    expect(translateDom(root).texts).toBe(1);
    const second = translateDom(root);
    expect(second.texts, 'English has no Hangul, so the walker skips it').toBe(0);
    expect(second.untranslated).toEqual([]);
    expect(root.textContent).toBe('Save');
  });
});
