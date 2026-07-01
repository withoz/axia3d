# ADR-197 — General 3D Solid Boolean Foundation (α spec)

- **Status**: Proposed (α spec — 결재 대기, 코드 변경 0)
- **Date**: 2026-06-11
- **Track**: 6 (NURBS / Boolean)
- **Trigger**: LOCKED #82 (ADR-196) follow-up "다른-솔리드 carve" 조사 → 기반인
  *일반 3D 솔리드 Boolean*이 양쪽 경로 모두 작동 안 함을 확정 (브라우저 실측 +
  5-lens 아키텍처 매핑). 사용자 결재 "C — multi-week 기반 전면 착수".
- **Cross-link**: ADR-064/066 (NURBS Boolean DCEL) / ADR-075 (Boolean E2E) /
  ADR-076 (legacy boolean_op sunset) / ADR-074 (group A/B) / ADR-078 (group
  persistence) / ADR-101 (coplanar auto-intersect, Greiner-Hormann walk) /
  ADR-007 (manifold invariant) / 메타-원칙 #4 (SSOT) / #16 (auto-carve 안티패턴).

## 1. Context — 기반이 없다 (확정)

"눌린 면이 다른 솔리드 침투 → carve" follow-up 조사 결과, 그 기반인 **일반 3D
솔리드 Boolean subtract/union/intersect 자체가 작동하지 않음**:

- **브라우저 실측** (corner-overlap 두 박스):
  - legacy `boolean_op` (triangle): 12면 → **3면 붕괴**.
  - production `booleanDispatchDcelMulti` (NURBS DCEL): **no-op** (36 face-pair
    모두 preservedFaces, 두 박스 그대로).
- **MVP 프로토타입**: triangle 경로에 일반 면-면 교차를 추가했더니 manifold는
  통과하지만 **기하학적으로 틀림** (제거 corner 밖 점 (1,1,1)이 결과의 밖) —
  되돌림.

### 5-lens 매핑이 밝힌 양쪽 경로의 정확한 상태

| | volumetric 분류 | 일반 3D 교차 | 다중-cut 분할 | 곡면 보존 | 상태 |
|---|---|---|---|---|---|
| **Legacy triangle** `mesh.boolean` | ✅ `point_in_solid` (3-ray) | ❌ coplanar만 (`detect_coplanar_faces`) | ❌ 단일 직선 (`split_polygon_2d` MVP) | ❌ (polygon) | coplanar/axis-aligned MVP |
| **NURBS DCEL** (ADR-064/066) | ❌ per-pair only | ✅ SSI (`nurbs_boolean_v2`) | ✅ trim walk | ✅ analytic | LOCKED "probe only", depth≤1 |

**핵심 finding**: **어느 경로도 volumetric 솔리드 Boolean이 아니다.**
- Triangle: volumetric 機構(point_in_solid)은 있으나 *일반 교차*(coplanar만)와
  *다중-cut 분할*(단일 직선만)이 빠짐.
- NURBS DCEL: *일반 교차*(SSI)는 있으나 *volumetric 조립*(전체 솔리드 inside/
  outside 분류 + keep/remove/stitch)이 없음 — per-face-pair 도구이며 ADR-064/066
  §2.3에 "probe only, 실제 Boolean DCEL 생성은 별도 ADR"로 LOCKED. depth≥2
  containment는 Err 반환.

## 2. 메타-원칙 #16 framing — 본 ADR은 *명시* Boolean 기반

본 ADR의 목표는 **사용자가 명시적으로 Subtract/Union/Intersect를 호출했을 때
실제로 작동하는 일반 3D 솔리드 Boolean**이다. "push로 다른 솔리드 침투 시 자동
carve"(auto-dispatch)는 메타-원칙 #16 휴리스틱 안티패턴이며 **본 ADR scope 밖**
(기반 완성 후, 별도 opt-in dispatch ADR로만 검토). 즉 본 ADR = *엔진 능력*
(explicit Boolean이 정확히 작동), 향후 auto-carve = *trigger 정책*(별개).

## 3. Decision — 중심 결정 (결재 포인트)

robust 일반 3D 솔리드 Boolean을 어느 경로 위에 만들 것인가:

### Path A — Legacy triangle `mesh.boolean` 확장 (추천)
volumetric 機構(point_in_solid 분류 + assemble + merge)을 *재사용*하고, 빠진
두 조각만 채운다:
1. **일반 면-면 교차** (`detect_general_intersections`) — plane∩plane 직선을 양
   convex 면에 clip. **프로토타입 검증됨** (segment 정확).
2. **robust 다중-cut 폴리곤 분할** — `split_polygon_2d`(단일 직선)를 N개 독립
   cut line 처리 가능한 *planar arrangement / constrained subdivision*으로 교체.
   **진짜 알고리즘 작업** (본 ADR의 핵심 난이도).

- **장점**: volumetric 분류(point_in_solid)가 이미 있고 테스트됨. 자기-완결적
  classic mesh Boolean (well-understood). NURBS 의존 0. 빠진 조각이 *2개로
  bounded*.
- **단점**: triangle 기반 — face 분할 후 곡면 metadata 손실 (Plane은 post-Boolean
  재-attach 가능, 곡면(Cylinder/Sphere)은 polygonize → metadata 손실). ADR-076이
  sunset한 legacy boolean_op 경로 (production 라우팅 변경 필요).

### Path B — NURBS DCEL에 volumetric 조립 신축
per-pair SSI 위에 *전체 솔리드 containment 분류 + keep/remove/stitch* (Phase L)
+ depth≥2 containment + 솔리드-레벨 dispatch를 신축.

- **장점**: analytic 곡면 보존 (kernel-native). production 경로(`boolean
  DispatchDcelMulti`) 그대로 활용.
- **단점**: volumetric DCEL 조립이 *진짜 큰 미구현* (Phase L 전체 부재). depth≥2
  containment + non-manifold 안전성 + ADR-064/066 LOCKED "probe only" 해제 필요.
  난이도/범위가 Path A보다 훨씬 크고 less-bounded.

### Claude 추천 = Path A (참고)
- 빠진 조각이 *2개로 bounded* (교차 ✅ prototyped + robust split 1개 알고리즘) vs
  Path B의 *volumetric 조립 전체 신축*.
- volumetric 분류 + watertight 검증이 *이미 작동* (point_in_solid, 테스트됨).

### ✅ 결정 = **Path B** (사용자 결재 2026-06-11)
사용자가 **Q1 = Path B (NURBS volumetric 신축)** + **Q2 = a (robust custom 조립)**
결재. 근거 (사용자 우선순위):
- **곡면 보존** — Cylinder/Sphere/NURBS surface가 Boolean 후에도 analytic로 유지
  (triangle 경로는 polygonize → metadata 손실). 엔진의 NURBS 커널 투자
  (ADR-027~034 / 064 / 066) 와 *일관*.
- **정공법** — 더 크지만 kernel-native "right thing"을 제대로 신축.

**정직한 scope 경고**: Path B는 Path A보다 *현저히 큰* 다단계(multi-month급)
작업이며, "volumetric 조립"(Phase L) 전체가 *미구현*. ADR-064/066의 "probe only"
LOCKED를 production volumetric으로 *명시 reaffirm/해제*해야 함 (governance). 따라서
β-1은 *구현이 아닌 정밀 조사* — NURBS DCEL per-pair가 *교차하는* 솔리드 면쌍에서
실제로 무엇을 생성하는지 확정 (5-lens dcel 미해결 질문: "교차하는 솔리드 box 면쌍
에서 Phase J SSI가 trim loop를 생성하나?"). 브라우저 실측은 corner-overlap에서도
모든 pair "preservedFaces"(surgery 0)였음 — 이 gap이 β-1의 anchor.

## 4. Step 분해 (Path B 채택, β 시리즈 — 각 별도 atomic + 결재)

NURBS DCEL per-pair SSI 위에 **volumetric 조립(Phase L)**을 신축. "면 자르기"는
trim-curve 기반 (Q2=a robust custom — trim_loop boolean 정밀 제작).

| step | 내용 | 크기 | 비고 |
|---|---|---|---|
| **β-1** | **정밀 조사** — NURBS DCEL per-pair(`nurbs_boolean_v2`/`nurbs_boolean_to_dcel`)가 *교차하는* 솔리드 면쌍에서 무엇을 생성하나 (trim loop? new face?). 브라우저 실측 "all preservedFaces" root cause. Phase L 조립 계약 정의. | 중 | 조사 (구현 0), audit-first canonical |
| **β-2** | **SSI/trim 정확성** — plane×plane 교차 면쌍이 trim curve + split face를 *실제로* 생성하도록 (β-1이 gap 발견 시). robust custom trim split (Q2=a) | **대** | 교차하는 면쌍 face split 회귀 |
| **β-3** | **volumetric 조립 (Phase L 핵심)** — per-pair split fragment를 *전체 솔리드* inside/outside로 분류 + op별 keep/remove/flip + watertight stitch | **대** | corner box subtract 정확성(point (1,1,1) INSIDE) + watertight + manifold |
| **β-4** | **depth≥2 containment + multi-loop** — 중첩 hole / nested. ADR-064 depth≤1 한계 해제 | 중 | nested 회귀 |
| **β-5** | **production 라우팅 + E2E + 시연** — `boolean_dispatch_dcel_multi` → volumetric, group A/B(ADR-074) 보존. ADR-075 intersecting fixture. 사용자 시연 (ADR-087 K-ζ) | 중 | Playwright intersecting round-trip + 메타-원칙 #16 |

각 β는 별도 PR (LOCKED #44) + 사전검토 + 결재. multi-month atomic (ADR-094 §E L1
additive-first 답습). **β-1 정밀 조사 결과에 따라 β-2~5 재조정** (audit-first
canonical — Phase J SSI 실상 확인 전 구현 약속 금지).

## 5. Lock-ins (α 시점, β에서 구체화)

- **L-197-1** 목표 = *explicit* 일반 3D 솔리드 Boolean (메타-원칙 #16 — auto-carve
  는 scope 밖, 기반 완성 후 별도 opt-in dispatch).
- **L-197-2 (Path B)** volumetric truth = NURBS DCEL volumetric 조립 (Phase L —
  전체 솔리드 containment 분류 + keep/remove/flip + stitch). watertight + manifold
  (ADR-007 `verify_face_invariants`) 강제.
- **L-197-3 (Path B, Q2=a)** "면 자르기" = trim-curve 기반 robust custom split
  (정밀 제작). 기존 자산 재사용 (Phase J `nurbs_boolean_v2` SSI / trim loop,
  ADR-101 GH walk 참고) 하되 새 정밀 trim split 신축.
- **L-197-4** LOCKED #5 (1.5μm spatial-hash dedup) topological tolerance 보존.
  geometric tolerance 1e-3mm (ADR-064 D-AD / `BooleanTolerance` 답습).
- **L-197-5** Multi-loop(holes) + depth≥2 nested containment은 β-4 (ADR-064
  depth≤1 한계 해제).
- **L-197-6 (Path B)** 곡면(Cylinder/Sphere/NURBS) surface **보존이 본 경로의
  핵심 가치** — Boolean 후에도 analytic surface 유지 (triangle polygonize와 대비).
- **L-197-7** Production 라우팅 = `boolean_dispatch_dcel_multi`를 volumetric으로
  승격. group A/B (ADR-074) + persistence (ADR-078) 보존. ADR-076 legacy sunset
  정합 (β-5).
- **L-197-8 (governance)** ADR-064/066의 "probe only" LOCKED를 production
  volumetric으로 **명시 reaffirm/amend** 필요 (β-3 또는 별도 amendment). 메타-원칙
  #10 (ADR 불변) 정합 — cross-ADR 명시 없이 override 금지.
- **L-197-9** 절대 #[ignore] 금지. β-5 E2E는 *intersecting* fixture (ADR-075의
  disjoint-only gap 해소).
- **L-197-10** 각 β 별도 atomic PR + 사전검토 + 결재 (LOCKED #44 + Path Z).
  β-1은 audit-first (구현 전 SSI 실상 확정).

## 6. Validation gates (β별 회귀 + 사용자 시연)

- **정확성**: corner box subtract → 제거 corner 밖 점 (1,1,1) INSIDE (MVP가 실패한
  바로 그 케이스), 제거된 점 OUTSIDE. point_in_solid + tessellation.
- **Watertight**: 결과 closed (boundary edge 0). manifold (verify_face_invariants
  0 violations).
- **Volume invariant**: Subtract vol ≤ vol_a, Intersect vol ≤ min(a,b).
- **3 ops × 3 교차 유형** (face/edge/corner) 매트릭스.
- **사용자 시연** (ADR-087 K-ζ): 실제 UI에서 두 솔리드 그리고 Subtract → carved.

## 7. Open Questions — 결재 결과

- **Q1 (중심)** ✅ **= Path B** (NURBS volumetric 신축, 사용자 결재 2026-06-11).
  곡면 보존 + kernel-native 일관성. (Claude 추천은 A였으나 사용자 우선순위 존중.)
- **Q2** ✅ **= a** (robust custom 정밀 제작 — trim-curve split을 제대로 신축).
- **Q3** (기본값): Production = `boolean_dispatch_dcel_multi` 승격 (L-197-7).
- **Q4** (Path B로 해소): 곡면 surface **보존** (L-197-6) — 본 경로의 핵심.
- **Q5** (기본값): Multi-loop/depth≥2는 β-4 (현 reject는 β-4까지 유지).

## 8. Non-goals (본 α + β 시리즈 밖)

- push-driven auto-carve dispatch (메타-원칙 #16 — 별도 opt-in ADR, 기반 완성 후).
- 성능 벤치마크 / latency 타겟 (별도).
- (곡면 surface-surface Boolean은 Path B의 *핵심 가치* — non-goal 아님.)
- (Multi-loop / depth≥2는 β-4 — 본 시리즈 안.)

## 9. β-1 정밀 조사 결과 (audit-first, 2026-06-11 — 코드 변경 0)

NURBS DCEL이 *교차하는* 솔리드 면쌍에서 왜 surgery를 안 하나 — **2중 확정**.

### 근본 원인 (코드 + 엔진 자체 주석)
- **`nurbs_boolean_v2`** (`surfaces/ssi/boolean.rs`): `for chain in &intersection
  { if !chain.closed { continue; } ... }` — **닫힌 SSI chain만** trim loop로 변환.
- **`nurbs_boolean_to_dcel`** (`operations/boolean_nurbs_dcel.rs` line 79-89,
  엔진 작성자 주석): *"SSI was non-empty but produced no closed loops (e.g., open
  chains only — Phase J skips them). new_faces empty → preserve both,
  disjoint:false"*.

### 의미
- NURBS Boolean은 *한 면을 완전히 관통하는 닫힌 곡선*(원통이 평면 뚫음 → 닫힌
  원)만 자른다.
- 두 솔리드의 면쌍은 **열린 선분**으로만 만난다 (평면×평면 = 열린 직선). → 닫힌
  loop 0 → 자르기 0 → preserved. 브라우저 실측(두 박스 모든 pair `disjoint:false`
  + `preservedFaces`)이 코드와 정확 일치.
- 열린 선분은 **여러 면쌍을 가로질러 조립될 때 비로소 닫힌다** — 솔리드 A 표면이
  B로 들어갔다 나오는 *전역 닫힌 교차 loop*. 엔진 주석이 명명: **"Phase L —
  full containment + trim-curve reconstruction across patch boundaries"**.

### β 계획 정밀화 (조사로 더 명확)
- **SSI(교차선 계산)와 닫힌-loop용 trim/containment 기계는 *이미 작동***.
- 빠진 한 조각 = **열린 per-pair 선분들을 전역 닫힌 loop로 잇기** (+ 솔리드-레벨
  분류). 이게 **β-2의 핵심** ("cross-patch reconstruction"). β-3은 그 닫힌 loop를
  기존 trim/containment 기계에 흘려 volumetric 결과 조립.

## 10. β-3 Path B — 곡면 SSI Boolean (α spec, 2026-06-12)

**사용자 결재 (2026-06-12): Path B (true 곡면 SSI) — 정밀/kernel-native.**

### 10.1 Gap (audit-first 확정)
β-2d 통합 arrangement는 **polygon 면 전제**. Path B 곡면 primitive(sphere/cylinder/
cone)의 면은 **self-loop 경계**(anchor→anchor + Circle curve) → `face_unit_normal_
and_poly`가 poly<3 → **skip** → 곡면 입력 결과 0 (회귀 `adr197_beta2d_curved_input_
skipped_pending_beta3` 가 lock). planar analytic surface 보존은 작동 확인 (회귀
`adr197_beta2d_planar_analytic_surface_preserved`).

### 10.2 기존 자산 (재사용 — 새 SSI 0)
- `surfaces/ssi/analytic.rs`: closed-form SSI 5쌍 — `plane_plane` / `plane_cylinder`
  (circle/ellipse/2-line) / `plane_sphere`(circle) / `plane_cone` / `cylinder_cylinder`.
  결과 `SurfaceIntersection { points, uv_a, uv_b, closed, tangent_warning }`.
- `surfaces/ssi/trim_gen.rs`: `ssi_to_trim_loops` / `ssi_batch_to_trim`.
- `surfaces/ssi/trim_classify.rs`: `build_containment_tree` (trim loop 중첩).
- `surfaces/{plane,cylinder,sphere,cone}.rs`: `AnalyticSurface::{evaluate, normal}`.

### 10.3 접근 — dispatch (planar arrangement ‖ curved SSI)
solid_boolean에 곡면 경로를 *나란히* 추가. face pair 마다:
- **둘 다 planar polygon** → β-2d arrangement (현행, 불변).
- **≥1 곡면 analytic surface** → **SSI 경로** (β-3 신규): `ssi/analytic.rs`로 교차선 →
  trim → classify → sew.

### 10.4 Sub-step 분해 (각 atomic + 사전검토 + 결재 + 커밋)
- **β-3-β** — SSI dispatch + curved-pair 검출: face의 `surface()` kind로 곡면 면 식별,
  surface 쌍별 `ssi/analytic.rs` 호출 → `SurfaceIntersection` 수집. (회귀: SSI 호출
  정합 — plane×sphere=circle 등)
- **β-3-γ** — curved 면 imprint/trim: 곡면 self-loop 경계를 SSI 곡선으로 분할
  (`ssi_to_trim_loops` + sub-face 경계 조립). sphere hemisphere ∩ plane → SSI circle
  로 2 sub-cap.
- **β-3-δ** — curved sub-face classify: IN/OUT 멤버십 (β-2d 답습) — probe는 곡면 위 점
  (`AnalyticSurface::evaluate`) ± 곡면 normal(`AnalyticSurface::normal`)*ε.
- **β-3-ε** — curved sew: trimmed 곡면 면 재생성 (self-loop + SSI sub-arc 경계) +
  `AnalyticSurface` 보존 (ADR-089 A-χ). SSI 공유 vert로 manifold weld.
- **β-3-ζ** — 검증: 곡면 Boolean watertight + manifold + surface 보존. 첫 tractable
  케이스 = **box ∩/− sphere** (box 6 평면이 sphere 표면을 circle로 trim → spherical
  cap + flat disk). 이후 plane×cylinder, sphere×box.

### 10.5 Lock-ins (α 시점)
- L-β3-1: SSI는 `ssi/analytic.rs` 재사용 (새 교차 알고리즘 0). NURBS-class surface는
  `nurbs_wrapper` / `intersect_bspline_pair` (closed chain) — 후속.
- L-β3-2: classify = β-2d IN/OUT 멤버십 동일 (곡면 normal probe). dedup 동일.
- L-β3-3: sew = `add_face_closed_curve` / trimmed-loop face 재생성, AnalyticSurface
  보존. planar 경로(β-2d) 불변 (dispatch 분기).
- L-β3-4: tangent SSI(`tangent_warning`) / open SSI chain → verify-and-bail
  (메타-원칙 #16), closed chain 우선.
- L-β3-5: 절대 #[ignore] 0. 각 sub-step atomic + 커밋.

### 10.6 Non-goals (β-3 밖)
NURBS-class surface SSI (Bezier/BSpline patch — `nurbs_wrapper` 후속) / self-
intersecting 곡면 / 곡면×곡면 open-chain (cylinder×cylinder 비평행) → 별도 β.

### 10.7 구현 진행 (2026-06-12, β-3-β ~ ε-2 auto-barrel + β-3-h~p 곡면 Boolean + 곡면 칼 + Union)
axia-geo 1752 → 1802 (+50). β-3-h/i/j/k/l/m 데모는 WASM/bridge 연결 + 브라우저 시연.
β-3-n 곡면 칼 end-to-end. β-3-o/p Union: Case B sphere∪sphere(캡슐)·cone∪cone opposing(hourglass) + **Case A 곡면∪box 4 곡면 완성**(sphere cap/cylinder stub/cone 혼합/torus annular) — audit + 엔진 + 시연.
- **β-3-β** (SSI dispatch): `detect_curved_intersections(faces_a, faces_b) ->
  Vec<CurvedIntersection>` — ≥1 곡면 면쌍에 closed-form SSI(`ssi/analytic.rs` 5쌍,
  symmetric). 검증 box×sphere → 12 circle, x=2∩sphere = 반경 √5.
- **β-3-γ-1** (latitude imprint): const-z SSI → v-range strip 2개 (직접).
- **β-3-γ-2 사전검토 시뮬레이션** (회귀 lock): sphere inversion round-trip 정확 /
  위도 seam-spanning chain → 2 region / 단일 oblique RAW은 u-seam 횡단으로 틀림 /
  **seam-shift(u를 미사용 gap으로 회전) → 2 region partition 정확**.
  ⚠ **`ssi/analytic.rs` uv는 placeholder** (`uv_b=(θ,0)`) → 곡면 inversion 직접 계산 필요.
- **β-3-γ-2a** (oblique seam-shift imprint): inversion → v-band clip(`longest_inrange_
  run`) → seam-shift(`largest_u_gap_mid`) → `arrange_polygon_2d` in uv → 2 sub-face.
  `CurvedSubFace { surface, uv_region, uv_holes, u_shift }` (실제 경도 = u + u_shift).
  검증 x=2 + y=−1 두 oblique 평면 모두 partition = π² 정확.
- **β-3-δ** (curved classify): `classify_curved_subface(subface, in_result) -> Option<bool>`
  — β-2d IN/OUT 멤버십 재사용 (대표점 `sphere::evaluate(uv centroid)` + `normal_at_
  world_pos` ± ε). 곡면 solid 멤버십은 **analytic**(`|p−c|<r`), 평면은 triangle.
  검증 sphere−halfspace(z<2 cap 유지) + sphere∩halfspace(z>2 cap 유지, op 반대).
- **β-3-ε-1** (curved sew primitive, **첫 곡면 Boolean DCEL**): `Mesh::sew_closed_
  curve_pair(anchor, curve, surf_fwd, n_fwd, surf_bwd, n_bwd, mat)` — create_sphere/
  cone의 1 anchor + 1 self-loop edge + 2 face 패턴을 임의 surface 쌍으로 일반화.
  검증 sphere∩{z>2} → capped sphere (cap Sphere v∈[v0,π/2] + disk Plane z=2, SSI
  circle 공유) **watertight + manifold**. 사전검토 회귀 `adr197_beta3e_sew_mechanism_
  validated` (2-face-on-self-loop = watertight, create_sphere 검증).
  발견: `add_face_closed_curve`는 단일 self-loop만(hole 미지원).
- **β-3-ε-3** (첫 *자동* 곡면 Boolean): `Mesh::boolean_sphere_halfspace(sphere_faces,
  plane_origin, plane_normal, mat)` — 파이프라인 자동 연결 (SSI `plane_sphere` → imprint
  → classify → sew ε-1). sphere ∩ {z>2} → capped sphere (cap 자동 v-range). 양극·양측
  검증. orchestration trace 회귀 `adr197_beta3e3_orchestration_trace`.
- **β-3-ε-2** (subtract = cap-MERGE, **annulus 회피**): 사전검토 시뮬레이션 발견 —
  sphere − {z>2}는 2 인접 cap(남극 hemisphere [−π/2,0] + 북극 band [0,v0])을 반환,
  **단일 cap [−π/2,v0]로 병합** (남극=점 → 경계 1개). `boolean_sphere_halfspace`에
  cap-merge 추가 → sphere − halfspace (sliced sphere) watertight. **진짜 annulus는
  pole 미도달 band (box∩sphere)에서만 필요.** 회귀 `adr197_beta3e2_*` (finding +
  subtract). → **halfspace cut 쌍 완결** (intersect + subtract, annulus 0).
- **box∩sphere 복잡도 사전검토** (회귀 `adr197_box_sphere_ssi_complexity`): box[−2,2]³
  ∩ sphere(r=3) = **12 SSI circle** (hemisphere당 6 = 2 latitude z=±2 + 4 oblique
  x/y=±2). sphere를 6 교차 원으로 동시 imprint = 대규모 다단계 확정.
- **β-3-ε-2 real annulus** (multi-loop 곡면 면, **DCEL 검증**): `Mesh::sew_curved_band(
  top/bot circle, band, 2 disk, …) -> (band, top_disk, bot_disk)` — band 면 = top circle
  outer + bot circle inner (둘 다 self-loop) = multi-loop 곡면 면. 검증 sphere ∩ {|z|<2}
  → **barrel** (band Sphere v∈[−v0,v0] pole 미도달 = 진짜 annulus + 2 disk) watertight +
  manifold + tessellate. 회귀 `adr197_beta3e2_real_barrel`. → **DCEL이 2 self-loop 경계
  곡면 면 표현** 증명 (box∩sphere annulus building block 확보, cap-merge로 collapse 안 됨).
- **β-3-ε-2 orchestration** (자동 barrel): `Mesh::boolean_sphere_slab(sphere_faces, z_lo,
  z_hi, mat)` — 파이프라인 자동 (2 SSI circle → imprint → classify → kept caps **merge
  to band** (pole 미도달 검증) → `sew_curved_band`). sphere ∩ {|z|<2} → barrel (band
  Sphere v∈[−v0,v0] multi-loop + 2 disk) watertight. 대칭 + 비대칭(z∈[−1,2]) 검증.
  회귀 `adr197_beta3e2_sphere_slab_barrel_auto`. → **자동 곡면 Boolean이 cap(halfspace)
  + band(slab) 모두 동작.**
- **box∩sphere periodic 필수 확정** (사전검토, 회귀 `adr197_beta3g2b_box_sphere_needs_
  periodic`): north hemisphere의 6 cut = z=2 latitude(**full-u**, seam-spanning) + 4
  oblique arc(x/y=±2). latitude가 u 전체를 덮어 **공통 gap 없음** → seam-shift(γ-2a)
  무력 → **진짜 periodic(cylinder-topology) arrangement 필요** lock-in. γ-2b는 2-piece
  트랙(periodic arrangement + 임의 N-arc curved sew).
- **β-3-h** (cylinder ∩ Z-slab, **sphere cap/band 패턴 답습**): `Mesh::boolean_cylinder_
  slab(cyl_faces, z_lo, z_hi, mat)` — cylinder는 surface param **v = 축방향 위치**(z =
  axis_origin.z + v)라 axis-aligned latitude cut = **v-range clamp**(inversion·seam·pole
  전부 불필요, sphere보다 단순). SSI sanity(`plane_cylinder` closed circle) → band
  Cylinder{v∈[lo−z0,hi−z0]} + 2 disk 직접 구성 → **`sew_curved_band` 재사용**(surface-
  generic). pole 없음 → 항상 band; halfspace {z>k} = slab(k, top_z) → **단일 함수가
  intersect+subtract 모두 커버**. clean 3-face cylinder(`extrude_cylinder_kernel_native`,
  via create_solid는 25-face polygonize) = base/top disk + side **multi-loop band**(outer
  1 self-loop + inner 1 self-loop). 검증 z∈[−3,3] ∩ {|z|<1.5} → barrel(v∈[1.5,4.5]) +
  halfspace {z>0.5} + whole-cylinder reject, watertight + manifold. 회귀 `adr197_beta3h_
  cylinder_clean_structure` + `adr197_beta3h_cylinder_slab_truncate`.
- **β-3-h cone** (cone ∩ Z-slab, 답습 + apex 특수 케이스): `Mesh::boolean_cone_slab(
  cone_faces, z_lo, z_hi, mat)` — cone surface **v = apex로부터 축거리**(radius =
  v·tan(half_angle)), axis-aligned cut = const-v latitude circle. **두 결과 형태**(둘 다
  기존 primitive 재사용): apex 유지(halfspace) → smaller cone(single self-loop side + 1
  disk) via `sew_closed_curve_pair`(ε-1); 아니면 → frustum(multi-loop band + 2 disk) via
  `sew_curved_band`. clean cone = `create_cone_kernel_native` 2-face(base disk + cone side
  single self-loop, apex degenerate). MVP apex-up Z축(axis_dir=−Z). 검증 frustum slab
  {1<z<3}(v∈[1,3]) + apex halfspace {z>2}(smaller cone v∈[0,2]) + base halfspace {z<2}
  (frustum-to-base) + whole-reject, watertight + invariants. 회귀 `adr197_beta3h_cone_
  torus_precheck` + `adr197_beta3h_cone_slab_and_halfspaces`.
- **β-3-h torus** (torus ∩ halfspace, **신규 SSI + washer primitive**): cone처럼 깨끗한
  답습 아님 — torus는 단일-curve SSI 없음. 신규 `torus_z_cut(center_z, R, r, k) ->
  (v1, v2, ρ_outer, ρ_inner)` = z=k의 **2 concentric circle**(z=center.z+r·sin v →
  v1=asin(d/r) outer ρ=R+√(r²−d²) / v2=π−v1 inner ρ=R−√(r²−d²)). 신규 sew primitive
  `Mesh::sew_torus_cap(outer/inner circle, band Torus, washer Plane, …) -> (band, washer)`
  — 2 self-loop edge(outer/inner)를 band + washer가 twin HE로 공유 → watertight. band =
  Torus multi-loop(outer+inner 경계), washer = **Plane multi-loop(outer + inner hole =
  annulus)**. `Mesh::boolean_torus_halfspace(torus_faces, k, keep_above, mat)`: keep_above
  (z>k) → top arc v∈[v1,v2] washer −Z / keep_below (z<k) → bottom arc v∈[v2,2π+v1] washer
  +Z. MVP Z-up **단일 cut(halfspace)만** — 2-cut slab은 sin v∈(a,b)가 **2 disjoint band**
  (각 2 washer) = follow-up. 검증 z>0.5 + z<0.5(seam wrap) + miss-reject, watertight +
  invariants + band/washer 모두 multi-loop. 회귀 `adr197_beta3h_torus_z_cut_geometry` +
  `adr197_beta3h_torus_halfspace`. **render 주의**: washer(Plane+hole)는 earcut-with-holes
  경로(곡선 경계 polygonize), tessellate_face_surface 아님 — standalone DCEL은 정확,
  render는 downstream.
- **β-3-h 데모 wiring + 브라우저 시연** (curved Boolean → viewport): 5 곡면 Boolean fn을
  `pub`化 + `Mesh::create_cylinder_kernel_native_clean`(clean 3-face, via_extrude polygonize
  회피) + WASM 5 export(`demoSphereHalfspace`/`demoSphereSlab`/`demoCylinderSlab`/`demoConeSlab`/
  `demoTorusHalfspace`, self-contained: primitive 생성 + Boolean + xia + 단일 Undo) + TS bridge
  5 래퍼. **render gate 회귀** `adr197_beta3h_curved_boolean_results_render`(5 결과 모두
  `export_buffers` valid triangle). **브라우저 시연**(Claude Preview, real WASM): 5 곡면 Boolean
  배치 → syncMesh → **10 mesh / 20172 triangle 렌더 + invariants valid + console error 0**.
  face count 정확(cap 2 / barrel·cylinder·cone 3 / torus 2). vitest WasmBridge +5(252).
  **주의 (재발 금지)**: `face_set_manifold_info`(meshManifoldInfo)는 **outer loop만** 순회
  (`face_outer_edges`) → band의 inner-loop edge 미카운트 → multi-loop 면에서 **false
  boundary_edge_count=1**. 실제 DCEL은 watertight(HE `face().is_null()==0` Rust 테스트 +
  verifyInvariants valid + non_manifold 0로 확인). boundary count는 multi-loop에 신뢰 불가.
- **β-3-i 일반 `boolean()` 곡면 라우팅** (intersect, additive dispatch): `boolean()` 상단에
  `try_curved_intersect_dispatch` 추가 — 한 operand이 Z-up 곡면 primitive(surface로 분류,
  AABB는 **surface param에서** 계산: self-loop 경계 AABB는 anchor 1점이라 부적합) + 다른
  operand이 **곡면을 XY로 덮고 Z만 자르는 axis-box**(cardinal 6면)면 → `boolean_sphere_slab`/
  `_halfspace`·`boolean_cylinder_slab`·`boolean_cone_slab`·`boolean_torus_halfspace`로 라우팅
  (surface 보존). box는 소거(`remove_box_solid`). **fall-through(None)**: subtract / box가 XY도
  자름(box∩sphere corner=γ-2b) / sphere 非straddle slab / torus 2-cut → 기존 legacy 경로
  (회귀 0, additive). `classify_curved_primitive`/`classify_axis_box`/`CurvedPrim` 헬퍼.
  WASM `demoBooleanSphereBox`(self-contained: clean sphere + box + 실제 `boolean()` Intersect)
  + TS bridge 래퍼. **브라우저 시연**(Claude Preview real WASM, rebuild): `boolean(sphere,
  box, Intersect)` → **3면 {Sphere band 1 + Plane disk 2} + box 소거 + watertight + 7312 tri
  렌더**(surface 보존 확인 — faceSurfaceKind 3=Sphere). 회귀 `adr197_beta3i_general_routing_
  sphere_box`(route sphere/cylinder/torus + bail XY-cut/non-straddle) + `_subtract_not_routed`
  + vitest WasmBridge +1(253).
  **함정 (재발 금지)**: `create_box(center, **width→X, height→Z, depth→Y**)` — Z-cut box는
  `height`가 얇아야(2번째 인자), `depth`가 wide(3번째). 인자 순서 혼동 시 XY-containment
  실패 → fall-through → legacy가 Path B sphere에서 `HeId not found` 에러.
- **β-3-j box∩sphere corner-cut 사전검토 + γ-2b-1 corner geometry**:
  - **정정된 audit (단정→실증)**: corner-cut은 periodic arrangement 문제가 **아님**.
    kept patch는 limited-u(seam-shift 가능). 실증 — **DCEL sew는 기존 API로 충분**
    (`add_face_with_holes(3+ crossing verts)` + `set_curve(Arc)` per edge + `set_surface(
    Sphere)` → invariants valid). 신규 sew primitive 불필요. 회귀 `adr197_beta3j_octant_
    sew_reuses_existing_api`(N≥3 octant; 2-arc wedge bigon만 degenerate).
  - **진짜 gap 2개**: ① **arc-bounded patch 렌더** — `tessellate_face_surface`가 surface
    uv-**rectangle** 전체를 tessellate(octant 작은 삼각형인데 208 tri = sphere band 전체,
    arc 경계로 clip 안 됨) → uv-space clipping 신규 필요. ② crossing/arc clipping 기하.
  - **γ-2b-1 corner geometry (구현)**: `sphere_plane_pair_crossings(center, r, na, oa, nb,
    ob)`(2 plane ∩ sphere = line ∩ sphere = 0/1/2 crossing, 공식 `(da·nb−db·na)×dir/|dir|²`)
    + `circle_angle_of_point` + `corner_arc_range`(2 crossing 중 kept halfspace 안쪽 arc
    선택). 회귀 `adr197_beta3j_sphere_plane_pair_crossings`(octant 3 crossing exact +
    parallel/tangent + arc-range). standalone(orchestration 미연결, β-2 패턴).
  - **분해**: γ-2b-1 crossing/arc-clip ✅ → **γ-2b-2 arc-bounded patch 렌더(진짜 gap)** →
    γ-2b-3 orchestration(기존 `add_face_with_holes` 재사용) → γ-2b-4 full box(8 corner).
- **γ-2b-2 사전검토 + 시뮬레이션 (Approach A 실증)**: arc-bounded curved patch를 **uv-earcut**로
  렌더 — arc 경계 polygonize → 3D on sphere → `sphere_invert`로 uv → **`earcutr::earcut`(기존
  lib 재사용)** → uv-triangle을 sphere로 evaluate. **octant 시뮬레이션**: uv-loop 48 pts →
  **46 tri, on_sphere=true, inside_octant=46/46**(완벽 clip) vs `tessellate_face_surface`의 208
  tri whole-band. box corner는 limited-u + pole 무관(u=±π/4,±3π/4 / v≈π/4)이라 seam-shift 불필요.
  회귀 `adr197_beta3j2_uv_earcut_clips_octant_patch`. **render gap 해결 가능 확정** — 구현 =
  `export_buffers_inner` 신규 분기(arc-bounded 곡면 면 → uv-earcut) + (품질) interior subdivision.
- **γ-2b-2 구현 (arc-bounded patch 렌더, render gap 해소)**: `Mesh::tessellate_arc_bounded_face(
  face_id, chord_tol) -> Option<SurfaceTessellation>` — arc-bounded Sphere 면(≥3 vert + ≥1 Arc
  edge)을 boundary polygonize(`tessellate_edge`, loop 방향 orient) → sphere invert(seam-unwrap:
  u-range>π면 low side +2π) → `earcutr::earcut`(uv) → winding-fix(outward sphere normal). 비-arc-
  bounded(self-loop/quad/non-Sphere)는 **None → 기존 경로 보존**. `export_buffers_inner` 곡면 분기
  wiring(arc-bounded Some면 사용, 아니면 `surface.tessellate()`). octant 검증: tessellate **21 tri
  inside 21/21** + export **21 tri clipped**(vs whole-sphere 208). 회귀 `adr197_beta3j2_tessellate_
  arc_bounded_clips`(helper + export 통합 + self-loop None). **메인 렌더 변경이나 기존 곡면 테스트
  무영향**. 품질 follow-up: interior subdivision(현 boundary-only=clip-correct/coarse).
- **γ-2b-3 octant orchestration + 브라우저 시연 (box∩sphere corner 완성)**: `Mesh::boolean_sphere_
  octant(sphere_faces, planes[3], mat)` — 3 cutting plane이 sphere 내부 box corner에서 만나는
  경우, corner geometry(γ-2b-1)로 직접 솔리드 빌드: **4 vert**(3 crossing `sphere_plane_pair_
  crossings`(3번째 halfspace로 선택) + box corner B(3 plane 교점, `DMat3` inverse, sphere 내부
  검증)) + **4 face**(1 curved Sphere patch[3 arc edge, `add_face_with_holes`+arc attach, γ-2b-2
  렌더] + 3 planar cap[각 1 arc + 2 line edge]). **공유 edge로 watertight manifold = topological
  사면체**(V4-E6-F4, Euler 2). winding은 outward(patch=radial, cap=−n) order. **첫 시도 PASS** —
  watertight + invariants valid + non_manifold 0 + **is_closed_solid true**. WASM `demoSphereOctant`
  + bridge 래퍼. **브라우저 시연**(real WASM rebuild): sphere r30 ∩ {x>10,y>10,z>10} → **4 face
  {Sphere 1 + Plane 3} + closedSolid true + 192 tri 렌더 + console error 0**. 회귀 `adr197_beta3j3_
  sphere_octant_orchestration`(manifold + closed + 1S/3P + render + corner-outside bail) + vitest
  WasmBridge +1(254). **box∩sphere corner-cut 완성** (1782).
- **γ-2b-4 일반 `boolean()` corner 라우팅 + 시연 (box∩sphere corner 통합 완결)**: `boolean()`
  의 `try_curved_intersect_dispatch`에 Sphere corner 분기 추가. 감지 = **per-axis cut count**
  (`sphere_box_corner_planes`): 각 축의 box plane이 sphere를 자름 ⟺ `center−r < plane < center+r`.
  정확히 **(1,1,1)**(축당 1 plane) + corner B(3 plane 교점) inside sphere → 3 plane 추출(normal
  toward kept) → `boolean_sphere_octant` → box 소거. (0,0,0)no-op/(0,0,1)cap/(0,0,2)slab은 기존
  Z-cut, (2,2,2)full box·(1,1,0)wedge·(2,1,1)complex는 fall-through. **브라우저 시연**(real WASM
  rebuild, `demoBooleanSphereCorner` offset box): `boolean(sphere r30, box@(30,30,30) size50,
  Intersect)` → (1,1,1) 자동 감지 → 4 face{Sphere1+Plane3} + closedSolid + 156 tri + error 0.
  회귀 `adr197_beta3j4_sphere_corner_box_routes_to_octant`(route + full-box bail) + `sim_beta3j4_
  box_corner_detection`(7 config 분류) + vitest 255. **box∩sphere corner-cut 완전 통합** (1784).
- **실제 UI 검증 + Gap A 수정 (sphere 붕괴 버그)**: corner 라우팅이 실제 UI Boolean fallback
  경로(`BooleanHandler.startBooleanOp` → `booleanDispatchDcelMulti` not-handled → `bridge.
  booleanOp` → `mesh.boolean()` → corner 감지)로 닿음을 브라우저 검증. **발견된 별개 버그**:
  `face_rederive_on_draw`(ADR-186 유도면, production 기본 ON)가 create_sphere 시 `intersect_
  faces_inner` → `rederive_coplanar_on_draw`로 **Path B sphere(2 self-loop hemisphere, Sphere
  surface)를 1 Plane disk로 collapse**(coplanar 재유도가 곡면 면을 planar로 오취급). **수정**:
  `intersect_faces_inner` 진입점에 곡면 surface(Sphere/Cylinder/Cone/Torus/NURBS) 면 제외 가드
  (None=polygonal·Plane만 coplanar 후보). 브라우저 검증(production 기본값 모두 ON): create_sphere
  → 2 Sphere face(붕괴 없음) + `boolean_op(sphere, corner-box, intersect)` → 4 face{Sphere1+
  Plane3} closedSolid + error 0. 회귀 `adr197_path_b_sphere_survives_face_rederive`(axia-core
  363). 기존 coplanar 회귀(245+ ADR-101/176, planar 면) 무영향. **남은 UI gap**: 선택 그룹
  A/B 명시(half/half split이 sphere/box 못 나눔, ADR-074) / DCEL-multi-first fallthrough 검증.
- **실제 UI 메뉴 워크플로우 완성 (Gap B/C/D closure)**: 도구 생성 → 8면 선택 → Boolean Intersect
  메뉴(`startBooleanOp`) → corner 솔리드. 브라우저 end-to-end 검증(production 기본값). 3 gap 처리:
  * **Gap B (선택 그룹)**: `BooleanHandler.startBooleanOp`의 half/half split이 sphere/box를
    못 나눔 → `resolveBooleanOperands`(명시 그룹 ADR-074 → **XIA(솔리드) 그룹** → half/half
    fallback) 신규. `getXiaForFace`(not-found −1)로 selection을 owning solid별 그룹핑. 두 split
    site(multi-DCEL + booleanOp) 모두 통합. vitest +3.
  * **Gap C (DCEL-multi fallthrough)**: **수정 불필요** — `booleanDispatchDcelMulti(sphere∩box)`
    → `pathUsed:'Mesh'` + `UnsupportedSurfaceKind`(no-op) → `handleMultiDcelResult` false →
    fallthrough → `booleanOp` → `mesh.boolean()` corner 라우팅 도달 (검증 완료).
  * **Gap D (sphere가 sheet로 오분류)**: `Mesh::is_face_in_volume`이 `he_twin`(self-loop edge는
    양 HE dst가 같은 anchor라 `dst!=start` 필터 never → twin=self → false)으로 Path B sphere를
    **sheet 오분류** → Sheet/Wall 혼합 체크가 sphere∩box reject. 수정: **radial chain(next_rad)
    으로 다른 face HE 탐색**(normal edge + self-loop 모두). 렌더 ADR-018 two-tone에도 영향(곡면
    primitive가 wall로 정분류). 회귀 `adr197_path_b_sphere_faces_are_in_volume`(sphere in-volume
    + lone disk sheet). axia-geo 1785. 기존 회귀 무영향.
- **β-3-k full sphere-rounded box (8 corner) + 라우팅 + 시연**: `Mesh::boolean_sphere_box_full(
  sphere_faces, bmin, bmax, mat)` — box 모든 corner가 sphere 밖 + 모든 face가 sphere 자르는
  경우(rounded cube). **24 vert(12 box edge × 2 crossing) + 14 face**(8 Sphere triangle[3 arc]
  + 6 Plane octagon[4 line + 4 arc], Euler V−E+F=2). γ-2b 머신리 재사용 — crossings(γ-2b-1) +
  arc-bounded render(γ-2b-2) + `add_face_with_holes` sew + `arc_range_toward`(box corner로 minor
  arc 선택) + angle-sort octagon + poly_normal winding-fix. **첫 시도 PASS**. 라우팅: `is_sphere_
  rounded_box`(모든 face cut + 모든 corner 밖) → `try_curved_intersect_dispatch`에서 (2,2,2)
  케이스 분기(corner (1,1,1) octant 다음). **브라우저 시연**(real WASM rebuild, 기존 `demoBoolean
  SphereBox` centered box): `boolean(sphere r30, box 40³ centered, Intersect)` → (2,2,2) 자동
  감지 → **14 face{Sphere8+Plane6} closedSolid + 812 tri + error 0**. 회귀 `adr197_beta3k_full_
  box_sphere`(14 face + watertight + manifold + 8S/6P + corner-inside bail) + `sim_beta3k_full_
  box_sphere_structure`(24/36/14 Euler). axia-geo 1787. web/WASM 소스 변경 0(기존 routing+demo).
- **β-3-l torus 2-cut slab + 라우팅 + 시연**: `Mesh::boolean_torus_slab(torus_faces, z_lo, z_hi,
  mat)` — Z-up torus의 두 평면이 **모두 tube를 자르는** 경우(|z−cz|<minor_radius) → 수평 도넛
  band(genus-1 ring) = **2 Torus band(outer/inner tube) + 2 Plane washer(z_hi/z_lo 환형 cap)**,
  4 cut circle(outer/inner × hi/lo)로 wire. 신규 DCEL 헬퍼 2개: `Mesh::add_self_loop_circle(anchor,
  circle)→(fwd,bwd)` + `Mesh::wire_2loop_face(outer_he, inner_he, surface, normal, mat)→FaceId`
  (각 circle의 fwd→band·bwd→washer 공유). band v-range = outer [asin(d_lo/r),asin(d_hi/r)] / inner
  [π−asin(d_hi/r),π−asin(d_lo/r)]. 라우팅: `try_curved_intersect_dispatch`의 Torus (cuts_lo &&
  cuts_hi) 분기를 fall-through에서 `boolean_torus_slab`로 전환(XY-containment Z-slab). **첫 시도
  PASS**. **브라우저 시연**(real WASM rebuild, `demoTorusSlab` + general `boolean()` routing):
  torus R5 r1.5, slab z∈[−0.5,0.5] → **4 face{Torus2+Plane2} watertight + manifold + 1412 tri**,
  z범위 정확 [−0.5,0.5] / 외곽 반경 6.5(=R+r) / **도넛 구멍 정상 open(hole 메우는 삼각형 0)**.
  회귀 `adr197_beta3l_torus_slab`(4 face + watertight + manifold + 2T/2P + multi-loop + halfspace
  bail) + `sim_beta3l_torus_slab_structure`(4 circle 특성) + routing case in `adr197_beta3i_general_
  routing_sphere_box`(2 Torus band + watertight). axia-geo 1787 → 1789(+2; routing은 기존 테스트
  케이스). WASM `demoTorusSlab` + bridge `demo_torus_slab` 추가.
- **β-3-m subtract 곡면 라우팅 (4 곡면, 사용자 결재 Q1=4 곡면 한 트랙 / Q2=concave defer)**:
  핵심 항등식 `A − box = A ∩ ¬box`. box(XY-포함 Z만 cut)는 Boolean 관점 Z-slab → **subtract =
  intersect 기계를 keep-side만 뒤집어 재사용**. (1) halfspace cut → 1 outer piece(cap/stub/
  frustum/band-ring) = **기존 intersect 함수 재사용** (sphere_halfspace plane flip / cylinder·
  cone_slab는 extent-bound clamp `lo=z.max(base)`로 ±1e9 전달 / torus_halfspace keep flip). (2)
  slab cut → **2 disjoint outer piece** = 신규 dedicated 2-piece builder (extract → remove once →
  sew primitive ×2, `boolean_torus_slab` 패턴). 신규 4 builder: `boolean_sphere_slab_subtract`
  (2 cap, sew_closed_curve_pair ×2) / `_cylinder_slab_subtract`(2 stub, sew_curved_band ×2) /
  `_cone_slab_subtract`(base frustum + tip cone, sew_curved_band + sew_closed_curve_pair) /
  `_torus_slab_subtract`(2 band-ring, sew_torus_cap ×2) + 공유 헬퍼 `remove_primitive_solid`.
  라우팅 `try_curved_subtract_dispatch`(order-sensitive: **curved − box만**, box − curved는
  concave fall-through) + `boolean()`에 op==Subtract 분기. **concave defer**(sphere − corner/full
  box = scooped octant/6 bulge-cap, XY-cut box → fall-through legacy). **첫 시도 PASS**(컴파일 후
  torus 단일 FaceId `&[t]` 1건만 정정). **브라우저 시연**(real WASM, `demoBooleanSubtractSphereBox`
  + general `boolean(Subtract)` routing): sphere r30 − slab box(z cut ±20) → **4 face{Sphere2+
  Plane2}**, z범위 [−30,30] / **제거 slab(|z|<20) 정점 0** / top cap 1220 + bottom cap 1220(2 분리
  cap) / 4326 tri. 회귀 `adr197_beta3m_curved_subtract`(4 곡면 slab subtract + sphere halfspace +
  routing + 2 concave bail) + `sim_beta3m_subtract_semantics_matrix`. axia-geo 1789 → 1791(+2).
  WASM `demoBooleanSubtractSphereBox` + bridge 추가.
- **lesson (β-3-m)**: subtract는 새 기하 거의 없음 — **intersect 기계의 keep-side flip + 2-piece**.
  halfspace는 기존 함수 재사용(cylinder/cone는 slab의 extent clamp가 halfspace 흡수), slab만 신규
  2-piece builder. concave(box − curved, scooped) = 별도 트랙(새 sew topology).
- **β-3-n 곡면 칼(cut/slice 도구) — 사전검토 + SLICE 엔진 (사용자 결재: 둘 다 토글 / 기존 SliceTool
  확장 / 메뉴 다듬기 3개)**: **사전검토 발견** — 칼 도구(`SliceTool`)가 이미 존재하나 폴리곤
  `slice_volume_by_plane` 호출 → 곡면 Path B 솔리드에서 **실패**(실증 sim: multi-loop 면 → `Err:
  has holes — not yet supported`) + 표면 파괴. Boolean 메뉴는 이미 곡면 라우팅 작동. → 칼을
  곡면-인식으로(수평 Z-평면 + Path B → 곡면 cut, 표면 보존). **두 의미**: TRIM(한쪽만, **기존
  boolean(Subtract) 재사용 = 엔진 0**) / SLICE(둘로 쪼갬, 신규 builder). **SLICE 엔진 구현**:
  `boolean_{sphere(2cap),cylinder(2stub),cone(tip+frustum),torus(2band-ring)}_slice(faces, k, mat)`
  — 단일 Z-평면 z=k → **2 disjoint 곡면 볼륨**. 두 조각이 평면 공유(gap 없음) → cut circle anchor를
  **반대 각도(+X/−X)에 배치**해 0.15μm dedup(LOCKED #5) 병합 회피(pinch 방지). **첫 시도 PASS**
  (`two_shells` connected-component 검증 = 2). 회귀 `adr197_beta3n_curved_slice`(4 곡면 slice +
  watertight + manifold + 2 disjoint shells + miss bail) + `sim_beta3n_curved_knife_gap_and_design`
  (폴리곤 갭 실증 + TRIM/SLICE 매트릭스). axia-geo 1791 → 1793(+2).
- **β-3-n dispatcher + WASM + TS 도구 + 메뉴 (곡면 칼 end-to-end 완성)**: **dispatcher**
  `Mesh::cut_curved_by_z_plane(faces, z, CurvedCutMode{Slice|KeepAbove|KeepBelow}, mat)` — 곡면
  primitive 분류 → slice/trim 라우팅, **비-곡면이면 None**(caller 폴리곤 fallback). KeepAbove/Below는
  기존 halfspace builder 재사용(intersect 기계 keep-side flip). **WASM** `cutCurvedByZPlane(faceIds,
  z, mode)` (transaction-wrapped, Err 시 snapshot 복원, `routed:false` = 폴리곤 fallback 신호) +
  `boolean_op`에 `curved` 플래그 추가(res.debug "curved" 포함 여부). **TS** SliceTool 곡면-인식 확장:
  cutMode 토글(M키, 쪼개기/위트림/아래트림) + **H키=수평 절단**(첫 점 높이에 Z-평면) + commit 시
  수평+곡면이면 cutCurvedByZPlane(곡면 보존 Toast), routed:false면 기존 폴리곤 slice fallback.
  **메뉴 다듬기 3개**: (a) Boolean 메뉴 `curved` → "곡면 보존됨 (NURBS surface)" Toast, (b) Slice
  단축키 `K`(knife) 추가(KeyboardShortcuts keyMap + AxiaCommands) + label "평면으로 자르기/칼", (c)
  subtract Toast에 "A(유지) N면 − B(제거) M면" minuend 명확화. 회귀 `adr197_beta3n_cut_curved_dispatch`
  (slice/above/below + 비-곡면 None). axia-geo 1793 → 1794(+1). vitest SliceTool/BooleanHandler 28
  무회귀. **브라우저 시연**(real WASM): sphere(2면) → SLICE z=10 → routed:true 4면 2 cap, z=10 정확
  분할(top 1496+bottom 2376 verts) / TRIM above → 1 cap 2면 / sphere∩box boolean_op → curved:true
  3면 barrel. **곡면 칼 end-to-end 완성** (도구→H 수평절단→곡면 cut 표면보존, 비-곡면→폴리곤 fallback).
- **lesson (β-3-n)**: 곡면 칼은 이미 SliceTool 존재 → 곡면-인식 확장만(중복 0, 메타-원칙 #1). SLICE는
  단일 평면 2조각이 평면 공유 → anchor 반대각도로 dedup 병합 회피. TRIM은 halfspace 재사용(엔진 0).
  결과 2 볼륨은 1 XIA 공유(boolean_op 답습; 별도 XIA 할당은 후속 refinement).
- **β-3-o Union 사전검토(4-agent audit) + Case B sphere∪sphere (사용자 결재: 둘 다 B먼저→A / 가능한
  4 곡면)**: **audit 발견** — Union은 곡면 dispatch 안 탐(Intersect/Subtract만) → 곡면에서 깨짐;
  NURBS-DCEL은 analytic primitive 거부=**dead-end** → #1~#4처럼 직접 빌드; sphere_sphere SSI 부재나
  radical-plane 공식 trivial; sew_closed_curve_pair가 curved∪curved에 그대로 맞음. **Case B
  sphere∪sphere**: 두 Z-coaxial 겹친 구 → 캡슐 = 각 구를 SSI 원에서 trim, OUTER cap 유지 → **2
  Sphere cap이 SSI 원 공유**(2면, watertight, Euler 2). 신규 `sphere_sphere_z_circle(c1,r1,c2,r2)→
  (z_ssi,rho,v1,v2)` 헬퍼(radical plane: a=(d²+r1²−r2²)/2d) + `boolean_sphere_sphere_union`
  (sew_closed_curve_pair 재사용, fwd/bwd가 각각 다른 Sphere cap). 라우팅 `try_curved_union_dispatch`
  (양 operand 곡면 분류, sphere×sphere만 MVP, disjoint/nested→None) + boolean() Union 분기. **첫
  시도 PASS**(f32 fold 1건 정정). 회귀 `adr197_beta3o_sphere_sphere_union`(2 cap + watertight +
  manifold + 2 Sphere + routing + disjoint/nested bail + 헬퍼 unit) + `sim_beta3o_union_gap_and_
  design`(audit + Case A/B 특성화). axia-geo 1794→1796(+2). **브라우저 시연**(real WASM,
  demoBooleanUnionSpheres + general boolean(Union)): 두 구 r30 sep40 → **2면 {Sphere×2} 캡슐**,
  z∈[−30,70](극에서 극), surface 보존(kind 3×2), 11136 tris. WASM demoBooleanUnionSpheres + bridge.
- **lesson (β-3-o)**: Union도 #1~#4 직접 빌드(NURBS-DCEL dead-end). curved∪curved = 2 cap이 SSI 원
  공유 = sew_closed_curve_pair 그대로(fwd/bwd 다른 surface). **Case B는 sphere∪sphere가 깨끗한
  canonical** — cyl/cone/torus∪동종은 horizontal-circle SSI 부재(coaxial cyl=연장/nested, 수직축=
  non-analytic) → 별도 평가 필요. Case A(4 곡면∪box)는 4 primitive 모두 깨끗(box+protruding cap).
- **β-3-o Union Case B cone∪cone opposing (hourglass)**: 사전검토(`sim_beta3o_case_b_same_kind_
  assessment`) 결과 동종 중 깨끗한 수평-원 SSI는 sphere∪sphere + **cone∪cone opposing 1개뿐**
  (cyl/torus∪동종=non-analytic 별도 ADR). **기하 정정**: 겹치는 마주보는 두 cone union은 bicone/
  diamond(2 cap)가 **아니라 HOURGLASS** — union은 wide part keep, apex는 상대 cone 안→제거 → 2
  Cone **frustum band** + 2 base disk = 4면, 2 band이 waist SSI 원 공유. 신규: `Mesh::create_cone_
  kernel_native_apex_down`(apex-up 미러, axis_dir=+Z) + `Mesh::sew_hourglass(base_a/waist/base_b 3원
  + 2 band[waist 공유] + 2 disk)` + free fn `cone_cone_hourglass(...)→(z_waist,rho,c1_up)` 검증
  (opposing+coaxial+overlap). `boolean_cone_cone_union`: waist=(apex_up·tan+apex_dn·tan)/(tan+tan).
  `classify_curved_primitive` Cone 분기 완화(apex-up −Z + apex-down +Z 모두, AABB 정합). dispatch
  Case B에 cone∪cone(faces[0]이 disk일 수 있어 find_map). 첫 시도 거의 PASS(cone 생성 mat 인자 +
  classify apex-down 거부 2회 정정). 회귀 `adr197_beta3o_cone_cone_union`(4면 + watertight + manifold
  + 2 Cone/2 Plane + waist r≈1 + routing + same-direction bail) + `sim_beta3o_case_b_same_kind_
  assessment`. axia-geo 1800→1802(+2). **브라우저 시연**(real WASM, demoBooleanUnionConeCone +
  general boolean(Union)): cone r20 h40 마주보기 → **4면 {2 Plane + 2 Cone}**, z∈[0,40], **waist
  r=10(좁음) < base r=20(넓음) = hourglass 정확**. WASM demoBooleanUnionConeCone + bridge.
- **lesson (β-3-o cone)**: 겹치는 마주보는 cone union = **hourglass(2 frustum + 2 disk), NOT diamond**
  (apex 상대 cone 안→제거). diamond는 base-to-base 접합(overlap 0)이라 Boolean 아님. apex-down cone
  생성 필요(create_cone_kernel_native_apex_down) + classify apex-up/down 양쪽 허용. **Case B 동종
  깨끗한 케이스 2개 완성**(sphere∪sphere capsule + cone∪cone hourglass). cyl/torus∪동종=non-analytic.
- **β-3-p Union Case A sphere∪box**: box가 곡면을 XY-포함 + Z-cut → box가 중간 band 흡수, cap이
  box top/bottom으로 **관통**. 결과 = box(4 wall + top/bottom 면이 원형 hole로 **pierced**) + 2
  Sphere cap = **8면**. 신규 `Mesh::pierce_face_with_cap(host_face, anchor, circle, cap_surf,
  normal, mat)→cap_face` — self-loop 원의 hole HE를 host(box) 면 inner loop에 추가 + 반대 HE를
  새 cap 면 outer로(twin HE 공유 → watertight). `boolean_sphere_box_union`: box top/bottom 면을
  **기하 normal+z-position**으로 찾고(make_box는 Plane surface 없음 — production create_box는 있음)
  cap 2개 pierce. 라우팅 `try_curved_union_dispatch` Case A 분기(곡면+box, XY-포함+Z-cut, sphere
  MVP). **첫 시도 거의 PASS**(box top/bottom 찾기를 Plane surface→기하 normal로 1회 정정). 회귀
  `adr197_beta3p_sphere_box_union`(8면 + watertight + manifold + 2 Sphere + box 2면 pierced + cap
  z=3 + routing + non-XY-contain bail). axia-geo 1796→1797(+1). **브라우저 시연**(real WASM,
  demoBooleanUnionSphereBox + general boolean(Union)): sphere r30 ∪ box(z±20) → **8면 {6 Plane +
  2 Sphere}**, cap이 box 위 관통(z=30), **pierce 구멍 정상 open**(hole-fill 삼각형 0, box top 79
  tris annular). WASM demoBooleanUnionSphereBox + bridge.
- **lesson (β-3-p sphere)**: 곡면∪box = box 면을 원으로 pierce(inner hole) + 반대편 cap = pierce_
  face_with_cap(기존 면에 hole 추가, sew_torus_cap washer 패턴 변형). box 면 찾기는 **기하 normal+z**가
  surface attach보다 견고(test make_box vs production create_box 차이).
- **β-3-p Union Case A cylinder∪box**: cylinder는 protruding part가 **stub(side band + end disk)**
  라 sphere(1 cap)와 다름. 신규 `Mesh::pierce_face_with_band_stub(host, pierce_circle, far_circle,
  band, disk, …)→[band,disk]` — pierce 원(box inner hole ↔ band inner loop) + far 원(band outer ↔
  end disk), sew_curved_band 변형(한 disk를 box face hole로). `boolean_cylinder_box_union`: box
  top/bottom을 `box_horizontal_faces`(공유 헬퍼)로 찾고 2 stub pierce. 결과 = box(4 wall + pierced
  top/bottom) + 2 stub(band+disk) = **10면**. dispatch Case A에 Cylinder 추가. 회귀 `adr197_beta3p_
  cylinder_box_union`(10면 + watertight + manifold + 2 Cylinder band + box 2면 pierced + stub z=±3 +
  routing). axia-geo 1797→1798(+1). **브라우저 시연**(real WASM, demoBooleanUnionCylinderBox +
  general boolean(Union)): cylinder r20 h60 ∪ box(z[20,40]) → **10면 {8 Plane + 2 Cylinder}**, stub이
  cylinder 끝(z=0,60) 관통, **양 pierce 구멍 open**. WASM demoBooleanUnionCylinderBox + bridge.
- **lesson (β-3-p cylinder)**: cylinder∪box stub = pierce_face_with_band_stub(box hole + 2-loop
  band + disk). pierce render 정상(양 구멍 open). `box_horizontal_faces` 공유 헬퍼로 box 면 찾기
  통일. **cone∪box는 tip(cap) + frustum(stub) 혼합**, **torus∪box는 annular(2원/pierce)** → 더 복잡.
- **β-3-p Union Case A cone∪box (MIXED)**: cone(apex-up)은 protruding part가 **혼합** — apex TIP은
  box top으로(Cone cap), base FRUSTUM은 box bottom으로(Cone band + base disk stub). **두 헬퍼 모두
  재사용**: `pierce_face_with_cap`(tip) + `pierce_face_with_band_stub`(frustum). `boolean_cone_box_
  union`: box top에 tip pierce(Cone v∈[0,v(z_hi)] apex degenerate) + box bottom에 frustum pierce
  (Cone band v∈[v(z_lo),v(base)] + base disk). 결과 = box(4 wall + pierced top/bottom) + tip(1) +
  frustum(2) = **9면**. dispatch Case A에 Cone. 회귀 `adr197_beta3p_cone_box_union`(9면 + watertight
  + manifold + 2 Cone + box 2면 pierced + tip→apex/base→0 + routing). axia-geo 1798→1799(+1). **브
  라우저 시연**(real WASM, demoBooleanUnionConeBox + general boolean(Union)): cone r20 h60 ∪ box
  (z[20,40]) → **9면 {7 Plane + 2 Cone}**, tip이 apex(z=60)·frustum base(z=0) 관통, **양 pierce 구멍
  open**. WASM demoBooleanUnionConeBox + bridge.
- **lesson (β-3-p cone)**: cone∪box = **두 헬퍼 혼합**(tip=cap, frustum=band-stub) — 누적 머신리
  재사용으로 신규 sew 0. Case A 3/4 완성(sphere/cylinder/cone). **torus∪box만 남음** — torus는
  annular(box 면에 2 원 구멍, band-ring stub) → pierce를 annular로 확장 필요.
- **β-3-p Union Case A torus∪box (annular pierce, Case A 완성)** 🎉: torus tube가 box top/bottom을
  **annulus**로 관통 — box 면이 **outer annulus(box rect − ρ_outer) + donut-center disk(ρ_inner 안)**
  으로 나뉘고 torus band-ring이 두 원(ρ_outer/ρ_inner)을 연결. 신규 `Mesh::pierce_face_with_torus_
  band(host, outer_circle, inner_circle, band, inner_disk, …)→[band, inner_disk]` — sew_torus_cap의
  washer를 **box hole(outer) + inner disk(donut center)** 로 분리한 변형. `boolean_torus_box_union`:
  box top/bottom에 band-ring pierce(torus_z_cut 2원) + donut-center disk. 결과 = box(4 wall +
  annular top/bottom + 2 donut disk) + 2 Torus band = **10면**. dispatch Case A에 Torus → **4 곡면
  exhaustive**. 회귀 `adr197_beta3p_torus_box_union`(10면 + watertight + manifold + 2 Torus + box 2면
  annular pierced + tube z=±1.5 + routing). axia-geo 1799→1800(+1). **브라우저 시연**(real WASM,
  demoBooleanUnionTorusBox + general boolean(Union)): torus R5 r1.5 ∪ box(z±0.5) → **10면 {8 Plane +
  2 Torus}**, **annular pierce 정확**(gap[4.2,5.8] open=채움0 / donut center 채움50 tris / 외곽
  annulus 채움54 tris), tube z=±1.5. WASM demoBooleanUnionTorusBox + bridge.
- **lesson (β-3-p torus / Case A 완성)**: torus∪box = annular pierce(box 면 분할 outer annulus +
  inner disk, sew_torus_cap washer 분리 변형). **Case A 4 곡면 모두 완성**(sphere cap / cylinder stub
  / cone 혼합 / torus annular) — pierce 헬퍼 3개(cap/band-stub/torus-band)로 모든 곡면 protruding
  part 커버. 누적 머신리 재사용 패턴 입증.
- **잔여 (순서)**: Case B cyl/cone/torus∪동종 평가 / curved patch interior subdivision(품질) +
  concave subtract + slice 2볼륨 별도 XIA.
- lesson: SSI는 3D point만 정확, uv placeholder → **곡면 inversion이 필수 인프라**.
  주기 도메인(u-seam)이 핵심 난점 — 단일/소수 oblique는 seam-shift, 다중은 periodic.
- lesson: primitive마다 surface param의 구조가 다름 — cylinder v=축위치(선형, pole無) →
  imprint/classify가 clamp로 degenerate. **답습은 PATTERN(SSI→band+disk→sew)이지 코드가
  아님**; `sew_curved_band` primitive만 공유, orchestration은 곡면별.

## §D Acceptance Log

- **2026-06-11 α + β-1** (`873bb7b`) — α spec + 5-lens 매핑 ground + **β-1
  audit-first 정밀 조사**(§9). **사용자 결재 Q1 = Path B (NURBS volumetric, 곡면
  보존) + Q2 = a (robust custom 조립)**. β-1 finding: NURBS DCEL은 닫힌 SSI chain
  만 처리(`if !chain.closed continue`), 솔리드 면쌍의 열린 선분은 skip → 전역 닫힌
  loop 조립(Phase L)이 빠진 핵심. 코드 변경 0.

- **2026-06-11 β-2 사전검토 (설계)** — 사용자와 깊은 설계 round. **아키텍처 정밀화**:
  "전역 loop 조립"보다 **imprint + classify + sew** (표준 B-Rep Boolean, Fusion/ASM
  의 ACIS 계보)가 정확한 모델. 전역 닫힌 loop는 *암묵적* — 면을 자르는 데는 각 면의
  열린 segment만 필요. **재사용 매트릭스**: classify(`classify_split_faces` +
  `point_in_solid`) 재사용 / imprint 평면(`split_face_by_line`/chain) 재사용 / **sew
  = 신규 핵심** (전략 2 = add_face 재구성으로 radial을 검증된 weld 기계에 위임) /
  곡면 imprint 후속. degenerate 설계: box 모서리 가로지르기(정상 케이스)는 dedup
  vertex 공유 + add_face radial 위임으로 처리, 진짜 degenerate(코너 일치/tangent/
  blind cut/3+ valence)는 가드 + verify-and-bail. 코드 변경 0.

- **2026-06-11 β-2a-i** (`39356c5`) — 일반 교차 primitive. `detect_general_
  intersections`(plane∩plane clip → segment) + 헬퍼. standalone(미연결). 회귀 +1
  (box-box L-cut segment 정확).

- **2026-06-11 β-2a-ii** (`52deab8`) — surface-preserving imprint. `order_segments_
  into_chain`(segment → cut chain) + `split_convex_polygon_by_chain`(면을 chain으로
  2 sub-loop 분할, 점 리스트 반환). 회귀 +1 (A +X면 L-chain → 2 sub-loop, inside-B
  corner 정확).

- **2026-06-11 β-2b+c** (`7db059b`) — **🎉 엔진 최초의 정확한 일반 3D 솔리드 Boolean**.
  `Mesh::solid_boolean`(imprint → classify → sew). A[0,4]³ − B[2,6]³ corner subtract
  → watertight(boundary HE 0) + manifold + (1,1,1) INSIDE + (3,3,3) OUTSIDE. legacy
  (3면 붕괴)/NURBS DCEL(no-op) 모두 실패하던 케이스 해결. **디버그로 잡은 2 핵심 버그
  / lesson**:
  * **remove_face가 remnant boundary HE를 남겨** re-add weld 방해 → 원본 edge 완전
    제거(`remove_edge_and_halfedges`)로 깨끗한 vertex cloud 위 재구성. + free-edge
    cleanup(`is_edge_completely_free`; `cleanup_dangling`은 valence-1만이라 버려진
    면의 닫힌 loop 잔재 못 지움).
  * **정점 평균 centroid가 비-convex L의 경계 코너에 떨어져** point_in_solid 모호 →
    **area centroid**(`polygon_area_centroid`, signed-area 가중 = L 면적중심 안전
    내부)로 해결. *비-convex 분류엔 area centroid 필수* — 향후 lesson.
  standalone(미연결). 회귀 +1.

- **2026-06-11 β-2 ops hardening** (`fe41545`) — Union/Intersect 검증. `solid_boolean`
  3 op 전부 정확(box-box). flip 규칙: Subtract만 B flip. 회귀 +2.

- **2026-06-12 β-2d 통합 planar arrangement + degenerate 가드** (본 commit) —
  multi-chain + degenerate 동시 해결. 사용자 결재: 점진 special-case 대신 **통합
  arrangement(B)** 즉시 구현.
  - **`arrange_polygon_2d(poly, cuts) -> Vec<Region2D{outer, holes}>`**: 면을 cut
    세그먼트로 planar 세분(vertex/edge 교차 → half-edge 각도정렬 → "twin의 CCW 직전=
    next" face walk → cycle → CCW=region/CW=hole, strictly-larger 포함으로 hole 배정,
    unbounded 자동 폐기). **단일 chain / 평행 strip / 교차 chain / 내부 closed loop
    (annulus)** 를 하나로 처리. 기존 special-case 3개(`order_segments_into_chain` /
    `split_convex_polygon_by_chain` / `polygon_area_centroid`, 190줄) **삭제** — SSOT.
  - **터널(B 관통) watertight**: A 윗/아랫면이 closed-loop 구멍 → annulus+disk,
    터널벽이 구멍 경계에 SSI 공유 vert로 자동 weld. 이전 16 boundary HE(균열) → **0**.
  - **degenerate 가드** (ultracode adversarial, Workflow rate-limit → 직접 6-렌즈):
    2D arrangement **20/20 robust** (partition 불변식; collinear tie / blind /
    Y-junction / notch-same-edge / nested depth-2 / concave / valence-6 / 비-convex
    L-hole-centroid-in-notch / 교차 loop 등). 3D 파이프라인 **3개 실제 break 발견+수정**:
    * thin slab — face_eps **clamp [2e-4,1e-3]** (얇은 feature 관통 방지)
    * union coplanar membrane(non-manifold) + identical A−A(inverted) — **IN/OUT
      멤버십 classify로 통합**: 면 양쪽(±normal)의 result 소속이 다를 때만 boundary →
      internal membrane 자동 cancel + identical 자동 empty. + coincident same-sense
      dedup(flush 공유면). 3 ad-hoc → 1 표준 B-Rep classify. flip = res_out.
  - 회귀: axia-geo **+33** (arrange core 6 + 2D degenerate 20 + 3D degenerate 7 =
    1717 → 1750). 절대 #[ignore] 0. lib dead-code 경고는 기존 ADR-197-경로(미연결)
    패턴 — β-4 production 연결 시 일괄 해소.
  - lesson: **proper planar arrangement은 2D degenerate에 본질적 robust** (special-
    case보다 견고). **IN/OUT 멤버십 = 표준 B-Rep classify**(coplanar 자연 처리).
    `point_in_solid`은 400:1 aspect ratio에서 ray-cast 불안정 → 극단 thin은 구조
    검증으로 대체(helper 한계, solid_boolean 정확).

- **다음 (별도 결재)**: surface 보존 검증(실제 곡면 입력 — make_box는 surface 없음) →
  β-3 곡면 SSI(닫힌-loop trim 기계 결합) → β-4 production 라우팅(`boolean_dispatch_
  dcel_multi` 승격 + group A/B). 잔존 degenerate(non-planar/NaN/self-intersecting 입력)
  은 best-effort no-crash, verify-and-bail은 future(메타-원칙 #16). governance:
  ADR-064/066 "probe only" reaffirm (L-197-8).
