/**
 * makeFloatingDraggable — make a small fixed/absolute floating chrome element
 * (console chip, Home button, …) draggable by mouse, persisting its position
 * to localStorage so it survives reloads.
 *
 * Unlike `DraggablePanelManager` (header-based docking state machine for the
 * Inspector/Style/Snap panels), this is a minimal "grab the thing and move it"
 * helper for header-less floating buttons/pills.
 *
 * Click vs drag: a press that moves less than `threshold` px stays a CLICK —
 * the element's own click handler (camera home / console toggle) fires
 * normally. A real drag repositions the element and suppresses the single
 * trailing `click` (document capture, one-shot) so the action does NOT fire
 * after a reposition.
 *
 * On first drag the element is converted to `position: fixed; left/top`
 * (clearing right/bottom anchors). The persisted position is clamped to the
 * viewport on restore so a window resize can't strand it off-screen.
 */

export interface FloatingDraggableOptions {
  /** localStorage key for the persisted `{left, top}`. */
  storageKey: string;
  /** Element that initiates the drag. Defaults to `el` itself. */
  handle?: HTMLElement;
  /** Pixels of movement before a press becomes a drag. Default 4. */
  threshold?: number;
}

interface StoredPos {
  left: number;
  top: number;
}

function readStored(key: string): StoredPos | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const p = JSON.parse(raw) as unknown;
    if (
      p && typeof p === 'object' &&
      typeof (p as StoredPos).left === 'number' &&
      typeof (p as StoredPos).top === 'number'
    ) {
      return { left: (p as StoredPos).left, top: (p as StoredPos).top };
    }
  } catch {
    /* corrupt / unavailable — ignore */
  }
  return null;
}

function viewport(): { vw: number; vh: number } {
  return {
    vw: typeof window !== 'undefined' ? window.innerWidth : 1280,
    vh: typeof window !== 'undefined' ? window.innerHeight : 720,
  };
}

/** Clamp a top-left position so the element stays on screen. */
function clampPos(left: number, top: number, w: number, h: number): StoredPos {
  const { vw, vh } = viewport();
  const maxX = Math.max(0, vw - Math.max(1, w));
  const maxY = Math.max(0, vh - Math.max(1, h));
  return {
    left: Math.max(0, Math.min(left, maxX)),
    top: Math.max(0, Math.min(top, maxY)),
  };
}

function applyFixed(el: HTMLElement, left: number, top: number): void {
  el.style.position = 'fixed';
  el.style.right = 'auto';
  el.style.bottom = 'auto';
  el.style.left = `${left}px`;
  el.style.top = `${top}px`;
}

/**
 * Wire drag behavior onto `el`. Returns a disposer that removes the listeners.
 */
export function makeFloatingDraggable(
  el: HTMLElement,
  opts: FloatingDraggableOptions,
): () => void {
  const handle = opts.handle ?? el;
  const threshold = opts.threshold ?? 4;

  // Restore a persisted position (clamped to the current viewport).
  const stored = readStored(opts.storageKey);
  if (stored) {
    const r = el.getBoundingClientRect();
    const c = clampPos(stored.left, stored.top, r.width, r.height);
    applyFixed(el, c.left, c.top);
  }

  let startX = 0;
  let startY = 0;
  let baseLeft = 0;
  let baseTop = 0;
  let lastLeft = 0;
  let lastTop = 0;
  let armed = false;
  let dragging = false;

  const onMove = (e: MouseEvent): void => {
    if (!armed) return;
    const dx = e.clientX - startX;
    const dy = e.clientY - startY;
    if (!dragging && Math.hypot(dx, dy) < threshold) return;
    if (!dragging) {
      dragging = true;
      el.style.cursor = 'grabbing';
      document.body.style.userSelect = 'none';
    }
    const r = el.getBoundingClientRect();
    const c = clampPos(baseLeft + dx, baseTop + dy, r.width, r.height);
    lastLeft = c.left;
    lastTop = c.top;
    applyFixed(el, c.left, c.top);
  };

  const onUp = (): void => {
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    armed = false;
    document.body.style.userSelect = '';
    if (!dragging) return;
    dragging = false;
    el.style.cursor = '';

    try {
      localStorage.setItem(
        opts.storageKey,
        JSON.stringify({ left: lastLeft, top: lastTop }),
      );
    } catch {
      /* storage unavailable — position still applied for this session */
    }

    // Eat the single click the browser synthesizes after a drag, so the
    // element's own action (resetCamera / console toggle) doesn't fire.
    const suppress = (ev: Event): void => {
      ev.stopPropagation();
      ev.preventDefault();
    };
    document.addEventListener('click', suppress, { capture: true, once: true });
    // If no click follows (drag released off-element), drop the one-shot.
    setTimeout(() => document.removeEventListener('click', suppress, true), 0);
  };

  const onDown = (e: MouseEvent): void => {
    if (e.button !== 0) return;
    armed = true;
    dragging = false;
    startX = e.clientX;
    startY = e.clientY;
    const r = el.getBoundingClientRect();
    baseLeft = r.left;
    baseTop = r.top;
    lastLeft = r.left;
    lastTop = r.top;
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  };

  handle.addEventListener('mousedown', onDown);
  handle.setAttribute('data-floating-draggable', 'true');

  return (): void => {
    handle.removeEventListener('mousedown', onDown);
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
  };
}
