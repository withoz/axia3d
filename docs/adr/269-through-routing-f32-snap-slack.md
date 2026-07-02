# ADR-269 — Through-Cut Robustness: f32 Snap Slack + Cross-Drill Rejection

**Status**: Accepted (구현 + node-WASM end-to-end 검증 완료 — §D, §F)
**Track**: Track 7 (Phase 1 — CAD-core 실제 갭)
**Cross-link**: ADR-268(Curved-Profile Cut + Drill Winding) · ADR-252(pocket carve / Amendment 2 through) · ADR-249(drill through-hole) · ADR-267(Watertight Gate) · ADR-018(two-tone render) · 메타-원칙 #5 #6 #9

> 본 ADR 은 두 개의 through-cut 견고성 결함을 다룸: **§1~§E — f32 snap
> slack** (사각 관통이 얇은 흰 바닥을 남김), **§F Amendment — cross-drill
> rejection** (면에 스냅해도 원이 안 뚫림, 드릴 축이 기존 구멍과 교차).

---

## 1. Problem (engine-grounded, 사용자 시연 2026-07-02)

"면 위에 사각형을 그려 반대편 면/모서리 스냅까지 밀어 관통(Cut)" 했는데 **관통되지
않고 밑면이 남는다** ("사각형은 관통했을때 밑면이 생김"). 밑면 색은 **흰색
(#e8e8e8)** — 즉 방향은 정상(void-facing, ADR-268 winding 수정 유효)인 **올바른
blind 포켓 바닥**이 far 벽 바로 앞에 종이처럼 얇게 남은 것.

원(circle)은 같은 조작에서 깨끗하게 관통되어 사용자에게는 "사각형만 안 된다"로
보였으나, 엔진 실측(node-WASM)으로 **원·사각은 through 라우팅/geometry 가 완전히
동일**함을 확인 (top/side/이미 뚫린 면 위 순차 컷, blind↔through 전환 깊이 모두 일치).
차이는 shape 가 아니라 **push 깊이가 벽두께에 얼마나 정확히 닿았는가** 였음.

## 2. Root Cause (실측으로 확정)

Scene 의 pocket-vs-through 자동 라우팅(`scene.rs::carve_pocket_from_source_face`):

```rust
let through = self.mesh.wall_thickness_from_source_face(source_face)
    .map(|t| depth >= t - 1e-3)   // 슬랙 = 1e-3 단위 (1000-scale 에서 1μm)
    .unwrap_or(false);
```

슬랙이 **고정 1e-3** — 1000-unit 박스에서 1μm. 그러나 도구는 스냅 좌표를
**Three.js(f32)** 로 왕복시켜 depth 를 계산하므로 `~1e-4·|coord|` 의 정밀도가
손실됨 (1000-scale ≈ 0.06 unit ≫ 1μm). 반대편 면에 정확히 스냅해도 depth 가
999.94 로 계산되어 `999.94 ≥ 999.999` 가 거짓 → **blind 로 라우팅** → carve_pocket
이 far 벽 바로 앞(999.94)에 얇은 floor cap 을 만듦. 슬랙(1μm)이 f32 오차(~60μm)
보다 작아서 사용자 의도("반대편까지 = 관통")를 못 잡는 것이 근본 원인.

원이 우연히 깨끗했던 이유: 사용자가 원은 far 면을 지나도록 조금 더 깊게 밀어
depth ≥ 벽두께가 되었을 뿐 — shape-specific 버그가 아님.

## 3. Decision — Lock-in

- **L1 (relative, f32-robust slack).** 라우팅 슬랙을 단위 무관 상대값으로 교체:
  `depth >= t - (t * 1e-3).max(1e-3)` — 벽두께의 0.1%, 최소 1e-3. 1000-scale 에서
  슬랙 1.0 unit 로 f32 오차(~0.06)를 여유 있게 흡수하고, 모델 스케일과 무관하게
  동작 (door 코드의 relative-threshold 관례 답습, LOCKED #86). 사용자가 반대편
  면/모서리 스냅까지 밀면 안정적으로 관통. 메타-원칙 #5 (명확하면 자동).
- **L2 (genuine pocket 보존).** 슬랙은 벽두께 0.1% 이내(≈far 벽 바로 앞)에만 적용 —
  실제 얕은 blind 포켓(예: 두께의 90%)은 그대로 blind 유지. paper-thin floor
  (구조적 무의미 + 시각상 관통처럼 보임)만 through 로 승격.
- **L3 (엔진 truth ≠ f32 view).** through 판정은 f64 엔진 truth 로 하되, 입력 depth
  가 f32 view 를 경유한다는 사실을 슬랙이 흡수해야 한다. 향후 view→engine 경계의
  임계 비교는 절대 고정 sub-μm 슬랙 금지 — 좌표 크기 대비 상대 슬랙 사용.

## 4. Acceptance Log (§D)

| # | commit | 내용 | 회귀 |
|---|---|---|---|
| 1 | (본 commit) | through 라우팅 슬랙 fixed 1e-3 → relative `(t*1e-3).max(1e-3)` + ADR-269 회귀 | axia-core +1 |

**실측 검증 (node-WASM, 도구가 부르는 scene 라우팅 경로 그대로, 1000³ 박스 −Y 면 rect):**
- depth 1000(=두께) → THROUGH(exitCov 0) · 999.95(f32-gap) → **THROUGH** (수정 전 BLIND)
  · 999.5 → THROUGH · 900(진짜 얕은 포켓) → BLIND(floor) — 모두 watertight.
- 원·사각 blind↔through 전환 깊이 동일함을 사전 확인 (shape-agnostic).

**회귀 무손상:** axia-geo `operations::carve` 43/43 · axia-core `adr252` 2/2 · 신규
`adr269` 1/1 PASS. 슬랙 확대는 through 를 *더 일찍* 발동시켜 carve_pocket 의 far-wall
bail 도달 케이스를 줄이므로 안전.

## 5. 회귀 자산 (절대 #[ignore] 금지)

- `adr269_near_thickness_push_routes_through_not_blind` — depth = 두께 − 0.05
  (f32 노이즈, old 1e-3 슬랙 초과)가 THROUGH(entry+exit 2 ring-with-hole cap +
  watertight tunnel)로 라우팅, blind(1 ring + solid floor)가 아님을 검증.

## E. 방법론 교훈 (canonical)

**View→Engine 경계의 임계 비교는 상대 슬랙.** 엔진은 f64 exact(LOCKED #5)이지만
사용자 입력은 Three.js(f32)를 경유한다. depth·좌표를 벽두께와 정확 비교하는 곳에
고정 sub-μm 슬랙을 쓰면 f32 왕복 오차(~1e-4·|coord|)에 무너진다 — 좌표 크기 대비
상대 슬랙(+절대 floor)이 정답. ADR-268 이 winding(방향)을, 본 ADR 이 routing
(임계) 을 잡음 — 둘 다 "topology valid 인데 시각/의도가 어긋남" 계열.

**shape 차이로 보여도 depth 차이일 수 있다.** 사용자에겐 "원은 되고 사각은 안 됨"
이었으나 엔진 실측으로 shape-agnostic 임을 먼저 확정한 것이 근본 원인(스냅 깊이
정밀도)으로 좁히는 결정적 단계였다. 재현 안 되는 shape-specific 주장은 엔진에서
두 shape 를 동일 경로로 실측해 falsify 먼저.

---

## F. Amendment — Cross-Drill Rejection (2026-07-02, 사용자 2차 시연)

### F.1 Problem
"면에 스냅이 걸렸는데도 안 뚫림" — 박스 윗면에 관통 구멍이 있는 상태에서
옆면에 원을 그려 관통(Cut)하면, 밑면이 흰색으로 정상인데도 구멍이 안 뚫리고
치수가 **-15,139 mm** 같은 엉뚱한 값. 도구는 `carvePocketFromSourceFace`
declined → extrude fallback(엉뚱한 보스) → 사용자는 "안 뚫림"으로 인지.

### F.2 Root Cause (실측 격리)
`face_outline_points`(원 곡선) 은 정확(y=-500)했고, disk render 도 정확.
문제는 **through 드릴의 반대벽 탐지** (`carve_ray_nearest_face`, drill_polygon/
circular/rect 공용): **가장 가까운** 면을 반대벽으로 잡는다. 옆면 드릴 축(Y)이
윗면 관통 구멍의 **수직 tube 빈 공간**(x²+y²<r², 축이 관통)을 지나면, ray 가
outer far 벽(1000) 대신 **tube 내벽**(≈200 앞)을 먼저 만나 거기 exit ring 을
punch 하려다 "polygon hole extends outside the face boundary" 로 실패.
= 기존 구멍을 가로질러 뚫는 **cross-drilling** (T-교차). straight-tube MVP 는
교차 위상을 만들 수 없음.

격리: 옆면 disk 를 축이 tube 를 **빗나가게**(offset) 두면 모든 조합(top
through/blind × side through/blind) 정상 작동. 축이 tube 를 **관통**할 때만 실패.

### F.3 Decision — Lock-ins
- **F-L1 (cross-drill 명시 거부).** `Mesh::carve_drill_is_cross_drill(profile,
  n)` 신설: 가장 가까운 반대면을 찾고, **프로파일을 그 평면에 투영해 전체가
  그 면 안에 들어가는지** 검사. 안 들어가면(tiny interior tube wall) → cross-
  drill → 명확한 한국어 메시지로 bail. drill_polygon(원/사각 carve 경로) +
  drill_circular + drill_rect 세 through 드릴 모두 가드.
- **F-L2 (multi-solid 오탐 금지).** 적층 2솔리드 관통(box2 → 아래 box1 벽
  포함)은 정상 — 반대면(box2 바닥)이 프로파일을 **담을 수 있으므로** 통과.
  (초기 winding-부호 / crossing-count 휴리스틱은 multi-solid 를 오탐 → 폐기.
  "exit 면이 프로파일을 담는가" 가 유일하게 두 경우를 정확히 가른다.)
- **F-L3 (tool: 명시 abort).** PushPullTool — sheet 소스를 안쪽으로 미는 것은
  명백히 pocket/through 의도. carve decline 시 `lastError()` 를 Toast 로
  띄우고 **abort** (안쪽 extrude 보스 fallback 금지 — 혼란 유발).
- **F-L4 (cross-drill 지원은 future).** 진짜 T-교차 구멍은 Boolean(빼기)
  급 위상 → 별도 ADR. 현재는 위치 이동 또는 Boolean 안내.

### F.4 Acceptance (§D 연장)
| # | 내용 | 회귀 |
|---|---|---|
| 2 | cross-drill 명시 거부 (`carve_drill_is_cross_drill`) + tool abort | axia-geo +1 (`adr269_cross_drilling_through_existing_hole_rejected`) |

**실측 (node-WASM):** top through/blind + 옆면 원 — 축이 tube 빗나감(sx=250)
→ 모두 OK(blind floor / through open, watertight); 축이 tube 관통(sx=0) →
명확 거부("관통 축이 기존 구멍/빈 공간과 교차합니다…") + mesh watertight 유지.
**회귀 무손상:** `operations::carve` 44/44 (adr251 multi-solid 허용 + adr269
cross-drill 거부 동시 PASS).

### F.5 방법론 교훈
- **가드는 실패-케이스 뿐 아니라 인접 정상-케이스로 반드시 검증.** 첫
  crossing-count(≥2) 휴리스틱은 cross-drill 을 잡았지만 multi-solid 적층
  관통(정상)을 오탐 → 회귀가 즉시 노출. 판별자는 "exit 면이 프로파일을
  담는가" 로 수렴 — 실패 메커니즘(punch host 못 찾음)에 직접 대응하는 검사가
  가장 견고.
- **winding 부호로 void-entry ↔ clean-exit 를 구분할 수 없다.** tube 내벽도
  outer 벽도 ray 는 뒷면(fn·dir>0)을 만난다(ADR-268 tube=void-facing). 국소
  법선만으로는 "그 너머에 같은 솔리드가 더 있는가"를 알 수 없음 → 프로파일
  containment 가 실질 판별.
