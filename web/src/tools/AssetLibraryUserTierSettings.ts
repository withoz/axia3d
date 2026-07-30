/**
 * AssetLibraryUserTierSettings — ADR-098 S-ε.
 *
 * `axia:asset-library-user-tier` — User tier 자산 라이브러리 활성화
 * 여부. **Default OFF** (S-E lock-in) — 사용자 명시 활성 (opt-in).
 *
 * Pattern reference: AutoTopologyRecoverySettings (ADR-097 T-ε) +
 * AutoReferenceImportSettings (ADR-096) — localStorage 'true' explicit
 * ON preference 보존, listener-reactive change events.
 *
 * Out of scope (별도 sub-step / future ADR):
 * - User tier 외부 file storage / cloud sync (별도 ADR)
 * - Project tier 활성 토글 (Project 는 항상 ON, S-E)
 *
 * 의의: 사용자가 User tier 를 명시 활성하지 않으면 AssetLibraryPanel
 * 가 User 섹션을 hide (host 가 본 flag 를 조회하여 panel 의 User row 를
 * conditional 처리).
 *
 * 그 conditional 처리가 오랫동안 없었다 — MVP 가 "항상 3 섹션" 이었고
 * flag 는 "향후 확장 anchor" 로 남아, 체크박스를 켜도 꺼도 화면이 같았다.
 * 지금은 main.ts 가 `userTierEnabled` 콜백으로 panel 에 넘기고,
 * `onAssetLibraryUserTierModeChange` 로 열려 있는 panel 을 즉시 갱신한다.
 */

const STORAGE_KEY = 'axia:asset-library-user-tier';

let current = false; // Default OFF (S-E lock-in)
try {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved === 'true') current = true; // explicit ON preference 보존
} catch { /* private mode */ }

const listeners = new Set<(enabled: boolean) => void>();

export function getAssetLibraryUserTierMode(): boolean {
  return current;
}

export function setAssetLibraryUserTierMode(value: boolean): void {
  if (current === value) return;
  current = value;
  try {
    localStorage.setItem(STORAGE_KEY, String(value));
  } catch { /* ignore */ }
  for (const cb of listeners) cb(value);
}

export function onAssetLibraryUserTierModeChange(
  cb: (enabled: boolean) => void,
): () => void {
  listeners.add(cb);
  return () => { listeners.delete(cb); };
}
