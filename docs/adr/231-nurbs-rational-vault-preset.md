# ADR-231 — NURBS Surface 고급화 (rational vault preset + mode toggle)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS surface 고급화 (ADR-224 DrawNurbsTool 후속, A1 scope)
- **Depends on**: ADR-224 (DrawNurbsTool — Bezier patch MVP) / ADR-033 (NURBS surfaces engine,
  AnalyticSurface::NURBSSurface) / ADR-228 (Text3DSettings mode-toggle 패턴) / 메타-원칙 #14

## 1. Context — 시뮬레이션 기반 결정

옵션(① ADR-197 곡면 Boolean edge case ② NURBS 고급화)을 상세 시뮬레이션으로 비교 (Boolean
β-4 defer 패턴 답습):

| | Track A — NURBS 고급화 | Track B — 곡면 Boolean edge case |
|---|---|---|
| 엔진 | **createNurbsSurface 완비** (rational/weights/knots, lib.rs:5605) | open 케이스 `deferred`/`bail!` (boolean.rs:942/4265/4615) |
| 시뮬 증거 | **rational quarter-cylinder 117정점 전부 r=500 정확 원호 (maxDev 2e-5mm, surfaceKind 8)** | torus pinched = α-de-risk sim만 (production 미배선) |
| gap | **도구만** (DrawNurbsTool=createBezierPatch uniform) | 깊은 spiric/SSI 기하 |
| 분류 | **Pattern-12, S-M** | M-L per case, 다일, 특수 |

→ **Track A 채택** (사용자 결재 A1). Track B 는 deep multi-day → 별도 dedicated ADR defer.

## 2. Decision — A1: rational vault 프리셋 + mode 토글

DrawNurbsTool 에 patch mode 토글 추가 (NurbsPatchSettings, default 'bezier' → 기존 동작 보존):
- **'bezier'** (default): 기존 4×4 uniform bicubic Bezier bulge (createBezierPatch, ADR-224).
- **'vault'**: **정확한 rational 반원통 vault** (createNurbsSurface). footprint WIDTH (along
  `right`, duLen) = 반원 diameter (radius r = |duLen|/2, peak = r along `normal`); LENGTH
  (along `up`, dvLen) = linear extrude. Cross-section = canonical 2-span rational semicircle
  (5 control points, weights [1, 1/√2, 1, 1/√2, 1], knots [0,0,0,.5,.5,1,1,1], degree 2) ×
  degree-1 extrude → **EXACT circular arc** (uniform Bezier 가 표현 불가능한 conic surface).

**구현**:
- `web/src/tools/NurbsPatchSettings.ts` (신규) — mode 'bezier'|'vault', localStorage
  `axia:nurbs-patch-mode`, listener (Text3DSettings 패턴).
- `DrawNurbsTool.commit` — `getNurbsPatchMode()` dispatch: vault → `buildVaultGrid` →
  `createNurbsSurface(controlPts, 5, 2, weights, uKnots, vKnots, 2, 1)`; bezier → 기존 경로.
- `buildVaultGrid` — 5-CP 2-span rational semicircle (right/normal basis) × 2-CP extrude (up).
- `SettingsPanel` 토글 `#sp-nurbs-vault` (체크=vault). draggable control-net / per-CP weight
  편집은 future (§후속).

## 3. Lock-ins

- **L-231-1** A1 scope — rational vault 프리셋 + mode 토글. draggable control-net / per-CP
  weight UI 는 defer (별도 ADR).
- **L-231-2** vault = canonical 2-span rational semicircle (5 CP, weights [1,1/√2,1,1/√2,1],
  knots [0,0,0,.5,.5,1,1,1], degree 2) × degree-1 extrude — EXACT 원호 (Pattern-12, engine
  createNurbsSurface 활용).
- **L-231-3** default 'bezier' (기존 ADR-224 동작 보존, localStorage 토글로 opt-in vault).
- **L-231-4** r = |duLen|/2 (peak height = 반 width), footprint orientation 추종 (duLen signed).
- **L-231-5** 엔진/WASM 변경 0 (createNurbsSurface 이미 존재 — 도구 dispatch만). ADR-046 P31 #4
  additive (tool-nurbs catalog 변경 0, 기존 진입점 mode 추가).
- **L-231-6** Track B (곡면 Boolean edge case) defer — deep SSI, 별도 dedicated ADR.
- **L-231-7** 절대 #[ignore] 금지.

## 4. 회귀

- vitest **+6** (NurbsPatchSettings 4 + DrawNurbsTool vault 2). 2381 → **2387** (159 files, 1 skipped).
- tsc 0. 엔진/WASM 변경 0. catalog 변경 0.

## 5. 브라우저 검증 (real WASM)

- 시뮬 (track decision): `createNurbsSurface` rational quarter-cylinder → surfaceKind 8, 117정점
  전부 r=500 정확 원호 (maxDev 2e-5mm) — uniform Bezier 불가.
- 구현 (buildVaultGrid 제어망): footprint 1000×1000, normal +Z → createNurbsSurface →
  **surfaceKind 8, 189정점 전부 정확한 반원 (x-500)²+z²=500² (maxDev 2e-5mm), z 범위 [0, 500]**
  (base→peak=radius) — **EXACT 반원통 vault** 확정.

## 6. Lessons

- **L1** 시뮬레이션이 track 선택을 결정 — Boolean β-4 defer 패턴 답습. Track A 가 Pattern-12
  (engine 완비, 도구만) vs Track B 가 deep SSI (M-L) 임을 시뮬레이션이 정량 확인.
- **L2** rational NURBS 의 EXACT 곡면 가치 — uniform Bezier (다항식) 는 원호를 근사만, rational
  (weights [1,1/√2,1]) 은 정확한 conic. createNurbsSurface 가 이를 이미 지원 (Pattern-12).
- **L3** mode 토글 패턴 (Text3DSettings) 재사용 — 기존 도구에 고급 모드 추가 시 default 보존 +
  localStorage opt-in + SettingsPanel 토글.

## 7. 후속 (별도 트랙)

- **draggable control-net + per-CP weight 편집** — 임의 rational patch (A2 scope, UI-heavy).
- 다른 vault 변형 — dome (rational sphere-cap), 임의 각도 arc, 회전 vault.
- **Track B (곡면 Boolean edge case)** — torus pinched/lemniscate/one-sided/thick slab, cyl
  corner V-outside, cone/torus N-plane corner (deep SSI, 별도 dedicated ADR).
- vault 사용자 시연 (DrawNurbsTool vault mode 로 실제 그려 매끈 곡면 확인).

## 8. Cross-link

- ADR-224 (DrawNurbsTool Bezier MVP — 본 ADR 이 vault mode 추가) / ADR-033 (NURBS surfaces engine,
  create_nurbs_surface / AnalyticSurface::NURBSSurface) / ADR-228 (Text3DSettings mode-toggle 패턴) /
  ADR-031 Phase D (surface tessellation) / ADR-038 P23 (surface-aware render).
- 메타-원칙 #14 (kernel-native analytic surface) / ADR-046 P31 #4 (additive — default 보존) /
  LOCKED #44 (Complete Meaning per Merge) / Boolean β-4 defer 패턴 (시뮬레이션 → defer/proceed).
