/**
 * Make `packages/axia-mcp-server` runnable here, in the order it needs.
 *
 * The MCP server is the engine's OTHER consumer — 240 tests over the capability
 * surface, the tier policy and the schema handshake (ADR-041 / ADR-042). CI
 * runs them on every PR that touches it. Locally they had never run at all, and
 * a 2026-08-23 audit found why: two things have to exist first and neither is
 * obvious from the failure.
 *
 *   1. `packages/axia-wasm-node/dist` — a SECOND wasm-pack build, `--target
 *      nodejs`, distinct from the `--target web` one under `web/src/wasm`. The
 *      MCP server cannot load the web build (ADR-082 Drift #2: Node ESM cannot
 *      resolve the `env` import of a browser-targeted .wasm).
 *   2. `packages/axia-mcp-server/node_modules` — from its OWN lockfile.
 *
 * ⚠ The package is deliberately NOT in the root `workspaces` array, and this
 * script exists instead of adding it there. `@axia/mcp-server` is published
 * (ADR-044), its `prepublishOnly` runs against its own `package-lock.json`, and
 * `mcp.yml` caches on that path and installs with `npm ci` inside the
 * directory. Hoisting it into the root workspace would silence that lockfile
 * and split what CI verifies from what ships.
 *
 *     npm run mcp:setup     # this — build the Node WASM, then `npm ci`
 *     npm run mcp:test      # the 240
 *
 * Skips work that is already done, so it is cheap to re-run. Exits non-zero
 * only when a step it actually attempted failed.
 */

import { existsSync, statSync } from 'fs';
import { spawnSync } from 'child_process';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const WASM_CRATE = join(ROOT, 'crates', 'axia-wasm');
const NODE_DIST = join(ROOT, 'packages', 'axia-wasm-node', 'dist');
const NODE_WASM = join(NODE_DIST, 'axia_wasm_bg.wasm');
const MCP = join(ROOT, 'packages', 'axia-mcp-server');
const MCP_MODULES = join(MCP, 'node_modules');

const MIN_VALID_WASM_BYTES = 100_000;

const log = (m) => console.log(`[mcp-setup] ${m}`);

function nodeWasmIsPresent() {
  if (!existsSync(NODE_WASM)) return false;
  try {
    return statSync(NODE_WASM).size >= MIN_VALID_WASM_BYTES;
  } catch {
    return false;
  }
}

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: 'inherit', shell: true });
  if (r.error && r.error.code === 'ENOENT') return 'missing';
  return r.status === 0 ? 'ok' : 'failed';
}

// ── 1. the Node-target WASM ───────────────────────────────────────────

if (nodeWasmIsPresent()) {
  log('Node WASM already built — skipping.');
} else {
  log('Building axia-wasm for Node (wasm-pack --target nodejs)…');
  const out = run(
    'wasm-pack',
    ['build', '--target', 'nodejs', '--out-dir', '../../packages/axia-wasm-node/dist'],
    WASM_CRATE,
  );
  if (out === 'missing') {
    console.error(`
[mcp-setup] wasm-pack is not installed, so the Node WASM was not built.

    cargo install wasm-pack --version 0.14.0
    npm run mcp:setup

  (The web build under \`web/src/wasm\` is a different target and will not
  work here — see ADR-082 Drift #2.)
`);
    process.exit(1);
  }
  if (out === 'failed') {
    console.error('[mcp-setup] wasm-pack build failed — see the output above.');
    process.exit(1);
  }
}

// ── 2. the MCP server's own dependencies ──────────────────────────────

if (existsSync(MCP_MODULES)) {
  log('MCP dependencies already installed — skipping.');
} else {
  log('Installing MCP server dependencies (npm ci, its own lockfile)…');
  if (run('npm', ['ci'], MCP) !== 'ok') {
    console.error('[mcp-setup] npm ci failed — see the output above.');
    process.exit(1);
  }
}

log('Ready. `npm run mcp:test` runs the suite.');
