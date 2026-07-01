# ADR-188 — Same-Plane Drawing from First Shape (orange removal + strong plane lock)

**Status**: Accepted (vitest 2097 passed + tsc 0 — 2026-06-02; browser demo
sign-off pending)
**Date**: 2026-06-02
**Author**: WYKO + Claude
**Trigger**: 사용자 시연 (2026-06-02, 스크린샷): 사각형이 원 면 위를 지나며
프리뷰가 **주황(on-face)** 으로 떠 lens 처럼 보임. 사용자 통찰 —
> "주황 = 같은 평면상이 아닌것을 표현한것입니다. 그러니까 오류의 표현입니다.
>  같은 평면상에 그리도록 자동으로 유도 되어야 합니다."
> "이 것을 지웁니다 의미가 없습니다. 우리 엔진에서 처음 도형을 그리기 시작할때
>  같은 평면으로 그리도록 하면 됩니다."

**사용자 결재 (2026-06-02)**: **"승인합니다"** — (1) 주황 제거 + (2) 첫 도형이
작업평면을 정하고 이후 모두 같은 평면.

**Supersedes**: ADR-179 (on-face amber preview), ADR-182 (in-progress-only lock
scope).
**Direct precursors**: ADR-166/167/168 (plane lock + EPS_PLANE + drift snap),
ADR-186 (유도면 모델 — 본 ADR 의 *입력 쪽 짝*).

---

## 1. Problem statement

ADR-178 (LOCKED #77) 이 DrawRect 를 face-aware 로 만들고, ADR-179 가 면 위에
그릴 때 프리뷰를 **주황(amber)** 으로 표시했다. 의도는 "지금 입체면 위에
놓이고 있다" 는 *명확성* cue 였다.

그러나 두 개의 **coplanar** 도형(예: 바닥의 원 + 그 위를 지나는 사각형)을
그릴 때:

- `getDrawPlane` 의 `onFace` 는 커서 아래 *아무* 면이면 `true` →
  **같은 평면(coplanar)** 인 면과 **진짜 다른 평면**(박스 윗면 z=200)을 구분
  못 함.
- ADR-182 가 매 새 draw 의 첫 클릭에서 lock 을 풀고 커서 아래 면을 *재검출*
  → 사각형이 원의 face plane 으로 "튐". 그 면 plane 에 미세 drift 가 있으면
  사각형이 정확히 coplanar 가 *아니게* 놓임.
- 결과: (a) 의미 없는 주황 경고, (b) 비-coplanar drift → ADR-186 유도면 모델이
  lens 를 못 나눔(면분할 안됨).

사용자 통찰: 주황은 *깨짐의 증상*이다. 해법은 단순 — **처음 도형이 평면을
정하고, 이후 모두 같은 평면에 그리게** 하면 된다.

---

## 2. Decision

### Change 1 — 주황 on-face 프리뷰 제거 (Supersedes ADR-179)

`DrawRectTool.updatePreview` 의 `onFace ? amber : blue` 분기 삭제 → 단일 blue
프리뷰 (fill `0x4488ff` / line `0x2266dd`). 같은-평면 그리기에서는 "다른 면
위" 라는 상태 자체가 없으므로 amber cue 가 무의미.

### Change 2 — 첫 도형이 작업평면을 정하고, 이후 모두 같은 평면 (Supersedes ADR-182)

`getDrawPlane` 의 plane lock 을 **첫 클릭부터(idle 포함) 적용** + **draw 간
지속**:

- `getDrawPlane`: `if (this._planeLock && this.isToolBusy())` →
  `if (this._planeLock)`. lock 이 set 되어 있으면 새 draw 첫 클릭에도 적용.
- mousedown 핸들러의 ADR-182 "new-draw-start 물리적 unlock" 제거 → lock 이
  draw 간 지속.

효과: 첫 도형의 첫 클릭이 작업평면을 정하면(face hit 또는 default), 이후 모든
도형이 **그 같은 평면(exact lock plane, drift 없음)** 에 놓인다 → 정확히
coplanar → ADR-186 유도면 모델이 면을 나눈다.

**진짜 다른 평면**(박스 윗면 등)은 여전히 도달 가능 — face hit 의 normal 이
lock 과 다르면(cos|dot| < 0.9999, ADR-167 anti-parallel safe) **자동 unlock +
face hit** (사용자의 명시적 "이 다른 면에 그린다" 의도, LOCKED #67 amendment
보존). 명시 unlock(Ctrl+Shift+P / 뷰 전환 / sketch / Esc)도 평면을 바꾼다.

---

## 3. Lock-ins (L-188-1 ~ L-188-9)

- **L-188-1** 주황 on-face 프리뷰 제거 — 단일 blue (`0x4488ff`/`0x2266dd`).
- **L-188-2** plane lock 이 첫 클릭부터(idle 포함) 적용 (`getDrawPlane`,
  `isToolBusy` guard 제거).
- **L-188-3** plane lock 이 draw 간 지속 (ADR-182 mousedown new-draw unlock
  제거).
- **L-188-4** 진짜 다른 평면 face hit → 자동 unlock + face hit (LOCKED #67
  amendment 보존, 사용자 명시 전환).
- **L-188-5** 명시 unlock 경로 보존 (Ctrl+Shift+P / 뷰 전환 / sketch / Esc).
- **L-188-6** `getDrawPlane` SSOT (메타-원칙 #4) — 모든 그리기 도구
  (Rect/Circle/Line/Polygon/Arc/Bezier/Freehand) 자동 적용.
- **L-188-7** LOCKED #63(빈공간 z=0) / #67(plane lock) / ADR-166/167/168
  정합 보존.
- **L-188-8** ADR-186 유도면 모델의 입력 쪽 짝 — coplanar 입력 보장.
- **L-188-9** 절대 #[ignore] 금지.

---

## 4. Consequences

**사용자 facing**:
- 첫 도형이 평면을 정하면 이후 모든 도형이 같은 평면 → 정확히 coplanar.
- 주황 프리뷰 사라짐(파랑 단일).
- 다른 평면이 필요하면 그 면을 클릭(자동 전환) 또는 명시 unlock.

**Engine**: 변경 0 (TypeScript only).

**ADR-186 결합**: 입력(본 ADR, coplanar 보장) + 출력(ADR-186, 유도면 재유도)
= "면사라짐/면분할 안됨"을 양쪽에서 해소.

**Known limitation (minor)**: lock set 상태에서 커서가 다른 평면 면 위를
*hover* 하면(클릭 전) 자동 unlock 이 먼저 발화 → 🔒 badge 가 잠깐 사라짐.
다음 draw 의 first_click 이 re-lock 하고 sticky(ADR-164)가 빈 공간 fallback 을
유지하므로 self-heal. 필요 시 auto-unlock 을 click-only 로 좁히는 후속 refine
가능(별도 ADR).

---

## 5. Regression assets (절대 #[ignore] 금지)

`web/src/tools/ToolManagerRefactored.test.ts` —
`describe('ADR-188 same-plane lock from first shape')` (5 tests):
- `adr188_idle_drawtool_honors_lock_same_plane` — idle 첫 클릭도 lock(exact)
  사용, onFace=false (no orange)
- `adr188_idle_drawtool_different_plane_face_auto_unlocks` — 다른 평면 면 →
  자동 unlock + face hit
- `adr188_busy_drawtool_honors_lock` — 진행 중 lock 유지 (불변)
- `adr188_mousedown_idle_drawtool_persists_lock` — 새 draw 첫 클릭에 lock 지속
  (ADR-182 unlock 제거)
- `adr188_mousedown_nondraw_tool_keeps_lock` — 비-draw 도구도 lock 유지

`web/src/tools/DrawRectTool.test.ts` —
`describe('ADR-188 — orange on-face preview removed')` (4 tests):
- `adr188_no_orange_amber_constants` — `0xff8800`/`0xffaa33` 제거
- `adr188_no_onface_color_ternary` — `onFace ?` 분기 제거
- `adr188_single_blue_preview` — `0x4488ff` + `0x2266dd`
- `adr188_supersede_note_present` — ADR-188 traceability

검증: vitest 2097 passed + 1 skipped, tsc 0 errors (2026-06-02).

---

## 6. Cross-link

- ADR-178 (LOCKED #77) — DrawRect face-aware (첫 도형 plane 확립 시 보존)
- ADR-179 — on-face amber preview (**Superseded** by 본 ADR Change 1)
- ADR-182 — in-progress-only lock scope (**Superseded** by 본 ADR Change 2)
- ADR-166 (LOCKED #67) — plane lock (강한 same-plane 으로 복원)
- ADR-167 (LOCKED #68) — EPS_PLANE same-plane 판정 (cos|dot| 0.9999)
- ADR-168 (LOCKED #69) — face plane drift snap
- ADR-164 — sticky last drawn plane (빈 공간 fallback coexist)
- ADR-186 — 유도면 모델 (본 ADR 의 출력 쪽 짝)
- LOCKED #63 — z=0 invariant (빈 공간 default 보존)
- 메타-원칙 #4 (SSOT) / #5 (사용자 편의) / #10 (ADR 불변 — supersede)
- LOCKED #44 (Complete Meaning per Merge)
