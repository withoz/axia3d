/**
 * ADR-294 D6 fixture — a module that calls t() at MODULE SCOPE, the exact
 * shape D6 warned about. Imported dynamically by i18n.test.ts to measure
 * whether the locale is already resolved when an importer's body runs.
 *
 * Not used by the app.
 */
import { t } from '../index';

export const LABEL = t('그 면을 찾을 수 없습니다 — 다시 선택해 주세요');
