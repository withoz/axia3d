/**
 * FaceRederiveSettings — ADR-186 (A) derived-face 모델 production 활성.
 *
 * "face_rederive_on_draw" — coplanar draw 시 boundary kernel re-derive
 * (analytic arrangement, ADR-186 (A)) 경로로 라우팅. 단일 명령으로
 * containment (annulus + smooth 곡선 hole) / overlap (sub-face) / 면 재유도를
 * 통합 처리 ("선만 그려, 케이크는 알아서 나뉜다").
 *
 * **production default ON** (사용자 결재 2026-06-02 "내가 그리면 안됨" —
 * bridge 데모는 동작했으나 UI 마우스 draw 는 flag OFF 라 미동작). engine
 * default 는 scene.rs 에서 OFF 유지 (회귀 자산 보존, ADR-049 P-5e-α
 * canonical). localStorage 'false' 명시 시 OFF preference 보존.
 *
 * AutoFaceSynthesisSettings / AutoIntersectSettings 패턴 답습.
 */

const STORAGE_KEY = 'axia:face-rederive-on-draw';

// production default ON. engine default 는 scene.rs 에서 OFF 유지.
let current = true;
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  // ADR-049 P-5e-α canonical: explicit OFF preference 보존
  if (saved === 'false') current = false;
} catch { /* private mode */ }

const listeners = new Set<(enabled: boolean) => void>();

export function getFaceRederive(): boolean {
  return current;
}

export function setFaceRederive(value: boolean): void {
  if (current === value) return;
  current = value;
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch { /* ignore */ }
  for (const cb of listeners) cb(value);
}

export function onFaceRederiveChange(cb: (enabled: boolean) => void): () => void {
  listeners.add(cb);
  return () => { listeners.delete(cb); };
}
