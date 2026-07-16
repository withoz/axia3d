import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { toggleShortcutHelp, closeShortcutHelpIfOpen } from './ShortcutHelpModal';
import { setLocale } from '../i18n';

/**
 * ADR-294 batch 3 — the F1 cheat sheet.
 *
 * The regex guard in i18n.test.ts only sees `t('literal')`, and this file
 * translates at render — `t(sec.title)`, `t(r.description)` — so it cannot
 * check the 75 strings in SECTIONS. Rendering the thing and looking for Hangul
 * can, and it does not care which shape the wrapping takes.
 */
describe('ShortcutHelpModal — i18n (ADR-294)', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
    setLocale('ko');
  });

  afterEach(() => {
    closeShortcutHelpIfOpen();
    document.body.innerHTML = '';
    setLocale('ko');
  });

  const open = () => {
    toggleShortcutHelp();
    const el = document.getElementById('shortcut-help-modal');
    expect(el, 'the modal must actually open').not.toBeNull();
    return el!;
  };

  it('renders the sheet in Korean by default', () => {
    const text = open().textContent ?? '';
    expect(text).toContain('키보드 단축키');
    expect(text).toContain('도구');
  });

  it('renders NO Korean at all when the locale is English', () => {
    // The real coverage guard for this file: one assertion over all 75 strings.
    // Miss one and it fails, whichever shape the wrapping takes.
    setLocale('en');
    const modal = open();
    const text = modal.textContent ?? '';
    expect(modal.querySelectorAll('tr').length, 'a sheet with no rows would pass vacuously')
      .toBeGreaterThan(50);
    expect(text).toContain('keyboard shortcuts');
    expect(text).toContain('Cycle tentative snaps');
    expect(text, 'every section title and row must be translated').not.toMatch(/[가-힣]/);
  });

  it('keeps the key column untranslated — Ctrl+Z is a key, not a word', () => {
    setLocale('en');
    const keys = [...open().querySelectorAll('.sh-key kbd')].map((e) => e.textContent);
    expect(keys).toContain('Ctrl+Z');
    expect(keys).toContain('F1');
    // …except the three that describe a gesture rather than name a key
    expect(keys).toContain('Alt + click an edge');
  });
});
