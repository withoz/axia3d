/**
 * EVERY EXPORT SAYS WHETHER IT WORKED — AND FOUR OF THEM USED TO SAY NOTHING.
 *
 * Measured 2026-09-03, in MenuBar's own export block:
 *
 *   DXF / OBJ / glTF / STL   success -> debugLog     failure -> blocking alert()
 *   IFC                      success -> Toast.success failure -> blocking alert()
 *
 * `debugLog` prints only when `window.__AXIA_DEBUG` is set, so clicking
 * "Export OBJ" told the user nothing at all while IFC, three cases below it in
 * the same switch, said "완료". And every failure opened a modal the rest of the
 * app stopped using — 302 Toast call sites against 20 alerts.
 *
 * ⚠ This reads the source. The handlers are `.then`/`.catch` on a lazily
 * imported exporter, so exercising them means mocking five dynamic imports and
 * a download; what went wrong was never a runtime bug, it was five sibling
 * cases written to different rules. Source is where that is visible, and it is
 * what a reviewer would check.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';

const SRC = readFileSync(join(__dirname, 'MenuBar.ts'), 'utf8');

/** The `case 'export-*'` block, which is the scope of this guard. */
function exportBlock(): string {
  const start = SRC.indexOf("case 'export-dxf'");
  const end = SRC.indexOf('// ── 편집 ──', start);
  expect(start, 'the export block moved — re-anchor this guard').toBeGreaterThan(0);
  expect(end, 'the block end marker moved — re-anchor this guard').toBeGreaterThan(start);
  return SRC.slice(start, end);
}

describe('MenuBar export feedback', () => {
  it('reports success for every format, not just IFC', () => {
    const block = exportBlock();
    for (const fmt of ['DXF', 'OBJ', 'glTF', 'STL']) {
      expect(
        block.includes(`Toast.success(t('${fmt} 내보내기 완료'))`),
        `${fmt} export does not tell the user it worked`,
      ).toBe(true);
    }
    // IFC's is worded differently (it distinguishes analytic), so match loosely.
    expect(block).toMatch(/Toast\.success\([^)]*IFC 내보내기 완료/);
  });

  it('does not block the app to report a failed export', () => {
    const block = exportBlock();
    expect(
      block.includes('alert('),
      'an export failure opens a modal again — the app reports failures with Toast.error',
    ).toBe(false);
    // And it must still SAY something: silence would also pass the line above.
    expect((block.match(/Toast\.error\(/g) ?? []).length).toBe(5);
  });

  it('keeps the console trail for whoever is debugging', () => {
    const block = exportBlock();
    expect((block.match(/console\.error\(/g) ?? []).length).toBe(5);
  });
});
