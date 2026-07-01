/**
 * FreeformOverlapSettings — ADR-186 A3 freeform overlap → smooth lens
 * production 활성.
 *
 * "freeform_overlap_on_draw" — 겹치는 freeform self-loop (Bezier/BSpline/
 * NURBS) 를 boundary kernel re-derive (analytic arrangement, curve-curve
 * CCI) 경로로 라우팅 → smooth lens sub-face 자동 분할. circle/rect overlap
 * 이 이미 자동 split 되듯 freeform 도 동일 ("선만 그려, 케이크는 알아서
 * 나뉜다" 의 freeform 확장).
 *
 * **의존**: `face_rederive_on_draw` 의 하위 branch — 둘 다 ON 이어야 효과
 * (rederive 훅 내부의 overlap-detection branch 만 enable). FaceRederive 는
 * 이미 production default ON 이므로 본 flag 도 default ON 이면 작동.
 *
 * **production default ON** (사용자 결재 2026-06-05, 구조 D1=(b) gated+flip).
 * engine default 는 scene.rs 에서 OFF 유지 (회귀 자산 보존, ADR-049 P-5e-α
 * canonical). localStorage 'false' 명시 시 OFF preference 보존.
 *
 * **Selective (D2=a)**: freeform×freeform only. mixed (bezier×rect/circle)
 * 는 B5 deferred (split 안 되지만 양쪽 보존 — 비-blocker).
 *
 * FaceRederiveSettings 패턴 1:1 답습.
 */

const STORAGE_KEY = 'axia:freeform-overlap-on-draw';

// production default ON. engine default 는 scene.rs 에서 OFF 유지.
let current = true;
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  // ADR-049 P-5e-α canonical: explicit OFF preference 보존
  if (saved === 'false') current = false;
} catch { /* private mode */ }

const listeners = new Set<(enabled: boolean) => void>();

export function getFreeformOverlap(): boolean {
  return current;
}

export function setFreeformOverlap(value: boolean): void {
  if (current === value) return;
  current = value;
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch { /* ignore */ }
  for (const cb of listeners) cb(value);
}

export function onFreeformOverlapChange(cb: (enabled: boolean) => void): () => void {
  listeners.add(cb);
  return () => { listeners.delete(cb); };
}
