# ADR-206 — Ellipse Tool (kernel-native, DrawCircle Path B reuse)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Foundation Tier 1 도구 (ADR-168 브리프 audit 정정 → ADR-206)
- **Depends on**: ADR-089 (closed-curve self-loop faces) / ADR-205 (`nurbs::ellipse`) /
  ADR-050 (Shape form layer) / ADR-026 P12 (cardinal plane snap) / ADR-027 (NURBS kernel)

## 1. Context — audit 정정 (ADR-168 → ADR-206)

외부 "ADR-168 Foundation Tier 1 도구" 브리프 (`reports/ADR_168_FoundationTier1Tools_
TaskBrief.html`) 는 stale 뷰로 작성되어 핵심 전제가 과대평가되었고 ADR 번호가 충돌했다
(4-agent 병렬 audit, 2026-06-22):

- **ADR 번호 충돌 → ADR-206**: 168 = Face plane drift snap (LOCKED #69), 167 = EPS_PLANE
  SSOT (LOCKED #68) 모두 사용 중. 브리프가 참조한 "ADR-167 amendment-2 = n-sided NURBS
  patch" 도 stale 충돌. 최고 번호 = 205 → free = **206**.
- **UI 과대평가**: fillet-edge / chamfer-edge / mirror-x/y/z / array-linear / array-radial
  5 도구는 **이미 action + MenuBar + toolbar + context-menu 등록·메뉴로 작동**. 브리프의
  "UI 미등록 / 낭비된 자산" 은 거짓 — 활성화 작업 0.
- **Ellipse 오기**: 브리프 "conic.rs 에 Ellipse, WASM 만 expose" — conic.rs 는 `arc_as_nurbs`
  only, Ellipse 없음.

본 ADR 의 **de-risk-first 조사가 이중 정정** (Pattern-12 engine-already-robust):

- 브리프 "Ellipse 는 WASM 만" — 결론은 근접하나 **이유가 틀림**.
- 1차 워크플로우 audit "Ellipse engine 부재 → 풀스택" — `nurbs::ellipse` 를 놓친
  **over-correction**.
- **실측**: ellipse engine 은 **이미 완비** — `nurbs::ellipse` (nurbs.rs:326, exact
  9-control-point rational quadratic NURBS) + `add_face_closed_curve` (NURBS arm) + render
  (Plane polygon path). ADR-205 Boolean family 가 elliptic-cap 경계용으로 구축한 자산.

## 2. Decision

Ellipse 는 **kernel-native closed-curve** (DrawCircle Path B 답습): 1 anchor vertex + 1
self-loop edge with the exact-ellipse `AnalyticCurve::NURBS` + 1 Plane face. **engine 신규 0**
— `nurbs::ellipse` 재사용. 전용 `AnalyticCurve::Ellipse` metadata variant (kernel-aware
offset / render fast-path) 는 **optional 별도 enhancement** (도구에는 불필요).

## 3. Lock-ins

- **L-206-1** engine 신규 0 — `nurbs::ellipse` + `add_face_closed_curve` 재사용 (Pattern-12).
- **L-206-2** DrawCircle Path B 답습 — Command / WASM / bridge / UI 모두 circle 경로 미러.
- **L-206-3** Ellipse 는 **항상 kernel-native** (polygon-Shape legacy 없음 — circle 과 다름).
- **L-206-4** ADR-026 P12 cardinal snap (center) 정합 (bridge + tool 양쪽).
- **L-206-5** `AnalyticCurve::Ellipse` metadata variant = optional 별도 ADR.
- **L-206-6** ADR-046 P31 #4 additive only — 기존 도구 동작 UNCHANGED.
- **L-206-7** 절대 #[ignore] 금지.

## 4. 구현 (Path Z atomic)

- **de-risk** (`bf4420a`): `adr206_ellipse_self_loop_face_renders` — ellipse self-loop face
  가 `export_buffers` 로 exact ellipse 렌더 (plane 위, rx/ry extent, smooth ring). engine
  완비 증명.
- **β-1** (`bcc4d34`): `Command::DrawEllipseAsCurve { center, ref_dir, normal, radius_x,
  radius_y }` + `exec_draw_ellipse_as_curve` (circle 미러: major u = ref_dir 투영, minor
  v = normal × u, `nurbs::ellipse` → anchor cp[0] → `add_face_closed_curve` → form Shape) +
  WASM `drawEllipseAsCurve` export + export_baseline lock.
- **β-2** (`0104cc5`): TS bridge `drawEllipseAsCurve` wrapper (snapCardinalCenter + graceful -1).
- **β-3** (`b36d5ae`): `DrawEllipseTool` (3-click center → major → minor) + ToolManager 등록
  `'ellipse'` + 메뉴 "타원" + CommandCatalog `tool-ellipse`.

## 5. 회귀 + 검증

- **회귀**: axia-geo +1 (de-risk) / axia-core +1 (β-1) / vitest +13 (WasmBridge 4 +
  DrawEllipseTool 9). tsc clean, 0 regression, #[ignore] 0.
- **브라우저** (real WASM rebuild, Chromium): `bridge.drawEllipseAsCurve(0,0,0, 1,0,0,
  0,0,1, 400, 200)` → shapeId, 1 face / 1 self-loop edge / 1 anchor vert, **exact ellipse
  render** (4096 tris, maxx=400, maxy=200, z=0, 모든 정점 (x/rx)²+(y/ry)² ≤ 1, non-manifold 0).

## 6. 후속 (별도 ADR — Foundation Tier 1 잔여)

- `AnalyticCurve::Ellipse` metadata variant (kernel-aware offset / render fast-path).
- **ADR-207** vertex 3-way chamfer expose (chamfer_vertex_3way; edge 챔퍼는 이미 작동).
- **ADR-208** Copy/Duplicate 도구 (arrayLinearFaces count=2 재사용 or clone).
- **ADR-209** interactive UX 폴리시 (5 wired 도구 live preview; 메뉴 이미 작동, marginal).

전체 corrected spec: `reports/ADR_206_FoundationTier1Tools_CorrectedSpec.md`.
