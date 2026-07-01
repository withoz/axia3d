# ADR-252 — Pocket Carve from a Profile Drawn on a Wall ("cut 안됨" fix)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: Push/Pull (Extrude/Cut) bug fix — "draw rect on a face → push in →
  pocket" (SketchUp parity). 사용자 시연 결함 "컷이 안됨".
- **Depends on**: ADR-249 (`bridge_through_loops` + `drill_extract_new_hole_loop`
  + `punch_polygon_hole` — reused) / ADR-196 (Push/Pull MoveOnly dispatch) /
  ADR-190 P0.2 (snapshot rollback) / ADR-007 (manifold) / ADR-246 (Extrude/Cut)

## 1. Context — bug diagnosis

사용자 시연: 솔리드 박스 앞면에 작은 사각을 그리고 Push/Pull(Extrude/Cut)로 안쪽
cut 을 시도 → **안 됨**. 실제 엔진 재현 (empirical, 9-probe) 으로 root cause 확정:

- 솔리드 면에 그린 *contained* 사각은 host 면을 **분할하지 않고** 별개 coplanar
  sheet 로만 겹침 (auto-intersect ADR-101 은 partial overlap 만 분할; containment 는
  ADR-015 B1-disabled). 앞면(area 240000)은 그대로, 사각(18000)은 별개 face.
- Push/Pull 로 사각을 안쪽 push → 사각이 **독립 작은 박스로 extrude** (4 새 벽),
  벽은 그대로 = pocket/cut 아님.

→ SketchUp 의 "면에 그리고 밀어서 pocket" 워크플로우에는 사각이 host 면의 sub-
region 으로 통합되거나, push 가 pocket carve 로 dispatch 돼야 함. 후자 채택 (Path 1,
de-risk 결재 — LOCKED draw 정책 무변경, drill 인프라 재사용).

## 2. de-risk 시뮬레이션 (2 경로)

scratch 실측: (1) **Path 1 blind pocket** (punch entry + floor cap + bridge walls)
= drill_rect 의 exit-punch→floor-cap 교체, **manifold 첫 시도 성공**. (2) Path 2
containment split = ADR-015/139 LOCKED 변경 + CreateFace push, 더 침습적. **Path 1
채택** (Push/Pull 자동 dispatch).

## 3. Decision

**Engine** (`operations/carve.rs`):
- `carve_pocket_from_source_face(source_face, depth) -> PocketResult` — profile
  sheet 를 consume + host 벽을 ring-with-hole 로 punch + `depth` inward floor +
  bridge walls + floor cap. (1) outline/normal 읽기 → (2) sheet(face+edges) 제거
  (punch host-search 가 벽을 hit 하도록) → (3) `punch_polygon_hole` 로 벽 punch →
  (4) depth guard (carve ray, opposite wall 도달 시 bail → through-hole) → (5) floor
  verts (inward) → (6) walls (ADR-249 reverse+align+quad) → (7) floor cap → (8)
  manifold guard. rect/polygon 공통 (loop-agnostic).
- `face_has_larger_coplanar_container(face) -> bool` — read-only "더 큰 coplanar
  면 안에 contained" 탐지 (sheet-on-wall signal; `isFaceInVolume` 은 coplanar sheet
  를 신뢰성 있게 구분 못 해 별도 geometric query).

**UI** (`PushPullTool`):
- Phase 1: `isSheetSource = bridge.faceHasLargerCoplanarContainer(face)` (pocket
  candidate). live preview 억제 (commit 이 dispatch).
- Phase 2 commit: `isSheetSource && dist < 0` (inward) → `carvePocketFromSourceFace
  (face, -dist)`. walls>0 → pocket. 아니면 normal extrude path (boss/MoveOnly).
- WASM `carvePocketFromSourceFace` (snapshot+rollback) + `faceHasLargerCoplanar
  Container` + bridge wrappers (graceful).

## 4. Lock-ins (L-252-1 ~ L-252-9)

- **L-252-1** Path 1 (punch entry + floor cap + walls) — drill_rect 의 exit→floor
  교체. ADR-249 `bridge_through_loops` 패턴 + `punch_polygon_hole` 재사용.
- **L-252-2** Source sheet consume — 제거 후 벽 punch (host-search 가 sheet 대신
  벽 hit). sheet 가 smallest coplanar 라 안 그러면 host 오선택.
- **L-252-3** `face_has_larger_coplanar_container` 가 dispatch signal (isFaceInVolume
  아님 — coplanar sheet 가 in-volume=true 로 잘못 분류됨, 실측).
- **L-252-4** Inward(dist<0) + pocket candidate 만 carve. outward = boss/extrude
  (기존). solid 면 = candidate 아님 → MoveOnly(ADR-196) 경로 무변경.
- **L-252-5** Depth guard — opposite wall 도달 시 bail (through-hole 은 Window/
  PolygonHole 도구 = ADR-249/250 drill).
- **L-252-6** rect/polygon 공통 (profile loop-agnostic).
- **L-252-7** snapshot rollback (ADR-190 P0.2) + manifold guard (ADR-007).
- **L-252-8** LOCKED draw 정책 (ADR-015/139 auto-intersect/containment) 무변경.
- **L-252-9** 절대 #[ignore] 금지.

## 5. 회귀 / 검증

- **axia-geo** carve `adr252_*` 4 (pocket_rect_from_source_sheet_manifold 4 walls
  watertight / pocket_triangle 3 walls / pocket_depth_through_wall_errors /
  larger_coplanar_container_detects_sheet_vs_wall) + carve **30** tests. axia-geo
  lib **2012** (2008 → +4).
- **axia-wasm** carvePocketFromSourceFace + faceHasLargerCoplanarContainer exports
  (baseline additive) — **64**.
- **vitest** 161 files **2414 passed** (PushPullTool mock 방어 `?.` 적용), tsc 0.
- **브라우저 e2e** (rebuilt WASM, ADR-087 K-ζ): 벽 박스 + 앞면 사각 sheet →
  faceHasLargerCoplanarContainer(sheet)=true / (box wall)=false → carvePocket
  FromSourceFace(sheet, 50) → 4 walls, faces 7→11, invariants valid 0 violations,
  console clean. = "push rect inward → pocket" (새 박스 아님).

## 6. Lessons

- **L1 empirical root-cause** — "cut 안됨" 을 추론 아닌 실제 엔진 재현으로 진단
  (rect=별개 sheet → push=새 박스). 시뮬레이션이 fix substrate (Path 1) 확정.
- **L2 신뢰 가능 signal** — `isFaceInVolume` 가 coplanar sheet 를 in-volume=true 로
  분류 (실측). geometric query (larger coplanar container) 가 정확한 sheet-on-wall
  탐지. UI dispatch 는 신뢰 signal 필수.
- **L3 drill 인프라 복리** — ADR-249/250 의 punch_polygon_hole + bridge 패턴이
  pocket 에 무수정 재사용 (floor cap 만 추가). de-risk + Pattern-12 복리.
- **L4 non-disruptive dispatch** — pocket candidate (larger coplanar container)
  AND inward 만 carve → solid 면 MoveOnly(ADR-196) / outward boss 경로 무변경.
  try-carve-on-every-inward (교란 위험) 대신 신뢰 gate.

## Amendment 1 (2026-06-24) — XIA reconciliation (사용자 시연 "내부 벽이 없음")

사용자 시연 결함 "내부 벽이 없음". 실제 엔진 재현으로 진단: **engine carve 는 완전
정상** — clean carve(depth 40)가 ring + 4 walls(area 9786) + 추-floor(y=-20 recessed)
+ manifold valid + 모든 pocket 면 inVolume=true + volumeFlag wall=1 (two-tone 렌더 가시,
floorVerts=12 렌더 버퍼 확인). **그러나 carve 가 Mesh 만 mutate 하고 Scene 의 XIA
face_ids 를 reconcile 안 함** → pocket 면(ring/floor/walls)이 box XIA 에서 orphan + 옛
front wall 이 stale + source sheet Shape 잔존. render 는 all-active export 라 가시하나,
**selection/inspector/volume/cost + XIA 정합성이 깨짐** (사용자 facing 결함).

**fix**: WASM `carvePocketFromSourceFace` 를 mesh 직접 호출 → **신규 Scene 메서드**
`Scene::carve_pocket_from_source_face` 경유로 변경:
- host 벽(larger coplanar container)의 owning XIA 를 carve **전** 식별 (punch 가 host
  id 재유도하므로).
- carve 후 reconcile: 옛 host + source sheet 를 XIA.face_ids 에서 제거, ring_face +
  floor_face + wall_faces 를 XIA + face_to_xia 에 등록. source sheet 를 그 Shape 에서
  drop (빈 Shape 제거). Err 시 scene snapshot 복원 (carve 가 guard 전 mutate, ADR-190 P0.2).
- `Mesh::find_larger_coplanar_container_face` (host id 반환) 추가, `face_has_larger_
  coplanar_container` 가 `.is_some()` 위임.
- exec_push_pull(ADR-079/191)의 face_to_xia reconcile 패턴 1:1 답습.

검증: axia-geo lib 2012(carve 30 + find_larger refactor) / axia-core 400(+1
`adr252_scene_carve_pocket_reconciles_xia`: carve 후 box XIA 11 면 소유 = 5 box walls +
ring + floor + 4 walls, 옛 front wall + sheet 제외) / axia-wasm 64. 브라우저 e2e
(rebuilt WASM): carve 후 box XIA `getXiaFaceIds` = 11 면 (f8 ring il=1 / f13 floor y=-20
/ f9-12 walls y=-40 모두 포함), shapes [] ← [1] (sheet 정리), invariants valid.

**사용자 재시연 가이드**: WASM 재빌드됨 → **Ctrl+Shift+R 하드 리로드** 후 재시연 필수
(stale WASM 시 carve export 없어 fallback extrude = degenerate). 깨끗한 박스 + 면에 사각
+ 안쪽으로 **명확한 거리**(예 50mm) push.

**L5 (canonical)** — **topology-changing mesh op 은 Scene XIA/Shape 를 reconcile 해야
한다.** carve/drill/punch 가 Mesh 만 mutate 하면 render(all-active)는 되나 XIA 정합성
(selection/inspector/volume)이 깨짐. 신규 face 는 owning XIA 에 등록, 제거 face 는 drop,
소비된 Shape 정리 (exec_push_pull/slice 패턴). (drill/punch 도 동일 reconcile 필요 가능 —
별도 audit.)

## Amendment 2 (2026-06-24) — Through transition (사용자 시연 "구멍이 안뚤림")

사용자 시연 결함 "구멍이 안뚤림" (얇은 벽에 사각 그리고 Push/Pull 로 관통 시도).
재현 확인: pocket carve 가 **depth ≥ 벽 두께 시 bail** (pocket 전용) → 얇은 벽을
끝까지 밀면 fallback → 구멍 안 뚫림. (Amendment 1 §7 의 "through transition"
follow-up 이 trigger.)

**fix — pocket vs THROUGH depth-dispatch** (SketchUp parity):
- `Mesh::wall_thickness_from_source_face(source)` — host 벽 → 반대 벽 carve ray
  거리 (= 벽 두께). pocket/through 판정용.
- `Mesh::carve_through_from_source_face(source)` — pocket 의 through 형제: sheet
  consume(face+edges) 후 `drill_polygon_through_hole`(ADR-250) 로 양 벽 관통
  (entry ring + exit ring + tube). DrillThroughResult 반환.
- `Scene::carve_pocket_from_source_face` 가 dispatch: `depth ≥ thickness - 1e-3`
  → through (carve_through), 아니면 pocket (carve_pocket). through 의 XIA reconcile
  — 옛 front + back 벽(punch 됨) drop(retain active) + entry ring + exit ring +
  tube 등록. sheet Shape 정리(`cleanup_consumed_sheet` 공유).
- PushPullTool/WASM 변경 0 — 같은 `carvePocketFromSourceFace` entry 가 depth 로
  자동 분기 (사용자는 "안쪽으로 밀기" 한 제스처, 깊이가 pocket/window 결정).

검증: axia-geo +1 (`adr252_through_from_source_sheet_manifold`: thin wall 4 tube
walls, depth=두께 40, watertight genus-1) → 2013. axia-core +1
(`adr252_scene_carve_through_reconciles_xia`: XIA 10 면 = 4 box sides + entry +
exit + 4 tube, 옛 front+back 제외) → 401. 브라우저 e2e (rebuilt WASM): thin wall
(Y=40) + 사각 + depth 60 → result=4 tube (이전 -1 bail 해소), entry ring(y=-20,il=1)
+ exit ring(y=+20,il=1), XIA 10 면, invariants valid 0 violations, console clean.

**L6** — pocket/through 는 **같은 제스처의 depth-continuum** (SketchUp): 면에 그리고
안쪽으로 밀기 → 두께 안이면 pocket, 두께 넘으면 window. 단일 entry(`carvePocketFrom
SourceFace`)가 depth 로 분기. UI 변경 0 (engine dispatch). 관통 전용 진입점(Window/
PolygonHole 도구, ADR-249/250)과 공존.

## 7. 후속 (별도)

- **Drag 중 pocket/through preview** — 현재 pocket candidate 는 live 억제
  (dimension only). recess/window ghost preview 는 follow-up.
- **Boss integration** — outward push 의 새 박스도 벽에 fuse (현재 별개). follow-up.

## 8. Cross-link

- ADR-249 (`bridge_through_loops` / `punch_polygon_hole` / `drill_extract_new_hole_
  loop` 재사용) / ADR-250 (polygon punch) / ADR-196 (Push/Pull MoveOnly dispatch —
  pocket 과 공존) / ADR-190 P0.2 / ADR-007 / ADR-246 (Extrude/Cut) / ADR-015 #139
  (auto containment — 무변경) / ADR-087 K-ζ (시연 게이트) / 메타-원칙 #5 #6 / LOCKED #44.
