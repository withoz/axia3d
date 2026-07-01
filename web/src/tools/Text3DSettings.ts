/**
 * Text3DSettings — 3D 텍스트 도구 렌더 모드 토글 (ADR-228).
 *
 *  'extruded' — Three.js TextGeometry 진짜 3D 압출 텍스트 (Latin, lazy OFL font).
 *               폰트에 없는 글자(예: 한국어)는 sprite 로 자동 fallback (graceful).
 *  'sprite'   — Canvas 텍스처 billboard 라벨 (한국어 즉시, 번들 0, 카메라 대면).
 *
 * 두 형태 모두 **render-only Reference** (메타-원칙 #2 — 형태/모양만, 엔진 DCEL
 * 미주입). localStorage 'axia:text3d-mode' 에 저장, default 'extruded' (도구 이름
 * "3D 텍스트" 의미 부합). AutoIntersectSettings 패턴 답습.
 */

export type Text3DMode = 'extruded' | 'sprite';

const STORAGE_KEY = 'axia:text3d-mode';

let current: Text3DMode = 'extruded';
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === 'sprite' || saved === 'extruded') current = saved;
} catch {
  /* private mode */
}

const listeners = new Set<(mode: Text3DMode) => void>();

export function getText3DMode(): Text3DMode {
  return current;
}

export function setText3DMode(mode: Text3DMode): void {
  if (current === mode) return;
  current = mode;
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    /* ignore */
  }
  for (const cb of listeners) cb(mode);
}

export function onText3DModeChange(cb: (mode: Text3DMode) => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
