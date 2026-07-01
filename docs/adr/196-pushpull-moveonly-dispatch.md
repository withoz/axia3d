# ADR-196 — Push/Pull MoveOnly Dispatch (밀기/넣기 정석)

- **Status**: Accepted
- **Date**: 2026-06-11
- **Track**: 6 (Push/Pull)
- **Amends**: ADR-087 L4 (LOCKED #34) — "Push/Pull = create_solid Extrude only".
  L4 의 *user-facing surface* (createSolidExtrude / live, mesh pushPull WASM
  export 폐지) 는 **불변 보존**. 본 ADR 은 *internal dispatch* 만 정정 —
  솔리드 면은 `exec_push_pull` (MoveOnly), 평평 프로파일은 `create_solid`.
- **Cross-link**: ADR-079 (create_solid surface-native) / ADR-087 K-ε (legacy
  push_pull deprecate) / ADR-102 (cleave) / ADR-190 (Phase 0 / P0.2 fallback) /
  ADR-193 (Live Push/Pull) / 메타-원칙 #4 (SSOT) / #6 (Preventive).

## 1. Context — 사용자 보고

> "밀기와 넣기의 작동이 불안전합니다. 세밀하게 검토해주세요."

세밀 검토(audit + 깨끗한 브라우저 repro) 결과 — **솔리드의 *기존* 면을 밀거나
(밀기, outward) 넣으면(넣기, inward) 둘 다 비-manifold** 가 됩니다.

깨끗한 200³ 박스(6면 manifold)의 윗면 push 실측 (pre-fix):

| 동작 | 결과 | manifold |
|---|---|---|
| 윗면 밀기 (+100) | 11면 | ❌ 4 엣지 × 3면 |
| 윗면 넣기 (−100) | 비정상 | ❌ 엣지 × 6면 (벽 겹침) |
| 윗면 관통 넣기 (−250) | 비정상 | ❌ 다수 |
| (대조) 평평 사각형 → 박스 | 6면 | ✅ |

## 2. Root cause — ADR-087 K-ε 회귀

`create_solid` 은 **"프로파일 → 새 솔리드"** 연산입니다. `extrude_planar_box`
가 프로파일 면을 **보존**합니다 (create_solid.rs §310 "Profile face is
preserved"):

- **평평한 스케치 → 박스**: 보존된 프로파일이 박스 *바닥*(닫힘)이 됨 → 정확.
- **솔리드의 기존 면 → push**: 보존된 프로파일이 기존 내부와 새 압출 사이에
  끼임(sandwiched) → 경계 엣지마다 face-bearing HE 3개(기존 벽 + 끼인 면 +
  새 벽) → **비-manifold**. 넣기는 새 벽이 기존 내부 벽과 겹쳐(coincident)
  더 심각.

레거시 `Mesh::push_pull` 은 **MoveOnly** 모드(연결 엣지가 노멀과 평행 = 솔리드
면 → 정점만 이동, 벽이 늘어남/줄어듦)로 이걸 올바르게 처리했고 *지금도 엔진에
살아있습니다*(`push_pull.rs:693`, 테스트 `push_pull.rs:978`). **ADR-087 K-ε
("Push/Pull = create_solid Extrude only")가 single-face push 를 전부
create_solid 로 라우팅하며 MoveOnly dispatch 가 누락**된 것이 회귀의 본질.
(PushPullTool 은 `isFaceInVolume` 을 *normal 뒤집기에만* 쓰고 routing 엔 안 씀.)

## 3. Decision — 정석 = scene-level MoveOnly dispatch (A)

사용자 결재 ("어떤 것이 정석인가?" → A) + 범위 ("3건 모두").

**`is_move_only`(연결 엣지가 노멀과 평행 = 솔리드 면 판별, 이미 엔진에 있는
SSOT 토폴로지 테스트 — `Mesh::push_pull` 이 내부적으로 쓰는 동일 체크)로
`exec_create_solid` 에서 dispatch 복원:**

```
is_move_only(face) ? exec_push_pull(MoveOnly 확장/축소)   // 솔리드 면
                   : create_solid(surface-native 새 솔리드) // 평평 프로파일
```

### 왜 A (scene-level) 가 정석인가
- **메타-원칙 #4 (SSOT)** — dispatch 는 mesh 토폴로지가 필요하고, *모든* push
  호출자(VCB·라이브·MCP·스크립트)가 `exec_create_solid` 한 곳으로 모임.
  Tool-level(B)은 호출자마다 dispatch 복제 + deprecated push_pull WASM
  되살리기 → SSOT 위반.
- **계약 무결성** — `create_solid` = "프로파일로 새 솔리드 생성"(ADR-079).
  거기에 "기존 솔리드 수정"을 끼우는 C는 계약을 흐림 + 큰 surgery.
- **기존 올바른 primitive 재사용** — `exec_push_pull`(MoveOnly)이 이미 있고
  테스트됨. Extrude 한정(Revolve/Sweep/Loft 는 fallback_dist=None → skip).
  Live Push/Pull(begin/commit_live_extrude → exec_create_solid)이 dispatch
  자동 상속.

### 부가 fix 2건 (audit, "3건 모두")
- **Fix 2 (major) — Q3 fallback undo 누수**: 곡면/Arc/실린더/sweep/NURBS
  프로파일 push 후 Undo 1회가 ADR-102 cleave 를 남김. fallback 이
  `transactions.cancel()`(outer frame 폐기) 후 inner `exec_push_pull` 이
  cleave *후* snapshot 을 재캡처해서 — 수정: outer transaction 을 *유지*,
  mesh 만 restore, exec_push_pull 을 recording 중 실행(own_transaction=false →
  outer frame 의 original before_snapshot 재사용) 후 set_after+commit.
- **Fix 3 (info) — update_live_extrude degenerate 가드**: 라이브 드래그를
  0 거리로 되돌리면 순간 0-높이 프리뷰. target.abs() < 1e-6 → no-op(begin 의
  1e-6 floor mirror). commit 은 이미 안전(create_solid EPSILON_LENGTH 거부).

## 4. Lock-ins

- **L-196-1** `is_move_only` = SSOT dispatch key (메타-원칙 #4). 솔리드 면 →
  exec_push_pull(MoveOnly), 평평 프로파일 → create_solid.
- **L-196-2** Extrude 한정 dispatch — Revolve/Sweep/Loft 무영향(profile→solid).
- **L-196-3** create_solid 계약 불변 (ADR-079) — 평평 프로파일 → 새 솔리드.
- **L-196-4** mesh pushPull WASM export 폐지 불변(ADR-087 K-ζ) — *internal*
  exec_push_pull 만 재engage, user-facing surface(createSolidExtrude/live)
  무변경 (ADR-046 P31 #4 additive only).
- **L-196-5** Multi-loop ring(P1.2 ADR-191) 처리는 dispatch *앞* — 무영향.
- **L-196-6** Fix 2 — outer transaction 유지로 단일 Undo = exact restore.
- **L-196-7** Fix 3 — degenerate 프리뷰 가드(commit 은 독립적으로 안전).
- **L-196-8** 절대 #[ignore] 금지.
- **L-196-9 (known minor, out-of-scope)** — exec_push_pull 의 legacy *Xia*-
  owned MoveOnly 경로는 `xia.face_ids.push(top_face)` 로 face_id 를 중복
  추가할 수 있음(default Shape 경로는 owning_xia_id=None 으로 미발동). 별도
  cleanup.

## 5. Verification

### 회귀 +5 (axia-core, 절대 #[ignore] 금지)
- `adr196_box_top_outward_push_is_moveonly_manifold` — 밀기 → 6면 MoveOnly
  manifold, 박스 z200→300 확장.
- `adr196_box_top_inward_push_is_moveonly_manifold` — 넣기 → 6면 manifold,
  z200→100 축소.
- `adr196_flat_profile_not_moveonly_uses_create_solid` — 대조: 평평 면 NOT
  move_only → create_solid 6면.
- `adr196_q3_fallback_single_undo_restores_pre_push` — Fix 2: arc 반원 push →
  Undo 1회 → active vert/edge count 정확 복원(cleave 누수 0).
- `adr196_update_live_extrude_clamps_degenerate` — Fix 3: update(0) no-op,
  6면 manifold 보존.

워크스페이스: axia-core **359** + axia-geo **1709** PASS, 0 failed.

### 브라우저 시연 (ADR-087 K-ζ canonical, rebuilt WASM)
- 깨끗한 박스 윗면 **밀기(+100)** → 6면/12엣지/12정점 MoveOnly, manifold 0
  violations (pre-fix: 11면 + 4 비-manifold).
- 윗면 **넣기(−100)** → 6면 manifold, 박스 z200→100 축소 (pre-fix: 비-manifold).

## 6. Out of scope (follow-up, 별도 결재)

- ~~넣기 over-push (관통)~~ → **Amendment 1 (§8) 로 해소** (clamp).
- **L-196-9 Xia MoveOnly 중복 face_id** cleanup.
- **곡면(Cylinder/Sphere) 면 push 의 MoveOnly 정합** — is_move_only 가
  곡면 cap 을 어떻게 분류하는지 별도 검증.
- **다른 솔리드 관통 carve** — 눌린 면이 *다른* 솔리드를 침투할 때 →
  Phase 2 (ADR-194 drill). 본 ADR 의 clamp 는 *자기-솔리드* 한정.

## 8. Amendment 1 — Inward over-push clamp (2026-06-11)

**사용자 결재**: over-push 처리 = **A (clamp)** (자기-솔리드 바닥 통과 →
두께에서 stick, carve 는 Phase 2 별개).

**finding**: §6 의 넣기 over-push 가 실제 inversion — 200-tall 박스 윗면 −250
push → 윗면 z=−50(바닥 통과) → inside-out, 비-manifold ("cached normal opposite
to winding dot=−1.0" × 4). `push_pull_move_only`(push_pull.rs:720)가 정점을
`normal*dist` 만큼 무조건 이동, 두께 가드 0.

**fix (clamp)**: 안쪽 push(dist<0)를 솔리드 두께에서 clamp.
- `move_only_max_inward(mesh, face)` 신규(push_pull.rs) — 연결 벽(∥ normal,
  is_move_only 의 wall 탐지 미러)의 min projected length = 국소 두께. flat 면
  → None.
- `push_pull_move_only`: `dist < 0` 시 `dist.max(-(thickness − MIN_SOLID_
  THICKNESS))` 로 floor. commit/direct 경로 커버.
- `update_live_extrude`: `LiveExtrudeSession.max_inward`(begin 때 원본 두께
  저장)로 라이브 드래그 slide 도 clamp.
- `MIN_SOLID_THICKNESS = 1e-3 mm` (1μm) — 0.15μm spatial-hash dedup(LOCKED #5)
  위, mm CAD scale 에서 invisible. EPSILON_LENGTH(1e-6) 은 dedup 아래라
  coincident vert merge 위험 → 부적합.

**L-196-10** — 안쪽 MoveOnly push 는 두께(min 벽길이) − MIN_SOLID_THICKNESS
에서 clamp. 바깥 push 무제한. flat 프로파일(max_inward None) 은 unclamped
(commit create_solid 가 degenerate 거부). 라이브 드래그도 동일 clamp.

**회귀 +5**: axia-geo `move_only_max_inward_returns_thickness` +
`move_only_inward_overpush_clamps_no_invert`; axia-core `adr196_inward_overpush_
clamps_no_invert` + `adr196_outward_push_not_clamped` (대조) + `adr196_live_drag_
overpush_clamps`. 워크스페이스 axia-core **362** + axia-geo **1711**, 0 failed.

**브라우저 (rebuilt WASM)**: 박스 윗면 −250 → 6면 manifold 0 violations, 윗면이
바닥(z=0.001 = MIN_SOLID_THICKNESS)에 stick, 뒤집힘 0.

**여전히 follow-up**: flat-profile 라이브 드래그 over-shrink(rare, commit 안전) /
다른-솔리드 관통 carve(Phase 2).

## 7. Lessons

- **L1** 사용자 "불안전" 보고 → 깨끗한 격리 repro 가 핵심 (오염된 scene 의
  첫 repro 는 가짜 신호 11337z; reset 메서드 부재로 누적 → fresh reload 당
  1 테스트로 격리해야 진짜 신호).
- **L2** ADR-087 같은 "X only" 정책은 *대체된 연산의 모든 케이스*를 X 가
  커버하는지 검증 필수 — create_solid 가 CreateFace 만 커버하고 MoveOnly 를
  빠뜨림.
- **L3** 정석 = SSOT chokepoint 에 dispatch (메타-원칙 #4) — tool/caller 복제(B)
  나 단일 함수 계약 오염(C) 회피.
- **L4** byte-identical snapshot 검증은 HashMap 직렬화 순서(프로세스 시드)에
  취약 — 격리 실행 통과/full run 실패. order-independent 신호(active vert/edge
  count)로 대체.
