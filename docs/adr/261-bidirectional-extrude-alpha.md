# ADR-261 — Bidirectional / Two-Sided Extrude (ExtrudeMode: OneWay / Symmetric / TwoSided)

- **Status**: Accepted (α~γ closure 2026-06-26 — engine + WASM + bridge + ExtrudeMode 토글 + 라이브 검증)
- **Date**: 2026-06-26
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL, push 금지)
- **Track**: 6 (Extrude/Cut/Punch) — "완벽한 extrude" 로드맵 **#3 (bidirectional)**
- **사용자 결재 (2026-06-26)**:
  - **Q1 = 사각 + 원 둘 다** (AllLinear box + AllCircular circle→cylinder)
  - **Q2 = translate + 기존 extrude 재사용** (ADR-060 translate가 곡선/surface 자동 갱신 → profile 보존 = bottom cap, ownership 무변경)
  - **Q3 = ExtrudeMode 토글 (AixiAcad parity)** — OneWay / Symmetric / TwoSided
- **De-risk**: `wf_06edc743-177` (workflow rate-limit → 인라인 audit). AixiAcad
  `extrude_planar_face_bidir` + `ExtrudeMode {OneWay/Symmetric(dp,dp)/TwoSided{dist_neg}}`
  parity 확인 (`D:/AixiAcad/engine/crates/xia-render/src/app.rs` + `xia-form/.../form.rs:7838`).
- **Cross-link**: ADR-259 (#1 taper) · ADR-260 (#2 cone — 직전 자매 ADR, dispatch/D5/
  fail-closed/sim-gate 패턴 source) · ADR-060 Phase O (translate_verts 곡선/surface
  갱신 — Q2 의 crux) · ADR-079 (create_solid W-track) · ADR-094 (extrude_cylinder_
  kernel_native) · ADR-102 (cleave) · ADR-087 K-ε (sandwich, is_move_only) · ADR-183
  (outward cap) · ADR-193 (live extrude — live bidirectional 후속) · 메타-원칙 #4/#5/#6 ·
  LOCKED #43 (Z-up) #44 (Complete Meaning per Merge) #84 (ADR-260)

---

## 1. 문제

[[feedback_extrude_sim_baseline]] GAP: **양방향(two-sided) extrude 부재.** 현재
`create_solid` Extrude{distance} 는 단방향 (profile = bottom cap, 솔리드는 +normal
한 방향). 사용자는 profile 평면 기준 **양쪽**으로 extrude 하고 싶음 — 대칭(±) 또는
비대칭(위 dist_pos / 아래 dist_neg).

AixiAcad parity 확인: `ExtrudeMode {OneWay / Symmetric / TwoSided{dist_neg}}` +
`extrude_planar_face_bidir(face, dist_pos, dist_neg)`. Symmetric => (dp, dp) (각 방향
dp), TwoSided => (dp, dist_neg). watertight 회귀 보유.

## 2. 핵심 기하 + 재사용 발견 (de-risk)

**중간 멤브레인 문제**: profile 을 그대로 두고 양쪽 extrude 하면 profile 이 내부
membrane → 비-manifold (edge 당 3 face-bearing HE). 해소 = profile 소비 또는 이동.

**우리 최적 (Q2, AixiAcad보다 더 Pattern-12)**: profile 을 `−normal·dist_neg` 이동
→ 기존 `extrude_planar_box`/`extrude_planar_cylinder` 로 `(dist_pos+dist_neg)` 돌출.
- **ADR-060 Phase O**: `translate_verts` 가 edge 양 끝점 모두 이동 시 Circle curve
  center translate + Plane surface origin transform 자동 (부분 이동만 `set_curve(None)`).
  profile 전체 boundary 이동 = full move → 곡선/surface 정확히 따라옴 (AllCircular crux 해소).
- profile **보존** (= bottom cap at −dist_neg, ADR-183 flip outward −N) → Shape/Xia
  ownership 무변경. top cap at +dist_pos (outward +N). 측벽 full-height.
- 결과 솔리드 `[−dist_neg, +dist_pos]` (AixiAcad build-fresh 와 동일 기하, 코드는 더 적음).

**Symmetric 의미 (AixiAcad parity)**: Symmetric(d) = `(dist_pos=d, dist_neg=d)` = 각
방향 d (총 두께 2d, profile 평면이 대칭면). TwoSided(d_pos, d_neg) = 명시 비대칭.

## 3. 설계

### 3.1 신규 `CreateSolidMode::ExtrudeBidirectional { dist_pos: f64, dist_neg: f64 }`

ADR-260 `ExtrudeCone` 직후. additive (serde-safe). SolidKind 은 delegated extrude
결과 (Box / Cylinder) 그대로. OneWay 는 기존 `Extrude { distance }` 불변.

### 3.2 Dispatch arm (ExtrudeCone:380 미러)

1. `dist_pos < 0` 또는 `dist_neg < 0` 또는 비유한 → reject.
2. `dist_pos + dist_neg < EPSILON_LENGTH` → reject (zero-volume).
3. **`is_move_only(self, profile_face)` (ORIGINAL face, cleave 前)** → reject
   (ADR-087 K-ε sandwich SSOT, fallback_dist=None).
4. ADR-102 γ cleave (coplanar siblings 격리 — 기존 면 무손상).
5. `extrude_planar_bidirectional(profile, dist_pos, dist_neg, ...)`.

### 3.3 Kernel `extrude_planar_bidirectional`

1. profile normal (Plane surface) 추출.
2. boundary verts = `collect_loop_verts(outer_start)`.
3. **`translate_verts(boundary_verts, −normal·dist_neg)`** (ADR-060 → 곡선/surface
   center 자동 갱신). `dist_neg == 0` → no-op (one-way up).
4. classify (Plane, AllLinear/AllCircular) → delegate:
   - AllLinear → `extrude_planar_box(profile, dist_pos+dist_neg, ...)`.
   - AllCircular → `extrude_planar_cylinder(profile, dist_pos+dist_neg, ...)`
     (self-loop → Path B `extrude_cylinder_kernel_native`).
   - else → `NotYetSupported` (Mixed/NURBS bidirectional = future).
5. return CreateSolidResult (delegated; profile_face = moved profile = bottom cap).

### 3.4 D5 — fail-closed (ADR-259/260 답습)

`ExtrudeBidirectional` = `fallback_dist = None` → scene 하드에러 경로
`restore_scene_snapshot` 자동 (translate + extrude + cleave 모두 byte-identical
rollback). silent 단방향 fallback 0. scene 코드 변경 0.

### 3.5 UX — ExtrudeMode 토글 (Q3, AixiAcad parity)

- **`ExtrudeModeSettings`** TS 모듈 (AutoIntersect/DrawCurveSettings 패턴): state
  `'oneway' | 'symmetric' | 'twosided'` + `distNeg` (TwoSided 용 mm). localStorage 보존.
- **토글 UI** (SettingsPanel): OneWay(기본) / Symmetric / TwoSided. TwoSided 선택 시
  dist_neg 입력 필드 노출.
- **PushPullTool commit 분기** (VCB 거리 / 최종 클릭 = `dp`):
  - oneway → `createSolidExtrude(faceId, dp)` (기존, 불변).
  - symmetric → `createSolidExtrudeBidirectional(faceId, dp, dp)`.
  - twosided → `createSolidExtrudeBidirectional(faceId, dp, distNeg)`.
- **Live-drag (ADR-193)**: v1 = commit 시점에 mode 적용 (drag preview 는 one-way,
  commit 이 mode 반영). **live bidirectional preview = 후속** (ADR-193 양방향 확장).
- VCB 문법 불변 (`거리,각도`=taper / `거리,비율%`=cone) — bidirectional 은 mode 토글로
  분리 (comma 충돌 없음).

## 4. Lock-ins (β 구현 시 강제)

- **L-261-1** 신규 `CreateSolidMode::ExtrudeBidirectional` (additive, serde-safe).
- **L-261-2** Q2 — translate(`−normal·dist_neg`) + 기존 extrude 재사용. profile 보존
  (bottom cap) → ownership 무변경.
- **L-261-3** ADR-060 translate_verts 가 곡선/surface center 자동 갱신 (AllCircular).
- **L-261-4** 가드 **변형 前**: dist_pos<0 / dist_neg<0 / 합<EPS / is_move_only reject.
- **L-261-5** D5 fail-closed (fallback_dist None → byte-identical rollback, silent
  단방향 fallback 0).
- **L-261-6** ADR-102 cleave (기존 coplanar 면 무손상, 사용자 #1 "면 안 깨짐").
- **L-261-7** ADR-183 — bottom cap (moved profile) outward −N, top outward +N
  (extrude_planar_box flip 그대로).
- **L-261-8** Symmetric(d) = (d, d) (AixiAcad parity, 각 방향 d).
- **L-261-9** ExtrudeMode 토글 (OneWay 기본) — VCB 문법 불변, comma 충돌 0.
- **L-261-10** commit-only v1 (live bidirectional preview 후속). additive (기존
  Extrude/Tapered/Cone/Revolve/Sweep/Loft 불변, ADR-046 P31 #4). 절대 #[ignore] 금지.

## 5. 시뮬레이션 게이트 (β-1, 먼저 시뮬 — ADR-259/260 답습)

`adr261_sim_*` Rust 테스트로 구현 후 라이브 전 검증:
- AllLinear symmetric (d, d) → box `[−d,+d]` manifold valid, top/bottom cap 위치 정확.
- AllLinear asymmetric (dp, dn) → box `[−dn,+dp]` manifold valid.
- AllCircular symmetric → cylinder `[−d,+d]` manifold valid, **Circle center 양 cap 정확**
  (ADR-060 translate 검증).
- AllCircular asymmetric → manifold valid.
- dist_neg=0 → one-way up (degenerate, valid). dist_pos=0 → one-way down.
- dist_pos<0 / dist_neg<0 / 합<EPS reject. is_move_only(solid face) reject + byte-identical.
- ownership: profile_face(=bottom cap) 보존 + top/side 새 face (scene 테스트).

## 6. Out of scope (별도 ADR / future)

- Mixed/NURBS profile bidirectional (AllLinear/AllCircular v1).
- Live-drag bidirectional preview (ADR-193 확장).
- Holed profile bidirectional (AixiAcad 도 v1 미지원).
- #4 벽 개구부 / #5 곡면 트랙 / #6 separated-disk.

## 7. Acceptance Log

- **2026-06-26 α** (`7c9607d`) — 본 spec + 결재 (Q1 사각+원 / Q2 translate+reuse /
  Q3 ExtrudeMode 토글). De-risk 인라인 (workflow `wf_06edc743` 서버 rate-limit →
  메모리 정책 "workflow rate-limit 빈번 → 직접 조사" 답습). AixiAcad
  `extrude_planar_face_bidir` + `ExtrudeMode` parity 확인.
- **2026-06-26 β-1** (`5340cdb`) — `CreateSolidMode::ExtrudeBidirectional` +
  dispatch arm + `extrude_planar_bidirectional` (translate `−n·dist_neg` →
  ADR-060 Phase O 곡선/surface 갱신 → 기존 extrude_planar_box/cylinder 재사용) +
  **시뮬 게이트 8 테스트**. scene.rs 코드 변경 0 (D5 자동 커버), scene 테스트 2개.
  회귀 axia-geo 2066→**2074** (+8), axia-core 412→**414** (+2). 시뮬이 box [−d,+d] /
  cylinder Circle center −d/+d (ADR-060 translate AllCircular 실증) 검증.
- **2026-06-26 β-2** (`934d1ce`) — WASM `create_solid_extrude_bidirectional` +
  step6 additive +2 + bridge `createSolidExtrudeBidirectional` (Toast) +
  WasmBridge vitest +4. D5: no PushPullDone fallback arm.
- **2026-06-26 β-3** (`428c60a`) — `ExtrudeModeSettings` (신규) + PushPullTool
  `commitBidirectional` + 2 commit 지점 mode 분기 + SettingsPanel OneWay/Symmetric/
  TwoSided 토글 + vitest 32→**37** (+5). tsc 0 errors.
- **2026-06-26 γ** (라이브, 재빌드 WASM, preview_eval) — **symmetric box**
  Z[−300,+300] 6면 `valid v=0` · **asymmetric box** Z[−300,+800] `valid` ·
  **cylinder symmetric** Z[−500,+500] 3면(Plane+Plane+Cylinder kind2) `valid` —
  ADR-060 translate가 Circle center 정확 이동 · **negative 거부** → false +
  **byte-identical** (fc 4→4) + `valid`. console crash 0.
- **2026-06-26 δ** (본 commit) — Status Proposed→Accepted + Acceptance Log +
  §8 Lessons + CLAUDE.md LOCKED #85 + memory 갱신.

## 8. Lessons (canonical)

- **L1 — translate + reuse 가 build-fresh(AixiAcad)보다 더 Pattern-12** — AixiAcad 는
  monolithic build-fresh (profile 소비 + fresh 정점 + ownership 재매핑). 우리는
  ADR-060 Phase O (translate_verts 곡선/surface 자동 갱신) 덕에 profile 을 *이동만*
  하고 기존 one-way extrude 를 전부 재사용 → profile 보존 (bottom cap) → Shape/Xia
  ownership 무변경. 신규 kernel 코드 최소, AllCircular 곡선 center 정확.
- **L2 — ADR-060 Phase O 가 AllCircular bidirectional 의 crux** — self-loop Circle
  의 anchor 이동 = 양 끝점(동일 vert) 이동 = full move → Circle curve center
  translate (partial move 만 Line fallback). de-risk 가 이 한 가지를 확인해
  translate-vs-build-fresh 결정을 종결 (먼저 시뮬 + 인프라 재검증 정책).
- **L3 — D5 fail-closed 가 scene 코드 변경 0 으로 무료 enforced** — ADR-259/260 답습.
  `ExtrudeBidirectional` = `fallback_dist = None` → MoveOnly/multi-loop/push_pull
  분기 구조적 skip + 하드에러 `restore_scene_snapshot` (translate+extrude+cleave
  모두 rollback). 자매 ADR 인프라 복리.
- **L4 — ExtrudeMode 토글 = bidirectional 의 자연 UX** (Q3, AixiAcad parity) — VCB
  comma (taper/cone) 와 충돌 없음 (mode 는 persistent 상태, value 인코딩 아님).
  commit 시점에 mode 적용; comma 입력은 mode 보다 우선 (명시 op). live preview 는
  v1 단방향 (live bidirectional 후속).
- **L5 — workflow rate-limit → 인라인 de-risk** (메모리 정책) — de-risk workflow 가
  서버측 rate-limit 으로 실패했으나, ADR-260 세션의 extrude 흐름 정독 + AixiAcad
  grep + ADR-060 확인으로 인라인 audit 완수. 결정 품질 저하 0.
