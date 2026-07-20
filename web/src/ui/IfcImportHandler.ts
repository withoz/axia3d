/**
 * IFC Import (ADR-203 I-1 → I-3).
 *
 * Opens a file dialog, then: reads the file with the engine's STEP-21 parser
 * (I-1), names the members it holds (I-2), and brings their B-rep geometry into
 * the scene as DCEL faces (I-3). The toast reports what actually landed —
 * elements, faces, vertices — and names anything skipped rather than implying a
 * clean import.
 */

import { WasmBridge } from '../bridge/WasmBridge';
import { ToolManager } from '../tools/ToolManagerRefactored';
import { debugLog } from '../utils/debug';
import { Toast } from './Toast';
import { t } from '../i18n';

export interface IfcImportDeps {
  bridge: WasmBridge;
  toolManager: ToolManager;
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
  const { bridge, toolManager } = deps;

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

      // I-2 — name the members and say how many we could actually convert.
      const elements = bridge.classifyIfc(text);
      const lines = [head, body];

      if (elements?.ok && (elements.elementCount ?? 0) > 0) {
        const total = elements.elementCount ?? 0;
        const convertible = elements.convertible ?? 0;
        lines.push(t('가져올 수 있는 형상: {convertible} / {total} 부재', { convertible, total }));

        // A short preview so the user recognizes their own model.
        const preview = (elements.elements ?? []).slice(0, 4).map((e) => {
          const name = e.name || e.type.replace(/^IFC/, '');
          return e.material ? `${name} (${e.material})` : name;
        });
        if (preview.length) {
          const more = total > preview.length ? t(' 외 {rest}개', { rest: total - preview.length }) : '';
          lines.push(preview.join(', ') + more);
        }

        const unsupported = Object.entries(elements.unsupportedGeometry ?? {});
        if (unsupported.length) {
          lines.push(
            t('아직 못 읽는 형상: {kinds}', {
              kinds: unsupported.map(([k, n]) => `${k.replace(/^IFC/, '')} ${n}`).join(', '),
            }),
          );
        }
      }

      // I-3 — actually bring the geometry in.
      const imported = bridge.importIfc(text);
      if (imported?.ok) {
        toolManager.syncMesh();
        lines.push(
          t('가져왔습니다: 부재 {elements}개, 면 {faces}개, 정점 {vertices}개', {
            elements: imported.elements ?? 0,
            faces: imported.faces ?? 0,
            vertices: imported.vertices ?? 0,
          }),
        );
        // I-4 — distinguish "placed where the file says" from "everything at
        // the origin", which is what an unapplied placement chain looks like.
        const placed = imported.placed ?? 0;
        if (placed > 0) {
          lines.push(t('{placed}개 부재를 배치 정보대로 놓았습니다.', { placed }));
        }
        for (const w of (imported.warnings ?? []).slice(0, 3)) lines.push(`· ${w}`);
        Toast.success(lines.join('\n'), 9000);
      } else {
        // Nothing was placed — say why instead of implying success.
        lines.push(
          t('형상을 가져오지 못했습니다: {reason}', {
            reason: imported?.error || t('지원하는 B-rep 형상이 없습니다'),
          }),
        );
        Toast.warning(lines.join('\n'), 9000);
      }
      debugLog('[IFC Import] 분석:', report, '분류:', elements, '가져오기:', imported);
    } catch (err) {
      console.error('[IFC Import] 오류:', err);
      Toast.error(t('IFC 가져오기 중 오류: {error}', { error: (err as Error).message }), 6000);
    }
  };

  input.click();
}
