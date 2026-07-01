# ADR-209 — Interactive Tool Modes for the Foundation Tier 1 Wired Ops

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Foundation Tier 1 도구 (ADR-168 audit 정정 → ADR-206 의 D)
- **Depends on**: mirror_faces / array_linear_faces / array_radial_faces / fillet_edge
  (모두 기존) / ADR-206 (audit + Pattern-12) / ADR-046 P31 #1 (가볍게) / #4 (additive)

## 1. Context

ADR-206 Foundation Tier 1 audit 의 잔여 D (사용자 결재 A+B+C+D). 5 wired 도구
(fillet-edge / mirror-x/y/z / array-linear / array-radial) 는 **이미 action + MenuBar +
toolbar + context-menu 로 작동** — 활성화 작업 0. D = 그 위에 **interactive Tool 모드**
(선택 모드 + live preview + 축 키 + VCB + 반복 + Esc) 추가 = 순수 UX 향상. ADR-046 P31 #1
"가볍게" 경계라 audit/spec 이 **marginal** 로 분류. 사용자 결재로 roadmap 완결 진행.

engine + WASM + bridge 모두 기존 → **UI-only (Pattern-12)**, engine 신규 0.

## 2. Decision

5 wired 도구 각각에 interactive Tool 클래스 추가 (기존 one-shot action **보존**):

- **MirrorTool** — live 미러-평면 indicator (반사 위치 시각화) + X/Y/Z 축 키 + 반복 commit.
- **ArrayLinearTool** — 2-click (base → spacing) + VCB count + dim preview.
- **ArrayRadialTool** — X/Y/Z 회전축 + VCB count + full-circle (2π) commit.
- **FilletTool** — 선택 엣지 + VCB radius (or 마지막 값) + 반복.
- **Chamfer** — vertex corner cut 은 ADR-207 ChamferTool, edge 챔퍼는 chamfer-edge action.

## 3. Lock-ins

- **L-209-1** UI-only — 기존 engine/WASM/bridge op 재사용 (Pattern-12), 신규 0.
- **L-209-2** one-shot action (mirror-x/y/z, array-linear/radial, fillet-edge) **보존**
  (ADR-046 P31 #4 additive). Tool 모드는 `tool-*` command 로 별개 dispatch.
- **L-209-3** Mirror = live 평면 indicator (geometric ghost 의 cheap 대용). Array/Fillet 의
  geometric ghost 는 **deferred** (transform 아닌 새 geometry).
- **L-209-4** Mirror/ArrayRadial = stateless (isBusy false). ArrayLinear = 2-click busy.
- **L-209-5** 절대 #[ignore] 금지.

## 4. 구현

- **β-1** (`e1efacd`) MirrorTool — 미러-평면 indicator + 축 키 + 반복. +9 vitest.
- **β-2** (`0b3d299`) ArrayLinearTool + ArrayRadialTool + FilletTool. +20 vitest.

## 5. 회귀 + 검증

- **회귀**: vitest +29 (MirrorTool 9 + ArrayLinear 6 + ArrayRadial 6 + Fillet ... = 4 tools).
  tsc clean, 0 regression, #[ignore] 0. engine/WASM/bridge 신규 0.
- **브라우저** (real WASM): mirrorFaces 6→7 / arrayRadialFaces([face],6,…,2π) 6→12.

## 6. 후속 (별도 ADR)

- Geometric ghost preview (array 복제본 ghosts / fillet 둥근 미리보기).
- Ctrl-drag MoveTool Copy 모드 (산업 CAD parity).

## 7. 🎉 Foundation Tier 1 (ADR-206 audit A+B+C+D) 완전 closure

- **ADR-206** Ellipse (A) ✅ — `nurbs::ellipse` 재사용.
- **ADR-207** Vertex chamfer (B) ✅ — `chamfer_vertex_3way` 재사용.
- **ADR-208** Copy/Duplicate (C) ✅ — `array_linear_faces(count=1)` 재사용.
- **ADR-209** interactive UX (D) ✅ — 5 wired op 재사용.

네 ADR 모두 **Pattern-12 (engine-already-robust)** — de-risk-first 조사가 매번 engine 신규
0 을 확인. 외부 "ADR-168 7도구 풀스택 multi-week" 추정이 실제로는 며칠 규모 reuse 였음.

전체 corrected spec: `reports/ADR_206_FoundationTier1Tools_CorrectedSpec.md`.
