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
 * `stop()` + `dispose()` and threw the renderer away, they kept going.
 *
 * `animate()` schedules the next frame on its FIRST line and calls the
 * callbacks after, so a throwing callback does not stop the loop — it skips
 * `renderer.render()`, and skips it again next frame. A frozen overlay becomes
 * a black viewport. `animate()` therefore isolates each callback and reports it
 * once, which also covers the two registrants that predate this change.
 *
 * ── What each assertion holds ──
 *
 *   1. no rAF in main.ts
 *   2. every registrant is present BY NAME — not merely counted
 *   3. the loop runs them, and stops
 *   4. a thrower is isolated: called inside try, catch does not rethrow
 *   5. the isolation reports ONCE, not once per frame
 *
 * Each of the five was checked by breaking the thing it guards and watching it
 * go red. Three of them exist because the first version of this file passed
 * mutations it should have failed:
 *
 *   - assertion 2 searched the raw source, so COMMENTING a registration out
 *     satisfied it. It now strips comments — block comments too, which the
 *     second version still missed while its docblock claimed otherwise.
 *   - assertion 2 counted `>= 3` and named only the two overlays, so deleting
 *     the LOD registrant while adding anything else passed. All three are named.
 *   - assertion 5 did not exist, so deleting the report-once WeakSet — the
 *     60-lines-a-second flood this whole thing is written against — passed 4/4.
 *
 * ── What it still cannot hold ──
 *
 * Assertion 1 reads a spelling. `setInterval(tick, 16)` or a self-re-arming
 * `setTimeout` is the same mistake wearing another name and would pass; banning
 * `setInterval` outright is not available, main.ts uses it for the 200ms stats
 * poll. And none of this EXECUTES `animate()` — no test calls `viewport.start()`,
 * so the isolation is verified by reading, not by running.
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

/**
 * Strip comments so a commented-out line cannot satisfy a search.
 *
 * Block comments first — a `//` inside `/* ... *​/` would otherwise be
 * half-eaten. Neither pass understands string literals, so a `/*` inside a
 * string would over-strip; main.ts has none, and over-stripping fails LOUD
 * (a registration disappears) rather than passing quietly.
 */
const stripComments = (src: string) =>
  src
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .map((l) => l.replace(/\/\/.*$/, ''))
    .join('\n');

const MAIN_CODE = stripComments(MAIN);

/** Every per-frame job main.ts owns. Named, so deleting one cannot hide behind a count. */
const REGISTRANTS = [
  'constraintVisual.update',
  'dimensionManager.update',
  'bridge.lodChordTol',
];

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

  it('every per-frame job is registered on the render loop, by name', () => {
    const bodies = MAIN_CODE.split('viewport.onFrame(').slice(1).map((s) => s.slice(0, 900));
    expect(
      bodies.length,
      'expected one viewport.onFrame() registration per per-frame job',
    ).toBeGreaterThanOrEqual(REGISTRANTS.length);

    // Plain string search on purpose. A regex built from a string literal eats
    // its own backslashes — `'onFrame\('` reaches RegExp as `onFrame(`, an
    // unterminated group. Escaping is not worth the footgun here.
    for (const job of REGISTRANTS) {
      expect(
        bodies.some((b) => b.includes(job)),
        `${job} must be reached through viewport.onFrame(); otherwise it ` +
        'either never runs or never stops.',
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

  /** The callback loop and a little after it — the window assertions 4 and 5 read. */
  const loopBody = VIEWPORT
    .slice(VIEWPORT.indexOf('for (const cb of this._onFrameCallbacks)'))
    .slice(0, 700);

  it('a throwing callback cannot take the render with it', () => {
    expect(
      /try\s*\{[\s\S]{0,80}cb\(\);[\s\S]{0,80}\}\s*catch/.test(loopBody),
      'Each onFrame callback must be called inside try/catch. Without it a ' +
      'throwing overlay skips renderer.render() on every frame — the ' +
      'viewport goes black while the loop keeps spinning.',
    ).toBe(true);

    // \b so this reads the KEYWORD. A bare /throw/ would also match the word
    // inside the report message ("threw" does not, but "throws" would), which
    // made the earlier version pass by accident of wording.
    expect(
      /\bthrow\b/.test(loopBody),
      'the catch must not rethrow — that would defeat the isolation',
    ).toBe(false);
  });

  it('the isolation reports once, not once per frame', () => {
    expect(
      /_onFrameFailed/.test(VIEWPORT),
      'Viewport must remember which callbacks it has already reported. At ' +
      '60fps an unguarded console.error is 60 identical lines a second, ' +
      'which buries the first one.',
    ).toBe(true);

    // The guard has to sit BEFORE the log, or it guards nothing.
    const failedAt = loopBody.indexOf('_onFrameFailed');
    const logAt = loopBody.indexOf('console.error');
    expect(failedAt, 'the report-once check must be inside the callback loop').toBeGreaterThan(-1);
    expect(logAt, 'the loop must report a failure somewhere').toBeGreaterThan(-1);
    expect(
      failedAt < logAt,
      'the report-once check must come before the log, not after it',
    ).toBe(true);
  });
});
