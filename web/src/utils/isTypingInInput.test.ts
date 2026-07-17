/**
 * isTypingInInput — the shared "is the user typing?" check, and a drift guard
 * that stops the next global listener from rolling its own weaker one.
 *
 * Measured 2026-07-16, before this module existed: of the 15 global keydown
 * listeners, exactly one checked all four cases. Six checked INPUT (some plus
 * SELECT) and missed TEXTAREA and contenteditable, and ToolManager's arrow-key
 * listener checked nothing while calling preventDefault() — so typing in the
 * VCB and pressing ArrowLeft set the axis lock to Z and ate the caret.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isTypingInInput } from './isTypingInInput';

const el = (tag: string, contentEditable = false): EventTarget => {
  const e = document.createElement(tag);
  if (contentEditable) e.setAttribute('contenteditable', 'true');
  return e;
};

describe('isTypingInInput', () => {
  it('is true for every field the user types into', () => {
    expect(isTypingInInput(el('input'))).toBe(true);
    expect(isTypingInInput(el('textarea'))).toBe(true);
    expect(isTypingInInput(el('select'))).toBe(true);
  });

  it('is true for a contenteditable host', () => {
    const div = document.createElement('div');
    // jsdom does not derive isContentEditable from the attribute, so set the
    // property the check actually reads.
    Object.defineProperty(div, 'isContentEditable', { value: true });
    expect(isTypingInInput(div)).toBe(true);
  });

  it('is false for the canvas, plain elements, and null', () => {
    expect(isTypingInInput(el('canvas'))).toBe(false);
    expect(isTypingInInput(el('div'))).toBe(false);
    expect(isTypingInInput(el('button'))).toBe(false);
    expect(isTypingInInput(null)).toBe(false);
  });
});

describe('drift guard — global listeners use the shared check', () => {
  /**
   * Files with a global keydown listener that claims BARE keys, so a focused
   * text field would collide with it. Listeners that only fire on Ctrl/Meta
   * combinations (CommandPalette's hotkey, CommandRegistry's Ctrl+`) or that
   * register on open and unregister on close (the palette's own arrow/Enter
   * handling, StatusBar's unit-menu Escape) are deliberately absent: they
   * cannot collide with typing, and demanding a guard there would be cargo.
   */
  const FILES = [
    'src/main.ts',
    'src/tools/ToolManagerRefactored.ts',
    'src/ui/KeyboardShortcuts.ts',
    'src/ui/StylePanel.ts',
    'src/ui/VCB.ts',
    'src/ui/XiaInspector.ts',
  ];

  const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

  it.each(FILES)('%s guards with isTypingInInput, not its own copy', (file) => {
    const src = read(file);

    // The hand-rolled shapes that were there before, each missing something.
    const homegrown = [
      /\(e\.target as HTMLElement\)\.tagName === 'INPUT'/,
      /e\.target instanceof HTMLInputElement/,
      /e\.target instanceof HTMLSelectElement/,
    ];
    const found = homegrown.filter((re) => re.test(src)).map(String);
    expect(
      found,
      `${file}: rolled its own typing check. Six listeners did this and each ` +
        'missed a case — import isTypingInInput from utils/isTypingInInput.',
    ).toEqual([]);

    // main.ts sits one level up, so its path is './utils/…'.
    expect(src).toMatch(/from '\.\.?\/utils\/isTypingInInput'/);
  });

  it('the guard list is not vacuous', () => {
    // If the import path ever moves, every check above would pass on files
    // that no longer guard anything. Anchor on the real export.
    const ssot = read('src/utils/isTypingInInput.ts');
    expect(ssot).toMatch(/export function isTypingInInput/);
    expect(ssot).toContain("'TEXTAREA'");
    expect(ssot).toContain('isContentEditable');
  });
});
