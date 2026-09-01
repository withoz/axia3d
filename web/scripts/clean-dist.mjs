/**
 * Empty `web/dist` before a build that is going to be shipped.
 *
 * ⚠ `npm run build` passes `--emptyOutDir false` on purpose — CLAUDE.md records
 * a permission error on Windows when Vite empties the directory itself — so
 * dist only ever grows. Measured 2026-08-31: it held 294 MB including TEN
 * hashes of the engine WASM, roughly 45 MB of it dead. That is invisible to the
 * web deploy, which uploads what index.html references, and very visible to the
 * desktop build, which embeds the whole directory into the executable.
 *
 * Removing the directory outright is not the same operation Vite was failing
 * at, and it succeeds where that did.
 */
import { rmSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const DIST = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
if (existsSync(DIST)) {
  rmSync(DIST, { recursive: true, force: true });
  console.log('[clean-dist] removed web/dist');
} else {
  console.log('[clean-dist] web/dist absent, nothing to do');
}
