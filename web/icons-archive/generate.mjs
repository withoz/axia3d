// Toolbar icon archive generator.
// Reads ../index.html, extracts every toolbar icon, and writes:
//   svg/<key>.svg   — self-contained, reusable SVG (uses currentColor)
//   icons.json      — { key, viewBox, style, inner } for each icon
//   index.html      — a browsable catalog (open in any browser)
//
// Re-run after changing toolbar icons:  node icons-archive/generate.mjs   (from web/)
import { JSDOM } from 'jsdom';
import { readFileSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(here, '..', 'index.html'), 'utf8');
const doc = new JSDOM(html).window.document;

// ---- extract (same logic as the live-DOM audit) ----------------------------
const seen = new Map();
const icons = [];
const add = (el, forcedKey) => {
  const svg = el.querySelector('svg');
  if (!svg) return;
  let key = forcedKey || el.getAttribute('data-tool') || el.getAttribute('data-action')
          || el.getAttribute('data-toggle') || (el.id ? el.id.replace(/-btn$/, '') : null);
  if (!key) return;
  const inner = svg.innerHTML.replace(/\s+/g, ' ').trim();
  const vb = svg.getAttribute('viewBox') || '0 0 24 24';
  const style = svg.getAttribute('style') || '';
  const sig = vb + '||' + inner;
  if (seen.has(key)) { if (seen.get(key) === sig) return; key += '-2'; }
  seen.set(key, sig);
  icons.push({ key, viewBox: vb, style, inner });
};
const toolbar = doc.getElementById('toolbar');
toolbar.querySelectorAll('.tool-btn, .tool-dropdown-item').forEach(el => add(el));
const home = doc.getElementById('home-btn');
if (home) add(home, 'home');

// ---- build a self-contained SVG (currentColor, no external CSS) -------------
function standalone({ viewBox, style, inner }) {
  const isFA = !viewBox.startsWith('0 0 24 24');   // 512/576 glyphs are solid fills
  let attrs, body = inner;
  if (isFA) {
    attrs = 'fill="currentColor" stroke="none"';
  } else {
    attrs = 'fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"';
    if (style) attrs += ` style="${style}"`;
    // class="filled" relies on toolbar CSS; bake it in for standalone use
    body = body.replace(/class="filled"/g, 'fill="currentColor" stroke="none"');
  }
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}" ${attrs}>${body}</svg>\n`;
}

// ---- category grouping for the catalog --------------------------------------
const CAT = {
  '그리기': ['select','line','centerline','freehand','bezier','spline','rect','polygon','rotrect','circle','arc','pie','hole'],
  '모델링·솔리드': ['pushpull','sweep','loft','plane','wall','window'],
  '프리미티브': ['sphere','cylinder','cone','box','torus','nurbs'],
  '변형': ['move','rotate','scale','offset','recess','erase','slice'],
  '유기·배열·엣지·디폼': ['subdivide','mirror-x','mirror-y','mirror-z','array-linear','array-radial','revolve-x','revolve-y','revolve-z','fillet-edge','chamfer-edge','bend-selection','twist-selection','taper-selection','thicken-faces'],
  '스케치·유틸': ['measure','sketch-start-xz','sketch-start-xz-2','sketch-start-xy','sketch-start-yz','sketch-start-face','sketch-exit','solidify','mesh-repair','synthesize-faces'],
  'Boolean': ['bool-union','bool-subtract','bool-intersect'],
  '그룹': ['group','ungroup','make-component','tool-explode'],
  '표시 토글': ['grid','ssao','shadow'],
  '실행취소/재실행': ['undo','redo'],
  '패널·홈': ['inspector','style','settings','home'],
};
const catOf = k => Object.entries(CAT).find(([, ks]) => ks.includes(k))?.[0] || '기타';

// ---- write svg files + icons.json -------------------------------------------
const svgDir = join(here, 'svg');
rmSync(svgDir, { recursive: true, force: true });
mkdirSync(svgDir, { recursive: true });
for (const ic of icons) writeFileSync(join(svgDir, ic.key + '.svg'), standalone(ic));
writeFileSync(join(here, 'icons.json'), JSON.stringify(icons, null, 2) + '\n');

// ---- write catalog index.html -----------------------------------------------
const groups = {};
for (const ic of icons) (groups[catOf(ic.key)] ??= []).push(ic);
const order = [...Object.keys(CAT), '기타'];
const esc = s => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
let cards = '';
for (const cat of order) {
  const list = groups[cat]; if (!list) continue;
  cards += `<h2>${cat} <span class="cnt">${list.length}</span></h2>\n<div class="grid">\n`;
  for (const ic of list) {
    cards += `  <figure class="cell"><div class="chip">${standalone(ic).trim()}</div>`
           + `<figcaption>${esc(ic.key)}<span class="fn">svg/${esc(ic.key)}.svg</span></figcaption></figure>\n`;
  }
  cards += `</div>\n`;
}
const catalog = `<!doctype html><html lang="ko"><head><meta charset="utf-8">
<title>AxiA 툴바 아이콘 아카이브</title>
<style>
  :root{ --bg:#0e141b; --card:#161f29; --ink:#e6edf4; --sub:#8595a5; --line:#26333f; --chip:#14161c; --accent:#33d6c2; }
  body{ margin:0; background:var(--bg); color:var(--ink); font-family:system-ui,-apple-system,"Segoe UI",sans-serif; }
  .wrap{ max-width:1040px; margin:0 auto; padding:26px 18px 80px; }
  h1{ font-size:20px; margin:0 0 4px; }
  p.lead{ color:var(--sub); font-size:13px; margin:0 0 8px; }
  h2{ font-size:12.5px; letter-spacing:.04em; color:var(--sub); text-transform:uppercase; margin:26px 0 10px; border-bottom:1px solid var(--line); padding-bottom:5px; }
  h2 .cnt{ color:var(--accent); font-weight:700; margin-left:4px; }
  .grid{ display:grid; grid-template-columns:repeat(auto-fill,minmax(96px,1fr)); gap:10px; }
  .cell{ margin:0; text-align:center; }
  .chip{ width:100%; aspect-ratio:1; border-radius:11px; background:var(--chip); border:1px solid var(--line); display:grid; place-items:center; color:#e8e8e8; }
  .chip svg{ width:34px; height:34px; }
  figcaption{ font-size:10.5px; color:var(--ink); margin-top:5px; word-break:break-all; }
  figcaption .fn{ display:block; color:var(--sub); font:10px ui-monospace,monospace; margin-top:1px; }
</style></head><body><div class="wrap">
<h1>AxiA 툴바 아이콘 아카이브</h1>
<p class="lead">현재 툴바 아이콘 ${icons.length}종의 스냅샷. 각 칩 아래는 재사용 가능한 파일 경로(<code>svg/&lt;key&gt;.svg</code>, currentColor).</p>
<p class="lead">아이콘 변경 후 갱신: <code>node icons-archive/generate.mjs</code> (web/ 에서 실행).</p>
${cards}</div></body></html>\n`;
writeFileSync(join(here, 'index.html'), catalog);

console.log(`Wrote ${icons.length} icons → icons-archive/svg/*.svg + icons.json + index.html`);
