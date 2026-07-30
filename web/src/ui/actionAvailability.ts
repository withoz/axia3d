/**
 * "I could not carry that out" — a one-slot flag read back by whoever dispatched.
 *
 * `dispatchMenuAction` used to report success whenever it found a `[data-action]`
 * element and clicked it, so the Capability Explorer recorded `result:'ok'` even
 * for a handler that bailed and only raised a warning toast. Four Window-menu
 * items were dead that way and the audit trail called every one a success
 * (ADR-299). A handler that cannot proceed says so here; the dispatcher reads it
 * back and reports honestly.
 *
 * This lives apart from MenuBar because MenuBar imports ToolManager, so a
 * handler inside ToolManager could not reach back for it without closing an
 * import cycle. MenuBar re-exports both functions, so existing importers are
 * unaffected.
 */
import { Toast } from './Toast';

let actionUnavailable = false;

/** A handler declares it could not carry the action out. */
export function markActionUnavailable(message: string): void {
  actionUnavailable = true;
  Toast.warning(message);
}

/** Read and clear the flag — call immediately after dispatching a click. */
export function consumeActionUnavailable(): boolean {
  const was = actionUnavailable;
  actionUnavailable = false;
  return was;
}

/**
 * Fresh slate before dispatching, so a flag left over from an earlier action
 * cannot be read as this one's verdict. Separate from `consume` because the
 * caller here is not asking a question, and a discarded return value would read
 * as though it were.
 */
export function resetActionUnavailable(): void {
  actionUnavailable = false;
}
