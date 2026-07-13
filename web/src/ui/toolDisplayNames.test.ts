import { describe, it, expect } from 'vitest';
import {
  TOOL_DISPLAY_NAMES,
  VIEW_DISPLAY_NAMES,
  toolDisplayName,
  viewDisplayName,
} from './toolDisplayNames';

/**
 * Every tool id registered in ToolManagerRefactored (`this.tools.set('<id>', …)`).
 * Kept here as a drift guard: if a new tool is registered without a friendly
 * name, this list drives the "no raw-id leak" test below to fail so the status
 * bar never shows a lowercase raw id again (the original "plane" bug).
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
  it('fixes the original bug: plane → "Work Plane" (not raw "plane")', () => {
    expect(toolDisplayName('plane')).toBe('Work Plane');
  });

  it('preserves the values other call sites / tests assert', () => {
    expect(toolDisplayName('select')).toBe('Select');
    expect(toolDisplayName('line')).toBe('Line');
    expect(toolDisplayName('rect')).toBe('Rectangle');
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
    expect(viewDisplayName('3d')).toBe('3D Perspective');
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
