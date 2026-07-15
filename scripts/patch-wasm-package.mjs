#!/usr/bin/env node
/**
 * ADR-044 R5 follow-up — patch the wasm-pack-generated package.json.
 *
 * ## Why this exists
 *
 * `@axia/wasm-node` is one of ADR-044's three publishables (P29.1), but unlike
 * the other two it has NO committed package.json: `release.yml` runs
 *
 *     wasm-pack build --target nodejs --out-dir ../../packages/axia-wasm-node/dist
 *
 * and publishes from that generated `dist/`. wasm-pack emits only name, version,
 * description, collaborators, files, main, types, type and sideEffects — so the
 * package shipped WITHOUT the P29.4 metadata, without `publishConfig`
 * (P29.3/P29.6), and crucially without the `prepublishOnly` guard hook that
 * refuses local publishes on the other two packages (P29.6).
 *
 * ADR-044 flagged exactly this as **R5** ("wasm-pack 자동 생성 package.json 이
 * P29.4 metadata 못 받음") and named the mitigation
 * `scripts/patch-wasm-package.mjs` as a follow-up. It was never written, yet
 * `release_meta.test.ts` excluded axia-wasm-node from all six P29.7 regressions
 * on the grounds that it is "covered by a separate post-build patch script".
 * That script is this file; until it existed, the exclusion was unjustified and
 * the package was entirely uncovered.
 *
 * ## The name
 *
 * wasm-pack derives `name` from Cargo.toml, so it emits `axia-wasm`, while
 * ADR-041 P26.4, ADR-043 P28.3 and ADR-044 P29.1/P29.3 all call this
 * publishable `@axia/wasm-node` — and release.yml publishes it with
 * `--access public`, which is a no-op for an unscoped name. This script
 * therefore renames it to the ADR name (사용자 결재 2026-07-14).
 *
 * Safe because nothing depends on the published name: `@axia/wasm-node` appears
 * only in docs/ADRs, no package.json declares either name as a dependency, and
 * `check-wasm.mjs` / `verify-schema-pin.mjs` resolve by PATH
 * (`packages/axia-wasm-node/dist/...`), not by package name. ADR-041 §114-117
 * already documents the intended consumer import as
 * `import { AxiaEngine } from '@axia/wasm-node'`.
 *
 * On ADR-044 R1 (the `@axia` npm scope may be unavailable → ADR-044.1 renames
 * everything): scoping wasm-node adds no NEW blocker — `@axia/mcp-server` is
 * already scoped, so the org is a prerequisite for the release either way. If
 * R1 fires, all three names move together, which is exactly the lockstep the
 * ADR asks for.
 *
 * ## Usage
 *
 *     node scripts/patch-wasm-package.mjs [--check]
 *
 * Run AFTER `wasm-pack build --target nodejs --out-dir packages/axia-wasm-node/dist`
 * and BEFORE `npm publish`. Idempotent — safe to re-run. With `--check` it
 * verifies without writing and exits non-zero if a patch is needed.
 */

import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, '..');
const distPkgPath = resolve(repoRoot, 'packages/axia-wasm-node/dist/package.json');
const checkOnly = process.argv.includes('--check');

if (!existsSync(distPkgPath)) {
  process.stderr.write(
    '\n[patch-wasm-package] packages/axia-wasm-node/dist/package.json not found.\n' +
      '  Build it first:\n' +
      '    cd crates/axia-wasm && wasm-pack build --target nodejs \\\n' +
      '      --out-dir ../../packages/axia-wasm-node/dist\n\n',
  );
  process.exit(1);
}

/** Lockstep version (P29.1/P29.2) — the sibling publishables are the source. */
const mcpPkg = JSON.parse(
  readFileSync(resolve(repoRoot, 'packages/axia-mcp-server/package.json'), 'utf8'),
);

/**
 * Fields wasm-pack does not emit. Values mirror the sibling publishables so the
 * three stay consistent (P29.4) — nothing here is invented.
 */
const PATCH = {
  // ADR-041 P26.4 / ADR-043 P28.3 / ADR-044 P29.1+P29.3 (사용자 결재 2026-07-14).
  // wasm-pack emits the Cargo crate name (`axia-wasm`); the scoped name is what
  // every ADR and release.yml's `--access public` assume. See header.
  name: '@axia/wasm-node',
  license: 'MIT',
  author: 'WYKO <withoz1111@gmail.com>',
  homepage: 'https://github.com/withoz/axia3d#readme',
  bugs: { url: 'https://github.com/withoz/axia3d/issues' },
  repository: {
    type: 'git',
    url: 'https://github.com/withoz/axia3d.git',
    directory: 'packages/axia-wasm-node',
  },
  keywords: ['axia-3d', 'cad', 'wasm', 'webassembly', 'geometry', 'dcel', 'nurbs'],
  // P29.3 `--access public` + P29.6 provenance. release.yml also passes these
  // on the CLI; declaring them here keeps the package self-describing and
  // matches what release_meta.test.ts asserts for the other two.
  publishConfig: { access: 'public', provenance: true },
  scripts: {
    // P29.6 — refuse local `npm publish`. Path is relative to the dist/ dir the
    // publish runs from: packages/axia-wasm-node/dist → ../../axia-mcp-server.
    // No build/test here (unlike the siblings): dist/ IS the build output, and
    // wasm-pack has already produced it by the time this hook could fire.
    prepublishOnly: 'node ../../axia-mcp-server/scripts/guard-publish.mjs',
  },
};

const pkg = JSON.parse(readFileSync(distPkgPath, 'utf8'));

const before = JSON.stringify(pkg);
const patched = {
  ...pkg,
  ...PATCH,
  // Preserve wasm-pack's own scripts if it ever emits any.
  scripts: { ...(pkg.scripts ?? {}), ...PATCH.scripts },
  // Lockstep with the siblings (P29.1). wasm-pack takes version from the Cargo
  // workspace, so these normally already agree — assert rather than assume.
  version: pkg.version,
};
const changed = JSON.stringify(patched) !== before;

if (patched.version !== mcpPkg.version) {
  process.stderr.write(
    `\n[patch-wasm-package] LOCKSTEP VIOLATION (ADR-044 P29.1/P29.2):\n` +
      `  packages/axia-wasm-node/dist  version ${patched.version}\n` +
      `  packages/axia-mcp-server      version ${mcpPkg.version}\n` +
      `  The Cargo workspace version and the npm package versions must match.\n\n`,
  );
  process.exit(1);
}

if (checkOnly) {
  if (changed) {
    process.stderr.write(
      '\n[patch-wasm-package] dist/package.json is NOT patched (run without --check).\n\n',
    );
    process.exit(1);
  }
  process.stdout.write('[patch-wasm-package] ✓ already patched\n');
  process.exit(0);
}

writeFileSync(distPkgPath, `${JSON.stringify(patched, null, 2)}\n`);

process.stdout.write(
  `[patch-wasm-package] ✓ patched packages/axia-wasm-node/dist/package.json\n` +
    `    name         : ${patched.name}\n` +
    `    version      : ${patched.version} (lockstep with @axia/mcp-server ✓)\n` +
    `    license      : ${patched.license}\n` +
    `    publishConfig: access=${patched.publishConfig.access} provenance=${patched.publishConfig.provenance}\n` +
    `    prepublishOnly: ${patched.scripts.prepublishOnly}\n`,
);

// Invariant: the published name must be the scoped ADR name. Unscoped would
// silently make release.yml's `--access public` a no-op and publish to the
// wrong package. Assert rather than trust the merge above.
if (patched.name !== '@axia/wasm-node') {
  process.stderr.write(
    `\n[patch-wasm-package] NAME INVARIANT VIOLATED (ADR-044 P29.1/P29.3):\n` +
      `  expected "@axia/wasm-node", got "${patched.name}"\n\n`,
  );
  process.exit(1);
}
