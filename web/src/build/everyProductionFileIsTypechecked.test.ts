/**
 * No production source file may sit outside the TypeScript build.
 *
 * `web/tsconfig.json` excluded `src/tools/OffsetSessionManager.ts` — a shipped
 * file, imported by `OffsetTool.ts`, with 15 tests of its own that kept running
 * while `tsc --noEmit` never looked at it. Found 2026-08-24 by an engine audit;
 * the exclusion dates to the squashed baseline `155e127`, so no commit records
 * why.
 *
 * ⚠ Measured before removing it: with the file included, `npx tsc --noEmit`
 * reports ZERO errors. Whatever the exclusion was for, it is not needed now.
 *
 * The excludes that remain are the two legitimate kinds — test files and mocks,
 * which the app never ships. This asserts that shape rather than an exact list,
 * so adding another test-shaped exclude is fine and adding a source file is not.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

/** tsconfig.json is JSONC in principle; this repo's is plain JSON. */
function tsconfig(): { include?: string[]; exclude?: string[] } {
  const raw = readFileSync(resolve(process.cwd(), 'tsconfig.json'), 'utf8');
  // PREMISE: the file parsed. If this ever throws, the guard is not silently
  // passing on an empty object.
  return JSON.parse(raw);
}

/** An exclude entry that can only ever match tests or mocks. */
function isTestShaped(pattern: string): boolean {
  return (
    /\.test\.[cm]?tsx?$/.test(pattern) ||
    /\.spec\.[cm]?tsx?$/.test(pattern) ||
    pattern.includes('__mocks__') ||
    pattern.includes('__tests__') ||
    pattern.includes('/e2e/')
  );
}

describe('the TypeScript build covers everything that ships', () => {
  it('excludes only test-shaped paths, never a source file', () => {
    const cfg = tsconfig();
    const excludes = cfg.exclude ?? [];
    // PREMISE: there are excludes to judge. If the field vanished, this test
    // would pass vacuously, so say so.
    expect(excludes.length, 'tsconfig has no exclude list to check').toBeGreaterThan(0);

    const sourceExcludes = excludes.filter((p) => !isTestShaped(p));
    expect(
      sourceExcludes,
      'these are production files hidden from `tsc --noEmit`. A file the app ' +
        'imports must be typechecked; if one genuinely cannot compile, fix it ' +
        'or say why here rather than excluding it silently',
    ).toEqual([]);
  });

  it('still builds from src', () => {
    // The exclude check means nothing if `include` stopped covering the tree.
    const cfg = tsconfig();
    expect(cfg.include, 'tsconfig must still include src').toContain('src');
  });

  it('the specific file this guard was written for is back in', () => {
    const cfg = tsconfig();
    const excludes = cfg.exclude ?? [];
    expect(
      excludes.some((p) => p.includes('OffsetSessionManager')),
      'OffsetSessionManager.ts was excluded and is now typechecked — if it ' +
        'returns to the exclude list, that is the regression this file exists for',
    ).toBe(false);
  });
});
