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
 * MCP/diagnostic-only — not registered in CommandCatalog. This is OK.
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
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { getCommandCatalog, __resetCommandCatalog, type CommandDef } from './CommandCatalog';
import { registerAxiaCommands } from './AxiaCommands';
import { getActionById, lookup, type ActionDef } from '@axia/action-catalog';

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

  it('CommandCatalog count matches expected total (187, after -3 ghosts, +1 surfaced tool, -1 hover-only)', () => {
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
    expect(count).toBe(188);
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
