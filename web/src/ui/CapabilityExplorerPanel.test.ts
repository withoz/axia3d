/**
 * CapabilityExplorerPanel — ADR-063 Phase 1 Path Z Step 2 regression.
 *
 * 1 invariant per Step 2 §3.2:
 *   #2 capability_explorer_imports_only_capability_explorer_panel
 *      — `@axia/action-catalog` is imported by AT MOST ONE file in
 *        `web/src/`, and that file is `CapabilityExplorerPanel.ts`.
 *      — §D #1 lock-in: Capability Explorer is the SOLE consumer of
 *        the catalog package in the web/ tree.
 */

import { describe, it, expect } from 'vitest';
import { CapabilityExplorerPanel } from './CapabilityExplorerPanel';

// Vite's import.meta.glob — source-level scan without node:fs deps.
// Captures all .ts files in web/src/ as raw strings for grep.
const allTsFiles = import.meta.glob('/src/**/*.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>;

describe('ADR-063 Step 2 — single import site lock-in', () => {
  it('capability_explorer_imports_only_capability_explorer_panel', () => {
    const importPattern = /from\s+['"]@axia\/action-catalog['"]/;
    const importers: string[] = [];
    for (const [path, content] of Object.entries(allTsFiles)) {
      // Skip test files (production-source-only contract).
      if (path.endsWith('.test.ts')) continue;
      // Skip generated WASM bindings + mocks (defensive).
      if (path.includes('/wasm/') || path.includes('/__mocks__/')) continue;
      if (importPattern.test(content)) {
        importers.push(path);
      }
    }
    expect(importers.length, `multiple files import @axia/action-catalog: ${JSON.stringify(importers)}`).toBe(1);
    expect(importers[0]).toBe('/src/ui/CapabilityExplorerPanel.ts');
  });

  it('capability_explorer_constructs_without_error', () => {
    const container = document.createElement('div');
    const panel = new CapabilityExplorerPanel(container, {});
    expect(panel.isVisible()).toBe(false);
    panel.show();
    expect(panel.isVisible()).toBe(true);
    panel.hide();
    expect(panel.isVisible()).toBe(false);
    panel.dispose();
  });

  it('capability_explorer_exposes_catalog_size_above_zero', () => {
    // §D #1 lock-in: only Capability Explorer surfaces catalog size.
    const size = CapabilityExplorerPanel.getCatalogSize();
    expect(size, 'catalog should have actions registered').toBeGreaterThan(0);
    // Step 1 added 13 endpoints to 82 baseline → 95 total.
    expect(size).toBeGreaterThanOrEqual(95);
  });
});

describe('ADR-063 Step 3 — actions tree + Tier groups + search', () => {
  it('capability_explorer_panel_renders_all_actions', () => {
    // Per ADR-063 §3.2 invariant — panel renders all 95 actions when
    // shown with no filter. We probe the rendered DOM for action ids.
    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});
    panel.show();

    const allActions = CapabilityExplorerPanel.getAllActions();
    const renderedIds = Array.from(
      container.querySelectorAll('.cep-action-row'),
    ).map((el) => (el as HTMLElement).dataset.actionId);

    expect(renderedIds.length).toBe(allActions.length);
    // Spot-check a few from each Phase O+P+L₂ batch (Step 1 entries).
    for (const id of [
      'edge-curve-info',
      'face-normals-cached',
      'attach-surface-cylinder-validated',
      'fillet-edge', // pre-Step-1 baseline action
    ]) {
      expect(renderedIds, `missing action id: ${id}`).toContain(id);
    }

    panel.dispose();
    container.remove();
  });

  it('capability_explorer_search_filter_works', () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});

    // Filter by 'cylinder' — should return ≥ 1 match (attach-surface-cylinder-validated etc.)
    const cylinderHits = panel.filterActions('cylinder');
    expect(cylinderHits.length).toBeGreaterThan(0);
    expect(cylinderHits.some((a) => a.id.includes('cylinder'))).toBe(true);

    // Filter by 'attach-surface' — must return all 5 W2 endpoints (Path Z).
    const attachHits = panel.filterActions('attach-surface');
    expect(attachHits.length).toBe(5);

    // Filter by impossible string — returns empty.
    const noHits = panel.filterActions('xyznonexistentabc12345');
    expect(noHits.length).toBe(0);

    // Empty query returns ALL.
    const allHits = panel.filterActions('');
    expect(allHits.length).toBe(CapabilityExplorerPanel.getCatalogSize());

    panel.dispose();
    container.remove();
  });

  it('capability_explorer_tier_groups_present', () => {
    // Each populated Tier should produce a .cep-tier-group node with
    // matching data-tier. Tiers 0/1/2 should always be populated; Tier
    // 3 may be small but should exist (Step 5 hides it by default).
    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});
    panel.show();

    const tiers = Array.from(
      container.querySelectorAll('.cep-tier-group'),
    ).map((el) => (el as HTMLElement).dataset.tier);

    expect(tiers, 'Tier 0 should appear').toContain('0');
    expect(tiers, 'Tier 1 should appear').toContain('1');
    expect(tiers, 'Tier 2 should appear').toContain('2');
    // Tier 3 may have at least one action (e.g. file-new) — best-effort check.
    // We only assert it's renderable when present (not hidden in Step 3).

    panel.dispose();
    container.remove();
  });
});

describe('ADR-063 Step 4 — Tier 0 form + Tier 1/2 launcher', () => {
  it('capability_explorer_tier0_form_executes_action', async () => {
    // Tier 0 (no-args) action: cache-stats. Click expand → Run → callback fires.
    const container = document.createElement('div');
    document.body.appendChild(container);
    const calls: { id: string; args: Record<string, unknown> }[] = [];
    const panel = new CapabilityExplorerPanel(container, {
      onActionInvoke: (actionId, args) => {
        calls.push({ id: actionId, args });
        return { ok: true, result: '{"schemaVersion":1,"totalBytes":0}' };
      },
    });
    panel.show();

    // Find the cache-stats row + click to expand.
    const row = container.querySelector('.cep-action-row[data-action-id="cache-stats"]') as HTMLElement;
    expect(row, 'cache-stats row must render').toBeTruthy();
    (row.querySelector('.cep-action-head') as HTMLElement).click();

    // Click the Run button.
    const btn = container.querySelector('.cep-action-row[data-action-id="cache-stats"] .cep-form-btn') as HTMLElement;
    expect(btn, 'Run button must render after expand').toBeTruthy();
    expect(btn.textContent).toContain('Run');
    btn.click();

    // Allow async callback resolution.
    await Promise.resolve();
    await Promise.resolve();

    expect(calls.length, 'callback must fire on Run').toBe(1);
    expect(calls[0].id).toBe('cache-stats');

    // Result element shows ok class.
    const result = container.querySelector(
      '.cep-action-row[data-action-id="cache-stats"] .cep-form-result',
    ) as HTMLElement;
    expect(result).toBeTruthy();
    expect(result.style.display).not.toBe('none');
    expect(result.classList.contains('cep-result-ok')).toBe(true);

    panel.dispose();
    container.remove();
  });

  it('capability_explorer_tier3_hidden_by_default', () => {
    // §D #2 lock-in — Tier 3 (destructive) actions are hidden unless the user
    // explicitly enables "Show advanced".
    //
    // The catalog currently has ZERO Tier 3 entries (measured 2026-07-16:
    // 54/72/88/0), so the toggle used to be a control that could never do
    // anything: renderTree looped the tier, found an empty bucket and
    // continued. This test now pins BOTH halves — the lock-in for when Tier 3
    // entries land, and the honest absence of the control until they do. It is
    // not a coverage gap: `delete` / `tool-erase` / `tool-explode` / `ungroup`
    // are catalogued at Tier 2, and moving them to 3 would hide everyday tools
    // behind an off-by-default toggle. Per ADR-045 D5 Tier 3 belongs to the
    // Debug Panel's Danger Zone.
    try { localStorage.removeItem('axia.capabilityExplorer.showAdvanced'); } catch {}

    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});
    panel.show();

    const hasTier3 = CapabilityExplorerPanel.getAllActions().some((a) => a.tier === 3);
    const toggle = container.querySelector(
      '.cep-toggle-advanced input[type="checkbox"]',
    ) as HTMLInputElement | null;

    // Default, either way: Tier 3 is not on screen.
    expect(panel.isAdvancedVisible()).toBe(false);
    expect(
      container.querySelector('.cep-tier-group[data-tier="3"]'),
      'Tier 3 group must NOT render by default (§D #2)',
    ).toBeNull();

    if (!hasTier3) {
      expect(toggle, 'no Tier 3 actions → no control that cannot do anything').toBeNull();
    } else {
      expect(toggle, 'Tier 3 actions exist → the toggle must be offered').toBeTruthy();
      toggle!.checked = true;
      toggle!.dispatchEvent(new Event('change'));
      expect(panel.isAdvancedVisible()).toBe(true);
      expect(
        container.querySelector('.cep-tier-group[data-tier="3"]'),
        'Tier 3 group must render after toggle on',
      ).toBeTruthy();
      try {
        expect(localStorage.getItem('axia.capabilityExplorer.showAdvanced')).toBe('1');
      } catch { /* localStorage unavailable */ }

      toggle!.checked = false;
      toggle!.dispatchEvent(new Event('change'));
      expect(panel.isAdvancedVisible()).toBe(false);
      expect(
        container.querySelector('.cep-tier-group[data-tier="3"]'),
        'Tier 3 group must hide after toggle off',
      ).toBeNull();
    }

    panel.dispose();
    container.remove();
    try { localStorage.removeItem('axia.capabilityExplorer.showAdvanced'); } catch {}
  });

  it('a stale showAdvanced=1 cannot leave an unreachable filter on', () => {
    // An earlier build persisted the flag; with no Tier 3 entries there is now
    // no toggle to turn it back off, so construction must clear it.
    try { localStorage.setItem('axia.capabilityExplorer.showAdvanced', '1'); } catch {}
    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});
    panel.show();
    if (!CapabilityExplorerPanel.getAllActions().some((a) => a.tier === 3)) {
      expect(panel.isAdvancedVisible()).toBe(false);
    }
    panel.dispose();
    container.remove();
    try { localStorage.removeItem('axia.capabilityExplorer.showAdvanced'); } catch {}
  });

  it('capability_explorer_tier2_requires_confirm', () => {
    // Tier 2 (migrate-curve-surface) — confirm() must be called before invoke.
    const container = document.createElement('div');
    document.body.appendChild(container);
    let confirmCalled = 0;
    let callbackCalled = 0;
    const origConfirm = window.confirm;
    window.confirm = () => { confirmCalled++; return false; }; // user cancels

    const panel = new CapabilityExplorerPanel(container, {
      onActionInvoke: () => { callbackCalled++; return { ok: true }; },
    });
    panel.show();

    const row = container.querySelector('.cep-action-row[data-action-id="migrate-curve-surface"]') as HTMLElement;
    expect(row).toBeTruthy();
    (row.querySelector('.cep-action-head') as HTMLElement).click();

    const btn = container.querySelector(
      '.cep-action-row[data-action-id="migrate-curve-surface"] .cep-form-btn',
    ) as HTMLElement;
    expect(btn.textContent).toContain('변경');
    btn.click();

    expect(confirmCalled, 'confirm dialog must trigger for Tier 2').toBe(1);
    expect(callbackCalled, 'cancelled confirm must skip callback').toBe(0);

    window.confirm = origConfirm;
    panel.dispose();
    container.remove();
  });
});

/**
 * ADR-294 batch 4 — the Explorer renders ActionCatalog's 210 Korean labels.
 *
 * This exists because the wiring silently did not land: a script asserted on a
 * bad anchor and never wrote the file, and I read a grep of the file's EXISTING
 * imports as confirmation. The commit claimed the Explorer was done; the app
 * showed 210 Korean labels. Rendering it and looking for Hangul is the check
 * that a grep cannot fake.
 */
describe('ADR-294 — Capability Explorer i18n', () => {
  const openPanel = () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const panel = new CapabilityExplorerPanel(container, {});
    panel.show();
    return { panel, container, el: document.getElementById('capability-explorer')! };
  };

  it('renders Korean labels by default', async () => {
    const { setLocale } = await import('../i18n');
    setLocale('ko');
    const { panel, container, el } = openPanel();
    expect(el.textContent, 'Korean is the default').toMatch(/[가-힣]/);
    panel.dispose(); container.remove();
  });

  it('renders NO Korean when the locale is English', async () => {
    const { setLocale } = await import('../i18n');
    setLocale('en');
    const { panel, container, el } = openPanel();
    const rows = el.querySelectorAll('.cep-action-label');
    expect(rows.length, 'an empty tree would pass vacuously').toBeGreaterThan(100);
    const ko = [...rows].map((r) => r.textContent ?? '').filter((s) => /[가-힣]/.test(s));
    expect(ko, 'every catalog label must be translated').toEqual([]);
    expect(el.querySelector('.cep-search-input')?.getAttribute('placeholder'))
      .toBe('Search (id / label / description)');
    panel.dispose(); container.remove(); setLocale('ko');
  });

  it('finds a command by its ENGLISH name when the locale is English', async () => {
    // The half that is easy to forget: translating the render but not the
    // haystack leaves search matching Korean only, so an English user types
    // "fillet" and gets nothing.
    const { setLocale } = await import('../i18n');
    setLocale('en');
    const { panel, container } = openPanel();
    // The query has to be one the id cannot answer. 'fillet' was a bad choice:
    // the ids ARE 'tool-fillet' / 'fillet-edge', so it matched with the label
    // haystack removed — the test passed while proving nothing. Measured, 104
    // of the catalog's 209 Korean labels are reachable ONLY through their
    // translated label; 'linear array' (id 'array-linear') is one of them.
    expect(panel.filterActions('linear array').map((a) => a.id),
      'an English label the id and description cannot answer').toContain('array-linear');
    expect(panel.filterActions('intersect with model').map((a) => a.id))
      .toContain('intersect-with-model');
    // …and the Korean must still reach it.
    expect(panel.filterActions('선형 배열').map((a) => a.id)).toContain('array-linear');
    panel.dispose(); container.remove(); setLocale('ko');
  });
});

