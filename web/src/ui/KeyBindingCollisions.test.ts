/**
 * ADR-300 — no two listeners may claim the same keystroke.
 *
 * Bare `O` was claimed by both the Offset tool (KeyboardShortcuts) and the
 * Component Panel (main.ts), and bare `J` by both the Slice tool and the
 * Constraint Panel: a single press did two unrelated things, and
 * ShortcutHelpModal documented `O` twice — once as "Offset", once as
 * "Outliner". The listeners live in different files, so nothing compared them.
 * This reads the sources and does.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

/** Every `<mods>+<letter>` a window keydown listener acts on, per file. */
function claims(src: string): Set<string> {
  const found = new Set<string>();
  // `e.key === 'x'` (optionally `|| e.key === 'X'`) plus the modifier guards
  // that appear in the SAME condition.
  const re = /e\.key === '([a-zA-Z])'(?:\s*\|\|\s*e\.key === '[a-zA-Z]')?([^{]*)\{/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const letter = m[1].toUpperCase();
    const tail = m[2];
    // A guard of `!e.shiftKey` means BARE; `e.shiftKey` (unnegated) means Shift.
    const shift = /(?<!!)e\.shiftKey/.test(tail);
    const ctrl = /(?<!!)e\.ctrlKey/.test(tail);
    const alt = /(?<!!)e\.altKey/.test(tail);
    const meta = /(?<!!)e\.metaKey/.test(tail);
    const mods = [ctrl && 'Ctrl', alt && 'Alt', meta && 'Meta', shift && 'Shift']
      .filter(Boolean)
      .join('+');
    found.add(mods ? `${mods}+${letter}` : letter);
  }
  return found;
}

describe('ADR-300 — keystroke ownership', () => {
  it('main.ts panel keys do not collide with the tool keymap', () => {
    const main = read('src/main.ts');
    const shortcuts = read('src/ui/KeyboardShortcuts.ts');

    // Tool keymaps: bare letters in TOOL_MAP, Shift+letters in SHIFT_TOOL_MAP.
    const bareTools = new Set(
      [...shortcuts.matchAll(/'([a-z])': '[a-z-]+'/g)].map((m) => m[1].toUpperCase()),
    );
    const shiftBlock = /SHIFT_TOOL_MAP[^}]*}/s.exec(shortcuts)?.[0] ?? '';
    const shiftTools = new Set(
      [...shiftBlock.matchAll(/'([A-Z])':/g)].map((m) => `Shift+${m[1]}`),
    );

    const mainClaims = claims(main);
    const collisions = [...mainClaims].filter(
      (k) => (k.length === 1 && bareTools.has(k)) || shiftTools.has(k),
    );
    expect(
      collisions,
      `main.ts claims keystrokes the tool keymap already owns: ${collisions.join(', ')}. ` +
        'Bare letters belong to tools (O = Offset is the CAD convention); move the ' +
        'panel to a modifier instead.',
    ).toEqual([]);
  });

  it('no keystroke is claimed twice inside main.ts', () => {
    const main = read('src/main.ts');
    const seen = new Map<string, number>();
    const re = /e\.key === '([a-zA-Z])'(?:\s*\|\|\s*e\.key === '[a-zA-Z]')?([^{]*)\{/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(main)) !== null) {
      const tail = m[2];
      const mods = [
        /(?<!!)e\.ctrlKey/.test(tail) && 'Ctrl',
        /(?<!!)e\.altKey/.test(tail) && 'Alt',
        /(?<!!)e\.shiftKey/.test(tail) && 'Shift',
      ]
        .filter(Boolean)
        .join('+');
      const key = (mods ? `${mods}+` : '') + m[1].toUpperCase();
      seen.set(key, (seen.get(key) ?? 0) + 1);
    }
    const dupes = [...seen.entries()].filter(([, n]) => n > 1).map(([k, n]) => `${k} ×${n}`);
    expect(
      dupes,
      `two listeners in main.ts fire on the same keystroke: ${dupes.join(', ')}`,
    ).toEqual([]);
  });

  it('the help modal advertises the keys that are actually bound', () => {
    const help = read('src/ui/ShortcutHelpModal.ts');
    const entries = [...help.matchAll(/\{ key: '([^']+)', description: '([^']*)'/g)].map(
      (m) => ({ key: m[1], desc: m[2] }),
    );
    // The specific pair that was wrong: O was listed as both Offset and Outliner.
    const oKeys = entries.filter((e) => e.key === 'O').map((e) => e.desc);
    expect(oKeys.length, `bare O is advertised ${oKeys.length}× : ${oKeys.join(' / ')}`)
      .toBeLessThanOrEqual(1);
    const jKeys = entries.filter((e) => e.key === 'J').map((e) => e.desc);
    expect(jKeys.length, `bare J is advertised ${jKeys.length}× : ${jKeys.join(' / ')}`)
      .toBeLessThanOrEqual(1);
  });

  it('menu key hints name a keystroke that exists', () => {
    const html = read('index.html');
    const main = read('src/main.ts');
    const shortcuts = read('src/ui/KeyboardShortcuts.ts');
    // Tools are bound through map LOOKUPS, not `e.key ===` comparisons, so the
    // extractor above cannot see them — fold both keymaps in.
    const shiftBlock = /SHIFT_TOOL_MAP[^}]*}/s.exec(shortcuts)?.[0] ?? '';
    const shiftTools = [...shiftBlock.matchAll(/'([A-Z])':/g)].map((m) => `Shift+${m[1]}`);
    const bound = new Set([...claims(main), ...claims(shortcuts), ...shiftTools]);
    // Shift+letter hints printed in the menu must be real.
    const hints = [...html.matchAll(/data-action="([^"]+)"[^<]*<span class="mk">(Shift\+[A-Z])</g)];
    const ghosts = hints
      .filter(([, , key]) => !bound.has(key))
      .map(([, action, key]) => `${action} → ${key}`);
    expect(
      ghosts,
      `menu advertises keystrokes nothing binds: ${ghosts.join(', ')}`,
    ).toEqual([]);
  });
});
