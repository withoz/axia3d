# ADR-269 — Through-vs-Pocket Routing Slack Must Absorb f32 Snap Noise

**Status**: Accepted (구현 + node-WASM end-to-end 검증 완료 — §D)
**Track**: Track 7 (Phase 1 — CAD-core 실제 갭)
**Cross-link**: ADR-268(Curved-Profile Cut + Drill Winding) · ADR-252(pocket carve / Amendment 2 through) · ADR-249(drill through-hole) · ADR-267(Watertight Gate) · ADR-018(two-tone render) · 메타-원칙 #5 #9

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
