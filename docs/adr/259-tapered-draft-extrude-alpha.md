# ADR-259 — α spec: Tapered / Draft Extrude (concave-capable, fail-closed, exact-Plane sides)

- **Status**: Proposed (α spec — 사용자 결재 D1~D6 반영; β 구현 대기)
- **Date**: 2026-06-26
- **Track**: 6 (Extrude/Cut/Punch) — "완벽한 extrude" 로드맵 #1 (taper)
- **Author**: WYKO + Claude (de-risk workflow `wf_aa14559e-205` 3-study + 3-lens adversarial review + live 재검증)

## 1. Context

LOCKED extrude 시뮬레이션 baseline([[feedback_extrude_sim_baseline]], 2026-06-26)에서
**테이퍼/드래프트 extrude 부재** 확정 (taperVerts=후처리 deform, 2D polygon offset
부재). AixiAcad 비교(ADR 비교 워크플로우)에서 `extrude_planar_face_tapered`
(draft 각 θ, inward/outward, convex-only)가 우리 gap 으로 식별. 본 ADR 은 이를
**native + concave-capable** 로 구현.

**사용자 결재 (2026-06-26):**
- 도구: **PushPullTool 확장** (VCB `거리,각도`) (D3=a)
- 범위: **concave 다각형 지원** (D1=b) — 단 fail-closed (아래 §4)
- 각도: **draft 각, 양방향, |θ|<89°** (D6=c)
- **핵심 directive**: "구현에 필요한 다른 기능도 파악해 같이 구현 → 면깨짐 최대 방지"

## 2. 핵심 기하 발견 (concave 측벽은 평면)

review 의 "concave → 비평면 측벽 → NURBS" 는 **틀림**. 증명:
- per-edge 수직 inward offset 은 각 변 i 의 offset-line 을 원래 변과 **평행**하게 이동.
- top 정점 wᵢ, wᵢ₊₁ 은 모두 offset-line-i 위 → top edge ∥ bottom edge (둘 다 d̂ᵢ 방향).
- 한 쌍의 평행변을 가진 사각형 = 사다리꼴 = **항상 평면** (span{d̂ᵢ, A→D} 안에 4점).
- ∴ **convex/concave 무관 측벽은 정확한 Plane** (`synthesize_plane_surface` Newell exact,
  best-fit 아님). NURBS/BezierPatch 측벽 불필요. exact-Plane 불변 유지.

concave 의 유일한 위험 = **reflex 정점에서 offset 자기교차 / 토폴로지 분할**.

## 3. Decisions (lock)

| # | 결정 | 근거 |
|---|---|---|
| D1 | **concave 지원**, fail-closed (자기교차→거부) | 사용자 결재; Vatti 클리핑은 future |
| D2 | **(Plane, AllLinear) only** v1. (Plane, AllCircular) cone-taper = 로드맵 #2 별도 | 2D polygon offset 이 핵심 산출물 |
| D3 | **PushPullTool 확장** — VCB `dist,angle`, taperDeg=0 → 기존 동작 그대로 | additive, muscle-memory (ADR-046 P31 #1/#4) |
| D4 | **신규 `CreateSolidMode::ExtrudeTapered{distance, taper_deg}`** variant | serde-safe (기존 Extrude byte-shape 불변, snapshot 호환) |
| D5 | **실패 = hard-error + rollback** (push_pull fallback 금지) | taper 무 fallback; silent 직선 extrude 차단 (review HIGH) |
| D6 | **draft 각 θ, 양방향(±), \|θ\|<89°** (+=inward 수축, −=outward flare) | CAD 표준 d=\|dist\|·tan θ |

## 4. Fail-closed 면깨짐 방지 전략 (canonical)

"깨진 offset → 깨진 solid" 를 만들지 않는다. offset 이 다음 중 하나라도 위반하면
**top 합성 전에 거부 → SolidError → transaction rollback → Toast** (scene byte-identical):
1. 자기교차 (reflex 에서 offset edge 쌍 교차) — `seg_intersect` 비인접 쌍 스캔
2. 축퇴/collapse (offset 면적 ≈ 0)
3. inversion (offset 면적 부호 flip)
4. spike (reflex/sharp 정점 miter 길이 > limit)
5. 토폴로지 분할 (offset 이 multi-loop 화 — 자기교차로 포착됨)

⇒ 사용자는 *유효한* concave taper 는 사용 가능, *깨질* taper 는 거부됨 (직선 extrude 로
조용히 변질되지 않음, D5).

## 5. Supporting features (같이 구현 — 사용자 directive)

| 기능 | 재활용/신규 | 위치 | 면깨짐 방지 역할 |
|---|---|---|---|
| `offset_polygon_2d(verts, d)` (convex+concave, per-edge) | 신규 | geom2.rs | top profile |
| `line_line_intersect_2d(p0,d0,p1,d1)` (unbounded) | 신규 소형 | geom2.rs | reflex corner 정점 (seg_intersect 는 [0,1] clamp 라 불가) |
| self-intersection 스캔 | **재활용** `seg_intersect`(139) | geom2.rs | ★ 자기교차 거부 |
| collapse/inversion 가드 | **재활용** `polygon_signed_area`(199) + `orient2d_sign`(74) | geom2.rs | 축퇴/flip 거부 |
| spike/miter-limit 가드 | 신규 소형 | geom2.rs | sharp reflex 스파이크 거부 |
| `extrude_planar_box_tapered` | 신규 (extrude_planar_box 구조 미러) | create_solid.rs | frustum 구성 |
| `add_vertex_force_new` (top verts) | **재활용** | mesh.rs | dedup(0.15μm) 병합 → 비매니폴드 방지 |
| ADR-102 cleave 전치 | **재활용** (unchanged) | create_solid.rs | 기존 coplanar 면 격리(미변경) |
| transaction snapshot rollback | **재활용** | scene.rs exec_create_solid | 실패 시 byte-identical |
| `verify_face_invariants` 게이트 | **재활용** | mesh.rs | 매 성공 후 manifold 검증 |
| D5 fallback gate (`is_taper`) | 신규 소형 | scene.rs exec_create_solid | silent 직선 extrude 차단 |

## 6. Clean-faces 보장 (3 mechanism + fail-closed)

- **(A) 기존 면 미변경**: additive-only + ADR-102 cleave (source 면만 처리, sibling
  vert/HE/surface/curve 불변, L-102-1). top = `add_vertex_force_new` 신규 vert,
  side = `add_face` 신규. 기존 면에 쓰는 코드 경로 0.
- **(B) 출력 manifold**: frustum 토폴로지(bottom profile + top + N 평면 사다리꼴
  side), 모든 boundary edge 정확히 2면 공유, add_face HE double-claim 거부.
  top verts FRESH → cap double-claim 불가.
- **(C) clean-on-failure**: §4 가드가 mutation 전 거부 → rollback → byte-identical.

## 7. Sub-step plan (Path Z atomic)

| sub-step | layer | 내용 |
|---|---|---|
| **W-1-α (본 spec)** | docs | ADR + 결재 반영 + supporting features + planarity 증명. 코드 0. |
| β-1 | axia-geo | `offset_polygon_2d`(concave) + `line_line_intersect_2d` + self-intersection 스캔 + spike/collapse/inversion 가드 (+8 unit) → `ExtrudeTapered` variant + dispatch(ADR-102 cleave 재사용) + `extrude_planar_box_tapered`(force_new top, 평면 trapezoid side, ADR-183 flip) + exec_create_solid `is_taper` D5 gate (+회귀, verify_face_invariants 단언) |
| β-2 | axia-wasm + bridge | `create_solid_extrude_tapered` export(PushPullDone fallback 없음) + `createSolidExtrudeTapered` bridge + step6 additive guard + vitest |
| β-3 | web/tools | PushPullTool `taperDeg` VCB `dist,angle` (0=기존 경로) + ActionCatalog(AC⊇CC, dist rebuild) |
| γ | web/e2e | real Chromium: rect/concave-L 프로파일 → taper → N+2 면 + invariants valid + **별도 coplanar sibling byte-unchanged**(면깨짐 회귀) + 자기교차 taper → 거부+scene 불변 |
| δ | docs | Status→Accepted + Acceptance Log + Lessons + LOCKED entry |

## 8. Test plan (절대 #[ignore] 금지)

- geom2 unit: offset_square_inward / outward / triangle / pentagon / **concave_L_inward_valid** /
  **concave_offset_self_intersect_returns_none** / collapse_none / inversion_none /
  parallel_edge_fallback / spike_reflex_rejected / degenerate_none
- engine: `adr259_taper_box_frustum_manifold` (verify_face_invariants valid) /
  `adr259_taper_concave_L_valid_manifold` / `adr259_taper_self_intersect_rejects_rollback`
  (scene byte-identical) / `adr259_taper_steep_near_89_guarded` /
  `adr259_taper_side_faces_exact_planes` (concave 포함, Newell exact) /
  `adr259_taper_coplanar_sibling_untouched` (★ 면깨짐 회귀) /
  `adr259_taper_force_new_distinct_top_verts` / `adr259_taper_hard_error_no_fallback` (D5) /
  `adr259_straight_extrude_unchanged` (taperDeg=0 == extrude_planar_box)
- wasm/bridge/tool/E2E: §7 대로

## 9. Out of scope (future ADR)

- 자기교차 offset **클리핑**(Vatti/Clipper, multi-loop 분할) — fail-closed 거부 대신 분할
- (Plane, AllCircular) cone-taper — 로드맵 #2 (원→콘)
- Live drag taper (ADR-193 session 은 직선 only; taper 는 VCB commit-only v1)
- 비평면(곡면 surface) 프로파일 taper

## 10. Lock-ins

- L-259-1 측벽 = 정확한 Plane (평면 사다리꼴, convex+concave) — 회귀 `..._side_faces_exact_planes`
- L-259-2 fail-closed (자기교차/collapse/inversion/spike → 거부+rollback, 깨진 solid 0)
- L-259-3 `add_vertex_force_new` top verts (add_vertex 금지 — dedup 병합 방지)
- L-259-4 D5 `is_taper` gate (silent 직선 fallback 차단)
- L-259-5 ADR-102 cleave + transaction rollback 재사용 (기존 면 byte-unchanged)
- L-259-6 additive (ExtrudeTapered variant, 기존 API/snapshot 불변, ADR-046 P31 #4)
- L-259-7 절대 #[ignore] 금지

## 11. Cross-link

- ADR-079 (create_solid surface-native) / ADR-102 (cleave 격리) / ADR-183 (winding flip) /
  ADR-007 (manifold invariant) / ADR-058·187 (robust predicates orient2d) / ADR-016 Q2
  (multi-loop offset 거부 — 본 ADR 은 multi-loop 프로파일 아닌 single-loop concave) /
  ADR-046 P31 (additive, muscle-memory) / ADR-193 (Live — taper 미적용 v1) /
  [[feedback_extrude_sim_baseline]] / [[feedback_aixxia_extrude_compare]]
- 메타-원칙 #5 (사용자 편의) / #6 (Preventive — fail-closed) / #14 (면은 닫힌 경계로부터)
