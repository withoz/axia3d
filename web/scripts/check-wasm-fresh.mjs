/**
 * Refuse `npm run build` when the built WASM is older than the Rust.
 *
 * Runs as npm's automatic `prebuild`. `npm run build` already REQUIRES the
 * artifact (tsc cannot resolve `src/wasm/axia_wasm` without it), so requiring a
 * FRESH one is a tightening of a rule that is already there, not a new one.
 *
 * Escape hatch for someone who knowingly wants the old engine:
 *
 *     AXIA_ALLOW_STALE_WASM=1 npm run build
 *
 * CI is unaffected — build.yml / ci.yml / deploy.yml each run `wasm-pack build`
 * before `npm run build`, so the artifact is always the newer of the two there.
 */

import { wasmFreshness, describeStaleness } from './wasm-freshness.mjs';

const f = wasmFreshness();

if (!f.stale) {
  process.exit(0);
}

if (process.env.AXIA_ALLOW_STALE_WASM === '1') {
  console.warn(
    `[check-wasm-fresh] ⚠ ${describeStaleness(f)}\n` +
      '[check-wasm-fresh] AXIA_ALLOW_STALE_WASM=1 이므로 계속합니다 — ' +
      '이 빌드는 옛 엔진을 담습니다.',
  );
  process.exit(0);
}

console.error(`
[check-wasm-fresh] 빌드된 WASM 이 낡았습니다.

  ${describeStaleness(f)}

  cargo test 가 초록이어도 브라우저는 옛 엔진을 돌립니다. 2026-08-21 에
  13일치 엔진 변경(커밋 90건)이 이렇게 가려져 있었습니다.

    cd web && npm run build:wasm

  뒤에 다시 빌드하세요. 옛 엔진으로 일부러 빌드하려면:

    AXIA_ALLOW_STALE_WASM=1 npm run build
`);
process.exit(1);
