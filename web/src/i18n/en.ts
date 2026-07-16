/**
 * ADR-294 — English strings, keyed by the Korean source text (D2).
 *
 * There is deliberately no `ko.ts`: Korean is the key, so `ko` is the identity
 * function. A key missing from this table renders Korean — which is exactly
 * today's behaviour, so a batch can be wrapped before it is translated.
 *
 * Keep `{name}` placeholders EXACTLY as they appear in the key. They are the
 * one part of a string that must not be translated.
 *
 * Grouped by the migration batch that introduced them (ADR-294 §3), not by
 * feature — so a reviewer can see what a batch actually touched.
 */
export const EN: Record<string, string> = {
  // ── batch 1 — humanizeEngineError (ADR-190 Phase 3) ──
  '곡면은 직접 밀 수 없습니다 — 곡면 위에 원을 그린 뒤 그 면을 미세요':
    'A curved wall cannot be pushed directly — draw a circle on it first, then push that face.',
  '테이퍼(draft)는 직선 경계의 평면 프로파일만 지원합니다 (곡선/곡면 미지원)':
    'Draft extrude supports flat, straight-edged profiles only (no curves or curved surfaces).',
  '위 지름 비율이 100% 이면 원기둥입니다 — 비율 없이 그냥 미세요':
    'A top ratio of 100% is a cylinder — push without a ratio instead.',
  '콘(비율) 돌출은 원형 프로파일만 지원합니다':
    'Cone (ratio) extrude supports circular profiles only.',
  '그 면을 찾을 수 없습니다 — 다시 선택해 주세요':
    'That face no longer exists — please select it again.',
  '곡면 포켓/보스는 곡면 위에 그린 원에서만 만듭니다':
    'Curved pockets and bosses are made from a circle drawn on the curved surface.',
  '이 위치에는 스케치할 수 없습니다 — 기존 구멍/포켓 경계와 겹칩니다 (모델은 그대로입니다)':
    'Cannot sketch here — it overlaps an existing hole or pocket rim. Your model is unchanged.',
  '이 작업은 모델을 깨뜨려서 취소했습니다 — 모델은 그대로입니다':
    'That operation would have broken the model, so it was cancelled. Your model is unchanged.',
};
