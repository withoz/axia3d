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
 *
 * ─────────────────────────────────────────────────────────────────────────
 * THE WIRING MAP — what connects to what, and which check holds each link.
 *
 * A click travels six layers. Only TWO of them can break silently; the rest
 * are held by a compiler, which is why the guards cluster where they do.
 *
 *   index.html            data-action / data-tool   ← A STRING. no type.
 *     │  A                                            #menubar 170
 *     │                                                #toolbar 75
 *     │                                                #context-menu 54
 *     │                                                #statusbar 7
 *     ▼
 *   dispatcher            MenuBar / ContextMenu / StatusBar switch,
 *     │  B                and main.ts dispatchToolbarAction (a FOURTH one —
 *     │                   the toolbar does not use the other three)
 *     ▼
 *   ToolManager           dispatchAction · tools registry
 *     │  C                (TypeScript from here down)
 *     ▼
 *   WasmBridge            354 public wrappers
 *     │  D                `this.engine.X?.()`  ← OPTIONAL. no type either.
 *     ▼
 *   WASM export           axia_wasm.d.ts, 834 declarations
 *     │  E
 *     ▼
 *   engine (Rust)         cargo
 *
 *   A  string → handler   THIS FILE (all four containers) +
 *                         e2e/toolbar-items-reach-a-handler.spec.ts (clicks them)
 *   B  container → owner  THIS FILE (`CONTAINERS` below names the owner)
 *   C  id → tool          THIS FILE ('every registered tool is reachable')
 *   D  bridge → export    THIS FILE ('every engine call the bridge makes exists')
 *                         — all THREE naming conventions, see the test, and
 *                         against AxiaEngine's members only (not DeltaBuffers')
 *   E  export → engine    wasm-pack; a missing fn does not compile
 *
 *   Other consumers of the same engine, deliberately NOT in this file:
 *     packages/axia-mcp-server        capability → export   (its own tests)
 *     packages/axia-action-catalog    identity SSOT         (CatalogConsistency)
 *
 * Two links are unguarded ANYWHERE, and both are held by tsc rather than by a
 * test: ToolManager → bridge (C→D) and every call inside a tool. A rename
 * there fails the build, so a guard would only repeat the compiler.
 *
 * ⚠ WHAT NO LINK CHECKS: the ARGUMENTS. Every guard here asks whether a call
 * reaches the engine, never whether the engine accepts what it is handed. The
 * `엣지 챔퍼` menu item passed all of them for as long as it existed and did
 * nothing on every click: its handler called `filletEdge(edge, distance, 1)`
 * and `fillet_edge` opens with `ensure!(segments >= 2)`. The op it wanted,
 * `chamferEdge`, was exported all along and called by the MCP server; only the
 * browser reached past it (2026-09-29). Swept the other engine guards of that
 * shape — `revolve: segments >= 3`, `array_*: count >= 1`, `loft: k >= 3` —
 * and found no sibling: none of them takes its value from a literal at a TS
 * call site. There is no general guard for this; it is measured per op, and
 * the two that hold the chamfer are named in `theChamferMenuItemChamfers`.
 *
 * ⚠ HOW LINK D FAILED (2026-09-29): the bridge names an engine method three
 * ways — `this.engine.X(…)`, `this.engine?.X`, and `(this.engine as …).X` —
 * and this file read only the first. It held 257 of 338 names; the other 81
 * were unguarded, including live ones. Nothing was missing when it was found,
 * so the gap cost nothing this time. The check now walks all three and carries
 * a witness for each, so a convention that stops matching fails instead of
 * quietly shrinking the check.
 *
 * ⚠ HOW THIS MAP FAILED ONCE (2026-08-10): link A was checked for three
 * containers and the toolbar was not one of them, because the toolbar has its
 * own dispatcher and nobody noticed there were four. 75 entry points sat
 * outside every guard in the repo, and the SKETCH PLANE dropdown's
 * "📐 작업 평면 · 세 점" did nothing for however long. `CONTAINERS` now drives
 * the checks, and a fifth container appearing in index.html fails a test
 * instead of quietly joining the blind spot.
 * ─────────────────────────────────────────────────────────────────────────
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

/** Every element in index.html that carries a data-action or a data-tool. */
function allEntryPointOwners(): { containers: Set<string>; loose: string[] } {
  const doc = new DOMParser().parseFromString(read('index.html'), 'text/html');
  const containers = new Set<string>();
  const loose: string[] = [];
  for (const el of doc.querySelectorAll('[data-action], [data-tool]')) {
    // Walk up to the nearest ancestor that has an id — that is the unit a
    // dispatcher owns.
    let node: Element | null = el;
    let owner: string | null = null;
    while (node) {
      if (node.id) {
        owner = node.id;
        break;
      }
      node = node.parentElement;
    }
    if (owner) containers.add(owner);
    else loose.push(el.getAttribute('data-action') ?? el.getAttribute('data-tool') ?? '?');
  }
  return { containers, loose };
}

describe('action wiring — every data-action reaches a handler', () => {
  const MENU = switchCases('src/ui/MenuBar.ts');
  const CONTEXT = switchCases('src/ui/ContextMenu.ts');
  const STATUS = switchCases('src/ui/StatusBar.ts');
  const DISPATCH = dispatchIds('src/tools/ToolManagerRefactored.ts');
  const MAIN = read('src/main.ts');
  const TM_SRC = read('src/tools/ToolManagerRefactored.ts');
  const REGISTERED = new Set(
    [...TM_SRC.matchAll(/tools\.set\('([^']+)'/g)].map((m) => m[1]),
  );

  /**
   * ★THE MAP, AS DATA. Link B of the header diagram: every DOM container that
   * holds entry points, and who dispatches for it. The checks below are driven
   * by this list, so the way the toolbar was forgotten in 2026-08-10 — three
   * hand-written `it()` blocks and a fourth container nobody enumerated —
   * cannot recur silently: `every entry point belongs to a known container`
   * fails the moment a fifth appears.
   */
  const CONTAINERS: Array<{
    id: string;
    owner: string;
    min: number;
    /** ids this container's own dispatcher handles before executeAction. */
    handled: (id: string) => boolean;
  }> = [
    {
      id: 'menubar',
      owner: 'src/ui/MenuBar.ts switch',
      min: 150,
      handled: (id) => MENU.has(id),
    },
    {
      id: 'context-menu',
      owner: 'src/ui/ContextMenu.ts switch',
      min: 20,
      handled: (id) => CONTEXT.has(id),
    },
    {
      id: 'statusbar',
      owner: 'src/ui/StatusBar.ts switch',
      min: 3,
      handled: (id) => STATUS.has(id),
    },
    {
      id: 'toolbar',
      owner: 'src/main.ts dispatchToolbarAction',
      min: 20,
      handled: (id) => {
        if (new Set([...MAIN.matchAll(/action === '([^']+)'/g)].map((m) => m[1])).has(id)) {
          return true;
        }
        // `tool-<x>` activates tool <x>. Requires BOTH the branch and what it
        // does — matching only `action.slice(5)` let `if (false)` pass once.
        const forwards =
          /action\.startsWith\('tool-'\)/.test(MAIN) && /setTool\(bare\)/.test(MAIN);
        return forwards && id.startsWith('tool-') && REGISTERED.has(id.slice(5));
      },
    },
  ];

  it('the dispatchers were actually parsed', () => {
    // Guards the guard: a regex that silently matches nothing would make every
    // assertion below vacuously pass.
    expect(MENU.size).toBeGreaterThan(100);
    expect(CONTEXT.size).toBeGreaterThan(30);
    expect(STATUS.size).toBeGreaterThan(3);
    expect(DISPATCH.size).toBeGreaterThan(50);
    expect(REGISTERED.size).toBeGreaterThan(30);
  });

  it('every entry point belongs to a known container', () => {
    // The check that would have caught the toolbar. An id inside a container
    // nobody listed reaches no dispatcher and no guard — it is invisible twice.
    const { containers, loose } = allEntryPointOwners();
    const known = new Set(CONTAINERS.map((c) => c.id));
    // Nested ids (e.g. #statusbar's own #sb-snap) resolve to themselves, so
    // accept any container that SITS INSIDE a known one.
    const doc = new DOMParser().parseFromString(read('index.html'), 'text/html');
    const orphans = [...containers].filter((cid) => {
      if (known.has(cid)) return false;
      const el = doc.getElementById(cid);
      return ![...known].some((k) => doc.getElementById(k)?.contains(el!));
    });
    expect(
      orphans,
      'Containers holding data-action/data-tool that no dispatcher owns. Add ' +
        'them to CONTAINERS above (with their dispatcher) or nest them inside ' +
        'an existing one.',
    ).toEqual([]);
    expect(loose, 'Entry points with no id anywhere above them').toEqual([]);
  });

  for (const c of CONTAINERS) {
    it(`#${c.id}: every item is handled by ${c.owner} or executeAction`, () => {
      const ids = [...new Set(idsInContainer(c.id))];
      expect(ids.length, `#${c.id} was read at all`).toBeGreaterThan(c.min);
      const dead = ids.filter((id) => !c.handled(id) && !DISPATCH.has(id));
      expect(
        dead,
        `Dead #${c.id} items: clicking does nothing. The palette/Explorer even ` +
          `record them as successful (main.ts dispatchMenuAction returns true on ` +
          `element existence). Handle them in ${c.owner} or in dispatchAction.`,
      ).toEqual([]);
    });
  }

  it('every engine call the bridge makes exists in the WASM export', () => {
    // Link D. `this.engine.X?.()` is optional by design (a legacy build may not
    // have X), which means a typo or a removed export is a SILENT no-op — tsc
    // cannot see it, because the interface above the class declares X optional
    // too. Measured by hand on 2026-08-10 (253 calls, 0 missing); this keeps it
    // measured.
    // `this.engine` is an AxiaEngine, so only AxiaEngine's members count.
    //
    // ⚠ Until 2026-09-30 this set was every name in the .d.ts, from every
    // class. The .d.ts also declares `DeltaBuffers`, whose `getPositions` /
    // `getNormals` / `getIndices` look exactly like engine reads — and a call
    // like `(this.engine as any).getPositions()` passed this check and threw
    // at runtime. That is not hypothetical: the MCP server shipped exactly that
    // call (packages/axia-mcp-server, #234) and its three mesh exports failed on
    // every call for a month. Measured here by mutation before the fix: a cast
    // call to the DeltaBuffers-only `getModifiedFaceIds` passed tsc and this
    // test both.
    const dts = read('src/wasm/axia_wasm.d.ts');
    const classStart = dts.indexOf('export class AxiaEngine {');
    const engineBlock = dts.slice(classStart, dts.indexOf('\n}', classStart));
    const exported = new Set([
      ...[...engineBlock.matchAll(/^ +(\w+)\s*\(/gm)].map((m) => m[1]),
      ...[...engineBlock.matchAll(/^ +readonly\s+(\w+)/gm)].map((m) => m[1]),
    ]);
    // PREMISE: the .d.ts was parsed. Indentation is 4 spaces, not 2 — a `\s{2}`
    // pattern found nothing here and made the whole check vacuous once.
    expect(classStart, 'class AxiaEngine must be found in the .d.ts').toBeGreaterThanOrEqual(0);
    for (const known of ['create_box', 'get_stats', 'drawEllipseAsCurve']) {
      expect(exported, `.d.ts parser must find ${known}`).toContain(known);
    }
    // ...and ONLY AxiaEngine: a DeltaBuffers name leaking in would reopen the
    // hole above.
    expect(exported, 'the set must be AxiaEngine alone').not.toContain('getModifiedFaceIds');
    const bridge = read('src/bridge/WasmBridge.ts');

    // THREE ways the bridge names an engine method, not one.
    //
    // ⚠ This check read only the first of them until 2026-09-29, and so held
    // 257 of the 338 names the bridge uses. The other 81 — `drawOpenSeamOnCurved`,
    // `setAutoIntersectOnDraw`, `beginLiveNurbsEdit`, all four
    // `drawPolylineOn*` among them — sat outside every guard in the repo,
    // which is exactly the silent no-op this test exists to stop. None was
    // missing when the gap was found; the point is that nothing would have
    // said so.
    const conventions: [string, RegExp, string][] = [
      // `this.engine.X(...)` — the call itself.
      ['direct', /this\.engine\.(\w+)\s*[?!]?\.?\s*\(/g, 'drawPointAsShape'],
      // `this.engine?.X` — the presence guard in front of a wrapper. A missing
      // export makes this fall to the wrapper's fallback instead of throwing,
      // which is the same silence by another route.
      ['optional', /this\.engine\s*\?\.\s*(\w+)/g, 'scaleVerts'],
      // `(this.engine as any).X` and `(this.engine as unknown as {…}).X` — the
      // cast, used wherever the interface above the class does not declare the
      // method at all.
      ['cast', /\(\s*this\.engine\s+as\s+[\s\S]*?\)\s*\.\s*(\w+)/g, 'beginLiveExtrude'],
    ];

    const called = new Set<string>();
    for (const [name, re, witness] of conventions) {
      const found = new Set([...bridge.matchAll(re)].map((m) => m[1]));
      // PREMISE, one per convention: a pattern that stops matching makes its
      // share of the check vacuous WITHOUT failing anything, which is how the
      // 4-space indentation above broke the .d.ts parser once.
      expect(found, `the ${name} pattern must still find ${witness}`).toContain(witness);
      for (const f of found) called.add(f);
    }
    expect(called.size, 'all three conventions together').toBeGreaterThan(300);

    const missing = [...called].filter((m) => !exported.has(m));
    expect(
      missing,
      'Bridge methods naming an engine function that is not exported — the ' +
        'optional-call syntax makes these silent no-ops at runtime. Either ' +
        'export them from crates/axia-wasm or delete the wrapper.',
    ).toEqual([]);
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
    // ADR-299 moved the dispatcher out of main.ts so its contract could be
    // tested without booting the app; the allow-list moved with it.
    const dispatcher = read('src/ui/dispatchMenuAction.ts');
    const allowMatch = /CONTEXT_SELECTION_ACTIONS = new Set\(\[([^\]]*)\]\)/.exec(dispatcher);
    expect(
      allowMatch,
      'CONTEXT_SELECTION_ACTIONS not found in src/ui/dispatchMenuAction.ts',
    ).toBeTruthy();
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
