# ADR-268 — Curved-Profile Cut Support + Drill Tube-Wall Winding Fix

**Status**: Accepted (구현 + 브라우저 검증 완료 — §D)
**Track**: Track 7 (Phase 1 — CAD-core 실제 갭)
**Cross-link**: ADR-267(Watertight Gate) · ADR-089(closed-curve citizenship) · ADR-249(drill through-hole) · ADR-252(pocket carve) · ADR-007(Face Orientation/winding) · ADR-018(two-tone render) · ADR-037 P22(owner-ID) · 메타-원칙 #5 #6 #9 #14

---

## 1. Problem (engine-grounded, 사용자 시연 2026-07-02)

"면 위에 도형을 그려 Extrude/Cut" 워크플로우에서 두 계열의 결함이 사용자 시연으로 드러남:

1. **원(circle) cut 무동작** — 원을 면 위에 그리면 "경계 → 면" 규칙(ADR-089
   closed-curve / 메타-원칙 #14)이 **채워진 disk 면**(1 anchor vert + self-loop
   edge + `AnalyticCurve`)을 만드는데, Extrude/Cut 이 아무 구멍도 못 만듦.
2. **drill/포켓 벽이 깨져 보임** — 원/사각/육각 관통 드릴의 tube 벽이 구멍 안에서
   backside(캡처럼 lavender)로 보임. 선택 시 깨진 삼각형(spear)으로 하이라이트.

## 2. Root Causes (실측으로 확정)

### R1 — cut 경로가 closed-curve 프로파일을 거부
`carve.rs`의 4개 함수가 프로파일 outline 을 `collect_loop_verts >= 3` (다각형)
으로만 읽음 → 원(1 anchor vert) disk 는 무조건 거부:
- `find_larger_coplanar_container_face` → `None` → `faceHasLargerCoplanarContainer
  = false` → PushPullTool 이 carve 스킵 → `createSolidExtrude` fallback (구멍 없음).
- `carve_pocket_from_source_face` / `carve_through_from_source_face` → bail.
- `wall_thickness_from_source_face` → `None` → through-vs-blind 판정 항상 blind →
  깊은 원 push 가 blind pocket 으로 처리 → carve bail → capped extrude fallback
  (사용자 본 "캡 원통").

### R2 — 공유 `bridge_through_loops` 의 winding + twist
모든 드릴(circular/rect/polygon) + `carve_through` 가 공유하는 tube-wall 브리지:
- **정점 페어링**: `b_rev[k]` 를 `e_loop[0]` 에만 최근접 정렬(offset k), 나머지는
  순서 일치 가정 → 다세그먼트(원/육각) 에서 쿼드 **twist (non-planar)**.
- **winding**: 고정 순서 `[a2, a, b, b2]` → **모든 벽이 축 반대(재료) 방향**. 
  `is_closed_solid` / `verify_face_invariants` 는 **winding 방향을 검사하지 않으므로**
  manifold "valid" 로 통과하며 시각만 backside(ADR-018 two-tone). 선택 하이라이트도
  잘못된 벽을 깨진 삼각형으로 렌더(spear) — 이 winding 이 근본 원인.

## 3. Decision — Lock-ins

- **L1 (face_outline_points SSOT).** `Mesh::face_outline_points(face) ->
  Option<Vec<DVec3>>` 신설: 다각형(≥3 vert) → vertex 위치, **closed-curve**
  (1-vert self-loop + `AnalyticCurve`) → 곡선 tessellate (Circle/Bezier/BSpline/
  NURBS, `punch_circular_hole` 의 faceted 방식 답습). cut 경로 4개 함수
  (container / pocket / through / thickness) 가 전부 이 helper 사용 → **원/사각/
  폴리곤 동일 경로**. Line/Arc 은 닫힌 프로파일 아님 → `None`.
- **L2 (drill winding = void-facing, uniform).** `bridge_through_loops` 는
  per-vertex 최근접 페어링(paired[i] = e_loop[i] 축수직면 최근접 exit vertex —
  straight-through convex 는 congruent loop ⇒ bijective rotation, twist 0) +
  축-radial 기준 **uniform flip** (벽 0 법선이 재료 쪽이면 전 tube 균일 flip →
  모든 벽이 void 향함). 균일 flip 이라 인접 벽 twin(HE) 유지 = manifold 무손상.
- **L3 (through auto-routing 재사용).** scene `carve_pocket_from_source_face` 의
  기존 through 분기(depth ≥ wall_thickness → carve_through)를 그대로 활용. R1 의
  wall_thickness 수정만으로 원도 through 자동 라우팅 발동 → **새 WASM export /
  도구 코드 불필요**. 얕게 push = blind 원통 포켓(floor cap), 깊게 = 관통(캡 없는
  열린 tube).
- **L4 (topology ≠ orientation).** ADR-267 게이트 + `is_closed_solid` 는 위상
  (manifold/watertight)만 검증하고 **면 outward winding 방향은 검증 안 함**. 향후
  cut/drill 검증은 **면 normal · radial 방향까지 측정** (§E 방법론 교훈).

## 4. Acceptance Log (§D)

| # | commit | 내용 | 회귀 |
|---|---|---|---|
| 1 | `91d6fb6` | 원(closed-curve) 프로파일 cut 지원 — face_outline_points 신설 + container/carve_pocket 배선 | axia-geo +2 (outline tessellate / 원 blind 포켓 watertight) |
| 2 | `036cb67` | drill tube-wall winding + twist 근본 수정 (bridge_through_loops nearest-pairing + uniform void-facing) + carve_through closed-curve | axia-geo +2 (winding 가드 / 원 관통 open tube) |
| 3 | `fed1845` | 원 관통 auto-routing — wall_thickness_from_source_face closed-curve | axia-geo +1 (원 두께 측정) |
| 4 | `077f44c` | 폴리곤/육각/자유곡선 cut 검증 + closed-Bezier carve 회귀 | axia-geo +1 |
| 5 | `4df324f` | carve_pocket 벽+floor void-facing winding (bridge 와 별도) + orientation 제도화 finding | axia-geo +1 (pocket walls/floor 가드) |

**최종 health:** axia-geo 2107 → **2112**, vitest 2474, wasm SIMD verified. 공유
`bridge_through_loops` 변경에도 기존 drill/boolean/carve manifold 테스트 무회귀.

**실측 검증 (node-WASM, 도구가 부르는 경로 그대로):**
- 원 disk → `carvePocketFromSourceFace` 얕게 = sides 100 blind 포켓(floor),
  깊게 = 열린 tube (TOP/BOTTOM center coverage 0, 캡 없음), integrity valid.
- rect drill: 벽 4개 dotRadial +1(away) → **-1(toward void)**.
- 32각형 drill: 32 벽 전부 toward void, twist 0.

**브라우저 검증 (localhost:5199, 사용자 확인 2026-07-02):**
- 원형 관통 구멍 + 사각 포켓 벽 front(흰색) 깨끗, 선택 하이라이트 정상(spear 없음).

## 5. 회귀 자산 (절대 #[ignore] 금지)

- `adr267_face_outline_points_tessellates_closed_curve`
- `adr267_pocket_circle_from_closed_curve_source`
- `adr267_through_circle_from_closed_curve_source`
- `adr267_drill_tube_walls_face_into_void` (drill winding 가드)
- `adr267_wall_thickness_works_for_circle_source`
- `adr268_pocket_walls_and_floor_face_into_void` (pocket winding 가드)
- `adr268_carve_pocket_from_closed_bezier_source` (freeform 분기)

## E. 방법론 교훈 (canonical — 향후 cut/drill 검증)

**Topology 검증 ≠ Orientation 검증.** `is_closed_solid` / `verify_face_invariants`
(ADR-267 게이트 포함) 는 manifold/watertight/winding-vs-hint 는 보지만 **면이 void
쪽인지 material 쪽인지(outward 방향)는 검사하지 않는다.** 따라서 벽이 통째로
뒤집혀도 게이트를 통과하며 시각만 깨진다(ADR-018 backside). cut/drill 결과를
검증할 때는 coverage/bounds/integrity 뿐 아니라 **면 normal 을 hole 축 radial 과
내적해 방향 일관성까지 측정**해야 한다. (본 ADR 이 처음 이 격차를 노출 — 초기
node-WASM 검증이 topology 만 봐서 winding 버그를 놓쳤고, 사용자 시연이 잡았다.)

**"도구 배선 필요" 추정 전에 상위 계층 기존 경로 확인.** 원 관통은 새 WASM
export/도구 코드가 필요하다고 추정했으나, scene 이 이미 through 를 auto-routing
하고 도구도 이미 carve 를 호출 — 하위 helper 하나(wall_thickness)만 곡선 미지원
이었다. Pattern-12 변형(상위 완비, 하위 helper 갭).

**orientation 검증 제도화 (2026-07-02 추가, commit `4df324f`).** winding 격차를
자동으로 잡으려 기존 `verify_outward_normals` 게이트化를 시도했으나 **실측으로
부적합 확정**: 그 함수는 mesh-centroid → face-centroid · normal 볼록 heuristic
이라 **오목 feature(구멍/포켓)를 오탐** — 올바르게 파인 드릴 박스의 tube 벽 4개
(void 향함=정상)를 inward 로 flag(inwardCount=4). 즉 **일반 orientation 게이트
로 쓰면 정상 구멍/포켓을 전부 거부**. 결론(canonical): 방향 검증은 **op 가 void
방향을 아는 per-op 가드**가 정답 — 축-radial 기준(오목 안전)으로 벽/floor 가
축(void)을 향하는지 검사. 회귀 자산 3종(drill/pocket walls + pocket floor)이
이 패턴. **또한 winding 버그는 한 곳이 아니다**: `bridge_through_loops`(through
드릴 공유, `036cb67`) 와 `carve_pocket_from_source_face`(blind 포켓 inline 벽,
`4df324f`) 가 **각각 별도로** 같은 버그를 가짐 — 공유 함수 하나 고쳤다고 전 op
가 낫는 것 아님, op 별 확인 필요.
