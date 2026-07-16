/**
 * ADR-294 — i18n (Korean + English). No dependency, ~80 lines.
 *
 * Why in-house: ADR-035 P20.C #2 locks initial bundle growth, and i18next is
 * ~40 kB for plural rules and lazy namespaces two languages do not need. The
 * repo's own idiom is a small SSOT module (AutoIntersectSettings,
 * DrawCurveSettings…), so this follows it.
 *
 * Why the Korean text is the key: inventing 1,731 stable key names is the
 * expensive half of an i18n migration and the half a user never sees. With
 * source-as-key, `ko` is the identity function (no locale file at all),
 * migration is a wrap rather than a rewrite, the call site stays legible, and
 * an untranslated string renders Korean instead of `toast.pushpull.tooShort`.
 *
 *   Toast.warning(t('돌출 거리가 너무 짧습니다'));
 *   Toast.warning(t('두께 {limit}mm 에서 멈췄습니다', { limit: 1000 }));
 *
 * The honest cost: editing the Korean orphans its English entry. `en.ts` is a
 * plain object and `i18n.test.ts` checks it, so an orphan is findable — and a
 * miss degrades to Korean rather than breaking.
 */
import { EN } from './en';

export type Locale = 'ko' | 'en';

const LS_KEY = 'axia:locale';

/** `ko` has no table — the key IS the Korean string (ADR-294 D2). */
const TABLES: Record<Locale, Record<string, string> | null> = {
  ko: null,
  en: EN,
};

function detect(): Locale {
  try {
    const saved = localStorage.getItem(LS_KEY);
    if (saved === 'ko' || saved === 'en') return saved;
  } catch {
    // private mode / no storage — fall through to the browser
  }
  try {
    // Anything not explicitly English keeps today's behaviour: Korean.
    return navigator.language?.toLowerCase().startsWith('en') ? 'en' : 'ko';
  } catch {
    return 'ko';
  }
}

let current: Locale = detect();

export function getLocale(): Locale {
  return current;
}

export function setLocale(next: Locale): void {
  current = next;
  try {
    localStorage.setItem(LS_KEY, next);
  } catch {
    // non-fatal: the session still switches, it just will not be remembered
  }
}

/**
 * Translate `key` (Korean source text) into the current locale, filling
 * `{name}` placeholders from `params`.
 *
 * Resolves the locale at CALL time, so a `t()` evaluated at module load would
 * freeze whatever locale was current at import — before main.ts runs. Module-
 * scope constants must therefore become getters, not stay constants
 * (ADR-294 D6 / L-294-5).
 *
 * Never throws and never surfaces a key name: an unknown or untranslated key
 * returns the Korean, which is exactly today's behaviour (L-294-3).
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const table = TABLES[current];
  // Explicit `undefined` check, not `||`: an empty translation is legitimate.
  // A sentence split across DOM nodes — `재질을 부여하면 이 객체는
  // <strong>XIA</strong>로 승격됩니다` — reorders in English, so the trailing
  // fragment must translate to ''. With `||` that fell back to the Korean and
  // the sentence rendered half-translated.
  const hit = table?.[key];
  let out = hit === undefined ? key : hit;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      out = out.split(`{${name}}`).join(String(value));
    }
  }
  return out;
}
