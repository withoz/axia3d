/**
 * Per-frame work belongs to the render loop, not to its own rAF chain.
 *
 * main.ts used to drive the two 2D overlays — the constraint indicator and the
 * dimension labels — from hand-rolled loops that re-armed themselves:
 *
 *     const tickCV = () => { constraintVisual.update(cam); requestAnimationFrame(tickCV); };
 *     requestAnimationFrame(tickCV);
 *
 * Nothing could stop those. `viewport.stop()` cancels the viewport's own
 * `_frameId`; these chains were separate. So after `beforeunload` ran
 * `stop()` + `dispose()` and threw the renderer away, they kept going —
 * still reaching into WASM (`ConstraintVisual.update` → `bridge.getVertexPos`,
 * `DimensionManager.update` → the same) against an engine being torn down.
 *
 * ── The move made one thing worse before it made it better ──
 *
 * `animate()` schedules the next frame on its FIRST line and calls the
 * onFrame callbacks after. So a throwing callback does not stop the loop —
 * it skips `renderer.render()`, and skips it again next frame, and the next.
 * A frozen overlay became a black viewport. Under the old rAF chains a throw
 * killed only the thrower. `animate()` therefore isolates each callback now,
 * which also covers the two registrants that predate this change.
 *
 * ── What each assertion holds, and what none of them can ──
 *
 *   1. no rAF in main.ts — restore `requestAnimationFrame(tickCV)` and this
 *      fails. Alone it would also pass if the registrations were DELETED.
 *   2. each overlay is registered — that closes it. Comments are stripped
 *      first, because commenting a registration out leaves the string behind
 *      and satisfied an earlier version of this test while both overlays sat
 *      dead.
 *   3. the loop runs the callbacks and stops — main.ts can satisfy 1 and 2
 *      and still have overlays that never draw, or never stop.
 *   4. the loop isolates a thrower — without it the move trades a frozen
 *      overlay for a dead viewport.
 *
 * What it cannot hold: assertion 1 reads a spelling. `setInterval(tick, 16)`
 * or a self-re-arming `setTimeout` is the same mistake wearing another name
 * and would pass. Banning `setInterval` outright is not available — main.ts
 * uses it legitimately for the 200ms stats poll.
 *
 * Source-level, like `ActionWiring.test.ts` and `isTypingInInput.test.ts`,
 * because main.ts is a single init function no test can import.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

const MAIN = read('src/main.ts');
const VIEWPORT = read('src/viewport/Viewport.ts');

/** Line comments only — enough for the commented-out-registration hole, and
 *  it cannot swallow a real line the way a block-comment stripper might. */
const stripLineComments = (src: string) =>
  src.split('\n').map((l) => l.replace(/^\s*\/\/.*$/, '')).join('\n');

const MAIN_CODE = stripLineComments(MAIN);

describe('per-frame work runs on the render loop', () => {
  it('main.ts drives no rAF chain of its own', () => {
    const hits = MAIN_CODE.match(/requestAnimationFrame/g) ?? [];
    expect(
      hits.length,
      'main.ts should schedule per-frame work through viewport.onFrame(), ' +
      'not requestAnimationFrame — a chain main.ts starts is a chain ' +
      'viewport.stop() cannot stop.',
    ).toBe(0);
  });

  it('each overlay is registered on the render loop', () => {
    const registrations = MAIN_CODE.split('viewport.onFrame(').slice(1);
    expect(
      registrations.length,
      'expected the LOD push and both overlays to register on the loop',
    ).toBeGreaterThanOrEqual(3);

    // Plain string search on purpose. A regex built from a string literal eats
    // its own backslashes — `'onFrame\('` reaches RegExp as `onFrame(`, an
    // unterminated group. Escaping is not worth the footgun here.
    const bodies = registrations.map((s) => s.slice(0, 200));
    for (const overlay of ['constraintVisual.update', 'dimensionManager.update']) {
      expect(
        bodies.some((b) => b.includes(overlay)),
        `${overlay}() must be reached through viewport.onFrame(); ` +
        'otherwise it either never runs or never stops.',
      ).toBe(true);
    }
  });

  it('the render loop actually runs those callbacks, and actually stops', () => {
    expect(
      /for \(const cb of this\._onFrameCallbacks\)/.test(VIEWPORT),
      'Viewport.animate() must run the onFrame callbacks — main.ts now ' +
      'depends on it for the overlays.',
    ).toBe(true);

    expect(
      /stop\(\)\s*\{[\s\S]{0,200}?cancelAnimationFrame\(this\._frameId\)/.test(VIEWPORT),
      'Viewport.stop() must cancel the frame — that is the whole reason ' +
      'the overlays were moved onto this loop.',
    ).toBe(true);
  });

  it('a throwing callback cannot take the render with it', () => {
    // The callback loop must be inside a try, and the catch must not rethrow.
    const loop = VIEWPORT.slice(VIEWPORT.indexOf('for (const cb of this._onFrameCallbacks)'));
    const body = loop.slice(0, 600);
    expect(
      /try\s*\{[\s\S]{0,80}cb\(\);[\s\S]{0,80}\}\s*catch/.test(body),
      'Each onFrame callback must be called inside try/catch. Without it a ' +
      'throwing overlay skips renderer.render() on every frame — the ' +
      'viewport goes black while the loop keeps spinning.',
    ).toBe(true);
    expect(
      /catch[\s\S]{0,300}?throw/.test(body),
      'the catch must not rethrow — that would defeat the isolation',
    ).toBe(false);
  });
});
