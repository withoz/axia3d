# ADR-228 — 3D Text Tool (render-only, extruded + sprite toggle)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: 24-tool follow-up — text3d feature build (greenfield)
- **Depends on**: ADR-035 P20.C #2 (initial bundle 0MB) / ADR-095 (Reference citizenship) /
  ADR-219 (DrawPointTool / standalone render layer) / ADR-046 Q7 (i18n Korean+English) /
  메타-원칙 #2 (외부 참조 = 형태/모양만)

## 1. Context

`tool-text3d` 는 catalog stub (`status:'stub'`) + MenuBar "준비 중" 만 있던 greenfield
기능. de-risk 3-facet workflow (geometry approach / citizenship+render / font+bundle+UI)로
설계 확정:
- **Geometry**: (A) Three.js TextGeometry render-only / (B) kernel-native glyph→`add_face_
  closed_curve` editable DCEL (L-XL — ttf-parser crate + holed-glyph multi-loop gap) / (C)
  TextGeometry triangle inject (trap — per-boundary-loop API 부적합). **MVP = A (render-only)**.
- **Citizenship**: 엔진 `ReferenceCategory`(reference.rs)는 Edge/Face/Vert id 바인딩 — 텍스트는
  DCEL geometry 0 → 4번째 variant는 ownership 모델 파괴. → **render-only Three.js scene-root
  overlay** (메타-원칙 #2, 엔진 미주입), DxfSceneBuilder canvas-sprite 선례.
- **Font/Bundle**: ADR-035 P20.C #2 strict → 폰트 + TextGeometry/FontLoader 전부 lazy.

사용자 결재: **A1+A2 둘 다 지원 — 토글**.

## 2. Decision — extruded + sprite 토글 (render-only)

| | A1 extruded | A2 sprite |
|---|---|---|
| geometry | TextGeometry 진짜 3D 압출 mesh | Canvas 텍스처 billboard |
| 폰트 | lazy helvetiker (Latin) | 시스템 폰트 (canvas) |
| 한국어 | glyph 미수록 → **자동 sprite fallback** | ✅ 즉시 |
| 결과 | 3D 공간 고정, draw-plane 배향 | 카메라 대면 라벨 |

- **DrawText3DTool** (ITool, `tools.set('text3d')`) — `onActivate()` prompt 로 문자열 입력
  (localStorage default, DrawPolygonTool 패턴) + click 배치 (DrawPointTool 패턴, 연속). Mode 는
  Text3DSettings.
- **Text3DBuilder.ts** (LAZY-only, `await import()`) — `buildExtrudedText` (TextGeometry +
  helvetiker, glyph 미수록 시 null) + `buildSpriteText` (canvas sprite). Vite 가 전체 subtree
  (FontLoader + TextGeometry + 폰트 JSON)를 단일 lazy chunk 로 code-split.
- **Text3DSettings.ts** — mode `'extruded'`(default) | `'sprite'`, localStorage
  `axia:text3d-mode` (AutoIntersectSettings 패턴). **SettingsPanel 토글** (`#sp-text3d-sprite`,
  체크=sprite).
- **Korean 자동 fallback** — extruded mode + 폰트 glyph map(`font.data.glyphs`)에 미수록 글자
  (한글 등) → `buildExtrudedText` null → tool 이 `buildSpriteText` 로 fallback + Toast.
  (helvetiker 가 미수록 글자를 `.notdef` 박스로 렌더 → bbox-degenerate 휴리스틱 부족 →
  **glyph map up-front gate** 가 reliable.)
- **Viewport** — `_textOverlay` THREE.Group **scene root** (meshGroup wipe 무관, ADR-219 Points
  와 달리 per-sync re-push 불요) + `addTextObject` / `clearTextObjects`.
- **catalog.ts** — `tool-text3d` status `'stub'` → `'ui-only'` (render-only TS). catalog.test.ts
  stub list 에서 제거.

## 3. Lock-ins

- **L-228-1** render-only — scene-root Three.js overlay (`Viewport._textOverlay`), 엔진 DCEL
  미주입 (메타-원칙 #2). ReferenceCategory enum 변경 0 (ownership 모델 보존).
- **L-228-2** A1+A2 toggle (사용자 결재) — extruded(default) + sprite, Text3DSettings + Settings
  토글. Korean = extruded 자동 sprite fallback (glyph map gate) / sprite 즉시.
- **L-228-3** lazy-only Text3DBuilder — `await import()` → 폰트 + TextGeometry/FontLoader 전부
  lazy chunk (64.89 kB). 초기 번들 폰트/geometry 누설 0 (grep 검증). ADR-035 P20.C #2 충족.
- **L-228-4** helvetiker (Latin) — MAGENTA/MgOpen permissive (패키지 일부 재배포 허용). Korean
  typeface = MB급 → Phase 2 (subset). ADR-046 Q7.
- **L-228-5** 문자열 입력 = `prompt()` + localStorage (DrawPolygonTool). VCB 숫자 전용이라 부적합.
- **L-228-6** Z-up / draw-plane 배향 (LOCKED #43) — extruded 는 getDrawPlane right/up/normal
  basis 로 배향, sprite 는 billboard (배향 불요).
- **L-228-7** ADR-046 P31 #4 additive — index.html 메뉴 항목(1891) + MenuBar case(395) 기존
  유지, tool 등록만으로 "준비 중" → 작동.
- **L-228-8** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+12** (Text3DSettings 5 + DrawText3DTool 7). 2369 → **2381** (158 files, 1 skipped).
- 패키지 catalog 24/24 (text3d stub list 제거 + status 'ui-only' 정합). tsc 0. 엔진/WASM 변경 0.
- **번들**: `Text3DBuilder` lazy chunk 64.89 kB (폰트 격리). 초기 번들 폰트 누설 0 (index grep:
  font glyph data 0, "TextGeometry" 매치는 catalog description 문자열만).

## 5. 브라우저 검증 (real three.js runtime)

- `hasTool('text3d')` = true (등록, "준비 중" 해소).
- `buildExtrudedText('ABC')` → Mesh **2568 verts** (진짜 3D 압출).
- `buildExtrudedText('한글')` / `'A한B'` → **null** (glyph gate → sprite fallback).
- `buildExtrudedText('Hi! 123')` → 정상 (Latin+숫자+구두점+공백).
- `buildSpriteText('한글 라벨')` → Sprite + CanvasTexture map.

## 6. 알려진 MVP 한계 (별도 follow-up)

- **persistence** — render-only scene-root overlay 라 .axia save/reload 미보존 (엔진 snapshot
  외). 별도 단계 (TS-side 직렬화 or kernel-native 승격).
- **editable/extrudable kernel text** (Approach B) — L-XL: ttf-parser crate + holed-glyph
  multi-loop (`add_face_closed_curve` single-self-loop only). 별도 ADR.
- **Korean extruded 3D** — Latin 폰트만 lazy 번들 → 한국어는 sprite. Korean typeface subset
  (per-string) = Phase 2 (ADR-046 Q7).
- **trim/extend stale-stub** — ADR-211 로 구현됐으나 catalog.ts status 여전히 `'stub'` (작동하는
  tool 인데 'stub' 오라벨, ADR-226 explode 패턴). catalog.test.ts stub list 에 잔존. 별도 정리
  follow-up.
- text3d 툴바 버튼 없음 (메뉴 only) — 의도 (24-tool 외 follow-up).

## 7. Cross-link

- ADR-035 P20.C #2 (initial bundle 0MB — 폰트 lazy chunk) / ADR-095 (Reference citizenship —
  text 는 render-only, enum 미변경) / ADR-219 (DrawPointTool click-place + 전용 render layer) /
  ADR-046 Q7 (i18n) / ADR-018 (text color #333366) / ADR-140·166·168 (getDrawPlane 배향).
- DxfSceneBuilder.buildText (canvas-sprite 선례) / AutoIntersectSettings (settings 패턴) /
  DrawPolygonTool (prompt + localStorage) / ADR-226 (status 오라벨 패턴 — trim/extend follow-up).
- ADR-046 P31 #4 (additive only) / 메타-원칙 #2 (외부 참조 형태만) / LOCKED #44 (Complete Meaning).
