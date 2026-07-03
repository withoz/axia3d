# ADR-270 — Plane Lock Is (Normal, Offset): Draw On the Hovered Face

**Status**: Accepted (구현 + 브라우저 라이브 검증 완료 — §D)
**Track**: Track 7 (Phase 1 — CAD-core UX 직관성)
**Cross-link**: ADR-188(first-shape plane lock) · ADR-166(plane lock β) · ADR-167(EPS_PLANE_NORMAL) · ADR-140(surface-aware getDrawPlane) · ADR-181(DrawRect getDrawPlane SSOT) · ADR-103(Z-up ground) · 메타-원칙 #4 #5

---

## 1. Problem (engine-grounded, 사용자 시연 2026-07-03)

> "먼저 입체면 위에 도형 그릴때 rect 윗면에 바로 그려지지 않아요. 입체면에
>  도형 그리는 기능을 직관적으로 편리하게 구현해줘."

바닥(ground)에 도형을 한 번 그린 뒤(→ plane lock 이 z=0 에 걸림), 박스의 **윗면**
(z=750) 에 커서를 올려 rect 를 그리면 — rect 가 윗면이 아니라 **바닥(z=0)** 에
그려짐. 사용자 의도("보이는 면 위에 그린다")와 어긋남.

## 2. Root Cause (브라우저 라이브 격리)

`getDrawPlane`(ToolManagerRefactored) 의 plane-lock "같은 평면" 판정이 **normal 만
비교**하고 **평면 offset(높이)은 무시**했다:

```
plane lock:        normal (0,0,1), origin z=0     ← 바닥 rect 의 평면
박스 윗면 pick:      point z=750,   normal (0,0,1)  ← 정확히 윗면 hit
getDrawPlane 결과:   onFace=false,  origin z=0      ← 윗면(z=750) 무시!
```

박스 윗면(+Z, z=750)은 바닥(+Z, z=0)과 **normal 이 같아** `|dot| = 1 ≥ 0.9999`
→ "같은 평면" 으로 오판 → lock(z=0) 유지 → rect 가 바닥에 그려짐. 평면은
`(normal, offset)` 인데 offset 을 안 봤다.

첫 draw(락 없음)는 정상 작동했다 — 락이 없으니 face-hit 로직이 바로 윗면을 씀.
버그는 **이미 다른 높이의 같은-normal 평면에 락이 걸린 상태**에서만 발현.

## 3. Decision — Lock-in

- **L1 (plane = normal + offset).** `getDrawPlane` 의 lock 판정에 **offset 비교
  추가**: 락 normal 로 face hit point 를 투영한 값(`lockNormal · hitPoint`)이
  락 자신의 offset(`lockNormal · origin`)과 `OFFSET_TOL = 0.5mm` 이상 다르면
  → **다른 평면** → auto-unlock + face-hit 로직으로 fall through. normal 이
  다르거나(기존 ADR-166 amendment) offset 이 다르거나 둘 중 하나면 unlock.
- **L2 (ADR-188 coplanar 값 보존).** 같은 면 반복 그리기(같은 normal + 같은
  offset, diff ≈ 0)는 **락 유지** — 여러 도형이 한 평면에 coplanar 로 쌓여
  hole/division 형성(ADR-186 유도면)이라는 ADR-188 핵심 가치 그대로. 면은 ≥ mm
  간격이라 0.5mm 가 "같은 면 재그리기"와 "다른 높이 면"을 깨끗이 가른다.
- **L3 (직관 = 보이는 면에 그린다).** SketchUp 관습 — 커서가 solid face 위면
  그 면에 그린다. 락은 "빈 공간에서 같은 평면 반복"을 돕는 보조일 뿐, 명시적
  다른-면 hover 를 이기지 못한다.

## 4. Acceptance Log (§D)

| # | 내용 | 회귀 |
|---|---|---|
| 1 | getDrawPlane lock 판정에 offset 비교 추가 (normal-only → normal+offset) | vitest +2 |

**브라우저 라이브 검증 (real Chromium, synthetic events + 카메라 투영):**
- 수정 전: 바닥 rect(z=0 lock) → 박스 윗면 rect → `getDrawPlane` onFace=false
  origin z=0 → rect 가 바닥에 그려짐(박스 footprint 근처 z=750 면 없음).
- 수정 후: 동일 조작 → `getDrawPlane` onFace=true → rect 가 **윗면 z=750**
  (centroid [-1,-3,750], 면적 247288 ≈ 250000)에 정확히 그려짐. 시각으로도
  바닥 rect + 윗면 rect 각각 제 위치 확인.

**회귀 무손상:** ToolManagerRefactored 141/141 + DrawRectTool 29/29 = 170/170 PASS.

## 5. 회귀 자산 (절대 #[ignore] 금지)

- `ADR-270 … same normal, DIFFERENT offset (box top z=750 vs locked ground z=0)
  → auto-unlock, onFace` — 다른 높이 면 = 다른 평면 → unlock + onFace.
- `ADR-270 … same normal, SAME offset (repeat draw on same face) → keeps lock` —
  같은 면 재그리기 = 락 유지(ADR-188 coplanar 보존).

## E. 방법론 교훈 (canonical)

**평면은 방향(normal)이 아니라 (방향, 위치)다.** 두 평면이 평행(normal 동일)해도
offset 이 다르면 다른 평면이다. 평면 동일성 판정에 normal 만 쓰면 "높이만 다른
평행면"을 같은 것으로 오인한다 — CAD 에서 흔한 함정(바닥 vs 박스 윗면, 벽 vs
반대편 벽). ADR-269 가 "topology valid 인데 의도 어긋남"(routing)이었다면, 본 ADR
은 "normal 같은데 위치 다름"(평면 동일성)이다.

**첫 케이스가 되면 회귀 없다고 단정 금지 — 상태(락) 있는 후속 케이스를 시연.**
첫 draw(락 없음)는 정상이라 "된다"고 보이지만, 락이 걸린 두 번째 draw 에서만
버그가 났다. 상태 의존 버그는 상태를 만든 뒤 재현해야 잡힌다.
