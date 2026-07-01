# ADR-193 — Live Push/Pull (Direct Manipulation, no ghost)

> Replace Push/Pull's translucent **ghost** preview with **live real-geometry
> direct manipulation**: during the click-move-click, the real solid extrudes
> and slides as you move (Fusion / SketchUp style). Approach **B** (2-stage:
> extrude once + slide the top cap per move). Single planar face first; smooth
> groups keep the ghost (curved / multi-face is a follow-up).

- **Status**: Accepted
- **Date**: 2026-06-10
- **Track**: UI/UX (PushPullTool) + engine session API
- **Builds on**: ADR-191 (`exec_push_pull` transaction-aware), ADR-050 P-5e-γ
  (`replace_last_after_snapshot`), ADR-079 (`create_solid`), ADR-190 (P0.2
  snapshot-restore fallback), Move/Rotate/Scale live-tool pattern.

---

## Canonical anchor (사용자 결재, 2026-06-10)

> "푸시풀 click-move-click 방식으로 변경 ... 고스트방식이 아닌."

사전검토(4-서브시스템 병렬 audit + 브라우저 latency 실측):
- 현재 Push/Pull 은 *이미* click-move-click — 바뀌는 건 *이동 중 미리보기*
  (반투명 ghost → 실제 형상 라이브 변형).
- Move/Rotate/Scale 은 *이미* 실제 형상을 라이브로 변형 (translateFaces +
  syncMesh per move) → Push/Pull 을 live 로 바꾸면 정합.
- 엔진 op 은 sub-ms (병목 아님). 비용은 JS 렌더 sync (ADR-111/112). delta
  경로는 dead code (`mark_faces_dirty` 호출자 0) → Move/Rotate/Scale 도 사실
  full-sync per move. 본 ADR 도 동일 (approach B, 사용자 결재).

**결재**: ① 라이브 실형상 직접조작 + ② **B (2-stage, Move 도구와 동일 비용)**
+ box(Plane) 먼저. delta wiring(B+) / 곡면 / NURBS-side 는 별도 ADR.

---

## 1. Design — 2-stage live extrude

```
Phase 1 click  →  select face (no engine op yet; no ghost for single face)
first move     →  beginLiveExtrude(face, dist)   — REAL preview extrude (1 frame)
each move      →  updateLiveExtrude(target)       — slide top cap (pure vertex
                                                    translation, NO new frame)
Phase 2 click  →  commitLiveExtrude()             — roll back preview + ONE clean
                                                    re-extrude (single Undo,
                                                    correct surfaces)
ESC / tool sw  →  cancelLiveExtrude()             — restore pre-op snapshot
```

**왜 commit 시 clean re-extrude?** preview 의 per-move translate 는 인접 side
face surface 를 transient 하게 None 으로 drop (adr_060 all-or-none 규칙) — render
무해하지만 committed 결과로는 부정확. commit 시 `restore_scene_snapshot(before)`
+ `exec_create_solid(seed, final_dist)` 로 **깨끗한 단일 extrude** 를 재실행 →
모든 케이스 (box / cylinder / sweep) 의 surface 정확 + **단일 Undo frame**.
per-move 는 raw `Mesh::translate_faces` (transaction frame 0) 라 drag 중 frame
폭발 없음.

---

## 2. Engine session API (axia-core `Scene`)

신규 transient 필드 (snapshot 직렬화 제외): `live_extrude: Option<LiveExtrude
Session>`, `last_solid_top_face: Option<FaceId>`.

| 메서드 | 동작 |
|---|---|
| `begin_live_extrude(face, dist) -> Result<FaceId, String>` | before_snapshot 캡처 → `exec_create_solid` (preview, 1 frame) → top FaceId 회수 (`last_solid_top_face`, `SolidCreated` variant 무변경). 하드 에러 시 restore + Err. |
| `update_live_extrude(target) -> Result<(), String>` | `mesh.translate_faces([top], normal*(target-applied))` — frame 0. |
| `commit_live_extrude() -> Result<CommandResult, String>` | restore before + `discard_last_undo()` + clean `exec_create_solid(seed, applied)`. |
| `cancel_live_extrude() -> Result<(), String>` | restore before + `discard_last_undo()`. |
| `is_live_extrude_active() -> bool` | session 활성 여부. |

신규 `TransactionManager::discard_last_undo()` (additive) — preview frame 을
redo 로 밀지 않고 *완전 제거* (commit/cancel 의 rollback 정합).

WASM (axia-wasm): `beginLiveExtrude`(top FaceId or -1) / `updateLiveExtrude` /
`commitLiveExtrude` / `cancelLiveExtrude` / `isLiveExtrudeActive`. 각 호출
후 `mark_topology_changed` + `invalidate_cache` (approach B full sync).

---

## 3. Tool changes (PushPullTool)

- **단일 평면 면**: live (ghost 미생성). `liveActive` 플래그 — 첫 move 가
  threshold(0.5mm) 넘으면 `beginLiveExtrude`, 이후 move `updateLiveExtrude`,
  Phase 2 `commitLiveExtrude`, ESC/cleanup `cancelLiveExtrude`.
- **Smooth group**: 기존 ghost 보존 (곡면 / multi-face live 는 follow-up).
- **begin 실패** (legacy/mock build, 엔진 reject): ghost fallback (재시도
  스팸 방지 `liveBeginFailed`).
- `ppRayDist` (mouse→normal 거리) / 치수 라벨 / align-snap / Tab 반전 보존.
  Tab during live → cancel + 방향 반전 (다음 move 재-begin). VCB during live
  → `updateLiveExtrude(value)` + commit.
- `cleanup()` 가 미커밋 live session 을 cancel (ESC / tool 전환 / deactivate).

---

## 4. Lock-ins

- **L-193-1** 직접조작 = 실제 형상 라이브 변형 (ghost 아님). Move/Rotate/Scale
  정합.
- **L-193-2** Approach B (2-stage): begin extrude once + per-move translate +
  commit clean re-extrude. naive per-move re-extrude (A) 거부.
- **L-193-3** Commit = clean `exec_create_solid` (정확 surface) + **단일 Undo**
  (`discard_last_undo` + clean frame).
- **L-193-4** Per-move = raw `translate_faces` (transaction frame 0) — drag 중
  frame/snapshot 폭발 없음.
- **L-193-5** Single planar face only (MVP). Smooth group = ghost 보존.
  곡면(Cylinder/Sweep) live = follow-up (all-outer-verts 이동 surface 보존
  필요).
- **L-193-6** `CommandResult::SolidCreated` variant 무변경 (7 exhaustive match
  site 보존) — top FaceId 는 transient `last_solid_top_face` 로 회수.
- **L-193-7** Approach B full-sync per move (delta wiring = B+ 별도 ADR; Move/
  Rotate/Scale 과 동일 비용, 사용자 결재).
- **L-193-8** ADR-046 P31 #4 additive — public API + 단축키 + 메뉴 무변경.
- **L-193-9** 절대 #[ignore] 금지.

---

## 5. Acceptance Log

### 5.1 사전검토 + 결재 (2026-06-10)
- 4-서브시스템 병렬 audit (engine move API / syncMesh perf / tool patterns /
  transaction) + 브라우저 latency 실측 (엔진 op sub-ms, 비용 = 렌더 sync).
- 결재: 직접조작 + B + box 먼저.

### 5.2 구현 — 본 commit (LOCAL, adr-186/boundary-kernel-port)
- axia-transaction: `discard_last_undo()` + 회귀 1.
- axia-core: `LiveExtrudeSession` + 5 session 메서드 + `last_solid_top_face` /
  `live_extrude` 필드 + 회귀 4 (`adr193_live_extrude_box_manifold_single_undo` /
  `_cancel_restores_flat_face` / `_commit_matches_direct` / `_rejects_
  reentrancy_and_no_session`).
- axia-wasm: 5 exports (additive baseline OK).
- WasmBridge.ts: 5 wrappers (graceful `as any` fallback).
- PushPullTool.ts: live 경로 (단일 면) + ghost 보존 (smooth group) + 회귀 7
  (begin/update/commit/cancel/VCB/smooth-uses-ghost/begin-fail-fallback).

### 5.3 검증
- 워크스페이스 **2335 PASS / 0 failed / 1 ignored** (doctest doc-fence).
- vitest PushPullTool **24 PASS** (17 기존 + 7 ADR-193). tsc 0 errors.
- **브라우저 (rebuilt WASM, real engine)**:
  - begin → 6면 real box preview, manifold ✅
  - **live slide**: top cap Z = 40 → 150 → 300 (updateLiveExtrude 정확 추적,
    실형상 직접조작 증명) ✅
  - commit → 6면 manifold 0 violations ✅
  - **단일 Undo** → flat face (1면) 복원 ✅
  - cancel → flat (1면) + undo 가 rect draw 만 제거 (phantom extrude frame 0) ✅

> Demo 분담: vitest = 실제 tool 이 begin/update/commit/cancel/ESC/VCB 정확 호출
> (real `onMouseDown`/`onMouseMove`/`ppRayDist` 로직). 브라우저 = real bridge→
> engine live 거동. 합쳐서 full chain. (Real DOM-event → ToolManager routing 은
> 본 ADR 무변경 generic plumbing.)

---

## 6. Out of scope (follow-up)

- **B+ delta wiring** — dead `mark_faces_dirty` 활성 + position-only delta
  (translate→delta) → true cheap live + Move/Rotate/Scale 보너스. 별도 ADR.
- **곡면 live** — Cylinder / Sweep top face slide 시 all-outer-verts 이동으로
  surface metadata 보존 (현재 single planar 면만 live; 곡면은 ghost/legacy).
- **Smooth group live** — multi-face 곡면 그룹 live (현재 ghost 보존).
- **Offset 도구 live** — 동일 ghost→live 패턴 (현재 ghost).

---

## 7. Cross-link

- **ADR-191** `exec_push_pull` transaction-aware (own_transaction) + **LOCKED #79**
- **ADR-050 P-5e-γ** `replace_last_after_snapshot` (transaction collapse 답습)
- **ADR-190** P0.2 snapshot-restore fallback (commit clean re-extrude 답습)
- **ADR-079** `create_solid` (commit re-extrude entry)
- **ADR-111 / ADR-112** syncMesh perf (approach B full-sync 근거)
- **ADR-046 P31 #4** additive only / **ADR-087 K-ζ** 사용자 시연 게이트
- **메타-원칙 #5** (사용자 편의) / **#11** (Latency Budget) / **#13** (One
  Source, Two Views)
- Move/Rotate/Scale 도구 (live real-geometry 패턴 source)
