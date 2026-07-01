/**
 * Text3DBuilder — 3D 텍스트 geometry 빌더 (ADR-228). **LAZY-ONLY 모듈.**
 *
 * 반드시 `await import('./Text3DBuilder')` 로만 로드된다 — Vite 가 이 모듈의
 * 정적 의존 (FontLoader + TextGeometry + helvetiker typeface.json) 전체를 하나의
 * lazy chunk 로 code-split 한다. 따라서 초기 번들 0MB 증가 (ADR-035 P20.C #2 strict)
 * 보존. 이 모듈을 top-level (static) import 하면 폰트가 초기 번들에 누설되므로 금지.
 *
 * 폰트: three 예제 helvetiker_regular (MAGENTA/MgOpen permissive — 패키지 일부
 * 재배포 허용). Latin 전용. 한국어 등은 buildExtrudedText 가 null 반환 → caller
 * 가 buildSpriteText 로 fallback.
 */

import * as THREE from 'three';
import { FontLoader, type Font } from 'three/examples/jsm/loaders/FontLoader.js';
import { TextGeometry } from 'three/examples/jsm/geometries/TextGeometry.js';
import helvetikerJson from 'three/examples/fonts/helvetiker_regular.typeface.json';

/** ADR-018 계열 가독 색 (edge #333366 과 일관). */
const TEXT_COLOR = 0x333366;

let _font: Font | null = null;
function getFont(): Font {
  if (!_font) {
    _font = new FontLoader().parse(
      helvetikerJson as unknown as Parameters<FontLoader['parse']>[0],
    );
  }
  return _font;
}

export interface TextBuildOpts {
  /** Glyph height in mm (default 100). */
  size?: number;
  /** Extrude depth in mm (default size * 0.2). */
  depth?: number;
  /** Hex color (default TEXT_COLOR). */
  color?: number;
}

/**
 * Extruded true-3D text (TextGeometry). Built in local XY (reading +X, height
 * +Y, extrude +Z), centered at origin — the caller positions + orients it to
 * the draw plane. Returns `null` when the Latin font produces no renderable
 * glyphs for the string (e.g. Korean) → caller falls back to {@link buildSpriteText}.
 */
export function buildExtrudedText(text: string, opts: TextBuildOpts = {}): THREE.Mesh | null {
  const size = opts.size ?? 100;
  const depth = opts.depth ?? size * 0.2;
  const font = getFont();
  // Reject strings containing glyphs absent from the (Latin) font — e.g. Korean.
  // The font renders garbage `.notdef` boxes for missing glyphs (a non-empty
  // bounding box), so a geometry check alone can't detect them. Gate on the
  // font's glyph map up-front → caller falls back to a sprite (system font).
  const glyphs = (font.data as { glyphs?: Record<string, unknown> } | undefined)?.glyphs;
  if (glyphs) {
    for (const ch of text) {
      if (ch.trim() === '') continue; // whitespace needs no glyph
      if (!glyphs[ch]) return null; // unsupported glyph → sprite fallback
    }
  }
  let geo: TextGeometry;
  try {
    geo = new TextGeometry(text, {
      font,
      size,
      depth,
      curveSegments: 6,
      bevelEnabled: false,
    });
  } catch {
    return null; // font/shape generation failed (unsupported glyphs)
  }
  geo.computeBoundingBox();
  const bb = geo.boundingBox;
  // Degenerate (no glyphs rendered — e.g. font lacks the characters) → null.
  if (!bb) return null;
  const w = bb.max.x - bb.min.x;
  const h = bb.max.y - bb.min.y;
  if (!(w > 1e-6) || !(h > 1e-6)) return null;
  // Center horizontally + vertically so position = label center.
  geo.translate(-bb.min.x - w / 2, -bb.min.y - h / 2, 0);
  const mat = new THREE.MeshStandardMaterial({
    color: opts.color ?? TEXT_COLOR,
    roughness: 0.6,
    metalness: 0.1,
  });
  const mesh = new THREE.Mesh(geo, mat);
  mesh.name = 'text3d-extruded';
  return mesh;
}

/**
 * Billboard sprite label (canvas texture). Renders ANY string the system font
 * supports — including Korean — at zero bundle cost. Camera-facing (not true
 * 3D). Mirrors DxfSceneBuilder.buildText (ADR canvas-sprite pattern).
 */
export function buildSpriteText(text: string, opts: TextBuildOpts = {}): THREE.Sprite | null {
  const size = opts.size ?? 100;
  const canvas = document.createElement('canvas');
  const probe = canvas.getContext('2d');
  if (!probe) return null;
  const fontPx = 64;
  probe.font = `${fontPx}px sans-serif`;
  const textW = Math.max(16, Math.ceil(probe.measureText(text).width));
  canvas.width = textW + 16;
  canvas.height = fontPx + 32;
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.font = `${fontPx}px sans-serif`;
  ctx.fillStyle = `#${(opts.color ?? TEXT_COLOR).toString(16).padStart(6, '0')}`;
  ctx.textBaseline = 'middle';
  ctx.fillText(text, 8, canvas.height / 2);
  const tex = new THREE.CanvasTexture(canvas);
  tex.needsUpdate = true;
  const mat = new THREE.SpriteMaterial({ map: tex, transparent: true, depthTest: false });
  const sprite = new THREE.Sprite(mat);
  const aspect = canvas.width / canvas.height;
  sprite.scale.set(size * aspect, size, 1);
  sprite.name = 'text3d-sprite';
  return sprite;
}
