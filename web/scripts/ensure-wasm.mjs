#!/usr/bin/env node
/**
 * Idempotent WASM-presence check, invoked from `npm install` via the
 * `postinstall` hook in package.json.
 *
 * Rationale (LOCKED #40 + follow-up): the `web/src/wasm/` directory
 * contains wasm-pack output (`.js` / `.d.ts` / `.wasm`) which used to be
 * committed alongside source. That allowed a desync where the Rust source
 * (`crates/axia-geo/src/mesh.rs`) advanced while the committed binary
 * stayed at an older build, producing the silent regression we hit in
 * PR #14 → #15 (chord_tol 0.1 baked into `axia_wasm_bg.wasm` even after
 * mesh.rs moved to 0.02).
 *
 * The architectural fix is to stop tracking the artifact and rebuild it
 * deterministically from source. This script is the dev-clone half of
 * that contract:
 *
 *   • CI / deploy: explicit `wasm-pack build` step before `npm ci` (or
 *     fired by this hook); always sees fresh source.
 *   • Dev clone: `git clone … && cd web && npm install` should leave the
 *     developer with a working WASM artifact without an extra command.
 *
 * Behaviour:
 *   1. If `web/src/wasm/axia_wasm_bg.wasm` exists, is non-trivial in size,
 *      and is NEWER than every Rust source and Cargo file under `crates/`,
 *      do nothing (idempotent — repeated `npm install` calls do not retrigger
 *      expensive builds).
 *   2. Otherwise, attempt `wasm-pack build --target web --out-dir
 *      ../../web/src/wasm` from `crates/axia-wasm/`.
 *   3. If wasm-pack is missing, print actionable install instructions
 *      and exit 0 (don't break `npm install` — the user may not need WASM
 *      yet, e.g. they're only running tsc/eslint).
 *
 * ⚠ This script used to check EXISTENCE AND SIZE and nothing else, and said
 * here that staleness detection "lives in a separate CI check (see follow-up)".
 * That follow-up was never written, and on 2026-08-21 the artifact was found
 * dated 2026-08-07 against sources dated 2026-08-20 — ninety engine commits
 * that had never reached a browser. Step 1 now also asks `wasm-freshness.mjs`
 * whether the sources have moved on, and rebuilds when they have.
 */

import { existsSync, statSync } from 'fs';
import { spawnSync } from 'child_process';
import { wasmFreshness, describeStaleness } from './wasm-freshness.mjs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const WEB_DIR = join(__dirname, '..');
const REPO_ROOT = join(WEB_DIR, '..');
const WASM_PATH = join(WEB_DIR, 'src', 'wasm', 'axia_wasm_bg.wasm');
const WASM_CRATE = join(REPO_ROOT, 'crates', 'axia-wasm');

const MIN_VALID_WASM_BYTES = 100_000;

function log(msg) {
  console.log(`[ensure-wasm] ${msg}`);
}

function wasmIsPresent() {
  if (!existsSync(WASM_PATH)) return false;
  try {
    const { size } = statSync(WASM_PATH);
    return size >= MIN_VALID_WASM_BYTES;
  } catch {
    return false;
  }
}

function tryWasmPackBuild(why) {
  log(`${why} — invoking wasm-pack build…`);
  const result = spawnSync(
    'wasm-pack',
    ['build', '--target', 'web', '--out-dir', '../../web/src/wasm'],
    { cwd: WASM_CRATE, stdio: 'inherit', shell: true },
  );
  if (result.error && result.error.code === 'ENOENT') {
    return 'no-wasm-pack';
  }
  if (result.status !== 0) {
    return 'build-failed';
  }
  return 'ok';
}

function printInstallInstructions() {
  console.error(`
[ensure-wasm] wasm-pack is not installed. The WASM artifact was not built.

  If you only need the TypeScript surface (tsc / eslint / vitest mock),
  you can ignore this message — those workflows do not require WASM.

  To enable the full dev experience (vite dev server, vitest non-mock,
  Playwright E2E):

    1. Install the Rust toolchain:    https://rustup.rs/
    2. Install wasm-pack:             cargo install wasm-pack
    3. Build WASM:                    cd web && npm run build:wasm

  This message will stop appearing once \`web/src/wasm/axia_wasm_bg.wasm\`
  exists.
`);
}

// ── Main ──────────────────────────────────────────────────────────────

const freshness = wasmFreshness();

if (wasmIsPresent() && !freshness.stale) {
  // Silent — happens on every `npm install` once the dev clone is set up.
  process.exit(0);
}

const outcome = tryWasmPackBuild(
  freshness.stale ? `WASM 이 낡았습니다 — ${describeStaleness(freshness)}` : 'WASM artifact missing',
);

if (outcome === 'ok') {
  log('WASM build succeeded.');
  process.exit(0);
}

if (outcome === 'no-wasm-pack') {
  printInstallInstructions();
  // Do not fail npm install — leave the developer's environment usable
  // for TS-only workflows.
  process.exit(0);
}

console.error('[ensure-wasm] wasm-pack build failed. See output above.');
process.exit(0); // still non-fatal; failing here would block `npm ci`
