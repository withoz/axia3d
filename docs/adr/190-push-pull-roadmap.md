# ADR-190 — Push/Pull 전체 구현 로드맵 + Phase 0 (Robustness)

> 사용자 요청 "푸시풀에 대한 전체적인 구현계획" 의 공식화. 5-phase 로드맵 +
> **Phase 0 (모든 면 pushable) closure**. Phase 1~4 는 별도 ADR 로 진행.

- **Status**: Accepted (Phase 0); roadmap (Phase 1~4 planned)
- **Date**: 2026-06-09
- **Track**: 6 (boundary kernel / 유도면) + W (ADR-079 create_solid)

---

## Canonical anchor (사용자 요청, 2026-06-09)

> "다음은 푸시풀에 대한 전체적인 구현계획을 세워주세요."
> 결재: **Phase 0 바로 진행** + **P0.1 + P0.2 모두**.

---

## 1. 비전 — "완전한 Push/Pull"

1. **모든 보이는 면은 push 가능** (하드 실패·침묵 실패 0)
2. **Surface-native** — 결과 솔리드의 모든 면이 analytic surface 보유 (ADR-079 L3)
3. **Hole-through carve** — 면을 솔리드 안으로 밀면 자동 절삭 (SketchUp 시그니처)
4. **CAD-parity UX** — repeat-last / modifier / target-push / smooth-group align
5. **유도면 coherent** — push 결과가 ADR-186 재유도 모델과 정합

---

## 2. 현재 상태 (audit 요약)

**이중 구조**: `Mesh::create_solid(Extrude)` (canonical, surface-native, ADR-079
W) + 레거시 `Mesh::push_pull` (ADR-079 Q3 fallback, surface-agnostic).

| 면 종류 | 지원 | 비고 |
|---|---|---|
| 평면 rect/circle (직접 그림) | ✅ native | Box / Cylinder |
| Cylinder/Sphere/Cone/Torus 면 | ✅ W-2 offset | smooth-group |
| Plane + Mixed 경계 | ⚠ fallback | NotYetSupported → push_pull |
| 구멍 있는 면 (ring) | ❌ 전역 거부 | ADR-016 Q2 |
| **유도면 arrange 산물 (arc 반원/lens/split)** | ❌ 하드 실패 → ✅ Phase 0 | NoProfileSurface (P0 해소) |
| 곡면/NURBS | ⚠ 침묵 fallback | UI 표시 0 |

---

## 3. 핵심 발견 (실측 confirmed)

> **#1 — 유도면이 만든 면은 push 가 완전히 안 됨** (Phase 0 의 trigger).
> arc 반원(면적 90000) → `createSolidExtrude` → `"profile face has no
> AnalyticSurface attached"` 하드 실패. `NoProfileSurface` 는 Q3 fallback 이
> 안 잡음.
> **Root cause**: rederive arrange materialize 가 self-loop(Circle) parent
> 파생 면에 Plane surface 를 안 붙임 (Circle disk 는 1-vertex boundary →
> `dirty_faces` inherit 누락). ADR-189 arc 전환이 이 gap 노출.
> 경계: 직접 그린 면 + auto-intersect lens = surface 보유 ✅ / 유도면 arc 파생
> = surface 없음 ❌ (Approach A polygon 은 pushable 였으나 Approach B arc 는 unpushable 였음).

---

## 4. Phased Roadmap (각 phase = 별도 ADR, Path Z atomic)

### Phase 0 — 모든 면 pushable 보장 ✅ **CLOSED (본 ADR)**
P0.1 rederive surface attach (root-cause) + P0.2 fallback safety net (3-part). §6 참조.

### Phase 1 — Surface-native 커버리지 완성 (예정)
- P1.1 Plane+Mixed native (arc+line 혼합 경계 → GeneralSweep, 현재 fallback)
- P1.2 **ring(구멍 면) push → tube** (ADR-016 Q2 전역 거부 해제 — 별도 결재 필요, LOCKED)
- P1.3 Closed-curve Path B 비-Circle (Arc/Bezier/BSpline/NURBS disk)
- 예상 회귀 +30~40. 위험 중

### Phase 2 — Hole-through / Boolean (signature CAD) ✅ **CLOSED — 이미 구현됨 (ADR-293 측정)**
- ~~P2.1 면을 솔리드 안으로 push → 자동 subtract (carve/recess)~~ → **SHIPPED** (ADR-264 fuse 가
  프로파일을 imprint + inward push 가 포켓 carve)
- ~~P2.2 관통 push → 구멍~~ → **SHIPPED** (ADR-252 "옵션 A 스마트 자동 전환" —
  `exec_create_solid` 이 `distance<0 && wall_thickness_from_source_face(face).is_some()` 게이트로
  `carve_pocket_from_source_face` 에 라우팅, blind↔through 자동 전환. rect + circle 모두 측정 확인)
- ~~예상 회귀 +40~50. 위험 중상. 최고 체감 가치~~ → 추정은 **신규 기하를 가정**했으나 기하는
  이미 있었음.

> ⚠ **본 Phase 2 항목이 stale 인 채 남아 ADR-293 α 를 오도했다** (2026-07-15). 위 텍스트는
> 2026-06-09 시점 계획이고, 그 뒤 ADR-252/264/267/269 가 실제로 구현했다. 로드맵의 "예정" 을
> 근거로 β 를 쓰면 **이미 있는 기능을 다시 만들게 된다** — 측정 우선(메타-원칙 #6).
>
> **남은 실제 갭 (ADR-293 §5)**: 면 **전체** inward push 가 두께를 넘으면 **말없이 clamp** 되어
> 솔리드가 sliver 로 붕괴(측정: vol 2e9 → 2000)하는데 `ret=true` + watertight + 알림 없음.
> clamp 유지는 옳음(면 전체 push 는 의도 모호 — 메타-원칙 #16); **결함은 침묵**. 별도 ADR 후보.

### Phase 3 — UX / CAD parity (진행 중)
- ~~repeat-last (`lastPPDist` 이미 캐시)~~ → ✅ **CLOSED** (2026-07-15). 로드맵의
  "이미 캐시" 는 정확했다 — `lastPPDist` 는 **쓰기 4 / 읽기 0** 의 죽은 캐시였고,
  더블클릭의 2번째 mousedown 이 이미 Phase 2 에 `dist ≈ 0` 으로 도착해
  MIN_COMMIT_DIST 에 삼켜지고 있었다(그 자리를 scene.rs 주석이 이미
  "single-face double-click with no movement" 로 예견). 그 죽은 슬롯에 읽기만
  붙임. 가드 2개로 additive 보장 — `lastPPDist !== 0` + `currentDragDist === 0`
  (모든 onMouseMove 가 `currentDragDist = dist` 로 끝나므로 드래그/align 값이
  항상 우선). `e.detail >= 2` 가 "커서 안 움직임" 을 이미 담으므로 별도
  거리 검사 불필요. 회귀 +4 (vitest, 뮤테이션 4/4 검출) + E2E +1 (실제
  더블클릭 → B 가 기억된 150mm 상승, violations 0).
- 잔여: modifier(Ctrl=병합 안 함, 단축키 충돌 검토 필요) / smooth-group
  align(현재 비활성, `PushPullTool.ts:369` "v1: 단일 면만 지원") / **침묵 깨기
  3종** (곡면 fallback 표시 · ADR-293 §5 whole-face silent clamp Toast ·
  align 발동 표시) / MIN_COMMIT_DIST config (가치 낮음)
- 위험 낮음 (TS 중심)

### Phase 4 — Advanced (예정)
- target-face push("push to") / push-pull copy / curved-surface offset polish

---

## 5. Lock-ins (Phase 0)

- **L-190-1** P0.1 — re-derive arrange 가 materialize 하는 **모든 평면 면**에
  Plane surface 부여 (plane 이미 알고 있음). parent surface 우선, 없으면 synthesize.
- **L-190-2** P0.2-a — `exec_create_solid` fallback 이 `NotYetSupported` +
  `NoProfileSurface` + **일반 내부 에러(downcast None)** 까지 catch → push_pull.
  단 *deliberate* SolidError(목록 외)는 하드 에러 유지.
- **L-190-3** P0.2-b — fallback 이 pre-op snapshot 복원 (`transactions.cancel()`
  은 recording 만 폐기, 복원 안 함 — ADR-102 cleave mutation 잔존 차단).
- **L-190-4** P0.2-c — fallback 이 push_pull 전 coplanar sibling **재-cleave**
  (native cleave 가 snapshot 복원으로 롤백됨 → manifold 보존).
- **L-190-5** Native success path 무변경 — control(plain rect) 회귀 0.
- **L-190-6** ADR-079 L3 (result faces = surface) + 메타-원칙 #4(SSOT) /
  #5(편의) / #6(preventive) 정합.
- **L-190-7** 절대 #[ignore] 금지.

---

## 6. Acceptance Log — Phase 0

### 6.1 audit + 사전검토 + 결재 (2026-06-09)
- 엔진(`create_solid`/`push_pull`/`cleave`) + 툴(`PushPullTool`) audit (2 agent).
- 실측 root-cause: arc 반원 → NoProfileSurface 하드 실패 (§3).
- 결재: Phase 0 (P0.1 + P0.2) 즉시 진행.

### 6.2 구현 — commit `4c0e9bb` (LOCAL, adr-186/boundary-kernel-port)
- `face_rederive.rs` +18 (P0.1 default_plane_surface + inherit fallback).
- `scene.rs` +161 (P0.2 3-part: catch 확장 + snapshot 복원 + 재-cleave + 회귀 2).
- 회귀: `adr190_p0_arc_halfdisk_pushable_and_manifold` (P0.1 surface +
  P0.2 push + manifold) + `adr190_p0_plain_rect_push_box_unchanged` (control).
- 워크스페이스: axia-core 338 / axia-geo 1694 / foreign 138 / transaction 4 /
  wasm 8 — **2182 PASS, 0 failed, 0 ignored**.

### 6.3 브라우저 검증 (clean scene, ADR-087 K-ζ)
| 면 | 이전 | 이후 |
|---|---|---|
| arc 반원 (ADR-189) | NoProfileSurface 하드 실패 | push + manifold valid ✅ |
| circle×circle lens | 케이스별 | push + manifold valid ✅ |
| plain rect (control) | box | box + manifold (회귀 0) ✅ |

3겹 추적: surface 없음(P0.1) → "Face not found"(snapshot 복원) → non-manifold(재-cleave).

---

## 7. Cross-link

- **ADR-079** L3 (surface = truth) + Q3 (push_pull fallback) — Phase 0 의 직접 layer
- **ADR-186** 유도면 re-derive (P0.1 surface attach 대상)
- **ADR-189** Arc-Preserving Split — #1 gap 을 노출한 직계 (LOCKED #75)
- **ADR-102** Detach-on-Arrangement cleave (P0.2-c 재-cleave 재사용)
- **ADR-101** coplanar auto-intersect (lens sub-face — pushable 확인)
- **ADR-016 Q2** multi-loop reject (Phase 1 P1.2 결재 대상)
- **ADR-064/066** NURBS Boolean (Phase 2 hole-through 연동)
- **ADR-087** K-ζ 사용자 시연 게이트 / **메타-원칙 #4/#5/#6**
- commit `4c0e9bb`

---

## 8. Lessons (Phase 0)

- **L1 — audit-first 가 #1 을 정확히 노출** — 엔진/툴 2-agent audit + 실측 probe
  로 "유도면 면 unpushable" 하드 실패를 코드+실측 양쪽 grounding.
- **L2 — 3-layer 추적의 가치** — surface 없음 → Face not found → non-manifold,
  각 layer 를 실측으로 벗겨내며 root-cause 까지. 한 번에 안 보이는 결함.
- **L3 — `transactions.cancel()` ≠ 복원** — cancel 은 recording 폐기일 뿐.
  fallback 의 mesh 정합은 caller 가 snapshot 복원 책임 (undo 패턴과 분리).
- **L4 — fallback 도 cleave 해야 manifold** — native cleave 가 snapshot 복원으로
  롤백되므로, fallback push_pull 전 재-cleave 필요 (ADR-102 자산 재사용).
- **L5 — ADR-189 가 노출한 gap** — 내 직전 arc 전환이 surface 누락을 드러냄.
  기능 추가가 인접 결함을 노출하는 자연스러운 연쇄 (Phase 0 trigger).
