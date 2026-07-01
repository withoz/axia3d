# ADR-260 — Circle → Cone / Frustum Extrude (원뿔·절두체 돌출, AnalyticSurface::Cone 재활용)

- **Status**: Accepted (α~γ closure 2026-06-26 — engine + WASM + bridge + tool + 라이브 검증)
- **Date**: 2026-06-26
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL, push 금지)
- **사용자 결재 (2026-06-26)**: "#2 원→콘 extrude(Cone surface 재활용, 최저 위험)으로 진행합니다"
  - **Q1 = apex + frustum 둘 다** (top_scale 한 파라미터로 전체)
  - **Q2 = top_scale 비율 [0,1)** (half-angle 아님 — 원의 자연 파라미터, AixiAcad 답습)
  - **Q3 = full kernel-native** (apex 2면 + **frustum 3면 self-loop helper 신규** — minimal-DCEL 전체, 산업 CAD parity)
- **Cross-link**: ADR-259 (#1 taper — 직전 자매 ADR, dispatch/D5/fail-closed 패턴 source) ·
  ADR-104 / ADR-114 (`create_cone_kernel_native` — Cone surface 재활용 source) ·
  ADR-094 (`extrude_cylinder_kernel_native` — frustum annulus 미러 source) ·
  ADR-089 (closed-curve self-loop face canonical) · ADR-079 (create_solid W-track) ·
  ADR-031 Phase D (AnalyticSurface::Cone) · ADR-102 (cleave) · ADR-087 K-ε (sandwich, is_move_only) ·
  ADR-183 (outward base cap) · ADR-038 P23 (surface-aware normal) · ADR-093 D-β (owner-id grouping) ·
  메타-원칙 #4/#5/#6/#14 · LOCKED #43 (Z-up) · LOCKED #44 (Complete Meaning per Merge)

---

## 1. 문제

[[feedback_extrude_sim_baseline]] 의 확정 GAP #3: **원(circle) profile 을 cone /
frustum 으로 extrude 하는 경로 부재.** `top_scale` API 가 0 — 현재 원 profile 의
push/pull 은 항상 직선 cylinder (Path B kernel-native, 3 face) 만 생성.

AixiAcad `extrude_circle_to_cone(face, dist, top_scale)` 와 등가 기능이 필요.
**그러나** AnalyticSurface::Cone (faceSurfaceKind 4) 가 이미 존재하고
(`create_cone_kernel_native`, ADR-114), `extrude_cylinder_kernel_native`
(ADR-094) 가 self-loop annulus side 구조를 이미 검증했으므로 — **신규 surface
타입 0, 신규 알고리즘 최소.** "최저 위험" 의 실체는 *기존 검증 자산 2개의 미러*.

## 2. 핵심 기하 (사용자 #1 요구 "면 안 깨짐" 정합)

원 profile (Plane surface, AllCircular boundary) + `top_scale ∈ [0,1)`:

- **apex (top_scale = 0)**: `create_cone_kernel_native` 구조 — base disk(profile,
  보존) + cone side(1 self-loop face, apex degenerate v=0). **2면.** apex vertex
  를 DCEL 에 추가하지 않음 (Cone surface 의 v=0 degenerate point 가 apex).
- **frustum (0 < top_scale < 1)**: `extrude_cylinder_kernel_native` 미러 —
  base disk(profile) + top disk(축소 원, radius·s) + annulus cone side(2 self-loop
  multi-loop face: bottom outer + top inner). **3면.** side surface 만 Cylinder →
  Cone 으로 교체.

### 2.1 Cone surface 파라미터 도출 (cone.rs 규약 확정)

`cone.rs`: `P(u,v) = apex + v·axis_dir + v·tan(α)·radial`, v=0 = apex,
outward normal = `cos(α)·radial − sin(α)·axis_dir` → **axis_dir 는 반드시
apex→base 방향** (그래야 outward).

원 profile: center, radius R, normal n, basis_u. 거리 dist, top_scale s.

- **virtual apex** = `center + n·(dist/(1−s))` (전체 원뿔의 가상 꼭짓점, top 너머)
- **axis_dir** = `(center − apex).normalize()` = `−sign(dist)·n` (apex→base)
- **half_angle** = `atan(R·(1−s)/|dist|)`
  (검증: radius(base_v) = base_v·tan(α) = (|dist|/(1−s))·(R(1−s)/|dist|) = R ✓;
  radius(top_v) = top_v·tan(α) = R·s ✓)
- **v_range** = `(top_v, base_v)` = `(|dist|·s/(1−s), |dist|/(1−s))` (top_v < base_v)
- **apex (s=0)**: apex = `center + n·dist`, half_angle = `atan(R/|dist|)`,
  v_range = `(0, |dist|)` — `create_cone_kernel_native` 와 동일.

`ref_dir` = profile basis_u, `u_range` = (0, 2π).

## 3. 설계 — full kernel-native (Q3)

### 3.1 신규 `CreateSolidMode::ExtrudeCone { distance: f64, top_scale: f64 }`

ADR-259 `ExtrudeTapered` 직후. additive (기존 Extrude byte-shape 불변, serde-safe).
신규 `SolidKind::Cone`.

### 3.2 Dispatch arm (ExtrudeTapered:301 미러)

1. `distance.abs() < EPSILON_LENGTH` → `DegenerateDistance` (rollback).
2. `top_scale < 0` 또는 비유한 → `NotYetSupported` (reject).
3. `top_scale ≥ 1 − 1e-4` → `NotYetSupported` ("원통이면 직선 Extrude 사용", 수치
   blow-up 차단).
4. **`is_move_only(self, profile_face)` (ORIGINAL face, cleave 前)** → reject.
   ExtrudeCone 은 `fallback_dist = None` (scene) → MoveOnly/multi-loop/push_pull
   분기 구조적 skip → 이 가드가 ADR-087 K-ε sandwich 차단 SSOT.
5. ADR-102 γ cleave (coplanar siblings 격리, L-102-1 — 기존 면 무손상).
6. `match (Plane, AllCircular) → extrude_planar_cone`, else `NotYetSupported`.

### 3.3 Kernel `extrude_planar_cone(profile, dist, top_scale, mat, surface)`

- snap-to-apex: `top_scale·R < EPSILON_LENGTH` → `top_scale = 0` (sub-tolerance top
  cap = degenerate 방지).
- circle params 추출 (self-loop = edge curve; polygonal = `extract_shared_circle_params`).
- **boundary_verts.len() == 1 (self-loop, Path B production default):**
  - **apex (s=0)**: `create_cone_kernel_native` 미러 — profile 보존(base), twin HE
    (`next_rad(outer_start)`) → cone_side_face self-loop, Cone surface attach. 2면.
  - **frustum (0<s<1)**: `extrude_cylinder_kernel_native` 미러 — top vert +
    `add_face_closed_curve`(축소 원) + annulus_face(bot outer + top inner self-loop,
    `set_face_boundary_loops`) + Cone surface (Cylinder 대신). 3면.
- **boundary_verts.len() >= 3 (polygonal-arc circle, legacy/비-self-loop):**
  - **apex**: apex vertex + N triangle fan + ONE Cone surface.
  - **frustum**: 축소 top verts + N quad + top cap Plane + ONE Cone surface.
- 모든 cone side 에 단일 `owner_id` (ADR-093 D-β — 한 클릭 = 전체 cone 측면 선택).
- CreateSolidResult { SolidKind::Cone, … }.

### 3.4 D5 — fail-closed (ADR-259 답습)

`ExtrudeCone` 은 `fallback_dist = None` → 어떤 에러든 scene 하드에러 경로
(`restore_scene_snapshot` + cancel + Error) → **byte-identical rollback.** 직선
extrude 로의 silent fallback 절대 없음. cleave 후 reject 도 rollback 으로 무손상.

### 3.5 UI (β-3, ADR-259 VCB 답습)

VCB `pushpull`: `거리,비율` (쉼표) → `top_scale`. `거리,각도°` (taper, ADR-259) 와
별도 — but VCB 는 둘 다 쉼표라 모호. **해소**: taper 는 각도(°, |값|<89 정수/실수),
cone 은 비율(0~1). β-3 에서 파싱 규약 확정 (잠정: 두 번째 인자에 `%` 또는 별도
명령 — β-3 sub-step 에서 결정. v1 은 `commitCone` 직접 진입 우선).

## 4. Lock-ins (β 구현 시 강제)

- **L-260-1** AnalyticSurface::Cone 재활용 — 신규 surface 타입 0.
- **L-260-2** apex = `create_cone_kernel_native` 미러 (2면, v=0 degenerate, apex
  vertex DCEL 미추가).
- **L-260-3** frustum = `extrude_cylinder_kernel_native` 미러 (3면 annulus, side만
  Cone 으로 교체) — **full kernel-native (Q3), self-loop 3면 helper 신규.**
- **L-260-4** axis_dir = `−sign(dist)·n` (apex→base, cone.rs outward 규약).
- **L-260-5** top_scale ∈ [0,1) — `≥1−1e-4` reject, `<0` reject, `s·R<EPS` snap→apex.
- **L-260-6** is_move_only 가드 (cleave 前, ORIGINAL face) — ADR-087 K-ε sandwich
  차단 SSOT (fallback_dist None).
- **L-260-7** D5 fail-closed — silent 직선 fallback 0, 하드에러 → byte-identical
  rollback (ADR-259 답습).
- **L-260-8** owner-id grouping (ADR-093 D-β) — cone 측면 단일 그룹.
- **L-260-9** ADR-102 cleave (기존 coplanar 면 무손상, 사용자 #1 요구).
- **L-260-10** additive — 기존 Extrude/ExtrudeTapered/Revolve/Sweep/Loft 불변
  (ADR-046 P31 #4). 절대 #[ignore] 금지.

## 5. 시뮬레이션 게이트 (β-1, 먼저 시뮬 — ADR-259 답습)

`adr260_sim_*` Rust prototype 테스트로 **구현 후 라이브 전** 검증:
- apex self-loop manifold valid (verify_face_invariants) + Cone surface + 2면.
- frustum self-loop manifold valid + 3면 + top radius = R·s + Cone surface.
- half_angle / apex / v_range 수식 정확 (radius(base_v)=R, radius(top_v)=R·s).
- dist<0 (아래 방향) manifold valid (axis_dir 부호).
- top_scale ≥1 reject / <0 reject / s·R<EPS snap→apex.
- is_move_only (solid face) reject + scene rollback byte-identical.
- polygonal fan/quad manifold (비-self-loop 원).
- base/top cap 방향 — create_cone_kernel_native primitive 와 비교 (flip 필요 여부
  시뮬이 판정).

## 6. Out of scope (별도 ADR / future)

- 비-circle closed curve (Bezier/BSpline/NURBS) → cone (ADR-192 GeneralSweep 와 직교).
- 곡면 host 위 cone extrude (planar profile only).
- top_scale > 1 (역 frustum / 확장) — 별도 검토.
- 양방향 cone (#3 bidir extrude track).
- VCB 모호성 (각도 vs 비율) 의 완전 UX — β-3 에서 결정.

## 7. Acceptance Log

- **2026-06-26 α** (`0aab484`) — 본 spec + 결재 (Q1 both / Q2 ratio / Q3 full
  kernel-native). 3-lens adversarial review: existing-face-corruption = **sound
  (위험 0)**, apex-frustum-manifold + cone-surface-winding = needs-revision
  (전부 구현 디테일 — axis_dir 부호 / top_scale 가드 / Path B frustum 3면 /
  is_move_only 순서, §4 Lock-ins + §5 시뮬 게이트로 흡수).
- **2026-06-26 β-1** (`312de15`) — `SolidKind::Cone` + `CreateSolidMode::ExtrudeCone`
  + dispatch arm + `extrude_planar_cone` 커널 (self-loop apex 2면 / self-loop
  frustum 3면 annulus / polygonal fan·quad) + **시뮬 게이트 12 테스트**. scene.rs
  코드 변경 0 (D5 자동 커버), scene 테스트 2개 추가. 회귀 axia-geo 2054→**2066**
  (+12), axia-core 410→**412** (+2). **시뮬레이션이 ADR-183 flip 불필요 확정**
  (no-flip 미러 = `verify_face_invariants().is_valid()`).
- **2026-06-26 β-2** (`cdbc89d`) — WASM `create_solid_extrude_cone` + step6
  additive +2 (export additive / no-PushPullDone) + bridge `createSolidExtrudeCone`
  (Toast) + WasmBridge vitest +4. D5: no PushPullDone fallback arm.
- **2026-06-26 β-3** (`07c0c1b`) — PushPullTool `commitCone` + VCB `거리,비율%`
  (`%` 접미사로 테이퍼/콘 모호성 해소) + vitest 28→**32** (+4). tsc 0 errors.
- **2026-06-26 γ** (라이브, 재빌드 WASM, preview_eval) — **frustum** (R500 dist800
  top40%) → 3면 (base/top Plane + annulus Cone kind4 inner1) `valid=true v=0` ·
  **apex** (top0%) → 2면 (base + Cone side) `valid=true v=0` · **top_scale≥1 거부**
  → false + **byte-identical** (face_count 6→6) + `valid=true` + profile 보존.
  console crash 0 (PR #101 "recursive use of object" 류 0).
- **2026-06-26 δ** (본 commit) — Status Proposed→Accepted + Acceptance Log +
  §8 Lessons + CLAUDE.md LOCKED #84 + memory 갱신.

## 8. Lessons (canonical)

- **L1 — 먼저 시뮬이 ADR-183 flip 논쟁 종결** — 두 review 가 "bottom-cap flip 필요"
  를 medium-risk 로 지적했으나, no-flip 미러 (`extrude_cylinder_kernel_native` /
  `create_cone_kernel_native` 답습) 의 sim 이 `verify_face_invariants().is_valid()`
  = true 로 **flip 불필요를 실증**. winding 논쟁은 추론 아닌 시뮬로 종결 (ADR-259
  containment-guard 발견과 동일 패턴 — 메타-원칙 #6 Preventive).
- **L2 — full kernel-native = 두 proven 함수의 미러** ("최저 위험"보다 정확한
  Q3 선택의 실체). apex = `create_cone_kernel_native` (2면, twin HE 재사용, apex
  degenerate v=0), frustum = `extrude_cylinder_kernel_native` (3면 annulus, side만
  Cylinder→Cone). 신규 알고리즘 0, 신규 surface 0 (AnalyticSurface::Cone 재활용).
- **L3 — D5 fail-closed 가 scene 코드 변경 0 으로 무료 enforced** — `ExtrudeCone`
  은 `fallback_dist = None` (scene `_ => None` arm) → MoveOnly/multi-loop/push_pull
  분기 구조적 skip + 하드에러 경로 `restore_scene_snapshot` (ADR-259 가 이미 추가)
  자동 적용. ADR-259 의 D5 인프라가 ADR-260 에 그대로 재사용 (자매 ADR 의 자산
  복리).
- **L4 — VCB 모호성 = 접미사로 해소** — taper(`거리,각도`) ↔ cone(`거리,비율%`)
  이 둘 다 쉼표라, `%` 접미사로 명확 분기 (bridge query 불필요, 직관적 "top N%").
  엔진이 프로파일 타입 불일치 시 fail-close (taper=AllLinear / cone=AllCircular).
- **L5 — cone.rs 규약 정독이 axis_dir 부호 CRITICAL 위험 해소** — `P=apex+v·axis+
  v·tanα·radial`, outward `=cosα·radial−sinα·axis` → axis_dir 는 반드시 apex→base.
  `axis_dir = −sign(dist)·n` + `v_range=(|dist|s/(1−s), |dist|/(1−s))` 도출,
  sim 이 dist<0 양쪽 부호 실증.
