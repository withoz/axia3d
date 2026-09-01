/**
 * The desktop icon, generated from the app's own mark.
 *
 * ⚠ The mark is not invented here and must not be. It is the cube wireframe
 * already inlined in `web/index.html` as the favicon, in the same `#74c0fc` —
 * this script reads it out of that file, so the window icon, the taskbar and
 * the browser tab can never drift apart. What is added is a ground: a
 * transparent 16px icon of three thin strokes is unreadable in a taskbar, and
 * the ground is the app's own chrome colour rather than a new one.
 *
 * Run from the repository root:  node desktop/scripts/render-icons.mjs
 *
 * ⚠ It lives in the repo rather than in a scratch directory for a reason
 * measured the hard way: Node resolves a bare specifier by walking up from the
 * IMPORTING FILE's directory, not the working directory, so the same script
 * outside the repo cannot find `playwright` at all.
 */
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = join(HERE, '..', '..');
const OUT = join(HERE, '..', 'icons');

/** The favicon out of index.html — the single source of the mark. */
function markSvg() {
  const html = readFileSync(join(REPO, 'web', 'index.html'), 'utf8');
  const m = html.match(/href="data:image\/svg\+xml,([^"]+)"/);
  if (!m) throw new Error('no inline svg favicon in web/index.html — the mark moved');
  return decodeURIComponent(m[1]);
}

/** The tile: mark at 62% on the app's chrome colour, corner radius 18%. */
function tile(svg, px) {
  return `<!doctype html><html><body style="margin:0">
<div style="width:${px}px;height:${px}px;display:grid;place-items:center;
     background:#23232c;border-radius:${Math.round(px * 0.18)}px">
  <div style="width:${Math.round(px * 0.62)}px;height:${Math.round(px * 0.62)}px">
    ${svg.replace('<svg ', '<svg width="100%" height="100%" ')}
  </div>
</div></body></html>`;
}

/**
 * A Windows .ico that embeds the PNGs as-is.
 *
 * Vista and later accept PNG payloads inside an ICO, so no bitmap encoder is
 * needed. A dimension of 256 is written as 0 — the field is one byte.
 */
function ico(pngs) {
  const head = Buffer.alloc(6 + pngs.length * 16);
  head.writeUInt16LE(0, 0); // reserved
  head.writeUInt16LE(1, 2); // 1 = icon
  head.writeUInt16LE(pngs.length, 4);
  let offset = head.length;
  pngs.forEach(({ size, data }, i) => {
    const e = 6 + i * 16;
    head.writeUInt8(size >= 256 ? 0 : size, e);
    head.writeUInt8(size >= 256 ? 0 : size, e + 1);
    head.writeUInt8(0, e + 2); // palette size, 0 = no palette
    head.writeUInt8(0, e + 3); // reserved
    head.writeUInt16LE(1, e + 4); // colour planes
    head.writeUInt16LE(32, e + 6); // bits per pixel
    head.writeUInt32LE(data.length, e + 8);
    head.writeUInt32LE(offset, e + 12);
    offset += data.length;
  });
  return Buffer.concat([head, ...pngs.map((p) => p.data)]);
}

const SIZES = [1024, 512, 256, 128, 64, 48, 32, 16];
/** What Windows actually reaches for; 1024 and 512 are for the other bundles. */
const IN_ICO = [256, 128, 64, 48, 32, 16];

const svg = markSvg();
mkdirSync(OUT, { recursive: true });
const browser = await chromium.launch();
const rendered = new Map();
for (const px of SIZES) {
  const page = await browser.newPage({ viewport: { width: px, height: px }, deviceScaleFactor: 1 });
  await page.setContent(tile(svg, px));
  const data = await page.screenshot({ omitBackground: true });
  await page.close();
  writeFileSync(join(OUT, `${px}x${px}.png`), data);
  rendered.set(px, data);
  console.log(`  ${px}x${px}.png  ${data.length} bytes`);
}
await browser.close();

const icoBytes = ico(IN_ICO.map((size) => ({ size, data: rendered.get(size) })));
writeFileSync(join(OUT, 'icon.ico'), icoBytes);
console.log(`  icon.ico     ${icoBytes.length} bytes  (${IN_ICO.join(', ')})`);
