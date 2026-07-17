/**
 * ADR-294 D8 — translate the STATIC markup in index.html.
 *
 * The α scope count ("1,731 literals across 98 files") measured TS string
 * literals and therefore missed the app's entire chrome: the menu bar, the
 * toolbars and their tooltips live as static markup in index.html — 344 Korean
 * text nodes (306 unique) and 44 Korean title attributes.
 *
 * Source-as-key (D2) pays for itself here: **the DOM already holds the keys.**
 * So this walks the markup once at boot and translates in place. index.html
 * needs no data-i18n attributes, no template compiler, and no rewrite.
 *
 * Two details that matter:
 *
 * - It translates TEXT NODES, not textContent. A menu row is
 *   `<div class="menu-action">새로 만들기<span class="mk">Ctrl+N</span></div>`;
 *   textContent would fuse the label and the shortcut into one unkeyable
 *   string, and writing it back would destroy the span.
 * - It runs BEFORE any panel is constructed, so its scope is exactly the
 *   static markup. Panels build their own DOM from TS, are re-rendered later,
 *   and so must be wrapped with t() in their own batch — a boot-time DOM sweep
 *   would only paint over them until their first re-render.
 */
import { getLocale, t } from './index';

const HANGUL = /[가-힣]/;
const SKIP_TAGS = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEXTAREA']);
const ATTRS = ['title', 'placeholder', 'aria-label'] as const;

export interface DomTranslationResult {
  /** text nodes actually changed */
  texts: number;
  /** attribute values actually changed */
  attrs: number;
  /** Korean strings with no English entry — the honest gap, for the guard */
  untranslated: string[];
}

/**
 * Translate `root`'s static markup into the current locale, in place.
 *
 * A no-op in Korean: the key IS the Korean, so there is nothing to do and no
 * reason to walk the tree.
 */
export function translateDom(root: HTMLElement = document.body): DomTranslationResult {
  const result: DomTranslationResult = { texts: 0, attrs: 0, untranslated: [] };
  if (getLocale() === 'ko') return result;

  const seen = new Set<string>();
  const note = (key: string) => {
    if (!seen.has(key)) { seen.add(key); result.untranslated.push(key); }
  };

  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      SKIP_TAGS.has((node.parentElement?.tagName ?? ''))
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT,
  });

  const textNodes: Text[] = [];
  while (walker.nextNode()) textNodes.push(walker.currentNode as Text);

  for (const node of textNodes) {
    const raw = node.nodeValue ?? '';
    const key = raw.trim();
    if (!key || !HANGUL.test(key)) continue;
    const out = t(key);
    if (out === key) { note(key); continue; }
    // preserve the surrounding whitespace — markup indentation is load-bearing
    // for inline layout in a few rows
    node.nodeValue = raw.replace(key, out);
    result.texts += 1;
  }

  for (const el of root.querySelectorAll<HTMLElement>('[title], [placeholder], [aria-label]')) {
    for (const attr of ATTRS) {
      const key = el.getAttribute(attr)?.trim();
      if (!key || !HANGUL.test(key)) continue;
      const out = t(key);
      if (out === key) { note(key); continue; }
      el.setAttribute(attr, out);
      result.attrs += 1;
    }
  }

  return result;
}
