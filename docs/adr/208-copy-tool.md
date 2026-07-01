# ADR-208 — Copy / Duplicate Tool (arrayLinearFaces count=1 reuse)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Foundation Tier 1 도구 (ADR-168 audit 정정 → ADR-206 의 C)
- **Depends on**: array_linear_faces (array_op.rs) / ADR-206 (audit + Pattern-12) /
  MoveTool (2-click 패턴) / ADR-046 P31 #4

## 1. Context

ADR-206 Foundation Tier 1 audit 의 잔여 C. 사용자 facing **복제(Duplicate)** 도구 부재 —
clipboard-copy (Ctrl+C/V/D) 와 별개의 *offset 복제*. audit 은 "copy clone op 부재" 로 보았으나,
de-risk 결과 **`array_linear_faces(count=1)` 으로 깨끗이 표현** 가능: count=1 = offset 위 1개
복제 + 원본 보존. `array_linear_faces` 는 **engine + WASM + bridge 모두 존재** (6 tests) →
**ADR-208 = UI-only** (Pattern-12, engine/WASM/bridge 신규 0).

## 2. Decision

Copy = `arrayLinearFaces(faces, count=1, offset)`. `CopyTool` (MoveTool 답습 2-click) 가
선택 면 + base/target click 으로 offset 을 정의 → 1개 복제. 신규 clone op 불필요.

## 3. Lock-ins

- **L-208-1** Copy = `array_linear_faces(count=1)` 재사용 — engine/WASM/bridge 신규 0
  (Pattern-12).
- **L-208-2** count=1 semantics = offset 위 1 복제 + 원본 보존 (de-risk lock).
- **L-208-3** CopyTool = MoveTool 답습 2-click (select faces → base → target) + VCB
  (axis distance).
- **L-208-4** clipboard-copy (Ctrl+C/V/D) 와 별개 — 본 도구는 in-place duplicate-at-offset.
- **L-208-5** ADR-046 P31 #4 additive only / 절대 #[ignore] 금지.
- **L-208-6** (future) Ctrl-drag MoveTool = 자동 Copy 모드 (산업 CAD parity) — 별도.

## 4. 구현 (단일 atomic `6d72292`)

- **de-risk** (array_op.rs): `adr208_copy_count1_preserves_original_and_renders` — count=1 →
  1 copy at +offset, 원본 보존 (face_count +1), 양 면 render, invariants valid.
- **β** (UI only): `CopyTool` (2-click select → base → target → `arrayLinearFaces(faces, 1,
  offset)`) + ToolManager `'copy'` + Modify 메뉴 "복제" + `tool-copy` command + 8 vitest.

## 5. 회귀 + 검증

- **회귀**: axia-geo +1 (de-risk) / CopyTool +8. tsc clean, 0 regression, #[ignore] 0.
- **브라우저** (real WASM): `create_box` → `arrayLinearFaces([face], 1, [20,0,0])` → 1 copy,
  **6 → 7 faces** (원본 + 1 복제).

## 6. 후속 (별도 ADR — Foundation Tier 1 잔여)

- **ADR-209** interactive UX 폴리시 (5 wired 도구 live preview; 메뉴 이미 작동, marginal).
- Ctrl-drag MoveTool Copy 모드 (산업 CAD parity).

전체 corrected spec: `reports/ADR_206_FoundationTier1Tools_CorrectedSpec.md`.
