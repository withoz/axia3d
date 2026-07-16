import { describe, it, expect, vi, beforeEach } from 'vitest';
import { setLocale } from '../i18n';
import {
  TOOL_DISPLAY_NAMES,
  VIEW_DISPLAY_NAMES,
  toolDisplayName,
  viewDisplayName,
} from './toolDisplayNames';

/**
 * These names are `t()` keys as of 2026-07-16 (ADR-294 batch 13), so this file
 * is locale-dependent where it used to be constant. The static import above
 * evaluates the map once, at the locale jsdom starts in — `navigator.language`
 * is 'en-US', so the English assertions below are what the module holds. That
 * is deliberate, not luck: the English values are the ones other call sites and
 * tests already assert, so they are the regression surface. The Korean side
 * needs a fresh module (the map is built at import time, which is exactly why
 * D7 reloads the page on a locale switch) and is checked at the bottom.
 */
const REGISTERED_TOOL_IDS = [
  'angular-dimension', 'arc', 'array-linear', 'array-radial', 'bezier',
  'boundary', 'box', 'centerline', 'chamfer', 'circle', 'cone', 'copy',
  'corner-chamfer', 'corner-fillet', 'cylinder', 'dimension', 'ellipse',
  'erase', 'extend', 'fillet', 'freehand', 'group', 'hole', 'join', 'line',
  'loft', 'measure', 'mirror', 'move', 'nurbs', 'nurbs-edit', 'offset', 'pie',
  'plane', 'point', 'polygon', 'polygon-hole', 'polyline', 'pushpull',
  'radial-dimension', 'recess', 'rect', 'reference-dimension', 'rotate',
  'rotrect', 'scale', 'select', 'slice', 'sphere', 'spline', 'split', 'sweep',
  'text3d', 'torus', 'trim', 'wall', 'window',
];

describe('toolDisplayNames (status-bar command indicator SSOT)', () => {
  it('fixes the original bug: plane → a name, not the raw "plane"', () => {
    // 'Work plane' (lower p) because the key 「작업 평면」 was already in en.ts
    // for the menu — a tool name is the same word the menu uses, and reusing
    // the key is what stops the two from drifting apart again.
    expect(toolDisplayName('plane')).toBe('Work plane');
  });

  it('preserves the values other call sites / tests assert', () => {
    expect(toolDisplayName('select')).toBe('Select');
    expect(toolDisplayName('line')).toBe('Line');
    expect(toolDisplayName('rect')).toBe('Rectangle');
    // Not a t() key: identical in both locales, and D2 keys on source text.
    expect(toolDisplayName('pushpull')).toBe('Extrude/Cut');
  });

  it('has a friendly name for EVERY registered tool id (no raw-id leak)', () => {
    const missing = REGISTERED_TOOL_IDS.filter(id => TOOL_DISPLAY_NAMES[id] === undefined);
    expect(missing).toEqual([]);
  });

  it('never returns a name equal to the raw id for a registered tool', () => {
    const raw = REGISTERED_TOOL_IDS.filter(id => toolDisplayName(id) === id);
    // A name identical to the id would look like the un-fixed lowercase label.
    expect(raw).toEqual([]);
  });

  it('falls back to the raw id for an unknown tool (never throws)', () => {
    expect(toolDisplayName('no-such-tool')).toBe('no-such-tool');
  });

  it('resolves the view modes shown in #tool-label', () => {
    expect(viewDisplayName('3d')).toBe('3D view');
    expect(viewDisplayName('top')).toBe('Top (XY)');
    expect(viewDisplayName('left')).toBe('Left (YZ)');
    expect(Object.keys(VIEW_DISPLAY_NAMES).sort()).toEqual(
      ['3d', 'back', 'bottom', 'front', 'left', 'right', 'top'],
    );
  });

  it('falls back to the raw mode for an unknown view (never throws)', () => {
    expect(viewDisplayName('iso')).toBe('iso');
  });
});

describe('toolDisplayNames under ko', () => {
  beforeEach(() => {
    setLocale('ko');
    // The map is built once, at import. Without a fresh module we would be
    // re-reading the English values the static import already froze — the test
    // would pass while proving nothing.
    vi.resetModules();
  });

  it('shows Korean names, which is the whole point of batch 13', async () => {
    const m = await import('./toolDisplayNames');
    // The bug: a Korean user clicked 「사각형」 and the status bar said
    // "Rectangle", because these were hard-coded English.
    expect(m.toolDisplayName('rect')).toBe('사각형');
    expect(m.toolDisplayName('plane')).toBe('작업 평면');
    expect(m.viewDisplayName('top')).toBe('평면도 (XY)');
  });

  it('leaves the id fallback and the un-keyed name alone', async () => {
    const m = await import('./toolDisplayNames');
    expect(m.toolDisplayName('no-such-tool')).toBe('no-such-tool');
    expect(m.toolDisplayName('pushpull')).toBe('Extrude/Cut');
  });
});
