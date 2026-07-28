/**
 * Fire a menu action by id and report whether it ACTUALLY happened.
 *
 * Lives in its own module so its contract can be tested without importing
 * `main.ts` (which boots the whole app). That contract matters: it used to
 * return `true` as soon as a `[data-action]` element existed, so the Capability
 * Explorer recorded `result:'ok'` — "Launched (menu dispatch)" — for four Window
 * items whose handlers bailed on a missing global and only raised a warning
 * toast. Every dead handler was invisible in the audit trail (ADR-299).
 */
import { consumeActionUnavailable } from './MenuBar';

/**
 * Context-menu ids the palette may reach. The context menu is built per
 * right-click, so only these selection-scoped ones are safe to dispatch blind.
 */
const CONTEXT_SELECTION_ACTIONS = new Set(['group-edit', 'group-hide', 'group-lock']);

export function dispatchMenuAction(id: string): boolean {
  // #statusbar too: osnap / grid / edge / axis / help / rename are F-key
  // buttons down there, handled by StatusBar.ts — they exist in neither
  // #menubar nor executeAction, so from the palette they fell through both
  // hops and produced "unknown command". Measured in the wiring audit.
  const item =
    document.querySelector<HTMLElement>(
      `#menubar [data-action="${id}"], #statusbar [data-action="${id}"]`,
    ) ??
    (CONTEXT_SELECTION_ACTIONS.has(id)
      ? document.querySelector<HTMLElement>(`#context-menu [data-action="${id}"]`)
      : null);
  if (!item) return false;
  item.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  // Finding the element is not the same as the action happening: a handler that
  // cannot proceed marks itself, and that is what we report.
  return !consumeActionUnavailable();
}
