# ADR-262 — Wall Opening: Door (floor-reaching notch) + Parametric Sill/W×H

- **Status**: Accepted (α + β-1 + β-2 + β-3 + γ closure 2026-06-26 — 문 cut
  end-to-end 라이브 검증 PASS. 수치 파라메트릭 sill/W×H 는 **β-3b deferred**)
- **Date**: 2026-06-26
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL, push 금지)
- **Track**: 6 (Extrude/Cut/Punch) — "완벽한 extrude" 로드맵 **#4 (벽 개구부)**
- **사용자 결재 (2026-06-26)**:
  - **Q1 = 문 + 수치 파라메트릭** (door notch + sill 높이 / 개구부 W×H)
  - **Q2 = split-face U-chain + push-pull cut** (Boolean 아님, drill-notch 아님 —
    벽 면을 U자로 분할 후 관통)
  - **Q3 = DrawWindowTool 확장** (바닥 스냅 자동 문 — 단일 "개구부" 도구; 안쪽=창,
    바닥 도달=문; AixiAcad `sill=0 → 문` 판정 답습)
- **De-risk (인라인 audit)**: 창(닫힌 rect 관통)은 이미 `DrawWindowTool` →
  `drillRectThroughHole` 로 작동. **진짜 gap = 문(floor-reaching)** — 모든 기존
  through-cut (`punch_*_hole` / `drill_*_through_hole` / `carve_through_from_
  source_face`) 이 개구부가 면 경계 *안쪽* 에 완전히 있어야 함 (`point_in` boundary
  거부) → 바닥에 닿는 문(U자 notch)은 거부. AixiAcad `add_window_wall(OpeningWall
  Params{sill_height,opening_w,opening_h})`, `sill=0 → 문`.
- **Cross-link**: ADR-249 (drill_rect_through_hole — 구조 미러 source) · ADR-252
  (carve_through_from_source_face) · ADR-194 (punch_rect_hole) · ADR-079/087
  (DrawWallTool 벽) · face_split.rs split_face_by_chain · ADR-007 (manifold/winding) ·
  ADR-190 P0.2 (snapshot rollback) · ADR-102 · 메타-원칙 #4/#5/#6/#16 ·
  LOCKED #43 (Z-up) #44 (Complete Meaning per Merge) · ADR-259/260/261 (#1~#3 자매)

---

## 1. 문제

벽 개구부의 **창(window)** 은 이미 완성 (`DrawWindowTool` → `drillRectThrough
Hole`: entry punch + exit punch + tube bridge, 재질 보존). **문(door)** 만 gap:
문은 바닥 모서리에 닿는 개구부 = **닫힌 ring(hole) 이 아닌 U자 notch** (바닥 열림).
기존 `punch_*_hole` 은 개구부가 면 경계 안쪽에 완전히 있을 것을 강제 → 문 거부.

**문 vs 창의 유일한 기하 차이**: 개구부가 벽 면의 **바닥 모서리에 닿는가**
(AixiAcad `sill_height == 0`). 창 = 닫힌 rect (4 inner 모서리), 문 = U (좌/상/우
3 모서리 + 바닥은 벽 경계).

## 2. 핵심 기하 — 문 notch (Q2 split + cut)

박스 벽 (front F / back B / bottom Bot / top / left / right). 문 rect (a, b) on
F, 바닥 모서리 = F∩Bot. 좌우 [x0,x1], 수직 [floor, header].

**문 cut (drill_rect_through_hole 구조 미러, U-loop 변형)**:
1. −normal ray 로 opposite(back) 벽 + depth(thickness) 측정 (drill 답습).
2. **F split** by U-chain `BL→TL→TR→BR` (BL/BR 은 F 바닥 모서리 위 — `split_edge`
   로 vertex 생성, `split_face_by_chain`) → 문 region 분리 (F → U-shape 잔여 +
   문 sub-face). 문 sub-face remove → F 가 U-notch.
3. **B split** 동일 (projected `−n·depth`) → B U-notch.
4. **Bot notch** — 바닥 면의 문-width × thickness rect 제거 (문 바닥 = 열림/floor).
5. **3-jamb bridge** — F-opening 의 좌/상/우 모서리 ↔ B-opening 대응 모서리 quad
   3개 (좌 jamb / header / 우 jamb). 바닥은 bridge 안 함 (열림). manifold 검증.

**창 (sill > 0)** = 기존 `drill_rect_through_hole` (닫힌 ring, 변경 없음).

## 3. 설계

### 3.1 Engine — `cut_wall_door_opening(wall_face, corner_a, corner_b, normal)`

- `drill_rect_through_hole` 미러 (carve.rs): opposite 벽 측정 + entry/exit split
  (punch 아닌 U-chain split) + bottom notch + 3-jamb bridge (`bridge_through_
  loops` 의 U/open-bottom 변형 또는 전용 3-quad bridge).
- door 판정 = corner_a/corner_b 의 바닥 모서리가 `wall_face` 의 outer boundary
  edge 위 (ε 이내). 안쪽이면 `NotYetSupported` (창은 drill_rect 경로).
- 재질 보존 (jamb/notch 면이 벽 재질 상속, 기존 punch/drill 답습).
- ADR-190 P0.2 — 실패 시 caller snapshot rollback (byte-identical). 메타-원칙 #6.

### 3.2 WASM + bridge

- WASM `cut_wall_door_opening(face_id, ax,ay,az, bx,by,bz, nx,ny,nz) -> i32`
  (tube/jamb quad count, ≤0 = 실패).
- bridge `cutWallDoorOpening(faceId, a[3], b[3], n[3]): number` (graceful + Toast).

### 3.3 UX — DrawWindowTool 확장 (Q3, 단일 "개구부" 도구)

- 2-클릭: bottom corner 가 벽 면의 **바닥 모서리에 스냅** 되면 → 자동 **문**
  (`cutWallDoorOpening`); 안쪽이면 → **창** (기존 `drillRectThroughHole`).
- **수치 파라메트릭 (Q1)**: VCB/다이얼로그 — `sill,W,H` (sill=0 → 문, >0 → 창;
  W×H = 개구부). AixiAcad `OpeningWallParams` 답습. v1 은 free-click + 바닥 스냅
  우선, 수치 입력은 β-3 에서 (sill/W/H 파싱).
- 라벨/Toast: "관통 창" / "문(door)" 구분.

### 3.4 D5 / 정합

- 명시 op (메타-원칙 #16 — 자동 trigger 아님). 실패 → snapshot rollback (기존 drill
  답습, ADR-190 P0.2). 기존 창/원/폴리곤 through 경로 불변 (additive).

## 4. Lock-ins (β 구현 시 강제)

- **L-262-1** 문 = U-notch (바닥 열림), 창 = 닫힌 ring (변경 없음). 판정 = 바닥
  모서리 접촉.
- **L-262-2** Q2 split + cut — `split_face_by_chain` (front+back) + bottom notch
  + 3-jamb bridge (`drill_rect_through_hole` 구조 미러).
- **L-262-3** 재질 보존 (jamb/notch 면 벽 재질 상속).
- **L-262-4** 명시 op + 실패 시 byte-identical rollback (ADR-190 P0.2, 메타-원칙 #6).
- **L-262-5** Q3 — DrawWindowTool 확장 (바닥 스냅 자동 문 / 안쪽 창). 단일 개구부 도구.
- **L-262-6** Q1 — 수치 파라메트릭 (sill,W,H; sill=0 문 / >0 창). AixiAcad parity.
- **L-262-7** 기존 창/원/폴리곤 through 불변 (additive, ADR-046 P31 #4).
- **L-262-8** manifold (ADR-007 verify_face_invariants) + 절대 #[ignore] 금지.
- **L-262-9** v1 = 박스 벽 (DrawWallTool 출력) straight-through axis-aligned 문.
  곡면 벽 / 경사 / 다중 개구부 = future.

## 5. 시뮬레이션 게이트 (β-1, 먼저 시뮬 — ADR-259/260/261 답습)

`adr262_sim_*` Rust 테스트 (구현 후 라이브 전):
- 박스 벽 + 문(바닥 도달) → manifold valid, front/back U-notch + 3 jamb + bottom
  notch, 4면(F-U/B-U/...) 검증.
- 문 height/width 정확 (opening 치수).
- 창(안쪽 rect) → 기존 drill_rect 경로 (변경 없음, regression guard).
- 바닥 미접촉 rect 를 door fn 에 → `NotYetSupported` (창 경로로).
- degenerate / opposite 벽 없음 → reject + rollback.
- 재질: jamb/notch 면 벽 재질 상속.

## 6. Out of scope (별도 ADR / future)

- 곡면 벽 / 경사 벽 개구부.
- 다중 개구부 일괄 / 파라메트릭 벽+개구부 일괄 (AixiAcad add_window_wall 통합).
- 문틀/문짝 (door leaf) / 창틀 (frame geometry) — 개구부만, 부재 아님.
- IFC IfcDoor/IfcWindow export (sill 비율 판정 — ADR-057 IFC 트랙).
- #5 곡면 완벽화 / #6 separated-disk.

## 7. Acceptance Log

- **2026-06-26 α** (`5a3d2f6`, docs-only) — 본 spec + 결재 (Q1 문+수치 / Q2
  split+cut / Q3 DrawWindowTool 확장). De-risk 인라인 audit: 창 이미 작동, 문
  (floor-reaching notch) 이 진짜 gap. AixiAcad `add_window_wall` sill=0 문 판정
  parity.
- **2026-06-26 β-1** (`fadd4f4`, axia-geo) — **문 notch 엔진 + 먼저-시뮬**.
  `carve.rs`:
  - `pub struct DoorOpeningResult { front_face, back_face, jamb_faces: Vec<FaceId> }`
  - `fn find_door_host(center, n) -> Option<FaceId>` — host-find (normal∥n
    |dot|>0.999 + coplanar 1μm + point_in even-odd). **α spec 대비 refinement
    #1**: `face_id` 인자 제거 — host 를 내부에서 신선하게 찾음 (no stale id,
    `punch_rect_hole` 답습).
  - `fn notch_wall_face_for_door(face, bl,br,tl,tr, mat) -> Result<(VertId×4)>`
    — F/B 공유: bottom 모서리 split + U-chain (`add_edge`) + `split_face_by_chain`
    (BL→TL→TR→BR) + 문 rect remove.
  - `pub fn cut_wall_door_opening(corner_a, corner_b, normal) -> Result<Door
    OpeningResult>` — 가드 (n.z>0.1 수직-only reject / 상대 게이트 door-vs-window
    / degenerate height / `carve_ray_nearest_face` opposite 벽) → F notch → B
    notch (projected −n·depth) → Bot notch (2× `split_face_by_chain` → 문 strip
    remove) → 3-jamb `add_face`. **jamb winding `[front_bot,back_bot,back_top,
    front_top]`** (manifold-correct, 첫-시도 검증).
  - **먼저-시뮬 발견**: `create_box(w,h,d)` 좌표 = **w→X, h→Z, d→Y** (벽 =
    `create_box(length, height, thickness)`; box200 은 정육면체라 숨어 있던 매핑,
    topology dump 로 확인). 시뮬 (`adr262_sim_door_notch_full_manifold`) 이
    문 notch = **watertight closed manifold 10면, 0 violations** 임을 *구현 전*
    검증.
  - 회귀 +5: `adr262_sim_door_notch_full_manifold` / `adr262_door_box_wall_
    manifold` (10면 + 3 jamb + valid) / `adr262_door_window_bottom_rejected` /
    `adr262_door_degenerate_normal_rejected` / `adr262_door_horizontal_face_
    rejected`. 0 regression.
- **2026-06-26 β-2** (`a2fb998`, axia-wasm + bridge) — **WASM + snapshot rollback**.
  - WASM `cutWallDoorOpening(ax,ay,az, bx,by,bz, nx,ny,nz) -> i32` (jamb count,
    ≤0 = 실패). **커널 self-rollback 없음** (multi-step mutation) → wrapper 가
    `scene_snapshot()` + `begin/set_before` → `Ok` 시 `set_after + commit +
    mark_topology_changed`, `Err` 시 `restore_scene_snapshot(&before) + cancel +
    set_error + -1` (ADR-190 P0.2, `drillRectThroughHole` 미러).
  - bridge `cutWallDoorOpening(cornerA, cornerB, normal): number` (graceful -1 +
    try/catch).
  - 회귀: step6 `adr262_beta2_door_export_with_rollback` (export + 라우팅 +
    snapshot guard 검증, source-scan) → step6 71 PASS. WasmBridge vitest +4
    (forward / endpoint-missing -1 / not-ready -1 / throw -1).
- **2026-06-26 β-3** (`84d3b48`, DrawWindowTool) — **도구 라우팅 + 자동 문/창**.
  - `const DOOR_FLOOR_FRACTION = 0.15`. **α spec 대비 refinement #2**: door
    판정 = "바닥 모서리 정확 접촉" → **상대 게이트** (개구부 바닥이 벽 높이의
    하위 15% → 문, 바닥 스냅; 위 → 창). 단위 무관, free-click UX 견고 (정확
    boundary 접촉 강제 안 함).
  - `commitWindow`: `cutWallDoorOpening` **먼저** 시도 (jambs>0 → 문, Toast
    "문(door)을 냈습니다") → else `drillRectThroughHole` (관통 창) → else
    `punchRectHole` (면 창). 창 개구부는 door fn 이 -1 (게이트 거부) + mesh 무손상
    (β-2 snapshot) → 깨끗한 fallback.
  - 회귀: DrawWindowTool.test +5 (문 먼저 / 관통창 fallback / 면창 fallback /
    순서 / off-face refuse). `applyVCBValue` 여전히 no-op (**수치 β-3b deferred**).
- **2026-06-26 γ** (라이브 검증, 코드 변경 0 — `npm run build:wasm` + preview_eval;
  WASM artifact gitignored) — **실앱 + 새 WASM, 3/3 PASS, console 에러 0**:
  - **문** (바닥 도달): `create_box(2000,2500,200)` 벽 (X=length / Z=height /
    Y=thick, 바닥 z=−1250) → `cutWallDoorOpening([-300,-100,-1250],[300,-100,850],
    [0,-1,0])` → **jambs=3, fc 6→10, verifyInvariants valid v=0** (watertight).
  - **창 회귀**: `drillRectThroughHole` (안쪽 z=−300~600) → **tube=4, fc→10,
    valid v=0** (기존 경로 무손상).
  - **문-거부-창** (높은 개구부 z=−300 = 38%): `cutWallDoorOpening` → **jambs=−1
    + byteIdentical** (fc 16→16, β-2 snapshot 롤백) + valid. 사용자 #1 "면 안
    깨짐" 확정 (성공=manifold valid / reject=byte-identical).
- **2026-06-26 δ** (본 commit, docs-only) — Status Proposed→Accepted + 본
  Acceptance Log + §8 Lessons + README Status + CLAUDE.md LOCKED #86. 문(핵심
  gap) end-to-end 완료; 수치 파라메트릭 (sill/W×H, Q1) 은 **β-3b deferred** 명시.

## 8. Lessons (canonical for future cut/notch ADRs)

- **L1 먼저-시뮬이 HE wiring 위험을 구현 전 종결** (ADR-259/260/261 답습). 문 notch
  의 careful HE wiring (F/B U-chain split + bottom notch + 3-jamb bridge) 을
  `adr262_sim_door_notch_full_manifold` 가 *generic 커널 추출 전* watertight
  manifold (10면/0 violations) 로 검증 → jamb winding `[front_bot, back_bot,
  back_top, front_top]` 첫-시도 정합. 메타-원칙 #6 (Preventive over Curative).
- **L2 좌표 매핑은 비대칭 fixture 로 노출** — `create_box(w,h,d)` = w→X/h→Z/d→Y 는
  정육면체 fixture (box200) 에서 숨어 있었음. 비대칭 벽 (`create_box(2000,2500,
  200)`) + topology dump 가 매핑을 강제 노출. ADR-103 §L2 (비대칭 fixture) 답습.
- **L3 커널 no-self-rollback → WASM layer 가 snapshot SSOT** — multi-step mutation
  커널 (`cut_wall_door_opening` 의 split×2 + notch + bridge×3) 은 부분 실패 시
  자체 복원 안 함. WASM wrapper 가 `scene_snapshot` + `restore_scene_snapshot`
  으로 byte-identical 롤백 보장 (ADR-190 P0.2). 향후 모든 multi-step 커널
  cut/notch 는 WASM snapshot guard 의무.
- **L4 상대 게이트 > 정확 boundary 접촉** (refinement) — door 판정을 "바닥 모서리
  정확 접촉" 대신 "벽 높이의 하위 15%" 상대 게이트로 → free-click UX 견고 + 단위
  무관. 정확 boundary 접촉 강제는 사용자가 픽셀-완벽하게 바닥을 찍어야 → 부적합.
- **L5 host-find 가 stale id 제거** (refinement) — `cut_wall_door_opening` 이
  `face_id` 인자 대신 `find_door_host(center, n)` 로 host 를 신선하게 찾음
  (`punch_rect_hole` 답습). topology 변경 후 stale face id 위험 0.
- **L6 도구 라우팅의 graceful fall-through** — `cutWallDoorOpening` (문) →
  `drillRectThroughHole` (관통창) → `punchRectHole` (면창) 3-단 fall-through.
  각 단계가 ≤0 + mesh 무손상 (snapshot 롤백) 으로 거부 → 다음 단계는 깨끗한
  mesh 에서 시도. 단일 "개구부" 도구가 문/관통창/면창 자동 판정 (Q3).
