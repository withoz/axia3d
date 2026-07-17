/**
 * Carry the drawing across the reload that a language switch performs.
 *
 * ADR-294 D7 reloads the page because the catalogs and panels build their
 * markup once — a live switch would leave English toasts over a Korean menu
 * bar, which is worse than either language alone. That reasoning stands. What
 * it did not account for is that the scene lives in memory only: no autosave,
 * and `beforeunload` merely disposes the viewport. So the reload took the
 * drawing with it, and the answer to "메뉴가 영어로 바뀌면 그리던 것이
 * 없어지나?" was yes.
 *
 * The fix is not to avoid the reload — it is to make the reload survivable.
 * The scene goes into sessionStorage on the way out and comes back on boot,
 * through the same `export_snapshot` / `import_snapshot` the .axia file format
 * uses. No new serialization.
 *
 * **sessionStorage, not localStorage**: this is a handoff between two loads of
 * one tab, and it must not outlive the tab. A stale scene reappearing in a new
 * session would be a far worse bug than the one being fixed.
 *
 * **What does not survive**: the undo stack. A snapshot is the scene, not the
 * transaction history — so after a switch, Undo starts from the restored
 * state. Announced in the toast rather than discovered by pressing Ctrl+Z.
 */

const KEY = 'axia:locale-switch-scene';

/**
 * Guard against sessionStorage's ~5MB ceiling.
 *
 * Base64 costs ~33%, so the encoded string is what has to fit. 3MB of raw
 * snapshot leaves headroom for whatever else the tab keeps. Past that, the
 * caller falls back to asking the user (SettingsPanel) rather than silently
 * dropping the work on the floor.
 */
export const MAX_SNAPSHOT_BYTES = 3_000_000;

/** Bytes → base64. Chunked: `String.fromCharCode(...bytes)` blows the stack. */
function toBase64(bytes: Uint8Array): string {
  let s = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

/** base64 → bytes. */
function fromBase64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * Park the scene for the reload. Returns false when it cannot be preserved —
 * too large, or storage refused — so the caller can warn instead of pretending.
 */
export function stashSceneForLocaleSwitch(snapshot: Uint8Array | null): boolean {
  if (!snapshot || snapshot.length === 0) return false;
  if (snapshot.length > MAX_SNAPSHOT_BYTES) return false;
  try {
    sessionStorage.setItem(KEY, toBase64(snapshot));
    return true;
  } catch {
    // QuotaExceededError, or storage disabled entirely (private mode).
    return false;
  }
}

/**
 * Take the parked scene, if any. Consumes it — a restore must happen once, so
 * the key is removed before the bytes are handed back, even if the caller then
 * fails to apply them. Returns null when there is nothing to restore.
 */
export function takeStashedScene(): Uint8Array | null {
  let raw: string | null = null;
  try {
    raw = sessionStorage.getItem(KEY);
    if (raw !== null) sessionStorage.removeItem(KEY);
  } catch {
    return null;
  }
  if (!raw) return null;
  try {
    return fromBase64(raw);
  } catch {
    return null; // corrupt payload — better an empty canvas than a crash on boot
  }
}

/** Drop anything parked. For a cancelled switch. */
export function clearStashedScene(): void {
  try {
    sessionStorage.removeItem(KEY);
  } catch {
    /* storage unavailable — nothing was stashed either */
  }
}
