# ADR-192 — Push/Pull Phase 1 잔존: Mixed Native Lock-in + Closed-Bezier Analytic Sweep (MVP)

> ADR-190 로드맵 Phase 1 의 잔존 두 항목.
> **P1.1** — mixed (Arc + Line) 평면 boundary 의 native push 는 *이미 작동* →
> 회귀 봉인 (lock-in only).
> **P1.3(b)** — closed **non-Circle** 곡선 disk (Bezier self-loop) → 진정한
> analytic GeneralSweep (side = swept **BSplineSurface**) MVP.
> **P1.3 §5.5 (2026-06-10 확장)** — **BSpline** profile 도 동일 경로 (native
> knots/degree passthrough, clamped + periodic).
> **P1.3 §5.6 (2026-06-10 확장)** — **NURBS** (rational) profile → rational 압출
> (`extrusion_surface_nurbs`, weights v-복제) → side = **NURBSSurface**. 닫힌
> 곡선 sweep family (Bezier/BSpline/NURBS) 완성.

- **Status**: Accepted
- **Date**: 2026-06-09
- **Track**: 6 (boundary kernel / 유도면) + W (ADR-079 create_solid)
- **Builds on**: ADR-190 (roadmap), ADR-191 (P1.2 ring), ADR-089 (closed-curve
  face), ADR-094 (Cylinder Path B kernel-native — *직접 mirror*), ADR-079
  (create_solid W track)

---

## Canonical anchor (사용자 결재, 2026-06-09)

> "Phase 1 잔존(P1.1 mixed / P1.3 closed-curve)" → "먼저 시뮬레이션 검토해줘"
> (2회 redirect) → 결재 **P1.1 = (a) lock-in only (회귀만)** + **P1.3 = (b)
> analytic GeneralSweep** → **MVP atomic 진행**.

P1.3(b) 채택 근거: tessellation 폴리곤화 대신 *진정한 analytic 곡면* 보존
(메타-원칙 #14 — 면은 닫힌 경계로부터, 곡선 metadata 보존). Circle 의 Path B
(ADR-094) 가 side 를 Cylinder 로 부여하는 것의 자연 일반화 — Bezier profile →
side 를 swept BSplineSurface 로.

---

## 1. P1.1 — Mixed (Arc + Line) 평면 native push (lock-in only)

### 시뮬레이션 (실측)
반원 disk (Arc rim + 지름 Line, mixed boundary) push:

| 시나리오 | 결과 | 결론 |
|---|---|---|
| mixed 반원 disk push | manifold solid + **Cylinder side walls** | *이미 작동* (ADR-079 dispatch + ADR-109 π-β Arc→Cylinder promote) |

mixed boundary 의 native push 는 ADR-079 create_solid 의 mixed 경로 +
fallback (ADR-109 π-β 가 Arc side face 를 Cylinder 로 promote) 가 이미 처리.
**신규 코드 0** — 회귀 봉인만.

### Lock-in
- 회귀 `adr192_p11_mixed_arc_halfdisk_push_manifold_cylinder_walls` (axia-core):
  반원 disk push → manifold + side 가 Cylinder surface.

---

## 2. P1.3(b) — Closed-Bezier Analytic GeneralSweep (MVP)

### 2.1 문제
closed Bezier disk (ADR-089 A-ω: 1 anchor + 1 self-loop edge with
`AnalyticCurve::Bezier`) 를 push 하면 **하드 실패** ("Face needs at least 3
verts") — push_pull 이 1-vertex self-loop boundary 를 다각형으로 다룰 수 없음.
ADR-094 Cylinder Path B 는 이 케이스를 Circle 에 한해 해결했으나 (side =
Cylinder), 비-Circle 곡선은 미지원.

### 2.2 Solution — `extrude_closed_curve_general_kernel_native`
`crates/axia-geo/src/operations/create_solid.rs` 신규 함수. ADR-094
`extrude_cylinder_kernel_native` 를 **1:1 mirror** — boundary-HE 위치 +
side-face DCEL wiring 은 *curve-agnostic*. 차이는 **top 곡선** (translated
Bezier) + **side surface** 뿐:

1. profile = Plane-surfaced Bezier self-loop 검증 (≥3 control points).
2. profile 을 `normal · dist` 만큼 translate → top Bezier 생성
   (`add_face_closed_curve`, ADR-089 A-ω).
3. boundary HE 위치 (each self-loop 의 `next_rad()` twin).
4. side face DCEL hand-wire — bottom self-loop = outer, top self-loop =
   inner (legacy "ring with hole" schema + `set_face_boundary_loops`).
5. side surface = `surfaces::sweep::extrusion_surface(bezier_profile,
   knots, degree, normal, dist)` → degree-1-in-v **`BSplineSurface`**
   (faceSurfaceKind 7, render-supported, ADR-038 P23 tessellation).
6. owner_id 부여 (ADR-093 D-δ 답습).

결과 = **3 faces** (base Plane + top Plane + side BSplineSurface),
`SolidKind::GeneralSweep`.

### 2.3 Dispatch (create_solid)
`create_solid` 의 surface 계산 직후, `classify_boundary` 전에:
profile 이 **single-loop** + **Plane surface** + outer edge 가 **Bezier
self-loop** → `extrude_closed_curve_general_kernel_native` 라우팅.

- **Plane guard** — side BSplineSurface (역시 Bezier self-loop bounded) 가
  재-push 시 본 경로로 되돌아오는 것을 차단 (finding #16).
- **single-loop guard** — multi-loop Bezier annulus 는 scene-level P1.2
  (ADR-191) 가 이미 가로채므로 방어적 (finding #10).

---

## 3. 적대적 검토 (Adversarial review, 32-agent workflow)

P1.3(b) MVP commit (`80f73e8`) 후 적대적 검토 워크플로우 실행 — 18 confirmed /
9 refuted. Triage:

### 3.1 Actionable (본 ADR 에서 수정)
| # | 발견 | 수정 |
|---|---|---|
| #9 | Bezier `< 2` control points 통과 → 후속 실패 | `< 3` (add_face_closed_curve 의 `bezier_best_fit_normal` 요구) |
| #16 | BSplineSurface side 재-push 시 본 경로 mis-route | dispatch **Plane guard** |
| #10 | multi-loop (holes) 미처리 | dispatch **single-loop guard** (P1.2 가 이미 가로챔 — 방어적) |
| #18 | 한계 문서화 누락 | 함수 doc 의 "Limitations" 절 + 본 §4 |

### 3.2 Shared with Cylinder Path B (parity — 본 MVP 회귀 아님, follow-up)
본 함수가 ADR-094 Cylinder Path B 를 mirror 하므로, 다음은 *공유 latent
parity* (Cylinder Path B 도 동일) — 본 MVP 가 **새로 도입한 버그 아님**.
manifold 검증 통과 + post-extrude 에 cleanup 미실행이라 무해:
- **he_twin self-loop** — self-loop boundary HE 의 twin 은 자기 자신. side
  DCEL 은 radial chain 의존. `< 1000` guard 로 무한루프 없음.
- **Boolean 비호환** — side 가 legacy outer/inner (`add_inner`) schema →
  결과에 Boolean 시 `boolean.rs` 가 non-empty `inners()` 거부.
- **`analytic_face_area` = 0** — BSplineSurface side 의 면적 0. tessellation
  fallback 은 follow-up. `cleanup_degenerate_faces` 가 post-extrude 미실행이라
  무해.

### 3.3 Refuted (내 판단 + verify 단계)
- closed Bezier degree `len-1` — *정상* (N+1 control points = degree N; 닫힘
  중복 endpoint 는 closed Bezier 표현 본질). render 정상.
- inner-loops 미처리 (#10 의 over-claim) — scene-level P1.2 가 multi-loop 을
  먼저 가로챔 (방어 guard 추가).
- 음수 거리 / snapshot / owner-id / undo / render tessellation — 모두
  verify 단계 + 회귀로 반박 (음수 거리 = manifold valid, 아래 §4.3).

---

## 4. Lock-ins

- **L-192-1** P1.1 mixed native = 회귀 봉인만 (신규 코드 0). ADR-079 dispatch
  + ADR-109 π-β Arc→Cylinder promote 가 이미 처리.
- **L-192-2** P1.3(b) closed Bezier = analytic GeneralSweep (side =
  BSplineSurface). 메타-원칙 #14 (곡선 metadata 보존), tessellation 폴리곤화 아님.
- **L-192-3** Dispatch는 **single-loop + Plane-surface + Bezier self-loop**
  3-gate. BSplineSurface side 재-push 미라우팅 (#16) + multi-loop 방어 (#10).
- **L-192-4** Bezier profile **≥ 3 control points** 강제 (#9).
- **L-192-5** ADR-094 Cylinder Path B **1:1 mirror** — side surface (Cylinder
  → BSplineSurface) + top 곡선 (Circle → Bezier) 만 차이. §3.2 latent parity 는
  Cylinder Path B 와 공유 (본 MVP 회귀 아님, follow-up).
- **L-192-6** 음수 거리 = manifold valid (회귀 봉인). side render orientation
  parity 는 follow-up.
- **L-192-7** ADR-046 P31 #4 additive (createSolidExtrude signature 무변경).
- **L-192-8** 절대 #[ignore] 금지.
- **L-192-9 (§5.5)** BSpline profile = **native knots/degree passthrough**
  (clamped Type A + periodic Type B 모두) — top rim 도 native BSpline 보존.
  Bezier 경로는 clamped knot 합성으로 byte-동일 (회귀 0).
- **L-192-10 (§5.6)** NURBS profile = **rational** sweep — per-control-point
  weights 가 `extrusion_surface_nurbs` 를 거쳐 v 방향으로 복제 → side =
  `NURBSSurface` (rational in u, linear non-rational in v). top rim 도
  native-weight NURBS self-loop 보존. degree-1-in-v 에서 `w_i0 = w_i1` →
  `N₀(v)+N₁(v)=1` 로 v-weighting 상쇄 → 단면 형상 무왜곡. Bezier/BSpline 경로
  byte-동일 (회귀 0). 닫힌 곡선 sweep family (Bezier/BSpline/NURBS) **완성**.

---

## 5. Acceptance Log

### 5.1 시뮬레이션 + 결재 (2026-06-09)
- P1.1 시뮬 — mixed 반원 push 이미 작동 (Cylinder walls). P1.3 시뮬 — closed
  Bezier push 하드 실패 확인.
- 결재: P1.1 (a) lock-in + P1.3 (b) analytic GeneralSweep + MVP atomic.

### 5.2 MVP 구현 — commit `80f73e8` (LOCAL, adr-186/boundary-kernel-port)
- `create_solid.rs` — `extrude_closed_curve_general_kernel_native` + dispatch.
- 회귀 `adr192_p13b_closed_bezier_disk_extrudes_to_bspline_sweep` (3-face
  manifold, 1 BSplineSurface side) + `adr192_p11_mixed_arc_halfdisk_push_
  manifold_cylinder_walls`.

### 5.3 적대적 검토 follow-up — 본 commit (LOCAL)
- 적대적 검토 (32-agent workflow): 18 confirmed / 9 refuted → §3 triage.
- 수정 (FIX): #9 (≥3 control points) + #16 (Plane guard) + #10 (single-loop
  guard) + #18 (doc).
- 회귀 +2: `adr192_p13b_negative_distance_manifold_valid` (음수 거리 manifold) +
  `adr192_p13b_side_face_repush_does_not_corrupt` (재-push 무손상).
- 워크스페이스: axia-geo **1694 PASS** / axia-core **344 PASS** — 0 failed,
  0 ignored.

### 5.4 브라우저 검증 (clean scene, ADR-087 K-ζ)
| 시나리오 | 결과 |
|---|---|
| closed Bezier disk push (dist +120) | 3면 (base + top + BSplineSurface side), manifold valid ✅ |
| closed Bezier disk push (dist −120, inset) | 3면, manifold valid 0 violations ✅ |
| BSplineSurface side 재-push | 무손상 (crash 0) ✅ |

### 5.5 P1.3 BSpline 확장 — 본 commit (LOCAL, 사용자 결재 "BSpline 먼저, NURBS 별도")

**사전검토 + 시뮬레이션 (2026-06-10)**: closed BSpline / NURBS disk push 둘 다
pre-MVP Bezier 와 *동일하게* 하드 실패 ("Face needs at least 3 verts") 실측 —
dispatch 가 `Bezier` 만 매치하는 단일 gap. BSpline 은 기존 `extrusion_surface`
가 native knots/degree 를 그대로 수용 → **거의 무료** 확장. NURBS 는 rational
압출 (`extrusion_surface_nurbs`) 신규 필요 → **별도 ADR 보류**.

**구현 — generalize (신규 함수 0)**:
- `extrude_closed_curve_general_kernel_native` 의 곡선 추출을 `SweptProfile`
  enum (Bezier | BSpline) 으로 generalize — Bezier 는 clamped knot 합성
  (기존과 byte-동일), BSpline 은 **native knots/degree passthrough**.
- top 곡선도 profile kind 보존 (BSpline top = native knots/degree clone).
- dispatch 3-gate 의 곡선 매치 확장: `Bezier | BSpline` self-loop.
  NURBS 는 의도적 미라우팅 (graceful Error 봉인).
- 추가 검증: `prof_ctrl.len() >= degree + 1`.

**적대적 검토 2차 (12-agent workflow): 5 confirmed / 1 refuted — 전부 minor,
행위 버그 0**:
- 검토 agent 가 periodic knots / 재-push / undo / snapshot 을 *직접 probe* 하여
  정상 동작 실증 (periodic profile end-to-end SolidCreated + manifold).
- FIX (모두 적용): stale Bezier-only doc 2곳 + bail prefix 통일 ("P1.3(b):" →
  "P1.3:") / top rim 곡선 봉인 assertion (silent Bezier-top regression 차단) /
  **periodic (Type B, A-Δ) 봉인 테스트 신규** (non-clamped uniform knots,
  본 diff 가 새로 라우팅하는 input class).
- Refuted: step-3 orphan vertex (pre-existing shared, P0.2 snapshot restore 가
  완전 mitigate — Bezier MVP 와 동일).

**회귀 +3** (axia-core 344 → 347):
- `adr192_p13c_closed_bspline_disk_extrudes_to_bspline_sweep` — 3면 manifold +
  side **native knots** (interior 0.5) + base/top rim 모두 native BSpline 봉인
- `adr192_p13c_periodic_bspline_disk_extrudes` — Type B periodic (uniform
  knots 0..=7, deg 2) → 3면 manifold + side native periodic knots
- `adr192_p13c_closed_nurbs_still_unrouted_graceful` — NURBS push = graceful
  Error + mesh 무손상 (deferred 봉인)

**워크스페이스**: **2330 PASS / 0 failed** (axia-geo 1694 / axia-core 347 /
foreign 138 / 그 외; doctest 1 ignored = plane_snap doc-fence, 정책 무관).

**브라우저 검증 (rebuilt WASM, ADR-087 K-ζ)**:
| 시나리오 | 결과 |
|---|---|
| closed BSpline disk push (+120) | 3면 (Plane+Plane+BSplineSurface), manifold 0 violations ✅ |
| closed NURBS disk push | graceful Error (crash 0, mesh 무손상) ✅ |
| Bezier 회귀 (동일 scene) | 3면 manifold ✅ |

### 5.6 P1.3 NURBS 확장 — 본 commit (LOCAL, 사용자 결재 "① P1.3 NURBS profile")

**사전검토 (2026-06-10, 4-트랙 워크플로우 rate-limit → 직접 audit)**: dispatch
gate (`create_solid.rs` ~219) 가 `Bezier | BSpline` 만 매치 → NURBS self-loop 은
fall-through → `(Plane, Mixed) NotYetSupported` → P0.2 fallback push_pull
(tessellation, analytic 손실). `NURBSSurface` variant + weights 필드 +
`add_face_closed_curve` NURBS arm (A-Β) 모두 이미 존재 → BSpline arm **1:1 미러
+ rational weights** 만 추가.

**구현 — generalize (BSpline arm 미러)**:
- `surfaces::sweep::extrusion_surface_nurbs` 신규 — `extrusion_surface` 의
  rational 버전. profile weights 를 v∈{0,1} 로 복제 → `(ctrl_grid, weights_grid,
  knots_u, knots_v, deg_u, deg_v)`.
- `SweptProfile` enum 에 `Nurbs` arm — `AnalyticCurve::NURBS { control_pts,
  weights, knots, degree }` 매치, weights 를 `Option<Vec<f64>>` 로 carry.
- top 곡선 = NURBS (native weights/knots/degree clone) → 같은 rational 형상 translate.
- side surface = `NURBSSurface { ctrl_grid, weights, knots_u/v, deg_u/v,
  trim_loops: [] }`.
- dispatch gate 곡선 매치 확장: `Bezier | BSpline | NURBS` self-loop.

**적대적 검토 (5-lens workflow → 1 verdict + 4 inline, server rate-limit)**:
- top-rim-weights (workflow): **holdsUp=true** — 회귀 test 가 weight-losing
  refactor 차단 충분. minor gap (top edge knots 미봉인) → rim assertion 에
  `knots` 추가로 close (본 commit).
- weight-replication (inline): `w_i0=w_i1` + degree-1-in-v → `N₀(v)+N₁(v)=1` →
  v-weighting 상쇄 → `S(u,0)`=rational profile, `S(u,1)`=translate, 무왜곡
  선형 보간.
- re-push-safety (inline): Plane guard 가 NURBSSurface side 재라우팅 차단 →
  `extrude_nurbs_class_profile` (BSpline side 와 동일, §5.3 repush 봉인).
- validation (inline): weights len mismatch → `extrusion_surface_nurbs` bail
  (unit test); zero/neg weights → `add_face_closed_curve` 가 disk 생성 시 거부;
  `prof_weights` None when `Nurbs` 불가 (match arm 이 `Some` 보장).
- snapshot/area (inline): NURBSSurface 는 기존 직렬화 variant; `active==3`
  회귀가 area-0 deactivation 부재 증명.

**회귀 +4** (axia-geo +2 sweep unit / axia-core +2 scene; deferred test rewrite):
- `extrusion_nurbs_replicates_weights_and_grid` /
  `extrusion_nurbs_rejects_weight_len_mismatch` (sweep.rs unit)
- `adr192_p13d_closed_nurbs_disk_extrudes_to_nurbs_sweep` — 3면 manifold +
  side NURBSSurface (native knots + replicated weights) + base/top rim 모두
  native-weight NURBS self-loop (**weights + knots** 봉인)
- `adr192_p13d_closed_nurbs_negative_distance_manifold_valid`
- `adr192_p13c_closed_nurbs_still_unrouted_graceful` → **rewrite** to the
  success test (deferral closed by 사용자 결재)

**워크스페이스**: axia-geo **1696 PASS** / axia-core **352 PASS** / transaction
**5** — 0 failed, 0 ignored. TS 변경 0 (bridge `drawClosedNURBSAsCurve` +
`createSolidExtrude` 기존).

**브라우저 검증 (rebuilt WASM, ADR-087 K-ζ)**:
| 시나리오 | 결과 |
|---|---|
| closed NURBS disk push (+120) | 3면 {Plane, Plane, **NURBSSurface (kind 8)**}, manifold 0 violations ✅ |
| NURBSSurface side tessellation | 530 tris, error 0 (render path OK) ✅ |
| 음수 거리 / Bezier·BSpline 회귀 | manifold valid (회귀 0) ✅ |

---

## 6. Out of scope (Phase 1 잔존 / follow-up)

- ~~P1.3 BSpline profile~~ — ✅ **§5.5 에서 closure** (2026-06-10, native
  knots/degree passthrough + periodic Type B 봉인).
- ~~P1.3 NURBS profile~~ — ✅ **§5.6 에서 closure** (2026-06-10, rational
  `extrusion_surface_nurbs` + `NURBSSurface` side, weights v-복제). 닫힌 곡선
  sweep family (Bezier/BSpline/NURBS) 완성.
- **§3.2 shared Cylinder Path B latent parity** (he_twin self-loop / Boolean
  `inners()` / `analytic_face_area`=0) — NURBSSurface side 도 동일 상속.
  Cylinder Path B 와 *함께* 고치는 별도 ADR.
- **§3.2 shared Cylinder Path B latent parity** (he_twin self-loop / Boolean
  `inners()` 비호환 / `analytic_face_area`=0) — *두 경로 동시* fix 하는 별도
  ADR (Cylinder + GeneralSweep 공통).
- **음수 거리 side render orientation** — manifold 정상, render 법선 parity 만.
- **P1.4+** advanced sweep (path-following / loft) — ADR-190 Phase 4.

---

## 7. Cross-link

- **ADR-190** Push/Pull roadmap (Phase 1 모체) + **LOCKED #78**
- **ADR-191** P1.2 ring face push (직전 Phase 1 sub-step) + **LOCKED #79**
- **ADR-094** Cylinder Path B kernel-native (**1:1 mirror source**) + **LOCKED #47**
- **ADR-089** closed-curve face (Bezier self-loop, A-ω) + **LOCKED #35**
- **ADR-079** create_solid W track (surface-native dispatch) + Q3 fallback
- **ADR-109** π-β Arc→Cylinder side promote (P1.1 mixed 경로)
- **ADR-038** P23 surface-aware tessellation (BSplineSurface render)
- **ADR-093** D-δ surface_owner_id (side owner_id 답습)
- **ADR-087** K-ζ 사용자 시연 게이트 / **메타-원칙 #4/#5/#6/#14**
- commits `80f73e8` (MVP) + 본 follow-up commit
