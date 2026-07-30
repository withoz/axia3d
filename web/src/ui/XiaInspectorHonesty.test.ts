/**
 * Two surfaces in the XIA Inspector said things that were not so.
 *
 * The 무게 box in the summary row had no writer at all — only two `.closest()`
 * lookups that show or hide its container — so it sat at its markup literal of 0
 * while 면적 and 부피 beside it filled in. Assign a material and the panel
 * contradicted itself outright: 질량 below showed the real mass while this still
 * read 0 kg.
 *
 * The 화재 등급 row was three `<button>`s with a hover state and one filled red,
 * so it read as a 3-way toggle with a current selection. Nothing listened. The
 * only code touching them paints `.active` from the assigned material, and no
 * built-in material carries 'semi', so clicking 준불연 — the natural way to test
 * the control — could not even repaint. Editing fire rating belonged to the
 * material-properties panel that ADR-045 D2 removed, and which needs a new ADR
 * to come back, so these are read-outs by governance as well as by
 * implementation.
 *
 * (That panel's name is spelled out nowhere here on purpose: its own
 * resurrection guard scans every .ts under src for the symbol, and while its
 * comment says "production source paths only" it excludes just itself and
 * __mocks__. Naming it in prose would trip it. Better a circumlocution here
 * than widening a guard until it can no longer see a real re-import.)
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(__dirname, '../..', p), 'utf8');
const HTML = read('index.html');
const INSPECTOR = read('src/ui/XiaInspector.ts');

describe('the 무게 summary box reports a real number', () => {
  it('something writes it, and it is the mass', () => {
    // The bug was the absence of any assignment: every reference walked up to
    // the container with .closest() to show or hide it.
    expect(INSPECTOR, 'no code writes #xi-weight — it is back to printing its markup literal')
      .toMatch(/weightSummaryEl\.textContent = formatNum\(physics\.mass/);
  });

  it('and it says "-" rather than 0 when there is no mass to report', () => {
    // 0 kg is a claim; "-" is the absence of one. The neighbours ship "-" for
    // exactly this state.
    const dashWrites = [...INSPECTOR.matchAll(/weightSummaryEl\.textContent = '-'/g)].length;
    expect(dashWrites, 'the no-material / no-volume paths stopped clearing the box')
      .toBeGreaterThanOrEqual(3);
    expect(HTML, 'the sibling readouts no longer use "-" — check this convention still holds')
      .toContain('id="xi-density" type="text" value="-" readonly');
  });

  it('it is written where every path that can change the answer passes', () => {
    // Writing it in the selection branch instead would go stale the moment
    // someone swapped material without reselecting — and would also throw,
    // since `commonMat` is declared thirty lines further down.
    const fn = INSPECTOR.slice(
      INSPECTOR.indexOf('const updatePhysicalPanel'),
      INSPECTOR.indexOf('\n  };', INSPECTOR.indexOf('const updatePhysicalPanel')),
    );
    expect(fn.length, 'updatePhysicalPanel not found').toBeGreaterThan(500);
    expect(fn, 'the write left updatePhysicalPanel').toContain('weightSummaryEl');
  });
});

describe('the 화재 등급 row does not pretend to be a control', () => {
  const row = HTML.slice(HTML.indexOf('class="xi-fire-btns"'), HTML.indexOf('</div>', HTML.indexOf('data-fire="retardant"')));

  it('the three ratings are not buttons', () => {
    expect(row.length, 'the fire-rating markup moved — this slice found nothing').toBeGreaterThan(80);
    expect(/<button[^>]*class="xi-fire-btn"/.test(row), 'they are buttons again, with nothing behind them')
      .toBe(false);
    for (const r of ['incombustible', 'semi', 'retardant']) {
      expect(row, `the ${r} readout disappeared`).toContain(`data-fire="${r}"`);
    }
  });

  it('and do not offer a click affordance', () => {
    const css = HTML.slice(HTML.indexOf('.xi-fire-btn {'), HTML.indexOf('.xi-fire-btn.active'));
    expect(css, 'cursor:pointer is back on a read-out').not.toContain('cursor: pointer');
    expect(HTML, 'the hover state is back on a read-out').not.toContain('.xi-fire-btn:hover');
  });

  it('still gets repainted from the assigned material', () => {
    // Removing the affordance must not have broken the display. The paint keys
    // on the class, which the spans keep.
    expect(INSPECTOR).toContain(".xi-fire-btn'");
    expect(INSPECTOR, 'the fire-rating readout stopped tracking the assigned material')
      .toMatch(/classList\.toggle\(\s*'active',[\s\S]{0,120}?dataset\.fire === mat\.physical\.fireRating/);
  });

  it('nothing has quietly added a click handler instead', () => {
    // If someone wires these up, the ADR-045 D2 removal is the decision being
    // reversed, and that needs an ADR — not a listener.
    expect(/xi-fire-btn[^\n]*addEventListener\('click'|addEventListener\('click'[^\n]*xi-fire-btn/.test(INSPECTOR))
      .toBe(false);
  });
});
