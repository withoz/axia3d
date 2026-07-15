/**
 * ADR-190 Phase 3 — turn the kernel's own vocabulary into something a modeller
 * can act on.
 *
 * `lastError()` carries the engine's error chain verbatim (`set_error(e.to_
 * string())`), which is right for logs and wrong for a Toast. Measured through
 * the production bridge, these are what users actually see today:
 *
 *   push a cylinder's side face
 *     → "Face needs at least 3 verts"
 *   taper a curved profile
 *     → "create_solid: not yet supported — tapered extrude v1 supports (Plane,
 *        AllLinear) only (ADR-259 D2) (Q3 fallback to legacy push_pull)"
 *   cone with 100% top
 *     → "create_solid: not yet supported — cone extrude: top_scale ≥ 1 is a
 *        cylinder — use straight Extrude (ADR-260 D2 [0,1)) (Q3 fallback to
 *        legacy push_pull)"
 *   a stale face id
 *     → "create_solid: face FaceId(3) not found or inactive"
 *
 * ADR numbers, Rust type names, internal enum variants and "Q3 fallback to
 * legacy push_pull" are implementation trivia leaking through the UI.
 *
 * Humanizing at the boundary — not in the engine — follows the pattern already
 * established here (`humanizeBoundaryError`, `humanizeDamageReport`,
 * `humanizeOrphanReport`; ADR-095 §E L3, ADR-100 L7): the engine keeps its
 * precise vocabulary for logs and tests, and the UI speaks the user's.
 *
 * UNKNOWN MESSAGES ARE NOT SWALLOWED. A whitelist that dropped anything it did
 * not recognise would trade one silence for another; unknown text is passed
 * through with only the internal noise stripped.
 */

/** Internal trivia that helps nobody outside the repo. */
function stripInternals(raw: string): string {
  return raw
    .replace(/\s*\(Q3 fallback to legacy push_pull\)/g, '')
    .replace(/\s*\(ADR-\d+[^)]*\)/g, '')
    .replace(/^(create_solid|createSolidExtrude|push_pull|boundaryFromPoint):\s*/, '')
    .replace(/\s*—\s*not yet supported\s*—\s*/, ' — ')
    .replace(/^not yet supported\s*—\s*/, '')
    .replace(/FaceId\((\d+)\)/g, '면 #$1')
    .trim();
}

/**
 * Map a raw engine error onto user language, saying what to do instead where
 * the engine's own text does not. Unknown input falls through `stripInternals`.
 */
export function humanizeEngineError(raw: string): string {
  const msg = (raw ?? '').trim();
  if (msg.length === 0) return '';

  // A closed curve's face is 1 anchor + 1 self-loop edge (ADR-089), so a
  // polygon-boundary op rejects it on vertex count. What the user did: tried to
  // push a curved side wall directly.
  if (msg.includes('Face needs at least 3 verts')) {
    return '곡면은 직접 밀 수 없습니다 — 곡면 위에 원을 그린 뒤 그 면을 미세요';
  }

  // Taper/cone v1 accept a flat, straight-edged profile only.
  if (msg.includes('tapered extrude') && msg.includes('Plane')) {
    return '테이퍼(draft)는 직선 경계의 평면 프로파일만 지원합니다 (곡선/곡면 미지원)';
  }
  if (msg.includes('top_scale') && msg.includes('cylinder')) {
    return '위 지름 비율이 100% 이면 원기둥입니다 — 비율 없이 그냥 미세요';
  }
  if (msg.includes('cone extrude') && msg.includes('AllCircular')) {
    return '콘(비율) 돌출은 원형 프로파일만 지원합니다';
  }

  // Stale / wrong pick.
  if (msg.includes('not found or inactive')) {
    return '그 면을 찾을 수 없습니다 — 다시 선택해 주세요';
  }

  // The curved ops already name the surfaces; drop the "cap"/"surface face"
  // jargon and say what to draw.
  if (msg.includes('cap must be a Cylinder/Sphere/Cone/Torus-surface face')) {
    return '곡면 포켓/보스는 곡면 위에 그린 원에서만 만듭니다';
  }

  return stripInternals(msg);
}
