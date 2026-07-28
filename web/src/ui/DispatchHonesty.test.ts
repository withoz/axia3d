/**
 * ADR-299 — `dispatchMenuAction` must report whether the action HAPPENED, not
 * merely whether a `[data-action]` element exists.
 *
 * The old contract returned `true` right after dispatching the click, so the
 * Capability Explorer recorded `result:'ok'` / "Launched (menu dispatch)" for
 * four Window items whose handlers bailed on a missing global and only raised a
 * warning toast. Every dead handler was invisible in the audit trail.
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { initMenuBar, MenuBarDeps, consumeActionUnavailable } from './MenuBar';
import { dispatchMenuAction } from './dispatchMenuAction';

vi.mock('../utils/debug', () => ({ debugLog: vi.fn(), debugWarn: vi.fn() }));

function dom(): void {
  document.body.innerHTML = `
    <div id="menubar">
      <div class="menu-item"><span>View</span><div class="menu-dropdown">
        <div class="menu-action" data-action="view-components">Components</div>
        <div class="menu-action" data-action="view-xia-inspector">Inspector</div>
      </div></div>
    </div>`;
}

function deps(): MenuBarDeps {
  return {
    viewport: { scene: { children: [] }, setViewMode: vi.fn(), resetCamera: vi.fn(),
      getStyleSettings: vi.fn().mockReturnValue({}), setGridVisible: vi.fn(),
      setAxisVisible: vi.fn() } as any,
    bridge: {} as any,
    toolManager: { setTool: vi.fn(), hasTool: vi.fn().mockReturnValue(true),
      executeAction: vi.fn(), selection: { clearSelection: vi.fn() } } as any,
    scene: { children: [] } as any,
    fileManager: { saveAsProject: vi.fn() } as any,
    saveProject: vi.fn(), openProject: vi.fn(), openOsnapPanel: vi.fn(),
  };
}

describe('ADR-299 — dispatchMenuAction reports the truth', () => {
  beforeEach(() => {
    dom();
    initMenuBar(deps());
    consumeActionUnavailable();
    delete (window as any).__axia_componentPanel;
    delete (window as any).__axia_xiaInspector;
  });

  it('reports false when the handler could not carry the action out', () => {
    // No panel global — the handler warns and marks itself unavailable.
    expect(dispatchMenuAction('view-components')).toBe(false);
  });

  it('reports true when the handler really ran', () => {
    const toggle = vi.fn();
    (window as any).__axia_xiaInspector = { toggle };
    expect(dispatchMenuAction('view-xia-inspector')).toBe(true);
    expect(toggle).toHaveBeenCalled();
  });

  it('still reports false for an id that is not in the DOM at all', () => {
    expect(dispatchMenuAction('no-such-action-id')).toBe(false);
  });

  it('a failed dispatch does not poison the next one', () => {
    expect(dispatchMenuAction('view-components')).toBe(false);
    const toggle = vi.fn();
    (window as any).__axia_xiaInspector = { toggle };
    expect(dispatchMenuAction('view-xia-inspector')).toBe(true);
  });
});
