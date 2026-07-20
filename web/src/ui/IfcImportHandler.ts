/**
 * IFC Import — step 1: read the file and report what is in it (ADR-203 I-1).
 *
 * Opens a file dialog, hands the text to the engine's STEP-21 parser, and shows
 * the user what the file holds — schema, element counts, entity histogram.
 * Nothing is added to the scene yet: turning IFC B-reps into DCEL geometry is
 * the next step, and claiming otherwise would be worse than saying so.
 */

import { WasmBridge } from '../bridge/WasmBridge';
import { debugLog } from '../utils/debug';
import { Toast } from './Toast';
import { t } from '../i18n';

export interface IfcImportDeps {
  bridge: WasmBridge;
}

/** Element kinds worth naming in the summary, in report order. */
const ELEMENT_LABELS: [string, string][] = [
  ['walls', '벽'],
  ['wallsStandardCase', '벽(표준)'],
  ['slabs', '슬래브'],
  ['beams', '보'],
  ['columns', '기둥'],
  ['doors', '문'],
  ['windows', '창'],
  ['spaces', '공간'],
  ['storeys', '층'],
  ['materials', '재질'],
];

export function importIfcFile(deps: IfcImportDeps): void {
  const { bridge } = deps;

  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.ifc';
  input.style.display = 'none';
  document.body.appendChild(input);

  input.onchange = async () => {
    const file = input.files?.[0];
    document.body.removeChild(input);
    if (!file) return;

    debugLog(`[IFC Import] 파일: ${file.name} (${(file.size / 1024).toFixed(1)} KB)`);

    try {
      const text = await file.text();
      const report = bridge.analyzeIfc(text);

      if (!report) {
        Toast.error(t('IFC 읽기 실패: 엔진이 준비되지 않았습니다.'), 5000);
        return;
      }
      if (!report.ok) {
        Toast.error(
          t('IFC 파싱 실패: {error}', { error: report.error || t('알 수 없는 오류') }),
          6000,
        );
        return;
      }

      const notable = report.notable ?? {};
      const parts = ELEMENT_LABELS
        .filter(([key]) => (notable[key] ?? 0) > 0)
        .map(([key, label]) => `${label} ${notable[key]}`);

      const head = t('{fileName} — {schema}, 엔티티 {entityCount}개', {
        fileName: file.name,
        schema: report.schema || t('스키마 미상'),
        entityCount: report.entityCount ?? 0,
      });
      const body = parts.length ? parts.join(', ') : t('식별된 BIM 부재 없음');

      // Say plainly that this reads the file but does not place geometry yet.
      Toast.info(`${head}\n${body}\n${t('현재는 내용 확인만 가능합니다 (형상 가져오기는 준비 중).')}`, 8000);
      debugLog('[IFC Import] 분석 결과:', report);
    } catch (err) {
      console.error('[IFC Import] 오류:', err);
      Toast.error(t('IFC 가져오기 중 오류: {error}', { error: (err as Error).message }), 6000);
    }
  };

  input.click();
}
