/**
 * NurbsPatchSettings — DrawNurbsTool 패치 종류 토글 (ADR-231).
 *
 *  'bezier' — 4×4 uniform bicubic Bezier patch (현재 MVP, ADR-224). 평평한 경계 +
 *             가운데 bulge "pillow dome" (다항식 근사).
 *  'vault'  — rational half-cylinder vault: degree-2 rational arc (정확한 반원,
 *             weights [1, 1/√2, 1, 1/√2, 1]) × degree-1 linear extrude → 정확한
 *             원통 곡면 (bridge.createNurbsSurface, AnalyticSurface::NURBSSurface).
 *             uniform Bezier 로는 표현 불가능한 EXACT conic surface.
 *
 * 두 모드 모두 form-layer kernel-native face (메타-원칙 #14). localStorage
 * 'axia:nurbs-patch-mode' 에 저장, default 'bezier' (기존 동작 보존). draggable
 * control-net + per-CP weight 편집은 future (ADR-231 §후속). Text3DSettings 패턴 답습.
 */

export type NurbsPatchMode = 'bezier' | 'vault';

const STORAGE_KEY = 'axia:nurbs-patch-mode';

let current: NurbsPatchMode = 'bezier';
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === 'vault' || saved === 'bezier') current = saved;
} catch {
  /* private mode */
}

const listeners = new Set<(mode: NurbsPatchMode) => void>();

export function getNurbsPatchMode(): NurbsPatchMode {
  return current;
}

export function setNurbsPatchMode(mode: NurbsPatchMode): void {
  if (current === mode) return;
  current = mode;
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    /* ignore */
  }
  for (const cb of listeners) cb(mode);
}

export function onNurbsPatchModeChange(cb: (mode: NurbsPatchMode) => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
