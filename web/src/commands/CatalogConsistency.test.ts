/**
 * ADR-133 — Dual Catalog Unification Invariant Test (Path E adapter layer).
 *
 * ADR-132 §A1.2 dual catalog architectural finding 의 implementation guard.
 * ADR-045 D1 SSOT invariant 실측 회복 — ActionCatalog 가 *모든 user-facing IDs* 의
 * identity SSOT.
 *
 * **Invariant** (canonical, single direction):
 *
 *   For every command registered in CommandCatalog (via `registerAxiaCommands`),
 *   the canonical id MUST exist in ActionCatalog.
 *
 * **Direction note**: AC ⊇ CC (ActionCatalog superset). 13 AC-only entries
 * (`attach-surface-*-validated`, `bool-dispatch`, `cache-stats`, etc.) are
 * Capability-Explorer / diagnostic entries — not registered in CommandCatalog.
 * This is OK. (They were described as "MCP/diagnostic-only" until 2026-09-30;
 * the MCP server serves none of them — see the surface-claim checks at the
 * bottom of this file.)
 *
 * **What this catches**:
 *   1. New CommandCatalog entry added without ActionCatalog counterpart →
 *      CI fails (caller must add AC entry first)
 *   2. ActionCatalog ID renamed/removed but CommandCatalog still uses old id →
 *      CI fails (drift signal)
 *
 * **What this does NOT catch**:
 *   - Label/description drift between AC and CC (separate field-level test
 *     would need to compare AC.label vs CC.label per shared id — deferred to
 *     ADR-134+ if needed)
 *   - Shortcut/tier metadata drift
 *
 * Cross-link:
 *   - ADR-133 (본 ADR — Path E adapter layer implementation)
 *   - ADR-132 §A1.2 (dual catalog finding)
 *   - ADR-045 D1 (ActionCatalog SSOT spec)
 *   - ADR-131 (CommandPalette already exists, dual catalog discovery)
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { getCommandCatalog, __resetCommandCatalog, type CommandDef } from './CommandCatalog';
import { registerAxiaCommands } from './AxiaCommands';
import { ALL_ACTIONS, getActionById, lookup, type ActionDef } from '@axia/action-catalog';

describe('ADR-133 — Dual catalog unification invariant', () => {
  beforeEach(() => {
    __resetCommandCatalog();
  });

  it('every CommandCatalog id exists in ActionCatalog', () => {
    // Minimal ToolManager stub — registerAxiaCommands needs `.setTool()` +
    // `.executeAction()` on the deps.toolManager arg. We don't actually
    // invoke any commands; just register their metadata.
    const toolManager = {
      setTool: () => {},
      executeAction: () => {},
      _currentTool: '',
    } as unknown as Parameters<typeof registerAxiaCommands>[0]['toolManager'];

    registerAxiaCommands({ toolManager });

    const ccCommands: CommandDef[] = getCommandCatalog().list();
    const missing: string[] = [];

    for (const cmd of ccCommands) {
      const ac: ActionDef | undefined = getActionById(cmd.id);
      if (!ac) {
        missing.push(cmd.id);
      }
    }

    expect(
      missing,
      `${missing.length} CommandCatalog id(s) missing from ActionCatalog:\n` +
        missing.map((id) => `  - ${id}`).join('\n') +
        `\n\nFix: add ActionDef entries to packages/axia-action-catalog/src/catalog.ts\n` +
        `(see ADR-133 § L-133-2 for new entries pattern).\n`,
    ).toEqual([]);
  });

  it('CommandCatalog count matches expected total (191, after -4 ghosts, +2 surfaced tools, -1 hover-only, +2 plane moves, +1 dimension labels)', () => {
    const toolManager = {
      setTool: () => {},
      executeAction: () => {},
      _currentTool: '',
    } as unknown as Parameters<typeof registerAxiaCommands>[0]['toolManager'];

    registerAxiaCommands({ toolManager });

    const count = getCommandCatalog().size();
    // ADR-132 §2.3 measured 148; ADR-206~219 added 14 tools; ADR-220 added
    // sweep + loft → 164; ADR-221 added hole + window → 166; ADR-224 added
    // plane + wall + nurbs → 169; ADR-225 added rotrect + pie + spline
    // (draw-tool drift sweep) → 172; ADR-233 added nurbs-edit → 173;
    // ADR-247 added loft-selected-faces → 174; ADR-248 added revolve-face-solid
    // → 175; ADR-249 P5 added tool-polygon-hole → 176; recess UI added
    // tool-recess (3D pocket) → 177. Cmd-K palette coverage batch added the
    // 9 view/diagnostic panel toggles + 3 imports (skp/step/iges) +
    // resynthesize-faces = +13 → 190. The matching ActionCatalog entries are
    // kept in sync (AC ⊇ CC, ADR-133 L-133-3 / CatalogConsistency).
    //
    // 187: the wiring audit removed view-shadow-pro / solar-heatmap /
    // solar-heatmap-off. Their MenuBar handlers were deleted on 2026-05-16
    // (shadow → ADR-106) but the catalog entries stayed, so the palette
    // listed three features that no longer exist — searching found them,
    // running them said "unknown command".
    //
    // 188: the mirror image of those three — tool-boundary (ADR-148 β-4) had
    // a handler, a bridge, an engine op and a Ctrl+B binding, and no catalog
    // entry, so the palette could not offer a feature that DID exist.
    //
    // 187: snap-override left. It is a ctx-submenu-trigger whose handler is
    // `return; // hover로 처리, 클릭 무시` and whose real choices are
    // `data-snap` items — the palette could only ever fire a silent no-op. It
    // keeps its ActionCatalog entry (a right-click item has an identity); it
    // just has nothing a dispatch surface can call. AC ⊋ CC is fine — that
    // invariant only runs one way.
    //
    // 188: export-ifc (ADR-203 β-1.5) — the first working DCEL→interchange
    // export (IFC4.3 IfcFacetedBrep), wired to the export menu + palette, with
    // a matching ActionCatalog entry.
    //
    // 187: view-sun-panel left (ADR-299). Unlike the three ghosts above it
    // never had an implementation at all — no SunPanel class exists anywhere
    // in the repo. Its handler read a global nothing assigns and swallowed
    // the miss with `sp?.toggle()`, while the menu advertised a Shift+U hint
    // bound to nothing. Removed from menu, palette and catalog together.
    //
    // 188: tool-split added (ADR-308). SplitTool was registered and bound to
    // bare X and listed in the shortcut sheet, but appeared in no menu and no
    // catalog — of 57 registered tools it was the only one that could not be
    // *browsed* to. Adding the menu item alone would have made it a DEAD menu
    // (the ADR-299 class); the MenuBar `case` went in with it.
    //
    // 190: sketch-offset + sketch-tilt (2026-08-05). They went into the menu
    // and the ActionCatalog when they landed and NOT into the palette, which
    // the AC ⊇ CC invariant permits, so nothing complained. The convention is
    // the other way: every other sketch action is in the palette
    // (sketch-start-face, sketch-align-up…), so leaving these out made them
    // the only two that could not be searched for.
    //
    // A third went in with them and came straight back out: `tool-workplane`
    // was a DUPLICATE of `tool-plane` (ADR-224's 3-point work plane), built
    // because the audit grep asked for a name the repo does not use. Two
    // near-identical entries in the palette is how a user finds out the
    // codebase disagrees with itself.
    // 191: view-dimensions (2026-08-25). DimensionManager.setVisible() was
    // implemented and tested with no way to reach it — the context menu's
    // '치수 표시 ON/OFF' toggles the SELECTION readout, a different thing.
    // Added to the View menu beside view-grid / view-axis.
    expect(count).toBe(191);
  });

  // Bottom-bar UX audit — DOM ⊆ ActionCatalog guard. Every data-action id
  // wired in index.html (menubar / context menu / F-keys) must resolve in the
  // ActionCatalog identity SSOT (canonical id OR legacy alias). This catches
  // future DOM-only ids that would be undiscoverable in the Capability
  // Explorer (the CC ⊆ AC test above only covers CommandCatalog, never the DOM).
  it('every index.html data-action resolves in ActionCatalog (DOM ⊆ AC)', () => {
    // vitest runs with cwd = web/ (config lives there); index.html is at web/index.html.
    const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8');
    const ids = new Set<string>();
    for (const m of html.matchAll(/data-action="([^"]+)"/g)) ids.add(m[1]);
    expect(ids.size).toBeGreaterThan(150); // sanity: index.html was actually read

    const unresolved = [...ids].filter((id) => lookup(id).kind === 'not-found');
    expect(
      unresolved,
      `${unresolved.length} index.html data-action id(s) missing from ActionCatalog:\n` +
        unresolved.map((id) => `  - ${id}`).join('\n') +
        `\n\nFix: add an ActionDef (or a legacy alias on the canonical entry) in\n` +
        `packages/axia-action-catalog/src/catalog.ts, then rebuild the package\n` +
        `(cd packages/axia-action-catalog && npm run build).\n`,
    ).toEqual([]);
  });

  it('ActionCatalog count is at least 161 (82 shared + 13 AC-only + 66 ADR-133 added)', () => {
    // Sanity check — ADR-133 added 66 entries to ActionCatalog → total 161.
    // Tighter equality is enforced by `catalog.test.ts` in the package.
    // Here we just guard against accidental regression (someone removes
    // ADR-133 entries without removing the matching CommandCatalog entries).
    const { CATALOG_SIZE } = require('@axia/action-catalog');
    expect(CATALOG_SIZE).toBeGreaterThanOrEqual(161);
  });
});

/**
 * The catalog says, per action, which surfaces expose it. Nothing checked that
 * against the surfaces themselves. Measured by the 2026-09-30 wiring audit:
 *
 *   - 13 actions claimed the 'mcp' surface (the ADR-063 Step 1 set: the
 *     diagnostic reads, bool-dispatch, fillet-dispatch, the five
 *     attach-surface-*-validated). The MCP server serves none of them — no
 *     handler, not even a tiers.ts declaration.
 *   - 6 of those also claimed 'palette' with status 'ok', and the Capability
 *     Explorer cannot launch them: they are not in its direct-dispatch map, not
 *     menu items, not dispatchAction ids, so launching one answers
 *     "알 수 없는 명령입니다". The Explorer even advised "MCP 호출 권장" for them.
 *
 * The package's own test (packages/axia-action-catalog/test/catalog.test.ts)
 * asserted exactly those claims — and no CI workflow runs that package's tests.
 * These checks live here because this file does run in CI.
 */
describe('ActionCatalog surface claims are backed by what they name', () => {
  const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

  it("every action that claims the 'mcp' surface is a capability the MCP server serves", () => {
    // Served = has a handler. tiers.ts declares names that may have no handler
    // yet (it says so itself), and tools/list shows only handled ones.
    const capDir = resolve(process.cwd(), '../packages/axia-mcp-server/src/capabilities');
    const served = new Set<string>();
    for (const f of readdirSync(capDir)) {
      if (!f.endsWith('.ts')) continue;
      const src = readFileSync(join(capDir, f), 'utf8');
      for (const m of src.matchAll(/^\s+name:\s*'([a-z][a-z0-9_]*)'/gm)) served.add(m[1]);
    }
    // PREMISE: the handlers were read.
    expect(served.size, 'MCP capability handlers were read').toBeGreaterThan(20);
    expect(served).toContain('draw_rect');

    const unserved = ALL_ACTIONS.filter((a) => a.surfaces.includes('mcp'))
      .filter((a) => !(a.aliases.mcp && served.has(a.aliases.mcp)))
      .map((a) => `${a.id} (mcp: ${a.aliases.mcp ?? 'none'})`);
    expect(
      unserved,
      "Actions that claim the 'mcp' surface but name no capability the MCP server " +
        'serves. Implement the capability (packages/axia-mcp-server/src/capabilities) ' +
        "or drop 'mcp' from the action's surfaces.",
    ).toEqual([]);
  });

  it("every 'ok' action that claims the 'palette' surface launches from the Capability Explorer", () => {
    // The Explorer's launch path (main.ts, onActionInvoke), in order: its
    // direct-dispatch map → dispatchMenuAction (#menubar / #statusbar items,
    // plus CONTEXT_SELECTION_ACTIONS) → executeAction (dispatchAction ids).
    // Anything else is answered "알 수 없는 명령입니다".
    const main = read('src/main.ts');
    const ddStart = main.indexOf('const directDispatch');
    const dd = main.slice(ddStart, main.indexOf('const direct = directDispatch', ddStart));
    const direct = [...dd.matchAll(/'([a-z0-9-]+)':\s*\(\)\s*=>/g)].map((m) => m[1]);
    const doc = new DOMParser().parseFromString(read('index.html'), 'text/html');
    const itemsIn = (id: string) =>
      [...(doc.getElementById(id)?.querySelectorAll('[data-action]') ?? [])].map((e) => e.getAttribute('data-action')!);
    const allow = /CONTEXT_SELECTION_ACTIONS = new Set\(\[([^\]]*)\]\)/.exec(read('src/ui/dispatchMenuAction.ts'));
    const dispatchIds = [...read('src/tools/ToolManagerRefactored.ts').matchAll(/action\s*===\s*'([^']+)'/g)].map((m) => m[1]);

    // PREMISES — one per hop, so a hop that stops parsing fails here instead of
    // shrinking the launchable set without a word.
    expect(direct, 'main.ts direct-dispatch map').toContain('edge-curve-info');
    expect(itemsIn('menubar').length, '#menubar items').toBeGreaterThan(100);
    expect(allow, 'CONTEXT_SELECTION_ACTIONS').toBeTruthy();
    expect(dispatchIds.length, 'dispatchAction ids').toBeGreaterThan(50);

    const launchable = new Set([
      ...direct,
      ...itemsIn('menubar'),
      ...itemsIn('statusbar'),
      ...[...allow![1].matchAll(/'([^']+)'/g)].map((m) => m[1]),
      ...dispatchIds,
    ]);
    const dead = ALL_ACTIONS.filter((a) => a.surfaces.includes('palette') && (a.status ?? 'ok') === 'ok')
      .filter((a) => !launchable.has(a.id))
      .map((a) => a.id);
    expect(
      dead,
      "Actions marked 'ok' on the 'palette' surface that the Capability Explorer cannot " +
        "launch. Wire the id (main.ts directDispatch, a menu item, or dispatchAction), or " +
        "mark it status: 'stub'.",
    ).toEqual([]);
  });
});
