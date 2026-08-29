/**
 * The cursor work-plane grid must not corrupt line rendering.
 *
 * Inherited from `DrawPlaneIndicatorH4Integration.test.ts`, which guarded the
 * fixed-size axis/quad gizmo the grid replaced. The gizmo is gone; the hazard is
 * not. An overlay that skips the depth test but still WRITES depth leaves depth
 * values where nothing was drawn, and lines behind it disappear — the
 * user-reported bug the original fix was for.
 *
 * `LineMaterial.depthWrite` defaults to true, so this has to be set explicitly
 * and has to stay set. It was missed when the grid was first written: the
 * material had `transparent` and `depthTest: false` and nothing else.
 *
 * Read from source rather than by constructing a viewport, because
 * `setMiniGridCursor` wants a camera, a sized container and a scene, while what
 * is being asserted is one property of one material.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { join } from 'path';

const SRC = readFileSync(join(__dirname, 'Viewport.ts'), 'utf8');

/** The `new LineMaterial({...})` the mini grid builds, braces balanced. */
function miniGridMaterial(): string {
  const at = SRC.indexOf('this._miniGridMat = new LineMaterial({');
  expect(at, 'the mini grid no longer builds its material here — retarget this test')
    .toBeGreaterThan(-1);
  let depth = 0;
  for (let i = SRC.indexOf('{', at); i < SRC.length; i++) {
    if (SRC[i] === '{') depth++;
    else if (SRC[i] === '}') {
      depth--;
      if (depth === 0) return SRC.slice(at, i + 1);
    }
  }
  throw new Error('unbalanced material literal');
}

describe('the cursor grid is an overlay that does not corrupt what is behind it', () => {
  it('does not write depth', () => {
    expect(
      /depthWrite:\s*false/.test(miniGridMaterial()),
      'the grid material writes depth — lines behind it will vanish',
    ).toBe(true);
  });

  it('skips the depth test and is transparent, so it reads as an overlay', () => {
    const mat = miniGridMaterial();
    expect(/depthTest:\s*false/.test(mat), 'the grid must draw over the model').toBe(true);
    expect(/transparent:\s*true/.test(mat), 'the grid must not be an opaque disc').toBe(true);
  });

  it('the gizmo it replaced is gone, so only one thing draws at the cursor plane', () => {
    const tm = readFileSync(join(__dirname, '..', 'tools', 'ToolManagerRefactored.ts'), 'utf8');
    // `flushDrawPlaneIndicator` keeps its name — it is still the cursor-plane
    // flush — but nothing may construct or drive the old overlay.
    const withoutMethodName = tm.replace(/flushDrawPlaneIndicator/g, '');
    expect(
      withoutMethodName.includes('DrawPlaneIndicator'),
      'the old fixed-size cursor gizmo is back alongside the grid',
    ).toBe(false);
  });
});
