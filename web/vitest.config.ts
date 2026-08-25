import { defineConfig } from 'vitest/config';
import { resolve } from 'path';

export default defineConfig({
  test: {
    // Every file, including the pure-logic ones. That is deliberate, and it
    // was measured on 2026-08-25 rather than assumed.
    //
    // `vitest run --environment node` over the whole suite: 97 of 188 files
    // pass without a DOM, 91 fail. jsdom setup is the largest single summed
    // component of a run — 160.93s against 21.43s of actual test bodies. But
    // summed is not wall clock: putting ALL 188 on node took the suite from
    // 29.33s to 23.24s, and only ~58 of the 97 passers are SAFE to switch
    // (39 of them import three, whose mock guards on `typeof document` and
    // would silently take the other branch). So the honest ceiling is ~2s of
    // a 29s suite, for ~58 per-file docblocks.
    //
    // And it cannot be captured config-side: the two sets do not separate by
    // directory. `tools` is 32 jsdom-needing against 41 not; `ui` is 29
    // against 9. A globbed two-project split would mean enumerating ~91
    // paths in this file — a list that rots silently, which is worse than a
    // marker sitting next to the code.
    //
    // There is also a hazard on the node side that does not announce itself:
    // src/i18n/index.ts runs `detect()` at module scope, which falls through
    // to `navigator.language`. `navigator` EXISTS in Node, so it returns the
    // OS locale instead of jsdom's 'en-US' — no throw, just a different
    // answer. That is CLAUDE.md L-98-4 all over again.
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts'],
      exclude: ['src/wasm/**', 'src/**/*.test.ts', 'src/**/*.d.ts'],
    },
  },
  resolve: {
    alias: [
      { find: '@', replacement: '/src' },
      // Mock Three.js — exact match only so three/examples/... still resolves from node_modules
      { find: /^three$/, replacement: resolve(__dirname, 'src/__mocks__/three.ts') },
    ],
  },
});
