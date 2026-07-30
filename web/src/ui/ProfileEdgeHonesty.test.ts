/**
 * The "프로필 엣지 (외곽 강조)" checkbox was write-only for its whole life:
 * StylePanel called `viewport.setEdgeStyle({ profileEdge })`, Viewport stored
 * it, `getStyleSettings()` reported it, and StylePanel read it straight back
 * into the same checkbox. A closed loop with no renderer in it — toggling the
 * box never changed a pixel, and the box remembering its own state is what
 * made it look like it worked.
 *
 * It is now disabled and labelled 준비중. This guard exists to keep those two
 * facts joined: the control stays disabled exactly while nothing draws from
 * the flag. Implement the silhouette pass and this test fails, which is the
 * moment to re-enable the checkbox — not a year later.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');

/** Every non-test line in web/src mentioning the flag, comments excluded. */
function codeReferences(): string[] {
  const files = ['src/viewport/Viewport.ts', 'src/ui/StylePanel.ts'];
  const out: string[] = [];
  for (const f of files) {
    read(f)
      .split('\n')
      .forEach((line, i) => {
        if (!/profileEdge/.test(line)) return;
        if (/^\s*(\/\/|\*|\/\*)/.test(line)) return; // prose, not plumbing
        out.push(`${f}:${i + 1}`);
      });
  }
  return out;
}

describe('profile edges — the checkbox is disabled exactly while nothing renders', () => {
  it('the extractor finds the plumbing (a guard that matches nothing passes for free)', () => {
    expect(codeReferences().length, 'no profileEdge references found — did the flag get renamed?')
      .toBeGreaterThan(0);
  });

  it('the flag is still stored-and-reported only, with no reader that draws', () => {
    // Declaration, the setEdgeStyle option, the write, and the getStyleSettings
    // readout. Four lines, all bookkeeping. A fifth means someone either wired
    // a renderer or a new consumer — either way the disabled checkbox below is
    // now wrong and this test is the prompt to fix it.
    expect(codeReferences(), 'profileEdge gained a reference — if it now renders, re-enable the checkbox')
      .toEqual([
        'src/viewport/Viewport.ts:113',
        'src/viewport/Viewport.ts:2524',
        'src/viewport/Viewport.ts:2527',
        'src/viewport/Viewport.ts:2954',
      ]);
  });

  it('the checkbox is disabled and does not claim to be on', () => {
    const html = read('index.html');
    const row = /<input[^>]*id="sty-edge-profile"[^>]*>/.exec(html)?.[0] ?? '';
    expect(row, 'the sty-edge-profile input vanished from index.html').not.toBe('');
    expect(row, 'a control nothing renders from must not be operable').toContain('disabled');
    expect(row, 'a checked-but-disabled box reads as "on and locked", which is also untrue')
      .not.toContain('checked');
  });

  it('StylePanel neither writes it nor ticks it back on', () => {
    const panel = read('src/ui/StylePanel.ts');
    expect(
      /addEventListener\([^)]*\)[\s\S]{0,120}setEdgeStyle\(\{\s*profileEdge/.test(panel),
      'StylePanel is writing profileEdge again — that was the write-only loop',
    ).toBe(false);
    expect(
      /getElementById\('sty-edge-profile'\)[^\n]*\.checked\s*=/.test(panel),
      'StylePanel is syncing the stored `true` back into the disabled box',
    ).toBe(false);
  });
});
