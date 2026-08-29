import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';
import {
  getMiniGridVisible, setMiniGridVisible,
  getMiniGridRadiusPx, setMiniGridRadiusPx,
  getMiniGridCells, setMiniGridCells,
  getMiniGridLineHwPx, setMiniGridLineHwPx,
  onMiniGridChange,
  MINI_GRID_RADIUS_PX_MIN, MINI_GRID_RADIUS_PX_MAX,
  MINI_GRID_CELLS_MIN, MINI_GRID_CELLS_MAX,
  MINI_GRID_LINE_HW_PX_MIN, MINI_GRID_LINE_HW_PX_MAX,
} from './MiniGridSettings';

/**
 * The cursor mini-grid: its settings, and the four links that make it appear.
 *
 * The geometry has its own file (`viewport/miniGridGeometry.test.ts`). What is
 * checked here is the wiring — a correct disc drawn by nobody is not a feature.
 */
const src = (rel: string) => readFileSync(join(__dirname, '..', rel), 'utf8');

describe('mini grid settings', () => {
  beforeEach(() => {
    setMiniGridVisible(true);
    setMiniGridRadiusPx(48);
    setMiniGridCells(4);
    setMiniGridLineHwPx(0.5);
  });

  it('clamps each value to the range it was specified with', () => {
    setMiniGridRadiusPx(10_000);
    expect(getMiniGridRadiusPx()).toBe(MINI_GRID_RADIUS_PX_MAX);
    setMiniGridRadiusPx(-5);
    expect(getMiniGridRadiusPx()).toBe(MINI_GRID_RADIUS_PX_MIN);

    setMiniGridCells(1000);
    expect(getMiniGridCells()).toBe(MINI_GRID_CELLS_MAX);
    setMiniGridCells(0);
    expect(getMiniGridCells()).toBe(MINI_GRID_CELLS_MIN);

    setMiniGridLineHwPx(99);
    expect(getMiniGridLineHwPx()).toBe(MINI_GRID_LINE_HW_PX_MAX);
    setMiniGridLineHwPx(0);
    expect(getMiniGridLineHwPx()).toBe(MINI_GRID_LINE_HW_PX_MIN);
  });

  it('rounds a cell count but not a half-width', () => {
    setMiniGridCells(7.6);
    expect(getMiniGridCells()).toBe(8);
    setMiniGridLineHwPx(1.3);
    expect(getMiniGridLineHwPx()).toBeCloseTo(1.3, 9);
  });

  it('ignores a value that is not a number rather than storing NaN', () => {
    setMiniGridRadiusPx(Number.NaN);
    expect(getMiniGridRadiusPx()).toBe(48);
    setMiniGridCells(Number.NaN);
    expect(getMiniGridCells()).toBe(4);
    setMiniGridLineHwPx(Number.NaN);
    expect(getMiniGridLineHwPx()).toBeCloseTo(0.5, 9);
  });

  it('tells a listener when something changed, and only then', () => {
    let n = 0;
    const off = onMiniGridChange(() => { n++; });
    setMiniGridRadiusPx(60);
    expect(n).toBe(1);
    setMiniGridRadiusPx(60); // same value
    expect(n).toBe(1);
    setMiniGridVisible(false);
    expect(n).toBe(2);
    off();
    setMiniGridRadiusPx(70);
    expect(n).toBe(2);
  });
});

/**
 * ⚠ Source-level, because the four links are what a unit test of the module
 * cannot see. Each assertion names the file it is about, so a rename fails here
 * rather than silently unhooking the feature.
 */
describe('mini grid wiring', () => {
  it('the viewport has a method to draw it, and disposes what it owns', () => {
    const v = src('viewport/Viewport.ts');
    expect(v).toContain('setMiniGridCursor(');
    // Both projections — the ortho branch is the one Kayac does not have.
    expect(v).toContain('perspectiveRadiusWorld(');
    expect(v).toContain('orthographicRadiusWorld(');
    // A LineSegments2 and a LineMaterial are created, so both must be released.
    expect(v).toContain('this._miniGridMat?.dispose()');
  });

  it('the tool manager calls it where the plane indicator is already computed', () => {
    const t = src('tools/ToolManagerRefactored.ts');
    const flush = t.slice(t.indexOf('private flushDrawPlaneIndicator'), 2000 + t.indexOf('private flushDrawPlaneIndicator'));

    // ⚠ Ask for the call that DRAWS, not just for the name. The clearing calls
    // in the same method mention `setMiniGridCursor` too, so an earlier version
    // of this line passed with the drawing call deleted — mutation-checked, and
    // it did.
    expect(flush, 'the mini grid is never given a centre and a plane').toMatch(
      /setMiniGridCursor\?\.\(\s*origin\s*,/,
    );
    // and it is cleared on EVERY path out of the flush that draws nothing —
    // wrong tool, tool busy, no cursor point. Counting rather than containing,
    // because with three of them `toContain` passes with two deleted.
    const cleared = flush.match(/setMiniGridCursor\?\.\(null\)/g) ?? [];
    expect(cleared.length, 'a path out of the flush leaves a stale grid on screen').toBe(3);

    // ⚠ and when the pointer leaves the canvas it HIDES rather than forgets —
    // reaching for the grid's own sliders means leaving the canvas, so the frame
    // has to survive the trip or there is nothing left to preview.
    const leave = t.slice(t.indexOf("addEventListener('mouseleave'"));
    expect(leave.slice(0, 700), 'leaving the canvas forgets the frame the sliders preview')
      .toContain('hideMiniGrid?.()');
  });

  it('the panel is an exception, and it is bounded by open and close', () => {
    const v = src('viewport/Viewport.ts');
    // Two different verbs, or there is no way to hide without forgetting.
    expect(v, 'nothing can hide the disc without forgetting where it was')
      .toContain('hideMiniGrid()');
    expect(v, 'nothing can bring it back').toContain('refreshMiniGrid()');

    const m = src('main.ts');
    // ⚠ Both directions. Opening without closing leaves a stale disc on screen
    // whenever the panel is dismissed with the pointer off the canvas.
    expect(m, 'the panel does not tell anyone it opened').toMatch(/onToggle:\s*\(open\)/);
    expect(m, 'opening the panel does not bring the disc back').toMatch(
      /if \(open\) viewport\.refreshMiniGrid\(\)/,
    );
    expect(m, 'closing the panel does not forget the disc').toMatch(
      /else viewport\.setMiniGridCursor\?\.\(null\)/,
    );

    const sp = src('units/SettingsPanel.ts');
    expect(sp, 'open() never fires the callback').toContain('this.deps.onToggle?.(true)');
    expect(sp, 'close() never fires the callback').toContain('this.deps.onToggle?.(false)');
  });

  it('a tool switch clears it, which no pointer event would', () => {
    const t = src('tools/ToolManagerRefactored.ts');
    // ⚠ `setTool` is not reached by the mousemove flush, so without a clear here
    // the disc sat on screen after switching to a non-drawing tool until the
    // mouse next moved. The old fixed-size gizmo had this and the grid did not.
    const at = t.indexOf('setTool(name: string)');
    expect(at, 'setTool was renamed — retarget this guard').toBeGreaterThan(-1);
    const body = t.slice(at, at + 2500);
    expect(body, 'setTool never clears the cursor grid').toMatch(
      /DRAW_PLANE_TOOLS\.has\(name\)\)\s*\{\s*[\s\S]{0,200}?setMiniGridCursor\?\.\(null\)/,
    );
  });

  it('the settings panel reaches the disc without waiting for a mouse move', () => {
    // ⚠ `MiniGridSettings` notifies listeners; for a while nobody listened, so
    // unticking the box left the grid up and a radius change did nothing until
    // the pointer moved. Both halves of the link are asserted: someone
    // subscribes, and the viewport can redraw from the frame it last had.
    const m = src('main.ts');
    expect(m, 'nothing subscribes to onMiniGridChange').toContain('onMiniGridChange(');
    expect(m, 'the subscription does not redraw the grid').toMatch(
      /onMiniGridChange\(\s*\(\)\s*=>\s*[\s\S]{0,80}?refreshMiniGrid\(\)/,
    );
    const v = src('viewport/Viewport.ts');
    expect(v, 'the viewport cannot redraw without a new pointer event').toContain(
      'refreshMiniGrid()',
    );
    expect(v, 'the last cursor frame is not remembered, so a redraw has nothing to use')
      .toContain('_miniGridLast');
  });

  it('the settings panel offers all four controls', () => {
    const p = src('units/SettingsPanel.ts');
    for (const id of [
      'sp-mini-grid',
      'sp-mini-grid-radius',
      'sp-mini-grid-cells',
      'sp-mini-grid-hw',
    ]) {
      expect(p, `#${id} is missing from the panel`).toContain(`id="${id}"`);
    }
    // and each one is actually read
    for (const fn of [
      'setMiniGridVisible',
      'setMiniGridRadiusPx',
      'setMiniGridCells',
      'setMiniGridLineHwPx',
    ]) {
      expect(p, `${fn} is never called`).toContain(`${fn}(`);
    }
  });

  it('its controls look like every other control in the panel', () => {
    const p = src('units/SettingsPanel.ts');

    // ⚠ The checkbox idiom. Nine of the panel's ten checkboxes are
    // `<label class="sp-label"><input type="checkbox" .../> text</label>`; the
    // cursor grid arrived with an `sp-toggle` class that is styled NOWHERE in the
    // project, and put the text before the box. One of ten is drift, not a
    // second idiom.
    expect(p, 'sp-toggle is a class this project does not style').not.toContain('sp-toggle');
    expect(p, 'the cursor-grid checkbox does not follow the panel idiom').toMatch(
      /<label class="sp-label">\s*<input type="checkbox" id="sp-mini-grid" \/>/,
    );

    // One `sp-section` per control, like every sibling. The four arrived crammed
    // into a single section, which is why they read as a sub-panel instead of as
    // four more rows of the same list.
    const sections = p.split('<div class="sp-section">');
    for (const id of [
      'sp-mini-grid',
      'sp-mini-grid-radius',
      'sp-mini-grid-cells',
      'sp-mini-grid-hw',
    ]) {
      const owning = sections.filter((sec) => sec.includes(`id="${id}"`));
      expect(owning.length, `#${id} is missing from the panel`).toBe(1);
    }
    // ...and no section holds two of them. ⚠ Count the INPUTS: each slider's
    // section also carries its `-val` readout span, so matching ids alone finds
    // two per section and reports crowding that is not there. (It did.)
    const crowded = sections.filter(
      (sec) => (sec.match(/<input[^>]*id="sp-mini-grid[a-z-]*"/g) ?? []).length > 1,
    );
    expect(crowded.length, 'two cursor-grid controls share one section').toBe(0);

    // Each slider is a `sp-row` with a `sp-value` readout, as the panel's other
    // sliders are — the readout is how the user knows what they picked.
    for (const id of ['sp-mini-grid-radius', 'sp-mini-grid-cells', 'sp-mini-grid-hw']) {
      expect(p, `#${id} has no value readout`).toContain(`id="${id}-val" class="sp-value"`);
    }
  });

  it('every Korean string it added has an English side', () => {
    const en = src('i18n/en.ts');
    for (const k of [
      '커서 그리드 표시',
      '커서 그리드 반지름 (px)',
      '커서 그리드 칸 수 (중심→가장자리)',
      '커서 그리드 선 반폭 (px)',
    ]) {
      expect(en, `'${k}' has no entry in en.ts`).toContain(`'${k}'`);
    }
  });
});
