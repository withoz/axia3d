/**
 * ExtrudeModeSettings — ADR-261 β-3 (2026-06-26).
 *
 * Push/Pull extrude **mode** toggle (AixiAcad `ExtrudeMode` parity, Q3 결재):
 * - `oneway`    — 기존 단방향 (+normal). default, 동작 불변.
 * - `symmetric` — 대칭 양방향: 각 방향 d (profile 평면이 대칭면, 총 두께 2d).
 *                 `(dist_pos, dist_neg) = (d, d)`.
 * - `twosided`  — 비대칭: +normal `d` (VCB/drag) + −normal `distNeg` (이 설정).
 *
 * `PushPullTool` 이 commit 시점에 `getExtrudeMode()` 를 읽어 `oneway` 가 아니면
 * `bridge.createSolidExtrudeBidirectional(faceId, dp, distNeg)` 로 라우팅
 * (ADR-261 β-2). VCB 문법 (`거리,각도`=taper / `거리,비율%`=cone) 은 mode 와
 * 독립 — comma 입력은 mode 보다 우선 (명시 op).
 *
 * AutoFaceSynthesisSettings 패턴 답습. localStorage 보존, default `oneway`
 * (기존 동작 불변, ADR-046 P31 #4 additive). live-drag preview 는 v1 에서
 * 단방향 (commit 시점에 mode 적용 — live bidirectional preview 후속).
 */

export type ExtrudeMode = 'oneway' | 'symmetric' | 'twosided';

const MODE_KEY = 'axia:extrude-mode';
const DIST_NEG_KEY = 'axia:extrude-dist-neg';

let currentMode: ExtrudeMode = 'oneway';
let currentDistNeg = 0; // mm — TwoSided 의 −normal 방향 거리 (≥ 0)

try {
  const m = localStorage.getItem(MODE_KEY);
  if (m === 'oneway' || m === 'symmetric' || m === 'twosided') currentMode = m;
  const d = localStorage.getItem(DIST_NEG_KEY);
  if (d !== null) {
    const v = parseFloat(d);
    if (Number.isFinite(v) && v >= 0) currentDistNeg = v;
  }
} catch {
  /* private mode */
}

const listeners = new Set<() => void>();

export function getExtrudeMode(): ExtrudeMode {
  return currentMode;
}

export function setExtrudeMode(value: ExtrudeMode): void {
  if (currentMode === value) return;
  currentMode = value;
  try {
    localStorage.setItem(MODE_KEY, value);
  } catch {
    /* ignore */
  }
  for (const cb of listeners) cb();
}

export function getExtrudeDistNeg(): number {
  return currentDistNeg;
}

export function setExtrudeDistNeg(value: number): void {
  const v = Number.isFinite(value) && value >= 0 ? value : 0;
  if (currentDistNeg === v) return;
  currentDistNeg = v;
  try {
    localStorage.setItem(DIST_NEG_KEY, String(v));
  } catch {
    /* ignore */
  }
  for (const cb of listeners) cb();
}

export function onExtrudeModeChange(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}
