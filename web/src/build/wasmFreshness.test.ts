/**
 * The built WASM must not be allowed to fall behind the Rust it came from.
 *
 * `web/src/wasm/` is gitignored and rebuilt only by `postinstall` or an
 * explicit `npm run build:wasm`. Before 2026-08-21 the only question anyone
 * asked was whether the file EXISTED and was big enough, so the artifact sat at
 * 2026-08-07 against sources at 2026-08-20 — ninety engine commits, nine of
 * them touching `axia-wasm/src/lib.rs`, that had never reached a browser.
 * `cargo test` was green the whole time; it says nothing about what the app is
 * running. What noticed in the end was `ActionWiring`'s link-D check reporting
 * three bridge calls "missing from the WASM export" — all three were in the
 * Rust source, just not in the build.
 *
 * These are source-level guards, in the shape this repo already uses for build
 * configuration (`wasmSimdActivation.test.ts`). They cannot run `npm install`,
 * so what they hold is the WIRING: that one freshness judgement exists, that
 * both callers use it, and that `npm run build` is gated on it.
 *
 * Mutation-checked 2026-08-21 by touching `crates/axia-geo/src/mesh.rs`:
 * `check-wasm-fresh.mjs` exited 1 naming the file, `npm run build` refused,
 * `AXIA_ALLOW_STALE_WASM=1` let it through with a warning, and
 * `ensure-wasm.mjs` rebuilt (28 s) and came back fresh.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const read = (p: string) => readFileSync(resolve(process.cwd(), p), 'utf8');

describe('WASM freshness — the artifact cannot silently fall behind the engine', () => {
  it('there is ONE freshness judgement, and it reads Rust sources and Cargo files', () => {
    const src = read('scripts/wasm-freshness.mjs');
    // PREMISE: the file was read, not an empty string.
    expect(src.length).toBeGreaterThan(500);

    expect(src).toContain('export function wasmFreshness');
    expect(src).toContain('export function describeStaleness');
    // It must compare TIMES, not just presence — presence is what failed.
    expect(src).toContain('mtimeMs');
    expect(src).toMatch(/\.rs['"]/);
    expect(src).toContain('Cargo.toml');
    expect(src).toContain('Cargo.lock');
  });

  it('a MISSING artifact is not reported as stale — that is ensure-wasm\'s job', () => {
    // Two different problems want two different messages. If `missing` were
    // folded into `stale`, a fresh clone would be told to run `build:wasm`
    // when `npm install` was about to build it anyway.
    const src = read('scripts/wasm-freshness.mjs');
    expect(src).toMatch(/if \(!existsSync\(WASM_PATH\)\)[\s\S]{0,200}stale: false/);
  });

  it('both callers use that one judgement rather than deciding for themselves', () => {
    for (const f of ['scripts/ensure-wasm.mjs', 'scripts/check-wasm-fresh.mjs']) {
      const src = read(f);
      expect(src, `${f} must import the shared judgement`).toContain(
        "from './wasm-freshness.mjs'",
      );
      expect(src, `${f} must call it`).toContain('wasmFreshness(');
    }
  });

  it('ensure-wasm rebuilds when stale, not only when the file is absent', () => {
    const src = read('scripts/ensure-wasm.mjs');
    // The old body was `if (wasmIsPresent()) exit(0)`. Presence alone must no
    // longer be enough to skip the build.
    expect(src).toMatch(/wasmIsPresent\(\)\s*&&\s*!\s*\w+\.stale/);
  });

  it('`npm run build` is gated on it', () => {
    const pkg = JSON.parse(read('package.json'));
    expect(pkg.scripts.prebuild).toBe('node scripts/check-wasm-fresh.mjs');
    // npm runs `prebuild` automatically before `build`; the gate is worth
    // nothing if `build` is renamed out from under it.
    expect(pkg.scripts.build).toContain('vite build');
  });

  it('the gate refuses by default and names its own escape hatch', () => {
    const src = read('scripts/check-wasm-fresh.mjs');
    expect(src).toContain('process.exit(1)');
    expect(src).toContain('AXIA_ALLOW_STALE_WASM');
    // The refusal must say what to run. A gate that only says "no" gets
    // bypassed by whoever hits it first.
    expect(src).toContain('npm run build:wasm');
  });

  it('CI still builds the WASM before the web build, so the gate never fires there', () => {
    for (const wf of ['build.yml', 'ci.yml', 'deploy.yml']) {
      const src = read(`../.github/workflows/${wf}`);
      const wasmAt = src.indexOf('wasm-pack build');
      const buildAt = src.indexOf('npm run build');
      expect(wasmAt, `${wf} must build WASM`).toBeGreaterThan(-1);
      expect(buildAt, `${wf} must build the web app`).toBeGreaterThan(-1);
      expect(wasmAt, `${wf}: wasm-pack must come first`).toBeLessThan(buildAt);
    }
  });
});
