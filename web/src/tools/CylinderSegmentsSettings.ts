/**
 * CylinderSegmentsSettings — 원통(Cylinder) 프리미티브 생성 시 원주 세그먼트 수.
 *
 * 기본값: 16 (기존 CylinderTool 하드코딩 값과 동일 → 미터치 시 동작 변화 없음).
 * 사용자가 설정 패널에서 조정하면 localStorage 에 저장되고 CylinderTool.commit
 * 이 create_cylinder 호출 시 사용.
 *
 * 범위: 3 ~ 128. 세그먼트가 많을수록 매끈하지만 face/vert 수 증가 (메타-원칙 #12
 * Memory Budget). 곡면 cut(ADR-271) 의 cap 경계 정밀도에도 영향.
 */

const STORAGE_KEY = 'axia:cylinder:segments';
const DEFAULT_SEGMENTS = 16;
const MIN_SEGMENTS = 3;
const MAX_SEGMENTS = 128;

let current = DEFAULT_SEGMENTS;

// 초기 로드 — localStorage
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved !== null) {
    const v = parseInt(saved, 10);
    if (Number.isFinite(v) && v >= MIN_SEGMENTS && v <= MAX_SEGMENTS) current = v;
  }
} catch {
  /* private mode */
}

const listeners = new Set<(segments: number) => void>();

/** 현재 원통 세그먼트 수 (3 ~ 128, 정수). */
export function getCylinderSegments(): number {
  return current;
}

/** 원통 세그먼트 수 설정 — 3~128 로 clamp + 정수화, localStorage 저장. */
export function setCylinderSegments(value: number): void {
  if (!Number.isFinite(value)) return;
  const clamped = Math.max(MIN_SEGMENTS, Math.min(MAX_SEGMENTS, Math.round(value)));
  if (clamped === current) return;
  current = clamped;
  try {
    localStorage.setItem(STORAGE_KEY, String(clamped));
  } catch {
    /* ignore */
  }
  for (const fn of listeners) fn(clamped);
}

export function onCylinderSegmentsChange(fn: (segments: number) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export const CYLINDER_SEGMENTS_DEFAULT = DEFAULT_SEGMENTS;
export const CYLINDER_SEGMENTS_MIN = MIN_SEGMENTS;
export const CYLINDER_SEGMENTS_MAX = MAX_SEGMENTS;
