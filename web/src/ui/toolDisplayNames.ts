/**
 * toolDisplayNames — SSOT for the human-friendly names shown in the status-bar
 * command indicator (#tool-label).
 *
 * Before this module, four call sites (main.ts, KeyboardShortcuts.ts,
 * MenuBar.ts, ContextMenu.ts) each carried their own partial `toolNames` /
 * `viewNames` map. They drifted: most tools (plane, arc, bezier, polygon, …)
 * were missing everywhere, so the label fell back to the raw tool id and
 * showed e.g. lowercase "plane" instead of "Work Plane" (메타-원칙 #4 SSOT
 * 위반). This is the single source every site now imports.
 *
 * Keep values in sync with the tool ids registered in
 * ToolManagerRefactored (`this.tools.set('<id>', …)`). `toolDisplayName`
 * falls back to the raw id for any unknown id so nothing ever throws.
 */

/** Friendly names for every tool id registered in the ToolManager. */
export const TOOL_DISPLAY_NAMES: Record<string, string> = {
  // Selection
  select: 'Select',
  // 2D draw
  line: 'Line',
  polyline: 'Polyline',
  rect: 'Rectangle',
  rotrect: 'Rotated Rectangle',
  circle: 'Circle',
  ellipse: 'Ellipse',
  arc: 'Arc',
  pie: 'Pie',
  polygon: 'Polygon',
  'polygon-hole': 'Polygon Hole',
  freehand: 'Freehand',
  bezier: 'Bezier',
  spline: 'Spline',
  centerline: 'Centerline',
  point: 'Point',
  text3d: '3D Text',
  // Direct edit
  pushpull: 'Extrude/Cut',
  move: 'Move',
  rotate: 'Rotate',
  scale: 'Scale',
  offset: 'Offset',
  recess: 'Recess',
  hole: 'Hole',
  erase: 'Erase',
  copy: 'Copy',
  mirror: 'Mirror',
  'array-linear': 'Linear Array',
  'array-radial': 'Radial Array',
  // Edge/face ops
  fillet: 'Fillet',
  chamfer: 'Chamfer',
  'corner-fillet': 'Corner Fillet',
  'corner-chamfer': 'Corner Chamfer',
  trim: 'Trim',
  extend: 'Extend',
  split: 'Split',
  join: 'Join',
  slice: 'Slice',
  loft: 'Loft',
  sweep: 'Sweep',
  boundary: 'Boundary',
  // Work plane
  plane: 'Work Plane',
  // Primitives
  box: 'Box',
  sphere: 'Sphere',
  cylinder: 'Cylinder',
  cone: 'Cone',
  torus: 'Torus',
  // Architectural
  wall: 'Wall',
  window: 'Window',
  // Organization
  group: 'Group',
  // Measure / annotate
  measure: 'Measure',
  dimension: 'Dimension',
  'angular-dimension': 'Angular Dim',
  'radial-dimension': 'Radial Dim',
  'reference-dimension': 'Reference Dim',
  // NURBS
  nurbs: 'NURBS',
  'nurbs-edit': 'NURBS Edit',
};

/** Friendly names for the camera view modes (also shown in #tool-label). */
export const VIEW_DISPLAY_NAMES: Record<string, string> = {
  '3d': '3D Perspective',
  top: 'Top (XY)',
  bottom: 'Bottom (XY)',
  front: 'Front (XZ)',
  back: 'Back (XZ)',
  right: 'Right (YZ)',
  left: 'Left (YZ)',
};

/** Resolve a tool id to its friendly name, falling back to the raw id. */
export function toolDisplayName(tool: string): string {
  return TOOL_DISPLAY_NAMES[tool] ?? tool;
}

/** Resolve a view-mode id to its friendly name, falling back to the raw id. */
export function viewDisplayName(mode: string): string {
  return VIEW_DISPLAY_NAMES[mode] ?? mode;
}
