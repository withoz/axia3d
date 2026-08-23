/**
 * The engine's OTHER consumer has to be runnable from this repo.
 *
 * `packages/axia-mcp-server` carries 240 tests over the capability surface, the
 * tier policy and the schema handshake (ADR-041 / ADR-042). CI runs them on
 * every PR that touches the package. Locally they had never run at all — a
 * 2026-08-23 audit found the directory had no `node_modules`, and the reason
 * was two prerequisites that no failure message names:
 *
 *   1. `packages/axia-wasm-node/dist` — a SECOND wasm-pack build,
 *      `--target nodejs`. The MCP server cannot load the `--target web` build
 *      under `web/src/wasm` (ADR-082 Drift #2).
 *   2. the package's own `npm ci`, from its own lockfile.
 *
 * ⚠ The package is deliberately NOT in the root `workspaces` array, and
 * `scripts/mcp-setup.mjs` exists instead of putting it there. `@axia/mcp-server`
 * is published (ADR-044); its `prepublishOnly` and `mcp.yml` both work from
 * `packages/axia-mcp-server/package-lock.json`, and hoisting it into the root
 * workspace would silence that lockfile and split what CI verifies from what
 * ships. These guards hold that arrangement in place — both halves of it.
 *
 * Source-level, in the shape this repo already uses for build configuration
 * (`wasmSimdActivation.test.ts`, `wasmFreshness.test.ts`): they cannot run
 * `npm ci`, so what they hold is the WIRING.
 *
 * Mutation-checked 2026-08-23: removing the script, removing either root entry
 * point, or hoisting the package into `workspaces` each fails exactly one of
 * these.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const readRoot = (p: string) => readFileSync(resolve(process.cwd(), '..', p), 'utf8');

describe('the MCP server is runnable from this repo', () => {
  it('there is a way in, and a way to run it', () => {
    const pkg = JSON.parse(readRoot('package.json'));
    expect(pkg.scripts['mcp:setup'], 'no `npm run mcp:setup`').toBe(
      'node scripts/mcp-setup.mjs',
    );
    expect(pkg.scripts['mcp:test'], 'no `npm run mcp:test`').toContain(
      'packages/axia-mcp-server',
    );
  });

  it('the setup script builds the NODE target, not the web one', () => {
    const src = readRoot('scripts/mcp-setup.mjs');
    // PREMISE: the file was read.
    expect(src.length).toBeGreaterThan(500);

    // ⚠ The ARGUMENT LIST, not the prose. A first version asserted
    // `toContain('nodejs')`, and flipping the argument to 'web' still passed —
    // the word survives in the comment right above it. Mutation-checked.
    expect(src).toMatch(/\[\s*'build',\s*'--target',\s*'nodejs',\s*'--out-dir',\s*'\.\.\/\.\.\/packages\/axia-wasm-node\/dist'/);
    // …and installs from the package's own lockfile.
    expect(src).toMatch(/'npm',\s*\['ci'\]/);
  });

  it('it is idempotent, so re-running it is cheap', () => {
    // A setup step that rebuilds a 2 MB wasm every time gets run once and then
    // avoided, which is the same as not having it.
    const src = readRoot('scripts/mcp-setup.mjs');
    expect(src).toContain('already built — skipping');
    expect(src).toContain('already installed — skipping');
  });

  it('it says what to do when wasm-pack is missing, rather than just failing', () => {
    const src = readRoot('scripts/mcp-setup.mjs');
    expect(src).toContain('cargo install wasm-pack');
  });

  it('the package stays OUT of the root workspaces — its lockfile is load-bearing', () => {
    const pkg = JSON.parse(readRoot('package.json'));
    const ws: string[] = pkg.workspaces ?? [];
    expect(
      ws.some((w) => w.includes('axia-mcp-server')),
      'hoisting @axia/mcp-server into the root workspace silences the lockfile ' +
        'that ADR-044 prepublishOnly and mcp.yml both install from',
    ).toBe(false);
  });

  it('CI still installs it from that lockfile, in its own directory', () => {
    const wf = readRoot('.github/workflows/mcp.yml');
    expect(wf).toContain('packages/axia-mcp-server/package-lock.json');
    expect(wf).toContain('npm ci');
    // And CI builds the Node target the same way the script does, so the two
    // cannot drift into disagreeing about which artifact the server loads.
    expect(wf).toContain('--target nodejs');
    expect(wf).toContain('packages/axia-wasm-node/dist');
  });
});
