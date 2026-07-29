/**
 * ADR-300 / ADR-301 — no two listeners may claim the same keystroke.
 *
 * ADR-300 fixed bare `O` (Offset tool + Component Panel) and bare `J` (Slice
 * tool + Constraint Panel), and moved the constraint-indicator toggle to
 * Shift+K "which is free". It was not free: the VIEW listener owns t/b/f/k and
 * reaches them with Shift held too, so Shift+K jumped the camera to Back view
 * AND toggled the indicator. The guard written alongside that fix compared
 * main.ts only against TOOL_MAP and SHIFT_TOOL_MAP — the view listener is a
 * third owner living in neither map, so it saw nothing.
 *
 * ADR-301 removes that global binding (the toggle moved into the Constraint
 * Panel) and widens this guard to know about all three owners, and to check
 * bare-letter menu hints as well as Shift+ ones — the stale `J` hint on the
 * Constraints menu item was invisible to the old Shift-only check.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

const MAIN = read('src/main.ts');
const SHORTCUTS = read('src/ui/KeyboardShortcuts.ts');
const HTML = read('index.html');

/** `e.key === 'x'` claims, with the modifier guards in the SAME condition. */
function claims(src: string): Set<string> {
  const found = new Set<string>();
  const re = /e\.key === '([a-zA-Z])'(?:\s*\|\|\s*e\.key === '[a-zA-Z]')?([^{]*)\{/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(src)) !== null) {
    const tail = m[2];
    // `!e.shiftKey` means BARE; an unnegated `e.shiftKey` means Shift.
    const mods = [
      /(?<!!)e\.ctrlKey/.test(tail) && 'Ctrl',
      /(?<!!)e\.altKey/.test(tail) && 'Alt',
      /(?<!!)e\.metaKey/.test(tail) && 'Meta',
      /(?<!!)e\.shiftKey/.test(tail) && 'Shift',
    ]
      .filter(Boolean)
      .join('+');
    found.add((mods ? `${mods}+` : '') + m[1].toUpperCase());
  }
  return found;
}

/** Bare letters in TOOL_MAP → the tool they select. */
function bareTools(): Map<string, string> {
  const out = new Map<string, string>();
  for (const m of SHORTCUTS.matchAll(/'([a-z])': '([a-z-]+)'/g)) {
    out.set(m[1].toUpperCase(), m[2]);
  }
  return out;
}

/** SHIFT_TOOL_MAP → the tool Shift+letter selects. */
function shiftTools(): Map<string, string> {
  const block = /SHIFT_TOOL_MAP[^}]*}/s.exec(SHORTCUTS)?.[0] ?? '';
  const out = new Map<string, string>();
  for (const m of block.matchAll(/'([A-Z])': '([a-z-]+)'/g)) {
    out.set(`Shift+${m[1]}`, m[2]);
  }
  return out;
}

/**
 * The third owner ADR-300 missed: the view listener's `key === 'x') mode = '…'`
 * chain. It runs under `!e.ctrlKey && !e.altKey` with only ONE shift escape —
 * `if (e.shiftKey && isShiftToolKey(key)) return;` — so every view letter that
 * is NOT in SHIFT_TOOL_MAP is claimed in BOTH its bare and its Shift form.
 */
function viewKeys(): Map<string, string> {
  const out = new Map<string, string>();
  const shiftEscapes = new Set([...shiftTools().keys()].map((k) => k.slice('Shift+'.length)));
  for (const m of SHORTCUTS.matchAll(/key === '([a-z])'\)\s*mode = '(\w+)'/g)) {
    const letter = m[1].toUpperCase();
    out.set(letter, `view:${m[2]}`);
    if (!shiftEscapes.has(letter)) out.set(`Shift+${letter}`, `view:${m[2]}`);
  }
  return out;
}

describe('ADR-300/301 — keystroke ownership', () => {
  it('the three keystroke maps are all non-empty (this guard is only as good as its extractors)', () => {
    // An extractor that silently matches nothing turns every check below into a
    // pass. ADR-300's guard did exactly that for view keys — by omission rather
    // than by a broken regex, but the failure mode is the same.
    expect(bareTools().size, 'TOOL_MAP extractor found no bare tool letters').toBeGreaterThan(10);
    expect(shiftTools().size, 'SHIFT_TOOL_MAP extractor found nothing').toBeGreaterThan(0);
    expect(viewKeys().size, 'view-listener extractor found no view keys').toBeGreaterThan(0);
    expect(claims(MAIN).size, 'main.ts claim extractor found nothing').toBeGreaterThan(0);
  });

  it('no keystroke has two owners', () => {
    // One map, every source. This is the check that would have caught Shift+K.
    const owners = new Map<string, string[]>();
    const add = (key: string, owner: string) => {
      owners.set(key, [...(owners.get(key) ?? []), owner]);
    };

    for (const [k, tool] of bareTools()) add(k, `tool:${tool}`);
    for (const [k, tool] of shiftTools()) add(k, `tool:${tool}`);
    for (const [k, mode] of viewKeys()) add(k, mode);
    for (const k of claims(MAIN)) add(k, 'main.ts');

    const contested = [...owners.entries()]
      .filter(([, os]) => os.length > 1)
      .map(([k, os]) => `${k} → ${os.join(' AND ')}`);

    expect(
      contested,
      `these keystrokes fire more than one handler: ${contested.join('; ')}. ` +
        'Bare letters belong to tools and to the view listener (t/b/f/k, which is ' +
        'ALSO reached with Shift). Do not relocate a binding onto a letter without ' +
        'checking all three maps — that is how ADR-300 landed on Shift+K.',
    ).toEqual([]);
  });

  it('no keystroke is claimed twice inside main.ts', () => {
    const counts = new Map<string, number>();
    const re = /e\.key === '([a-zA-Z])'(?:\s*\|\|\s*e\.key === '[a-zA-Z]')?([^{]*)\{/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(MAIN)) !== null) {
      const tail = m[2];
      const mods = [
        /(?<!!)e\.ctrlKey/.test(tail) && 'Ctrl',
        /(?<!!)e\.altKey/.test(tail) && 'Alt',
        /(?<!!)e\.shiftKey/.test(tail) && 'Shift',
      ]
        .filter(Boolean)
        .join('+');
      const key = (mods ? `${mods}+` : '') + m[1].toUpperCase();
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    const dupes = [...counts.entries()].filter(([, n]) => n > 1).map(([k, n]) => `${k} ×${n}`);
    expect(dupes, `two listeners in main.ts fire on the same keystroke: ${dupes.join(', ')}`)
      .toEqual([]);
  });

  it('the help modal advertises one thing per bare key', () => {
    const help = read('src/ui/ShortcutHelpModal.ts');
    const entries = [...help.matchAll(/\{ key: '([^']+)', description: '([^']*)'/g)].map(
      (m) => ({ key: m[1], desc: m[2] }),
    );
    const byKey = new Map<string, string[]>();
    for (const e of entries) {
      if (!/^[A-Z]$/.test(e.key)) continue; // bare single letters only
      byKey.set(e.key, [...(byKey.get(e.key) ?? []), e.desc]);
    }
    const dupes = [...byKey.entries()]
      .filter(([, ds]) => ds.length > 1)
      .map(([k, ds]) => `${k}: ${ds.join(' / ')}`);
    expect(dupes, `the help sheet lists one bare key for two things: ${dupes.join('; ')}`)
      .toEqual([]);
  });

  it('every menu key hint names the key that performs THAT action', () => {
    // The old version only looked at Shift+ hints, so `view-constraints`
    // printing bare `J` — which arms the Slice tool — sailed past it. A hint
    // being *bound to something* is not enough; it must be bound to this item.
    const bare = bareTools();
    const shift = shiftTools();
    const views = viewKeys();
    // Both files, not just main.ts. Widening the hint regex above surfaced
    // `flip-faces prints Shift+N, which nothing binds` — but Shift+N *is*
    // bound, at KeyboardShortcuts.ts:119. The hint was honest and the guard was
    // looking in one of the two places that bind keys.
    const mainClaims = new Set([...claims(MAIN), ...claims(SHORTCUTS)]);

    const wrong: string[] = [];
    let hintCount = 0;
    // Three hint classes, not one. `mk` is the menu bar; `tdi-key` is a toolbar
    // dropdown row and `ctx-key` a context-menu row, and both were invisible
    // here — which is how the Pie dropdown went on printing `I` (the XIA
    // Inspector) after the identical menu-bar hint was removed by the very
    // commit that added this guard. Dropdown rows key off `data-tool`, so match
    // either attribute and normalise to the `tool-x` form the check expects.
    // The gap must be lazy and unbounded in tag count — a dropdown row puts an
    // icon span (with nested svg) and a label span between the attribute and
    // the key — but must stop at the next `data-action`/`data-tool`, so a row
    // WITHOUT a hint cannot steal the next row's. A tighter gap silently
    // matched nothing for those rows, which is how the Pie hint survived being
    // "covered".
    // The row boundary is ANY `data-` attribute, not just action/tool: a
    // context row keyed `data-snap="tempTrack"` carries its own `K` hint, and a
    // narrower stop paired that hint with the `view-3d` row five lines above
    // it — a false accusation, which is as bad as a missed one.
    const re =
      /data-(action|tool)="([^"]+)"(?:(?!data-[a-z-]+=)[\s\S])*?<span class="(?:mk|tdi-key|ctx-key)">([A-Za-z]|Shift\+[A-Za-z])<\/span>/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(HTML)) !== null) {
      hintCount++;
      const action = m[1] === 'tool' ? `tool-${m[2]}` : m[2];
      const raw = m[3];
      const key = raw.startsWith('Shift+')
        ? `Shift+${raw.slice(6).toUpperCase()}`
        : raw.toUpperCase();

      if (action.startsWith('tool-')) {
        const tool = action.slice('tool-'.length);
        const owner = bare.get(key) ?? shift.get(key);
        if (owner !== tool) {
          wrong.push(`${action} prints ${key}, which selects ${owner ?? 'nothing'}`);
        }
      } else if (views.has(key)) {
        // A view letter on a non-view item is the bare-J defect's shape.
        const mode = views.get(key)!.slice('view:'.length);
        if (action !== `view-${mode}`) {
          wrong.push(`${action} prints ${key}, which switches to ${mode} view`);
        }
      } else if (bare.has(key) || shift.has(key)) {
        wrong.push(
          `${action} prints ${key}, which arms the ${bare.get(key) ?? shift.get(key)} tool`,
        );
      } else if (!mainClaims.has(key)) {
        wrong.push(`${action} prints ${key}, which nothing binds`);
      }
    }

    expect(hintCount, 'the menu-hint regex matched nothing — check index.html markup')
      .toBeGreaterThan(15);
    expect(wrong, `menu hints that lie: ${wrong.join('; ')}`).toEqual([]);
  });
});
