/**
 * Fail the build when the first-load JS payload grows past its budget.
 *
 * CLAUDE.md LOCKED #28/#30/#33 record an initial bundle of ~725 kB, and roughly
 * ten ADRs after them went on asserting "724.99 UNCHANGED (P20.C #2)". Measured
 * 2026-08-25, the entry chunk is 1,151 kB — +58.6% on the same metric — and a
 * sweep for bundlesize / size-limit / bundlewatch / any CI step found nothing
 * checking it. The number had been a claim, not a constraint, for ~177 ADRs.
 *
 * ⚠ ADR-035 P20.C #2 itself is NOT violated: that rule says the OCCT dynamic
 * import must add 0 MB to the initial bundle, and `opencascade-deps` (5.4 MB)
 * appears zero times in dist/index.html. What drifted is the absolute baseline
 * those entries recorded. Nothing is functionally broken; what was missing is
 * anything that would notice.
 *
 * WHAT IS MEASURED — the bytes a browser fetches before it can run anything:
 * the entry `<script type="module">` plus every `<link rel="modulepreload">`.
 * Both come from dist/index.html.
 *
 * ⚠ Resolve through index.html, never by globbing dist/assets/index-*.js. The
 * build runs with `--emptyOutDir false` (a documented Windows permission
 * workaround, CLAUDE.md:8152), so dist/ holds a dozen stale entry chunks and a
 * glob picks an arbitrary one.
 *
 * To raise the budget, edit BUDGET below in the same commit as the growth, so
 * the increase is a line in a diff someone approved rather than a drift nobody
 * saw.
 */

import { readFileSync, statSync, existsSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const WEB = join(dirname(fileURLToPath(import.meta.url)), '..');
const DIST = join(WEB, 'dist');
const INDEX = join(DIST, 'index.html');

/**
 * Seeded at the 2026-08-25 measurement plus 2% headroom, so landing this is
 * green and the next real growth has to be an explicit bump.
 *
 *   entry      1,151,068 B
 *   preloaded    751,339 B   (three-loaders)
 *   first load 1,902,407 B
 */
const BUDGET = {
  entryBytes: 1_174_000,
  firstLoadBytes: 1_940_000,
};

const kb = (n) => `${(n / 1000).toFixed(2)} kB`;

if (!existsSync(INDEX)) {
  console.error('[check-bundle-size] dist/index.html 이 없습니다 — 빌드가 먼저입니다.');
  process.exit(1);
}

const html = readFileSync(INDEX, 'utf8');

const entryMatch = html.match(/<script[^>]*type="module"[^>]*src="([^"]+)"/);
const preloads = [...html.matchAll(/<link[^>]*rel="modulepreload"[^>]*href="([^"]+)"/g)].map(
  (m) => m[1],
);

// ⚠ Fail loudly on zero matches. A regex that silently stops matching would
// otherwise report a passing 0-byte total — the vacuous-guard shape this repo
// has been bitten by before (ADR-299 / ADR-302 / ADR-304).
// The preload count is part of the contract, not an incidental. dist/index.html
// has emitted exactly one modulepreload since the chunking was set up. ⚠ An
// earlier version of this script only guarded the entry regex, and mutating the
// PRELOAD regex to match nothing left it exiting 0 with 751 kB counted as zero —
// vacuous in the exact way the header warns about. Caught by mutation, so the
// count is asserted rather than assumed.
if (preloads.length === 0) {
  console.error(
    '[check-bundle-size] modulepreload 링크를 하나도 못 찾았습니다.\n' +
      '  Vite 가 태그 형식을 바꿨거나 청크 구성이 달라졌습니다. 0 바이트로\n' +
      '  통과시키면 예산이 무의미해지므로 실패시킵니다. 정말 preload 가 없어야\n' +
      '  한다면 이 검사를 지우는 것이 아니라 여기 이유를 적으세요.',
  );
  process.exit(1);
}
if (!entryMatch) {
  console.error(
    '[check-bundle-size] index.html 에서 entry <script type="module"> 를 못 찾았습니다.\n' +
      '  Vite 의 태그 형식이 바뀌었을 수 있습니다. 통과시키지 않고 실패시킵니다.',
  );
  process.exit(1);
}

const rel = (href) => resolve(DIST, href.replace(/^\.\//, ''));

function sizeOf(href) {
  const p = rel(href);
  if (!existsSync(p)) {
    console.error(
      `[check-bundle-size] index.html 이 가리키는 파일이 없습니다: ${href}\n` +
        '  중단된 빌드거나 stale index.html 입니다.',
    );
    process.exit(1);
  }
  return statSync(p).size;
}

const entryBytes = sizeOf(entryMatch[1]);
const preloadBytes = preloads.reduce((a, h) => a + sizeOf(h), 0);
const firstLoad = entryBytes + preloadBytes;

const lines = [
  `  entry       ${kb(entryBytes).padStart(12)}   ${entryMatch[1]}`,
  ...preloads.map((h, i) => `  preload     ${kb(sizeOf(h)).padStart(12)}   ${h}`),
  `  first load  ${kb(firstLoad).padStart(12)}   (예산 ${kb(BUDGET.firstLoadBytes)})`,
];
console.log('[check-bundle-size]\n' + lines.join('\n'));

const over = [];
if (entryBytes > BUDGET.entryBytes) {
  over.push(`entry ${kb(entryBytes)} > ${kb(BUDGET.entryBytes)}`);
}
if (firstLoad > BUDGET.firstLoadBytes) {
  over.push(`first load ${kb(firstLoad)} > ${kb(BUDGET.firstLoadBytes)}`);
}

if (over.length > 0) {
  console.error(`
[check-bundle-size] 초기 로드가 예산을 넘었습니다.

  ${over.join('\n  ')}

  브라우저가 무엇이든 실행하기 전에 받아야 하는 바이트입니다. 늘릴 만한
  이유가 있다면 web/scripts/check-bundle-size.mjs 의 BUDGET 을 같은 커밋에서
  올리세요 — 아무도 못 본 드리프트가 아니라, 누군가 승인한 diff 한 줄이 되도록.
`);
  process.exit(1);
}
