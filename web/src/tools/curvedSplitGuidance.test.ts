/**
 * What each tool offers on a curved face has to match what the engine does.
 *
 * The engine's answer is measured in
 * `crates/axia-core/tests/an_open_seam_on_the_shapes_the_app_builds.rs`:
 *
 * ```text
 *   sphere   closed circle -> DIVIDES     open seam -> no
 *   cone     open seam     -> DIVIDES
 *   cylinder closed circle -> DIVIDES
 *   torus    closed circle -> DIVIDES
 * ```
 *
 * ⚠ The sphere was routed into the open-seam path until 2026-08-30, on a premise
 * that expired when Path B moved to the axis definition: the app's sphere is ONE
 * face with an interior meridian seam and no boundary, so an open cut has nothing
 * to run between. The engine declined, the tools only `console.warn`ed, and the
 * user's stroke vanished with nothing said — while the line tool's advice sent
 * them to exactly that path.
 *
 * A closed surface cannot be divided by an open arc. That is not a limitation to
 * be lifted later; it is what "closed" means.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';

const src = (rel: string) => readFileSync(join(__dirname, '..', rel), 'utf8');

describe('the curved-face guidance matches what the engine can do', () => {
  for (const tool of ['tools/DrawFreehandTool.ts', 'tools/DrawBezierTool.ts']) {
    it(`${tool} sends only the cone to the open-seam split`, () => {
      const s = src(tool);
      const at = s.indexOf('drawOpenSeamOnCurved');
      expect(at, 'the open-seam call is gone — retarget this guard').toBeGreaterThan(-1);
      // The guard on that call, i.e. the ~600 chars before it.
      const guard = s.slice(Math.max(0, at - 600), at);
      expect(guard, 'the sphere is being sent to a split the engine declines')
        .not.toMatch(/curvedKind === 'sphere'/);
      expect(guard, "the cone's seam split is no longer reachable")
        .toMatch(/curvedKind === 'cone'/);
    });

    it(`${tool} tells the user what DOES divide a sphere`, () => {
      const s = src(tool);
      // The sphere must reach the branch that speaks, not the one that warns.
      expect(s, 'the sphere has no guidance branch').toMatch(
        /curvedKind === 'sphere'\)\)?\s*(&&[^)]*\))?\s*\{[\s\S]{0,160}?Toast\.info/,
      );
      expect(s, 'the guidance does not name the sphere').toContain('구·원통·토러스는');
    });
  }

  it('the line tool advises per surface, not "sphere and cone" together', () => {
    const s = src('tools/DrawLineTool.ts');
    // The cone is the only kind it offers the seam for.
    expect(s, 'the line tool offers the seam on more than the cone').toMatch(
      /\{\s*4:\s*'cone'\s*\}/,
    );
    // ⚠ The old advice paired 구 with 원뿔. It cannot: they need opposite tools.
    expect(s, 'the advice still pairs the sphere with the cone')
      .not.toContain('자유곡선·베지어(구·원뿔)');
    expect(s, 'the advice does not say what divides a sphere')
      .toContain('구·원통·토러스는 닫힌 원');
  });
});
