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
 * The names were English-only until 2026-07-16, so a Korean user picked
 * 「사각형」 from the toolbar and the status bar answered "Rectangle". The
 * SSOT had drifted from AxiaCommands in *language*: the palette said
 * 「선 (Line)」, this file said "Line". The i18n survey never saw it — the
 * scanner looks for raw Korean, and an English hard-coded string has none.
 *
 * The Korean here is not newly invented: it is the AxiaCommands label with
 * its parenthetical dropped (「선 (Line)」 → 「선」), so the two SSOTs cannot
 * disagree. Where the label carried a 「… 도구」 suffix or a 3-click note, the
 * status bar takes the short form. Values are `t()` keys per ADR-294 D2, and
 * 52 of the 64 already existed in en.ts — a tool name is the same word the
 * menu already uses, so reusing the key is what keeps them in sync.
 *
 * `t()` runs at module scope, which is safe by D6: `i18n/index.ts` resolves
 * the locale during its own module evaluation, before any importer's body
 * runs, and D7 reloads the page on a locale switch.
 *
 * Keep values in sync with the tool ids registered in
 * ToolManagerRefactored (`this.tools.set('<id>', …)`). `toolDisplayName`
 * falls back to the raw id for any unknown id so nothing ever throws.
 */
import { t } from '../i18n';

/** Friendly names for every tool id registered in the ToolManager. */
export const TOOL_DISPLAY_NAMES: Record<string, string> = {
  // Selection
  select: t('선택'),
  // 2D draw
  line: t('선'),
  polyline: t('폴리선'),
  rect: t('사각형'),
  rotrect: t('회전 사각형'),
  circle: t('원'),
  ellipse: t('타원'),
  arc: t('호'),
  pie: t('부채꼴'),
  polygon: t('다각형'),
  'polygon-hole': t('다각형 구멍'),
  freehand: t('자유선'),
  bezier: t('Bezier 곡선'),
  spline: t('스플라인'),
  centerline: t('중심선'),
  point: t('점'),
  text3d: t('3D 텍스트'),
  // Direct edit
  // Not wrapped: the same in both locales, and D2 keys on the source text —
  // there is no Korean here to key on. AxiaCommands spells it this way too.
  pushpull: 'Extrude/Cut',
  move: t('이동'),
  rotate: t('회전'),
  scale: t('크기'),
  offset: t('오프셋'),
  recess: t('포켓'),
  hole: t('구멍'),
  erase: t('삭제'),
  copy: t('복제'),
  mirror: t('미러'),
  'array-linear': t('선형 배열'),
  'array-radial': t('원형 배열'),
  // Edge/face ops
  fillet: t('필렛'),
  chamfer: t('꼭짓점 챔퍼'),
  'corner-fillet': t('코너 필렛'),
  'corner-chamfer': t('코너 챔퍼'),
  trim: t('트림'),
  extend: t('익스텐드'),
  split: t('분할'),
  join: t('조인'),
  slice: t('슬라이스'),
  loft: t('로프트'),
  sweep: t('스윕'),
  boundary: t('경계'),
  // Work plane
  plane: t('작업 평면'),
  // Primitives
  box: t('박스'),
  sphere: t('구'),
  cylinder: t('원통'),
  cone: t('원뿔'),
  torus: t('토러스'),
  // Architectural
  wall: t('벽'),
  window: t('창'),
  // Organization
  group: t('그룹'),
  // Measure / annotate
  measure: t('측정'),
  dimension: t('선형 치수'),
  'angular-dimension': t('각도 치수'),
  'radial-dimension': t('반지름 치수'),
  'reference-dimension': t('참조 치수'),
  // NURBS
  nurbs: t('NURBS 곡면'),
  'nurbs-edit': t('NURBS 편집'),
};

/**
 * Friendly names for the camera view modes (also shown in #tool-label).
 *
 * The Korean is the architectural drawing vocabulary AxiaCommands already
 * uses (평면도 / 정면도 / 저면도), not a literal rendering of "Top". The axis
 * hint stays: with Z-up (LOCKED #43) "Top" being XY is not self-evident.
 */
export const VIEW_DISPLAY_NAMES: Record<string, string> = {
  '3d': t('3D 뷰'),
  top: t('평면도 (XY)'),
  bottom: t('저면도 (XY)'),
  front: t('정면도 (XZ)'),
  back: t('배면도 (XZ)'),
  right: t('우측면도 (YZ)'),
  left: t('좌측면도 (YZ)'),
};

/** Resolve a tool id to its friendly name, falling back to the raw id. */
export function toolDisplayName(tool: string): string {
  return TOOL_DISPLAY_NAMES[tool] ?? tool;
}

/** Resolve a view-mode id to its friendly name, falling back to the raw id. */
export function viewDisplayName(mode: string): string {
  return VIEW_DISPLAY_NAMES[mode] ?? mode;
}
