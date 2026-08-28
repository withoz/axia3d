/**
 * MiniGridSettings — the cursor's mini work-plane grid.
 *
 * The small patch drawn under the cursor while a draw tool is waiting for its
 * first point, so you can see the plane the next point will land on.
 *
 * ⚠ Its size is a SCREEN quantity, not a world one. The renderer derives the
 * world radius from the cursor's depth every frame, so the patch keeps its
 * apparent size at any zoom — a fixed world radius vanishes when you zoom out
 * and swallows the model when you zoom in.
 *
 * Ported from Kayac's `MiniGridSettings` (native/rust/src/settings.rs), whose
 * shipped defaults these are. Values live here rather than in the engine because
 * they are a cursor decoration: nothing in the kernel reads them, so the WASM
 * boundary stays untouched.
 *
 * Follows the `CylinderSegmentsSettings` numeric pattern (clamp + localStorage +
 * listeners); `line_hw_px` is fractional so it clamps without rounding.
 */

const RADIUS_KEY = 'axia:mini-grid-radius-px';
const CELLS_KEY = 'axia:mini-grid-cells';
const LINE_HW_KEY = 'axia:mini-grid-line-hw-px';
const VISIBLE_KEY = 'axia:mini-grid-visible';

/** Apparent radius in CSS pixels. 48 is the shipped look. */
const DEFAULT_RADIUS_PX = 48;
const MIN_RADIUS_PX = 1;
const MAX_RADIUS_PX = 400;

/** Cells from the centre to the rim. */
const DEFAULT_CELLS = 4;
const MIN_CELLS = 1;
const MAX_CELLS = 32;

/**
 * Line HALF-width in physical pixels, so 0.5 is one CSS pixel wide at DPR 2.
 *
 * ⚠ Hairline by default on purpose. Kayac's note, which applies to
 * `LineSegments2` the same way: the line shader gives every segment a square cap
 * that also extends this far ALONG the line, so at a grid's forty-odd crossings
 * a fat value piles up into blobs rather than reading as a thicker grid.
 */
const DEFAULT_LINE_HW_PX = 0.5;
const MIN_LINE_HW_PX = 0.1;
const MAX_LINE_HW_PX = 4;

let radiusPx = DEFAULT_RADIUS_PX;
let cells = DEFAULT_CELLS;
let lineHwPx = DEFAULT_LINE_HW_PX;
let visible = true;

try {
  const r = parseFloat(localStorage.getItem(RADIUS_KEY) ?? '');
  if (Number.isFinite(r) && r >= MIN_RADIUS_PX && r <= MAX_RADIUS_PX) radiusPx = r;

  const c = parseInt(localStorage.getItem(CELLS_KEY) ?? '', 10);
  if (Number.isFinite(c) && c >= MIN_CELLS && c <= MAX_CELLS) cells = c;

  const w = parseFloat(localStorage.getItem(LINE_HW_KEY) ?? '');
  if (Number.isFinite(w) && w >= MIN_LINE_HW_PX && w <= MAX_LINE_HW_PX) lineHwPx = w;

  // Default ON, so only an explicit 'false' turns it off (ADR-049 P-5e-α).
  if (localStorage.getItem(VISIBLE_KEY) === 'false') visible = false;
} catch {
  /* private mode */
}

const listeners = new Set<() => void>();

function notify(): void {
  for (const cb of listeners) cb();
}

function store(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

export function getMiniGridVisible(): boolean {
  return visible;
}

export function setMiniGridVisible(value: boolean): void {
  if (visible === value) return;
  visible = value;
  store(VISIBLE_KEY, String(value));
  notify();
}

export function getMiniGridRadiusPx(): number {
  return radiusPx;
}

/** Clamped to 1–400 CSS pixels. */
export function setMiniGridRadiusPx(value: number): void {
  if (!Number.isFinite(value)) return;
  const clamped = Math.max(MIN_RADIUS_PX, Math.min(MAX_RADIUS_PX, value));
  if (clamped === radiusPx) return;
  radiusPx = clamped;
  store(RADIUS_KEY, String(clamped));
  notify();
}

export function getMiniGridCells(): number {
  return cells;
}

/** Clamped to 1–32 and rounded — a cell count is whole. */
export function setMiniGridCells(value: number): void {
  if (!Number.isFinite(value)) return;
  const clamped = Math.max(MIN_CELLS, Math.min(MAX_CELLS, Math.round(value)));
  if (clamped === cells) return;
  cells = clamped;
  store(CELLS_KEY, String(clamped));
  notify();
}

export function getMiniGridLineHwPx(): number {
  return lineHwPx;
}

/** Clamped to 0.1–4. Not rounded — a half-width of 0.5 is the default. */
export function setMiniGridLineHwPx(value: number): void {
  if (!Number.isFinite(value)) return;
  const clamped = Math.max(MIN_LINE_HW_PX, Math.min(MAX_LINE_HW_PX, value));
  if (clamped === lineHwPx) return;
  lineHwPx = clamped;
  store(LINE_HW_KEY, String(clamped));
  notify();
}

export function onMiniGridChange(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

export const MINI_GRID_RADIUS_PX_DEFAULT = DEFAULT_RADIUS_PX;
export const MINI_GRID_RADIUS_PX_MIN = MIN_RADIUS_PX;
export const MINI_GRID_RADIUS_PX_MAX = MAX_RADIUS_PX;
export const MINI_GRID_CELLS_DEFAULT = DEFAULT_CELLS;
export const MINI_GRID_CELLS_MIN = MIN_CELLS;
export const MINI_GRID_CELLS_MAX = MAX_CELLS;
export const MINI_GRID_LINE_HW_PX_DEFAULT = DEFAULT_LINE_HW_PX;
export const MINI_GRID_LINE_HW_PX_MIN = MIN_LINE_HW_PX;
export const MINI_GRID_LINE_HW_PX_MAX = MAX_LINE_HW_PX;
