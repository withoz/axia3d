/**
 * A documentation commit must not spend a deploy — and the ignore list must not
 * grow to cover anything that changes the site.
 *
 * ⚠ Measured 2026-08-31: two documentation-only merges (`.rs` comments, then
 * CLAUDE.md alone) each ran the full deploy — wasm-pack install, Rust build,
 * WASM build, `npm ci`, vite, and a 231 MB upload — to publish bytes identical
 * to the ones already served.
 *
 * The second assertion is the one that matters. `paths-ignore` is a list anyone
 * can extend, and an entry like `web/**` would silently stop deploying the app
 * while every check stayed green: the site would simply freeze at whatever was
 * last published, and nothing would say so.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

const YML = readFileSync('../.github/workflows/deploy.yml', 'utf8');

/** The `paths-ignore:` entries, as written. */
function ignoredPaths(): string[] {
  const start = YML.indexOf('paths-ignore:');
  if (start < 0) return [];
  const rest = YML.slice(start + 'paths-ignore:'.length);
  const out: string[] = [];
  for (const line of rest.split('\n')) {
    const m = line.match(/^\s+-\s+'([^']+)'\s*$/);
    if (m) out.push(m[1]);
    else if (line.trim() !== '') break; // first non-entry line ends the list
  }
  return out;
}

describe('the deploy skips prose and nothing else', () => {
  it('ignores markdown and docs', () => {
    expect(ignoredPaths().sort()).toEqual(['**/*.md', 'docs/**']);
  });

  it('never ignores a path that changes what is published', () => {
    // ⚠ The load-bearing one. `web/**` or `crates/**` here would freeze the
    // site at its last deploy with every check still green.
    for (const p of ignoredPaths()) {
      expect(p).not.toMatch(/^crates/);
      expect(p).not.toMatch(/^web\/(?!.*\.md)/);
      expect(p).not.toMatch(/package(-lock)?\.json/);
      expect(p).not.toMatch(/\.github/);
    }
  });

  it('keeps the manual escape hatch, so a skipped deploy can still be run', () => {
    expect(YML).toContain('workflow_dispatch:');
  });

  it('still fires on pushes to main', () => {
    expect(YML).toMatch(/push:\s*\n\s*branches:\s*\[\s*main\s*\]/);
  });
});
