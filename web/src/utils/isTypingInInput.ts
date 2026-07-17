/**
 * isTypingInInput — the one answer to "is the user typing right now?"
 *
 * Every global keydown listener has to ask this, and before this module each
 * one answered for itself. Measured 2026-07-16 across the 15 global listeners:
 *
 *   - 1 checked all four cases (KeyboardShortcuts, which is where this lived)
 *   - 6 checked only `tagName === 'INPUT'` or `instanceof HTMLInputElement`,
 *     some adding SELECT
 *   - ToolManagerRefactored's arrow-key listener checked nothing at all, and
 *     called preventDefault(), so typing in a field and pressing ArrowLeft set
 *     the axis lock to Z and ate the caret movement
 *
 * A guard that is copied is a guard that drifts (메타-원칙 #4). Import this.
 *
 * Not every listener needs it: one that only fires on Ctrl/Meta combinations
 * (Ctrl+K, Ctrl+`) cannot collide with typing, and one that registers on open
 * and unregisters on close (the palette's own arrow/Enter handling) is
 * handling its own field on purpose. This is for listeners that claim bare
 * keys while a text field may hold focus.
 */

/**
 * True when the event target is a field the user types into — `<input>`,
 * `<textarea>`, `<select>`, or any `contenteditable` host.
 *
 * TEXTAREA and contentEditable are covered even though the app currently
 * renders neither: this is the check every listener defers to, so it should
 * be right before the first one appears rather than after the bug report.
 */
export function isTypingInInput(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    el.isContentEditable === true
  );
}
