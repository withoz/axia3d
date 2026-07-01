# ADR-254 — P1 closure: Free-form & Closed-curve Disk Face Area (XIA Inspector SSOT)

- **Status**: Accepted
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch)
- **Author**: WYKO + Claude (de-risk workflow → 결재 → 5-layer impl → browser gate)

## 1. Context

ADR-253 가 정정한 "진짜 열린 결함" 우선순위의 **P1 (E2 Cylinder Path B
N-quad hover + §3.2 latent parity)** 진행. de-risk 감사 workflow (5 agent,
file:line) 결과:

- **E2 ("측면 N-quad hover/selection")** = **Pattern-12 already-resolved**.
  Path B 측면은 1 annulus face (Cylinder surface). 선택 그룹화는 ADR-093
  + K3 (`SelectTool.ts:266`), hover 는 1 DCEL face → 측면 전체 tint,
  render 는 균일 faceMap + smooth-group hide. "N quads" 전제는 Path A
  (legacy, default OFF) 기준. → **코드 변경 불필요.**
- **§3.2 parity**: (a) he_twin self-loop = 무해 (`<1000` guard) → defer.
  (b) Boolean `inners()` reject = 실제 버그, 단 **P2 (C1 hole-face
  Boolean)와 동일 root** → P2 로 fold. (c) `analytic_face_area=0` =
  GeneralSweep BSpline/NURBS 측면이 XIA Inspector 에 "면적 0" → **P1 의
  유일한 독립 fix 대상.**

de-risk 후 §3.2(c) 구현 중 **browser probe 가 더 깊은 인접 버그를 드러냄**:
closed-curve planar disk 면 (base/top, cylinder/cone/sphere bases)이 Plane
analytic `u_ext × v_ext` (파라미터 사각형 450² = 202500)를 면적으로 보고 —
실 enclosed disk 면적 (~10272 / πr²)의 8× 과대보고. **pre-existing** (ADR-089
A-ω Plane attach + ADR-121 Plane branch, `faceArea`/measure-selection 에 이미
존재). 사용자 결재 **"완전히 fix"** → 둘 다 교정.

## 2. Decision

### D1 — E2 closure (Pattern-12 already-resolved)
E2 의 selection / hover / render triad 는 Path B 에서 모두 닫혀 있음
(ADR-093 + K3 + LOCKED #40 L3). 코드 변경 0. 회귀 자산은 기존 ADR-093/094
가 봉인.

### D2 — Free-form swept-surface side area (analytic_face_area)
`Mesh::analytic_face_area` 의 `_ => 0.0` 를 **tessellation triangle-sum**
fallback (`tessellated_surface_area`)으로 교체. BezierPatch / BSplineSurface
/ NURBSSurface / RectangularTrimmedSurface 측면이 `SurfaceOps::tessellate
(0.1mm)` → 삼각형 면적 합. GeneralSweep (ADR-192 닫힌 Bezier/BSpline/NURBS
돌출) 측면 면적이 0 → 실제 값.

### D3 — Closed-curve planar disk enclosed area (face_area)
`Mesh::face_area` 의 self-loop (verts < 3) + Plane surface 분기에서, Plane
analytic (파라미터 사각형) 대신 **boundary curve enclosed 면적**을 계산
(`closed_curve_enclosed_area`):
- **Circle** → exact πr² (Path B cylinder/cone/sphere disk bases)
- **Bezier / BSpline / NURBS** → `CurveOps::tessellate` 폴리라인 →
  Newell/shoelace (render fast-path mesh_export.rs ADR-089 A-κ 패턴 mirror)
- Arc / Line self-loop → None (surface analytic 으로 fallback)

곡면 측면 (Cylinder/Sphere/Cone/Torus/BSplineSurface/NURBSSurface)은 D2
surface tessellation 유지 (disk 아님).

### D4 — XIA Inspector surfaceArea SSOT (axia-wasm)
scene-info (`get_xia_info`)의 표면적 계산 — 자체 in-line shoelace (`verts ≥ 3`
요구 → self-loop 면 silent 0) 를 **`mesh.face_area(fid)` 합산**으로 교체
(메타-원칙 #4 SSOT). Inspector surfaceArea 가 measure-selection (`faceArea`
export)와 일치 + free-form/disk fallback 자동 수용.

### D5 — §3.2(a)/(b) defer
- (a) he_twin self-loop: 무해 latent (guard), 재-architecture 불균형 → defer.
- (b) Boolean `inners()` multi-loop reject: P2 (C1 hole-face Boolean)와 동일
  root → P2 로 fold (별도 ADR).

## 3. Lock-ins

- **L-254-1** E2 = Pattern-12 already-resolved (코드 0). Path B 측면
  selection/hover/render 닫힘 (ADR-093/094/LOCKED #40 L3).
- **L-254-2** Free-form 측면 면적 = surface tessellation triangle-sum
  (`tessellated_surface_area`, 0.1mm). polygon 면적 위 underestimate, 표시용.
- **L-254-3** Closed-curve planar disk 면적 = boundary enclosed (Circle exact
  πr² / Bezier·BSpline·NURBS tessellation shoelace). **Plane 파라미터
  사각형 면적 금지** for self-loop disk faces.
- **L-254-4** 곡면 측면 (curved surface)은 D2 surface tessellation; planar
  disk 만 D3 boundary enclosed. `matches!(surface, Plane)` 분기.
- **L-254-5** XIA Inspector surfaceArea = Σ `mesh.face_area` (SSOT, 메타-원칙
  #4). measure-selection (`faceArea`)과 일치 강제.
- **L-254-6** polygon 면 (≥3 verts) Newell 경로 불변 (회귀 guard:
  `face_area_is_correct_for_unit_square`, `adr121_polygon_face_area_unchanged`,
  box surfaceArea 24800).
- **L-254-7** §3.2(b) Boolean multi-loop = P2 (C1)와 동일 root, P2 로 fold.
  §3.2(a) he_twin = 무해 defer.
- **L-254-8** ADR-046 P31 #4 additive — public API (faceArea/get_xia_info
  signature) 불변, 사용자 facing 은 면적 *정확도* 향상만.
- **L-254-9** 절대 #[ignore] 금지.

## 4. 회귀 + 검증

**Rust 회귀 +5** (절대 #[ignore] 금지 5/5):
- axia-geo (+3): `adr253_p1_bezier_patch_area_via_tessellation` (flat
  BezierPatch ≈ 100) / `adr253_p1_curved_bspline_surface_area_nonzero`
  (curved > flat) / `adr253_p1_circle_disk_face_area_is_pi_r_squared`
  (cylinder base = πr², not (2r)²). 전체 lib **2016 PASS**.
- axia-core (+2): `adr253_p1_general_sweep_side_face_area_nonzero` (BSpline
  측면 > 0) / `adr253_p1_general_sweep_base_disk_area_not_over_reported`
  (Bezier disk base/top < AABB, not 202500). 전체 lib **403 PASS**.
- 0 regression (face_area Plane disk 변경이 기존 측면/polygon 테스트 무영향).

**Browser gate (ADR-087 K-ζ, rebuilt WASM, real engine round-trip)**:
- 닫힌 Bezier 돌출 → 3 faces: base/top Plane disk **202500 → 10272.05**
  (enclosed, 8× 과대보고 제거), BSpline 측면 **0 → 46682.77** (surface
  tessellation), Inspector surfaceArea **451682 → 67226.86** (전부 정확).
- polygon 회귀: rect 100×60 faceArea = 6000 (Newell 불변), box surfaceArea
  = 24800 (SSOT routing 동일).

## 5. 변경 파일

- `crates/axia-geo/src/mesh.rs` — `analytic_face_area` `_ => tessellated_
  surface_area` + `tessellated_surface_area` 신규 + `face_area` Plane disk
  분기 + `closed_curve_enclosed_area` 신규. 회귀 +3.
- `crates/axia-wasm/src/lib.rs` — scene-info 표면적 in-line shoelace →
  `Σ mesh.face_area` (SSOT).
- `crates/axia-core/src/scene.rs` — 회귀 +2.

## 6. Lessons

- **L1 de-risk Pattern-12** — E2 의 nominal scope ("N-quad hover")가 이미
  해결됨을 audit 으로 확인 → 가짜 작업 0. ADR-093/094 가 이미 닫음.
- **L2 browser probe > 추론** — §3.2(c) 측면 fix 후 browser probe 가 더
  깊은 인접 버그 (base/top Plane disk 8× 과대보고)를 드러냄. 코드/테스트만
  으로는 안 보였음 (테스트는 측면만 assert). ADR-087 K-ζ canonical.
- **L3 SSOT 노출의 양면** — WASM surfaceArea SSOT routing (메타-원칙 #4)이
  free-form 면적을 surface 하면서 pre-existing disk 과대보고도 노출 → 완전
  fix 필요. SSOT 통합은 잠복 버그를 드러낸다 (audit 가치).
- **L4 surface vs boundary 면적** — 곡면 측면 = surface tessellation; planar
  disk = boundary curve enclosed. 두 개념을 분리 (`matches!(Plane)` 분기).
- **L5 truth over completion** — P1 의 명목 작업 (E2 + §3.2 전부)을 그대로
  구현하지 않고 audit 진실 (E2 done / (a) 무해 / (b) = P2 / (c) + disk
  실제)로 재구성.

## 7. Cross-link

- ADR-253 (P1 anchor — 진짜 열린 결함 우선순위) + de-risk workflow
- ADR-093 (cylinder side owner-id grouping — E2 selection already-resolved)
- ADR-094 (Cylinder Path B annulus — E2 1-face side) / LOCKED #40 L3 (edge
  hover group)
- ADR-192 §3.2 (closed-curve sweep latent parity — (a)/(b)/(c) source) /
  LOCKED #80
- ADR-089 A-κ / A-ω (closed-curve face render fast-path — boundary curve
  tessellation pattern mirror)
- ADR-121 (analytic_face_area 5 primitives — D2 fallback 확장, Plane branch
  source) / ADR-031 Phase D (AnalyticSurface infra)
- ADR-087 K-ζ (사용자 시연 게이트) / ADR-046 P31 #4 (additive)
- P2 (C1 hole-face Boolean — §3.2(b) Boolean multi-loop fold 대상)
- 메타-원칙 #4 (SSOT) / #6 (Preventive) / #14 (면은 닫힌 경계) / LOCKED #44
