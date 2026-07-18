// Reference-set generator.
// Recreates the shapes from the user's reference image as icons. Several are
// DOUBLE-LINE (wall/offset style). Red = pick-point DOTS, green = hatch.
// NOT applied to the engine.
//   node icons-archive/accent-drafts/reference-set/generate.mjs   (from web/)
import { writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const here = dirname(fileURLToPath(import.meta.url));

const RED = '#ef4444', GREEN = '#22c55e';
const dot = (x, y, r = 1.7) => `<circle class="accent-fill" cx="${x}" cy="${y}" r="${r}"/>`;

const ICONS = [
  { key:'region-nested', label:'영역/사각형 (region)', note:'이중 사각 + 빨간 점(코너)',
    inner:`<rect x="4" y="4" width="16" height="16"/><rect x="8.5" y="8.5" width="7" height="7"/>${dot(4,4)}${dot(20,4)}${dot(4,20)}${dot(20,20)}` },
  { key:'line', label:'선 (double line)', note:'두 줄',
    inner:`<path d="M4 10 L20 10"/><path d="M4 14 L20 14"/>` },
  { key:'corner-open', label:'모서리/벽 코너 A', note:'두 줄 ㄴ',
    inner:`<path d="M4 19 L4 5 L18 5"/><path d="M8 19 L8 9 L18 9"/>` },
  { key:'polyline', label:'폴리라인 B', note:'두 줄',
    inner:`<path d="M4 20 L4 4 L20 4 L20 12"/><path d="M8 20 L8 8 L16 8 L16 12"/>` },
  { key:'channel', label:'채널/브래킷 ( [ )', note:'두 줄',
    inner:`<path d="M18 4 L4 4 L4 20 L18 20"/><path d="M18 8 L8 8 L8 16 L18 16"/>` },
  { key:'arc-u', label:'호/U 곡선', note:'두 줄',
    inner:`<path d="M4 5 L4 12 A 8 8 0 0 0 20 12 L20 5"/><path d="M8 5 L8 12 A 4 4 0 0 0 16 12 L16 5"/>` },
  { key:'arrow-2way', label:'양방향 화살 (빨강)', note:'red ↔ (아이콘 전체 빨강)',
    inner:`<path class="accent" d="M4 12 L20 12 M7.5 8.5 L4 12 L7.5 15.5 M16.5 8.5 L20 12 L16.5 15.5"/>` },
  { key:'corner-mark', label:'벽 코너 + 빨간 점', note:'두 줄 ㄴ + 빨간 점',
    inner:`<path d="M6 4 L6 18 L20 18"/><path d="M10 4 L10 14 L20 14"/>${dot(6,18,2)}` },
  { key:'fillet', label:'필렛(둥근 벽 코너)', note:'두 줄, 빨강 없음',
    inner:`<path d="M4 3 L4 12 A 8 8 0 0 0 12 20 L21 20"/><path d="M8 3 L8 12 A 4 4 0 0 0 12 16 L21 16"/>` },
  { key:'ring', label:'링/동심원', note:'이중 원',
    inner:`<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3.8"/>` },
  { key:'diamond-nested', label:'다이아 중첩', note:'이중 다이아',
    inner:`<path d="M12 3 L21 12 L12 21 L3 12 Z"/><path d="M12 8 L16 12 L12 16 L8 12 Z"/>` },
  { key:'triangle', label:'삼각형', note:'단선, 빨강 없음',
    inner:`<path d="M5 5 L5 19 L19 19 Z"/>` },
  { key:'boundary-poly', label:'경계 폴리곤', note:'집/펜타곤',
    inner:`<path d="M4 20 L4 10 L12 4 L20 10 L20 20 Z"/>` },
  { key:'rect-basepoint', label:'사각형 + 빨간 점', note:'단선 + 빨간 점(코너)',
    inner:`<rect x="5" y="6" width="14" height="12"/>${dot(5,18,2)}` },
  { key:'triangle-diag', label:'삼각형 B', note:'단선, 빨강 없음',
    inner:`<path d="M5 19 L19 19 L19 5 Z"/>` },
  { key:'quad', label:'사변형/트래피조이드', note:'단선, 빨강 없음',
    inner:`<path d="M4 18 L7 7 L19 5 L20 16 Z"/>` },
  { key:'lens', label:'렌즈/뾰족 타원', note:'vesica',
    inner:`<path d="M12 3 Q 18 12 12 21 Q 6 12 12 3 Z"/>` },
  { key:'eight', label:'8 (이중 루프)', note:'double circle',
    inner:`<circle cx="12" cy="8" r="4.3"/><circle cx="12" cy="16" r="5.3"/>` },
  { key:'hatch-green', label:'해치(초록)', note:'green hatch fill',
    inner:`<rect class="hatch" x="4" y="4" width="16" height="16"/><path class="hatch" d="M4 12 L12 4 M4 20 L20 4 M12 20 L20 12 M4 8 L8 4 M16 20 L20 16"/>` },
];

function standalone({ inner }) {
  const body = inner
    .replace(/class="accent-fill"/g, `fill="${RED}" stroke="none"`)
    .replace(/class="accent"/g, `stroke="${RED}"`)
    .replace(/class="hatch"/g, `stroke="${GREEN}"`)
    .replace(/class="filled"/g, `fill="currentColor" stroke="none"`);
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>\n`;
}

const svgDir = join(here, 'svg');
rmSync(svgDir, { recursive: true, force: true });
mkdirSync(svgDir, { recursive: true });
for (const ic of ICONS) writeFileSync(join(svgDir, ic.key + '.svg'), standalone(ic));
writeFileSync(join(here, 'icons.json'), JSON.stringify(ICONS, null, 2) + '\n');

const esc = s => s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
let cells = '';
for (const ic of ICONS) {
  cells += `<figure class="cell"><div class="chip"><svg viewBox="0 0 24 24">${ic.inner}</svg></div>`
        +  `<figcaption><b>${esc(ic.label)}</b><span class="fn">${esc(ic.key)}.svg</span><span class="nt">${esc(ic.note)}</span></figcaption></figure>\n`;
}
const catalog = `<!doctype html><html lang="ko"><head><meta charset="utf-8">
<title>참조 아이콘 세트 (WIP)</title>
<style>
  :root{ --bg:#0e141b; --ink:#e6edf4; --sub:#8595a5; --line:#26333f; }
  body{ margin:0; background:var(--bg); color:var(--ink); font-family:system-ui,-apple-system,"Segoe UI",sans-serif; }
  .wrap{ max-width:940px; margin:0 auto; padding:26px 18px 70px; }
  h1{ font-size:20px; margin:0 0 4px; }
  p.lead{ color:var(--sub); font-size:13px; margin:0 0 6px; }
  .warn{ background:rgba(239,68,68,0.12); border:1px solid rgba(239,68,68,0.4); border-radius:8px; padding:9px 12px; font-size:12.5px; margin:12px 0 18px; }
  .grid{ display:grid; grid-template-columns:repeat(auto-fill,minmax(120px,1fr)); gap:12px; }
  .cell{ margin:0; text-align:center; }
  .chip{ width:100%; aspect-ratio:1; border-radius:11px; background:#14161c; border:1px solid var(--line); display:grid; place-items:center; color:#e8e8e8; }
  .chip svg{ width:40px; height:40px; fill:none; stroke:currentColor; stroke-width:2; stroke-linecap:round; stroke-linejoin:round; }
  .chip svg .filled{ fill:currentColor; stroke:none; }
  .chip svg .accent{ stroke:${RED}; }
  .chip svg .accent-fill{ fill:${RED}; stroke:none; }
  .chip svg .hatch{ stroke:${GREEN}; }
  figcaption{ font-size:11px; margin-top:6px; line-height:1.35; }
  figcaption b{ display:block; }
  figcaption .fn{ display:block; color:var(--sub); font:10px ui-monospace,monospace; margin-top:1px; }
  figcaption .nt{ display:block; color:var(--sub); font-size:10px; margin-top:1px; }
</style></head><body><div class="wrap">
<h1>참조 아이콘 세트 (WIP)</h1>
<p class="lead">사용자 참조 이미지의 모양들을 우리 SVG 스타일로 재현 (${ICONS.length}종). 2·3·4·5·6·8·9=이중선, 빨강=점, 초록=해치.</p>
<div class="warn">⚠️ 참조 이미지가 저해상도라 <b>제 해석</b>이 들어갔습니다. 틀린 모양/이름은 알려주시면 고칩니다. <b>엔진 미적용</b> — 드래프트 보관용.</div>
<div class="grid">${cells}</div>
</div></body></html>\n`;
writeFileSync(join(here, 'index.html'), catalog);
console.log(`Wrote ${ICONS.length} reference icons`);
