/**
 * Is the built WASM artifact older than the Rust it was built from?
 *
 * `web/src/wasm/` is gitignored and rebuilt only by `postinstall` or an
 * explicit `npm run build:wasm`. Until this file existed, `ensure-wasm.mjs`
 * checked EXISTENCE AND SIZE and nothing else — its own header said the
 * staleness check "lives in a separate CI check (see follow-up)", and that
 * follow-up was never written.
 *
 * Measured 2026-08-21: the artifact was dated 2026-08-07 while the sources
 * were 2026-08-20. Ninety engine commits, nine of them touching
 * `axia-wasm/src/lib.rs`, had never reached a browser. `cargo test` was green
 * throughout — it says nothing about what the app is running. What finally
 * noticed was a wiring guard reporting three bridge calls "missing from the
 * WASM export"; all three were in the Rust source the whole time.
 *
 * Used from two places, so there is one answer rather than two:
 *   - `ensure-wasm.mjs`      rebuilds when stale, not only when missing
 *   - `check-wasm-fresh.mjs` refuses `npm run build` on a stale artifact
 *
 * Only `crates/<name>/src/**` and the workspace Cargo files are read. `target/`
 * is excluded — it is enormous, and it is output rather than source.
 */

import { existsSync, statSync, readdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const WEB_DIR = join(__dirname, '..');
const REPO_ROOT = join(WEB_DIR, '..');

export const WASM_PATH = join(WEB_DIR, 'src', 'wasm', 'axia_wasm_bg.wasm');
export const CRATES_DIR = join(REPO_ROOT, 'crates');

/** Newest mtime under a directory, walking subdirectories. */
function newestUnder(dir, acc = { ms: 0, path: null }) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return acc;
  }
  for (const e of entries) {
    const p = join(dir, e.name);
    if (e.isDirectory()) {
      newestUnder(p, acc);
      continue;
    }
    if (!e.name.endsWith('.rs')) continue;
    try {
      const { mtimeMs } = statSync(p);
      if (mtimeMs > acc.ms) {
        acc.ms = mtimeMs;
        acc.path = p;
      }
    } catch {
      /* a file that vanished between readdir and stat is not evidence */
    }
  }
  return acc;
}

/**
 * `{ stale, artifactMs, newestMs, newestPath }`.
 *
 * `stale` is false when the artifact is MISSING — that is a different problem
 * with a different message, and `ensure-wasm.mjs` already owns it.
 */
export function wasmFreshness() {
  if (!existsSync(WASM_PATH)) {
    return { stale: false, missing: true, artifactMs: 0, newestMs: 0, newestPath: null };
  }
  const artifactMs = statSync(WASM_PATH).mtimeMs;

  const acc = { ms: 0, path: null };
  let crateDirs = [];
  try {
    crateDirs = readdirSync(CRATES_DIR, { withFileTypes: true })
      .filter((e) => e.isDirectory())
      .map((e) => join(CRATES_DIR, e.name));
  } catch {
    // No crates directory — nothing to be stale against.
    return { stale: false, missing: false, artifactMs, newestMs: 0, newestPath: null };
  }
  for (const c of crateDirs) {
    newestUnder(join(c, 'src'), acc);
    for (const f of ['Cargo.toml']) {
      const p = join(c, f);
      try {
        const { mtimeMs } = statSync(p);
        if (mtimeMs > acc.ms) {
          acc.ms = mtimeMs;
          acc.path = p;
        }
      } catch {
        /* absent is fine */
      }
    }
  }
  for (const f of ['Cargo.toml', 'Cargo.lock']) {
    const p = join(REPO_ROOT, f);
    try {
      const { mtimeMs } = statSync(p);
      if (mtimeMs > acc.ms) {
        acc.ms = mtimeMs;
        acc.path = p;
      }
    } catch {
      /* absent is fine */
    }
  }

  return {
    stale: acc.ms > artifactMs,
    missing: false,
    artifactMs,
    newestMs: acc.ms,
    newestPath: acc.path,
  };
}

/** A human-readable line naming the file that is newer, and by how long. */
export function describeStaleness(f) {
  const rel = (f.newestPath ?? '').replace(REPO_ROOT, '').replace(/^[\\/]/, '');
  const hours = (f.newestMs - f.artifactMs) / 3_600_000;
  const age = hours >= 48 ? `${Math.round(hours / 24)}일` : `${Math.round(hours)}시간`;
  return `${rel} 가 빌드된 WASM 보다 ${age} 새것입니다.`;
}
