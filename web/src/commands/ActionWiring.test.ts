/**
 * Wiring guard — every `data-action` in index.html reaches a handler.
 *
 * CatalogConsistency already proves DOM ⊆ ActionCatalog, but that only says
 * the id is *known*, not that clicking it *does* anything. Those are different
 * questions, and the gap between them is where dead menu items live.
 *
 * A click on `#menubar [data-action="x"]` runs MenuBar's switch. That switch
 * has no `default:`, so an unmatched id is a silent no-op — the menu closes
 * and nothing happens. Worse, the Command Palette and Capability Explorer
 * route through `dispatchMenuAction` (main.ts), which fires a synthetic click
 * and returns `true` because the *element* exists; main.ts then writes
 * `audit.record({ result: 'ok' })`. So a dead id is recorded as a success —
 * exactly what ADR-069 set out to stop, one layer further down.
 *
 * Measured when this landed: 3 dead ids (file-export, file-import, import-ifc).
 *
 * The four dispatchers are matched to their DOM containers, because an id
 * handled by ContextMenu does nothing for a #menubar item.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

/** `case 'x':` / `case 'x': case 'y':` in a dispatcher switch. */
function switchCases(file: string): Set<string> {
  const src = read(file);
  return new Set([...src.matchAll(/^\s*case\s+'([^']+)'\s*:/gm)].map((m) => m[1]));
}

/** `action === 'x'` in ToolManager.dispatchAction. */
function dispatchIds(file: string): Set<string> {
  const src = read(file);
  return new Set([...src.matchAll(/action\s*===\s*'([^']+)'/g)].map((m) => m[1]));
}

/** data-action ids inside a given container element of index.html. */
function idsInContainer(containerId: string): string[] {
  const html = read('index.html');
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const root = doc.getElementById(containerId);
  if (!root) return [];
  return [...root.querySelectorAll('[data-action]')]
    .map((el) => el.getAttribute('data-action')!)
    .filter(Boolean);
}

describe('action wiring — every data-action reaches a handler', () => {
  const MENU = switchCases('src/ui/MenuBar.ts');
  const CONTEXT = switchCases('src/ui/ContextMenu.ts');
  const STATUS = switchCases('src/ui/StatusBar.ts');
  const DISPATCH = dispatchIds('src/tools/ToolManagerRefactored.ts');

  it('the dispatchers were actually parsed', () => {
    // Guards the guard: a regex that silently matches nothing would make every
    // assertion below vacuously pass.
    expect(MENU.size).toBeGreaterThan(100);
    expect(CONTEXT.size).toBeGreaterThan(30);
    expect(STATUS.size).toBeGreaterThan(3);
    expect(DISPATCH.size).toBeGreaterThan(50);
  });

  it('#menubar: every item is handled by MenuBar or executeAction', () => {
    const ids = idsInContainer('menubar');
    expect(ids.length).toBeGreaterThan(150);
    const dead = [...new Set(ids)].filter((id) => !MENU.has(id) && !DISPATCH.has(id));
    expect(
      dead,
      'Dead menu items: clicking does nothing, and the palette/Explorer record ' +
        'them as successful (main.ts dispatchMenuAction returns true on element ' +
        'existence). Add a `case` in MenuBar.ts or a branch in dispatchAction.',
    ).toEqual([]);
  });

  it('#context-menu: every item is handled by ContextMenu or executeAction', () => {
    const ids = idsInContainer('context-menu');
    expect(ids.length).toBeGreaterThan(20);
    const dead = [...new Set(ids)].filter((id) => !CONTEXT.has(id) && !DISPATCH.has(id));
    expect(dead, 'Dead context-menu items').toEqual([]);
  });

  it('#statusbar: every item is handled by StatusBar or executeAction', () => {
    const ids = idsInContainer('statusbar');
    expect(ids.length).toBeGreaterThan(3);
    const dead = [...new Set(ids)].filter((id) => !STATUS.has(id) && !DISPATCH.has(id));
    expect(dead, 'Dead statusbar items').toEqual([]);
  });

  it('every registered tool has a display name', () => {
    // toolDisplayName falls back to the raw id, so a missing entry does not
    // throw — it just shows the user "nurbs-edit" where a name belongs. That
    // is the drift this module was created to end (메타-원칙 #4); without a
    // guard it re-opens the moment someone registers a tool.
    const tm = read('src/tools/ToolManagerRefactored.ts');
    const registered = [...tm.matchAll(/tools\.set\('([^']+)'/g)].map((m) => m[1]);
    expect(registered.length).toBeGreaterThan(30);

    const disp = read('src/ui/toolDisplayNames.ts');
    const block = disp.slice(
      disp.indexOf('TOOL_DISPLAY_NAMES'),
      disp.indexOf('VIEW_DISPLAY_NAMES'),
    );
    const named = new Set([...block.matchAll(/^\s+'?([\w-]+)'?:/gm)].map((m) => m[1]));

    const unnamed = registered.filter((id) => !named.has(id));
    expect(
      unnamed,
      'Tools with no entry in TOOL_DISPLAY_NAMES — the status bar will show ' +
        'the raw tool id instead of a name.',
    ).toEqual([]);
  });

  it('every registered tool is reachable from the UI', () => {
    // The mirror of the test below. That one catches a catalog entry with no
    // handler; this catches a handler nobody can invoke. BoundaryTool
    // (ADR-148 β-4) shipped with an engine op, a WASM export, a bridge wrapper,
    // a Ctrl+B binding and tests — and no menu item, no toolbar button and no
    // catalog entry. It worked; you just had to already know it was there.
    const tm = read('src/tools/ToolManagerRefactored.ts');
    const registered = [...tm.matchAll(/tools\.set\('([^']+)'/g)].map((m) => m[1]);
    expect(registered.length).toBeGreaterThan(30);

    const html = read('index.html');
    const domActions = new Set([...html.matchAll(/data-action="([^"]+)"/g)].map((m) => m[1]));
    const domTools = new Set([...html.matchAll(/data-tool="([^"]+)"/g)].map((m) => m[1]));
    const cmds = read('src/commands/AxiaCommands.ts');
    const catalogTools = new Set(
      [...cmds.matchAll(/tool\('[^']+'\s*,\s*'([^']+)'/g)].map((m) => m[1]),
    );
    const ks = read('src/ui/KeyboardShortcuts.ts');
    // Both shapes: the keyMap/shiftMap tables, and direct setTool('x') calls
    // like Ctrl+B — my first sweep only read the tables and reported boundary
    // as unreachable when the key had worked all along.
    const keyTools = new Set([
      ...[...ks.matchAll(/'[A-Za-z0-9]':\s*'([a-z0-9-]+)'/g)].map((m) => m[1]),
      ...[...ks.matchAll(/setTool\('([a-z0-9-]+)'\)/g)].map((m) => m[1]),
    ]);

    const unreachable = registered.filter(
      (id) =>
        !domTools.has(id) &&
        !domActions.has(`tool-${id}`) &&
        !catalogTools.has(id) &&
        !keyTools.has(id),
    );
    expect(
      unreachable,
      'Registered tools with no menu item, toolbar button, catalog entry or ' +
        'key binding — the code runs, the user cannot get to it.',
    ).toEqual([]);
  });

  it('no palette command falls through to "unknown command"', () => {
    // The palette runs dispatchMenuAction(id) and falls back to
    // executeAction(id). dispatchMenuAction searches #menubar and #statusbar,
    // plus the three context-menu ids on its allowlist. Anything else that is
    // in the catalog but in none of those places is offered, searchable, and
    // dead on execute — the same failure as the three ghost commands, arrived
    // at from the other side.
    //
    // Measured when this landed: group-edit / group-hide / group-lock (fixed
    // by the allowlist — they read the selection, which survives opening the
    // palette) and snap-override (removed from CommandCatalog: hover-only).
    const cmds = read('src/commands/AxiaCommands.ts');
    const actionIds = [...cmds.matchAll(/\baction\('([^']+)'/g)].map((m) => m[1]);
    expect(actionIds.length).toBeGreaterThan(100);
    // action(..., deps, () => …) runs its own closure and needs no dispatch.
    const custom = new Set(
      [...cmds.matchAll(/action\('([^']+)'[^\n]*?,\s*\(\)\s*=>/g)].map((m) => m[1]),
    );

    const menubar = idsInContainer('menubar');
    const statusbar = idsInContainer('statusbar');
    const main = read('src/main.ts');
    const allowMatch = /CONTEXT_SELECTION_ACTIONS = new Set\(\[([^\]]*)\]\)/.exec(main);
    expect(allowMatch, 'CONTEXT_SELECTION_ACTIONS not found in main.ts').toBeTruthy();
    const allowed = [...allowMatch![1].matchAll(/'([^']+)'/g)].map((m) => m[1]);
    expect(allowed.length).toBeGreaterThan(0);

    const reachable = new Set([...menubar, ...statusbar, ...allowed, ...DISPATCH]);
    const stranded = actionIds.filter((id) => !custom.has(id) && !reachable.has(id));
    expect(
      stranded,
      'Catalog commands the palette cannot execute — they will report ' +
        '"unknown command". Wire the id, add it to CONTEXT_SELECTION_ACTIONS ' +
        '(only if it reads the selection, not the right-click position), or ' +
        'drop it from CommandCatalog.',
    ).toEqual([]);
  });

  it('no catalog command points at a handler that no longer exists', () => {
    // The palette lists what the catalog holds. view-shadow-pro and the two
    // solar-heatmap ids outlived their MenuBar cases (deleted 2026-05-16) and
    // sat in the catalog for two months: searchable, and "unknown command" on
    // execute. Anything reachable from the palette must land somewhere.
    const src = read('src/commands/AxiaCommands.ts');
    const actionIds = [...src.matchAll(/\baction\('([^']+)'/g)].map((m) => m[1]);
    expect(actionIds.length).toBeGreaterThan(100);

    const html = read('index.html');
    const domIds = new Set(
      [...html.matchAll(/data-action="([^"]+)"/g)].map((m) => m[1]),
    );
    const stranded = actionIds.filter(
      (id) => !domIds.has(id) && !DISPATCH.has(id) && !MENU.has(id),
    );
    expect(
      stranded,
      'Catalog commands with no handler anywhere — the palette offers a feature ' +
        'that does not exist. Remove the entry, or wire it.',
    ).toEqual([]);
  });
});
