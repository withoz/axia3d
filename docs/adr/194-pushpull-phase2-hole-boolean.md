# ADR-194 — Push/Pull Phase 2: Hole-through / Boolean (α spec — Hybrid staged)

> ADR-190 로드맵 **Phase 2** (= "hole-through·Boolean, signature CAD, 최고 체감
> 가치"). 면을 솔리드 **안으로** push → 자동 carve(P2.1 recess) / 관통 → 구멍
> (P2.2 through-hole). 본 문서는 **α spec only** — 아키텍처/범위/분해/회귀 계획을
> lock 하고, **β 구현은 별도 사용자 결재** 후 진행 (ADR-118→119 패턴 답습).

- **Status**: Proposed (α spec — β 진입은 별도 결재)
- **Date**: 2026-06-10
- **Track**: 6 (boundary kernel) + W (ADR-079 create_solid) — Push/Pull 로드맵
- **Builds on**: ADR-190 (roadmap Phase 2), ADR-191 (P1.2 ring→tube), ADR-064/066
  (NURBS Boolean DCEL), ADR-101 (coplanar auto-intersect), ADR-007 (manifold),
  메타-원칙 #4/#5/#6/#14/#15/#16
- **Supersedes**: 없음 (신규)

---

## 1. Canonical anchor (사용자 결재, 2026-06-10)

> 사전검토 + 시뮬레이션 후 **"C Hybrid 단계"** 결재 — MVP=**B (Tube-carve:
> punch + inward extrude + cap 제거)** 먼저 → **A (일반 침투 Boolean subtract)**
> 후속. ADR-191/192 MVP-우선 패턴 답습.

---

## 2. Problem — 면을 솔리드 안으로 push 해도 carve 가 안 된다

SketchUp Push/Pull 의 시그니처 동작: 솔리드 면 위에 작은 면을 그리고 **안쪽으로**
밀면 → 포켓(P2.1) 또는 관통 구멍(P2.2). AxiA 는 Phase 0(모든 면 pushable) +
Phase 1(ring/sweep)까지 왔으나, **안쪽 push = 자동 subtract** 는 미구현.

---

## 3. 시뮬레이션 (실제 브라우저 엔진 측정, 2026-06-10)

| # | 실험 | 측정 |
|---|---|---|
| **S1** | 박스(200³) 윗면 inner 100×100 sub-face → **−100 push** | **별개 100×100×100 박스(6면) 생성 → 큰 박스 관통**. carve 아님, **subtract 0**. (faces 9~14, 각 area 10000) — **결정적 gap** |
| **S2** | 박스 윗면 `punchHole([0,0,200],[0,0,1],50,24)` | verts 12→**36** / edges 12→36 / faces 6 — **2D ring-with-hole** ✅ (윗면만, 바닥 solid → 관통 아님) |
| **S3** | 솔리드 면 위 sub-face draw (bridge) | 깔끔한 ring+inner 분할 **안 됨** — 윗면 40000 유지 + surface drop(kind 0) + inner 겹침 (실제 마우스 face-draw 는 다를 수 있음 → 별도 확인) |
| **S4** | 솔리드-솔리드 `mesh.boolean(Subtract)` | 엔진 레벨 존재(cavity 검증) **BUT bridge 미노출** + triangle 기반(analytic 손실) + hole-face 거부 |

**S1 결론**: push 가 솔리드 안으로 향해도 **침투 감지 0 + 자동 subtract 0** →
겹치는 솔리드만 생김. 이것이 Phase 2 의 핵심.

---

## 4. 자산 인벤토리 (현재 브랜치 adr-186/boundary-kernel-port)

**있음 ✅**
- `mesh.boolean` Subtract/Intersect/Union — `boolean.rs` (triangle, cavity 검증,
  `boolean_subtract_creates_cavity`). **단 hole-face 거부 + analytic 손실.**
- `punch_circular_hole` (mesh.rs:6826, 8 테스트) + `punch_rect_hole` — 2D face
  hole (ring-with-hole, stable id, atomic). WASM `punchHole` + bridge 노출.
- NURBS DCEL Boolean — `boolean_dispatch_dcel(_multi)` (ADR-064/066), bridge
  `booleanDispatchDcelMulti` = **face**-Boolean.
- ADR-191 ring push → tube (P1.2).
- ADR-101 coplanar auto-intersect (default ON, ADR-176).

**없음 ❌ (= Phase 2 작업)**
1. **침투/carve 방향 감지** — push 가 솔리드 boundary sub-face 를 anti-normal
   로(material 안으로) 미는가? 관통(through) vs 포켓(pocket)?
2. **push → subtract dispatch** — carve intent 시 extrude-add 대신 carve.
3. **surface-보존 swept-tube subtract** — triangle boolean 은 analytic 손실.
4. **solid-solid subtract 의 bridge 노출.**
5. **솔리드 면 위 sub-face 깔끔한 ring+inner 분할** (S3 — inner 가 진짜 솔리드
   boundary sub-region 이어야 push-carve 대상).

---

## 5. 아키텍처 — Hybrid 단계 (사용자 결재 C)

### 5.1 P2.1 / P2.2 의미론 (lock)
- **P2.1 carve/recess**: 솔리드 boundary sub-face 를 안쪽으로 push → 멈춘 깊이
  까지 swept prism 을 subtract → **포켓**.
- **P2.2 through-hole**: 같은 push 가 반대편 wall 을 **관통** → swept prism 양끝
  open → **관통 구멍**.
- **방향 dispatch (canonical)**: outward push(법선 방향) = 재료 추가(현 extrude);
  **inward push(anti-normal, material 안으로) = carve/subtract** (신규). 단
  **full face** inward = MoveOnly 리사이즈(현행 보존), **sub-face(coplanar ring
  으로 둘러싸인) inward = carve** — 둘의 구분이 trigger 핵심.

### 5.2 Stage B — Tube-carve MVP (먼저)
**범위**: 직선·수직 push, **circular(Circle/Arc) 또는 rect(polygon)** profile,
convex. swept volume = prism/cylinder.
**메커니즘 (기존 primitive 재활용)**:
- 침투 감지(단순): sub-face 가 솔리드 boundary + push 가 anti-normal + swept
  prism 이 (a) 반대 wall 도달 → Through, (b) 내부 정지 → Pocket.
- Through: punch_circular_hole/rect 를 entry+exit 양면에 + ADR-191 ring-push
  변형으로 tube wall 연결 → 관통 tube (양끝 open, analytic 보존).
- Pocket: inner cap 제거 + inward tube wall + pocket 바닥(translated profile).
**장점**: surface-native(analytic 보존), punch 8-테스트 + ADR-191 자산 재활용,
빠른 체감 가치. **한계**: 비스듬/임의 형상/multi-solid 침투 = 범위 외(Stage A).

### 5.3 Stage A — Surface-native Boolean (후속)
일반 침투 감지(임의 각도/형상) → swept volume 을 **NURBS DCEL subtract**
(ADR-064/066) → analytic 보존 carve. 비스듬 push, 곡면 wall, multi-solid 침투
포괄. 별도 ADR(가칭 ADR-195) 또는 본 ADR 의 후속 β 트랙.

---

## 6. 분해 (제안 — Path Z atomic, Stage B 먼저)

| sub-step | 내용 | 회귀(예상) |
|---|---|---|
| **α** | 본 spec + 결재 (Hybrid C lock) | +0 |
| **β-0** ✅ | S3 검증 — **완료 (§10)**: draw-inner-push 비viable(containment no-op + shape↔solid-face 미pairing) → **MVP carve 진입 = punch-based 확정**, draw-inner-push ruled out | +0 (검증) |
| **β-1** ✅ | `detect_carve_intent(face, dist) -> CarveIntent{None\|Pocket{depth}\|Through}` — 방향/침투/관통 판정 (read-only) — **완료 (§11)** | +8 |
| **β-2** ✅ | Through MVP — punch entry+exit + **dedicated bridge** tube wall → 관통 구멍, manifold (사전검토 §12 + **구현 §13 ✅** + 적대적 검토 3 fix) | +5 (실측) |
| **β-3** | Pocket MVP — inner cap 제거 + inward wall + pocket 바닥 → recess, manifold | +10 |
| **β-4** | exec_create_solid/exec_push_pull dispatch wiring (inward sub-face → carve, default ON/OFF 결재) + transaction 단일 Undo | +8 |
| **β-5** | 브라우저 시연 게이트(S1 시나리오가 이제 carve) + 회귀 sweep + 거버넌스 | +5 |
| **(A 후속)** | 일반 침투 → NURBS DCEL subtract (별도 결재) | 별도 |

각 β 는 별도 atomic PR(LOCKED #44) + 별도 사용자 결재(메타-원칙 #16 — 자동
carve default 는 신중히).

---

## 7. Lock-ins (α spec)

- **L-194-1** Hybrid 단계 (B MVP → A 후속, 사용자 결재 C).
- **L-194-2** 방향 dispatch — outward=add(보존), inward sub-face=carve(신규),
  inward full-face=MoveOnly(보존).
- **L-194-3** Stage B = 직선·수직·convex·circular/rect 한정. 비스듬/임의/곡면
  wall = Stage A.
- **L-194-4** surface-native 우선 — triangle `mesh.boolean` 은 MVP carve 의
  primary 아님(analytic 손실); punch + ring-push 재활용.
- **L-194-5** carve 자동 trigger 는 **메타-원칙 #16 정합** — default ON/OFF +
  명시 경로(Hole 도구 등)는 β-4 에서 별도 결재. 휴리스틱 침투 추측 금지.
- **L-194-6** ADR-007 manifold + ADR-016 Q2(multi-loop) + LOCKED #1/#15 정합.
- **L-194-7** ADR-046 P31 #4 additive — 기존 push(outward add) signature/동작
  무변경.
- **L-194-8** 절대 #[ignore] 금지. 각 β 시연 게이트(ADR-087 K-ζ) 필수.

---

## 8. Cross-link
- ADR-190 (roadmap Phase 2 정의) / ADR-191 (ring→tube, β-2 재활용) / ADR-192
  (sweep) / ADR-193 (live)
- ADR-064/066 (NURBS Boolean DCEL — Stage A subtract)
- ADR-101 (coplanar auto-intersect — ring+inner split) / ADR-176 (auto default ON)
- ADR-016 Q2 (multi-loop) / ADR-007 (manifold) / LOCKED #1 P7 / LOCKED #15
- `punch_circular_hole`/`punch_rect_hole` (mesh.rs) / `mesh.boolean` (boolean.rs)
- 메타-원칙 #4(SSOT) / #5(편의) / #6(preventive) / #14(면=닫힌 경계) /
  #15(split contract) / #16(자동화 antipattern)
- ADR-087 K-ζ (시연 게이트) / ADR-118→119 (α spec → β 패턴)

---

## 9. Out of scope (α — 별도 결재/ADR)
- Stage A 일반 Boolean subtract 구현 (β 후속 또는 ADR-195).
- 비스듬(oblique) push carve / 곡면 wall carve / multi-solid 침투.
- Hole 전용 UI 도구(DrawHoleTool) — 별도(메모리 `project_hole_tool_punch`).
- triangle `mesh.boolean` 의 bridge 노출 + hole-face 지원(constrained Delaunay).

---

## 10. β-0 Acceptance — ring-split 검증 (세밀 시뮬레이션, 2026-06-10)

**질문**: 솔리드 면 위에 sub-face 를 그리면 ring+inner 로 깔끔히 분할되어
(SketchUp-style) push-carve 대상이 되는가?

**시뮬레이션 매트릭스 (실제 브라우저 엔진)**:

| Case | 셋업 | 결과 |
|---|---|---|
| **A 격납(containment)** | 박스 윗면 안에 100×100 rect (완전 내부) | **분할 안 됨** — inner 10000 floating + 윗면 40000 유지 + surface drop(kind 0). `cleanRingSplit=false`. ADR-101 b3b `Ok(None)` no-op 정책 정합 |
| **B 부분 overlap(on solid)** | 박스 윗면 가장자리를 가로지르는 rect | **분할 안 됨** — rect 10000 floating, 윗면 40000 유지 (3-way split 미발생) |
| **Control 부분 overlap(shape↔shape)** | 지면 z=0 두 rect 부분 overlap | **3-way split ✅** (areas 20000×3). auto-intersect 플래그 ON 확인 |
| **C punch entry+exit** | 박스 top+bottom `punch_circular_hole` | **양면 ring-with-hole ✅** (verts 12→36→60, both manifold valid). host 7/8 |

**진단 (결정적)**: `auto_intersect_coplanar` (ADR-101) 는 **갓 그린 shape 끼리만**
coplanar pairing → 분할. **기존 솔리드 face 와는 pairing 안 함** → 솔리드 면 위
draw 는 분할 0. 격납은 추가로 정책 no-op(ADR-101 b3b / ADR-015 B1, 메타-원칙 #16
정합).

**결론 (β-0)**:
1. **draw-inner-push 워크플로우 비viable** (현 브랜치). containment no-op +
   shape↔solid-face 미pairing + 메타-원칙 #16 (auto-carve-on-draw 회피).
2. **MVP carve 진입 = punch-based 확정** — `punch_circular_hole`/`punch_rect_hole`
   가 솔리드 face 위에서 직접 atomic ring-with-hole 생성 (world point 로 host
   탐색). entry+exit punch → 정렬된 양면 hole → β-2 가 tube 연결 → 관통 구멍.
   α spec §5.2 (B Tube-carve) 의 punch-기반 설계를 **검증·확정**, draw-inner-push
   를 명시 ruled out.
3. **β-2 (Through)** = punch entry+exit + tube wall 연결(ADR-191 변형). **β-3
   (Pocket)** = punch + inward wall + pocket 바닥. 둘 다 punch 자산 재활용.
4. **대안(범위 외)**: shape↔solid-face coplanar pairing 활성화(SketchUp-style
   draw-inner-push 복원)는 별도 큰 작업 + 메타-원칙 #16 위험 → Stage A 또는
   별도 ADR. MVP 는 punch-기반(명시 진입)으로 진행.

**다음**: β-1 (`detect_carve_intent`) 진입은 **별도 사용자 결재** (메타-원칙 #16
— 자동 carve trigger default 신중).

---

## 11. β-1 Acceptance — `detect_carve_intent` (read-only 감지, 2026-06-10)

**구현** (`crates/axia-geo/src/operations/carve.rs`, 신규 module, 사용자 결재):
- `CarveIntent { None | Pocket{depth} | Through }` enum + `Mesh::detect_carve_
  intent(face_id, dist) -> CarveIntent`.
- **순수 read-only query** — DCEL mutation 0, **WASM/bridge surface 0, 자동
  trigger 0** (메타-원칙 #16 정합; carve dispatch + default ON/OFF 는 β-4 별도 결재).
- 알고리즘 (Stage B MVP, 직선·수직 push): 방향 = ADR-007 outward normal 기준
  (`dist >= 0` outward → None / `dist < 0` inward). inward 시 face centroid 에서
  `-normal` ray → 비-coplanar 최근접 opposite face 탐색. hit `< |dist|` → Through,
  `> |dist|` → Pocket{depth:|dist|}, 없음 → None (닫힌 wall 없음 = 보수적).
- coplanar-skip: source plane 의 sibling(ring) 제외 (`|n·n'|>0.999` AND offset
  `<1.5e-3` LOCKED #5). anti-parallel 반대 wall(box bottom)은 offset 달라 미제외.
  side face 는 ray 평행 → 자동 제외.

**회귀 +8** (axia-geo 1696 → 1704, 절대 #[ignore] 금지 8/8):
- `adr194_b1_outward_push_is_none` / `_zero_and_nonfinite_is_none`
- `adr194_b1_inward_pocket` (depth 정확) / `_inward_through` / `_exact_to_wall_
  is_through` (THROUGH_SLACK 경계 + just-short Pocket)
- `adr194_b1_bottom_face_inward_through` (대칭) /
  `_standalone_face_no_material_is_none` (opposite wall 없음 → None) /
  `_inactive_or_unknown_face_is_none` (panic 0)

**워크스페이스**: axia-geo **1704** / axia-core **352** / transaction **5** — 0
failed, 0 ignored. TS 변경 0.

**시연 게이트 (ADR-087 K-ζ) 노트**: β-1 은 read-only 분류기로 *시각적 효과 0* →
엔진 테스트가 검증의 전부. 사용자 facing 시연은 **β-2** (실제 carve → 관통 구멍
geometry) 에서. **β-2 진입은 별도 결재** (메타-원칙 #16).

---

## 12. β-2 사전검토 — punch entry+exit + dedicated bridge (시뮬레이션 + 외부 엔진 비교, 2026-06-10)

**시뮬레이션 (실제 브라우저 엔진)**:

| 실험 | 측정 |
|---|---|
| 박스 윗면 ring-push (`punchHole` top + `createSolidExtrude(ring,−200)`) | **no-op** (pushOk=true, stats 36/36/6 불변) — ADR-191 ring-push 는 **닫힌 box ring 미작동** (standalone washer 는 LOCKED #79 작동; 닫힌 솔리드가 outer 테두리 제약) |
| 박스 punch **entry+exit** (top+bottom) | 양면 ring-with-hole 정렬·manifold (β-0: verts 12→36→60) |
| connect-loops 헬퍼 | **없음** (`bridge`/`loft_loops`/`connect_loops` grep 0). `Mesh::loft(sections,closed)` 존재 (후보) |

**진단**: ADR-191 ring-push 는 through-hole 비viable (기존 솔리드 ring 의 outer
테두리가 묶여 no-op). viable = punch entry+exit + 두 hole loop 을 tube wall 로
연결 (연결 op 신규).

**bridge 메커니즘 결재 = A Dedicated bridge** (사용자):
punch 가 만든 **기존 hole loop verts** 로 N quads 직접 생성 (top[i]→bot[i]) →
dedup 무관 manifold 보장 + Cylinder analytic surface attach. ADR-191 tube wiring
답습. (B loft 재사용 = dedup-weld 위험 / C mesh.boolean = analytic 손실, 둘 다
reject.)

**외부 엔진 비교 — AixxiA (`D:\AixiAcad\engine`, xia-form `carve_rect_opening`)**:
AixxiA 는 관통 개구부(창/문)를 **"기존 솔리드를 자르지 않고 holed prism 으로
재생성"** — 픽한 box 에서 (baseline·두께·높이) 유도 → `add_planar_face_with_holes`
(바깥 + 안쪽 opening loop) + `extrude_planar_face` → 관통 벽. validate + rollback
은 우리 snapshot restore 와 동일 패턴. 일반 Boolean 은 `boolean_csg` (TriMesh CSG,
우리 `mesh.boolean` 과 동급) + **비파괴 `BooleanScene`** (feature tree, 우리엔 없음).
IFC IfcWindow/Door export 까지.
- **시사점 (대안 D)**: AixxiA 의 "재생성" = 우리 **ADR-191 (annulus → tube) 을
  fresh 면에 적용**한 것과 동일. 우리 ring-push no-op 의 원인(기존 ring 제약)을
  fresh 재생성으로 회피. → β-2 를 dedicated bridge 없이 "holed prism 재생성 +
  ADR-191 extrude" 로 더 간단히 할 수도 있음 (신규 코드 ~0).
- **결정 (사용자 "진행하던 방향")**: **A 유지** — D(재생성)는 솔리드 교체로
  다른 feature 손실 + box 류 한정 (AixxiA v1 도 box 벽·vertical·rect 한정 +
  orphan vertex 누수 명시). A 는 in-place 수정 → 임의 솔리드 + 기존 feature 보존.
  D 는 §6 후속 대안으로 보존 (필요 시 fallback).

**β-2 drill op 설계 (A, 명시 op, MVP = circular through-hole)**:
1. β-1 `detect_carve_intent` → Through 확인 + exit face + 거리 (β-1↔β-2 연결)
2. punch **entry** (near face) → ring-with-hole (reuse `punch_circular_hole`)
3. punch **exit** (far face, projected center) → ring-with-hole
4. **dedicated bridge**: 두 hole loop 의 기존 verts 로 N quads cylindrical tube
   wall + Cylinder surface attach (ADR-191 tube wiring 답습)
5. manifold 검증 (ADR-007)

**범위**: circular through-hole MVP. rect / pocket(recess) 는 후속 sub-step.
**명시 drill op** (auto-trigger 아님 — β-4 dispatch + default ON/OFF). 메타-원칙
#16 정합.

**다음**: β-2 **구현 진입은 별도 결재** ("구현"). 시연 게이트 = 박스 관통 구멍
manifold (첫 사용자 facing geometry).

---

## 13. β-2 Acceptance — `drill_circular_through_hole` (구현, 2026-06-10)

**구현** (`crates/axia-geo/src/operations/carve.rs` + WASM + TS bridge):
- `Mesh::drill_circular_through_hole(center, normal, radius, segments) ->
  DrillThroughResult` — **명시 op** (auto-trigger 0 — 메타-원칙 #16; push-driven
  dispatch 은 β-4).
- 메커니즘 (A Dedicated bridge):
  1. depth = `carve_ray_nearest_face(center, -n, …)` (opposite wall 거리, 원본 솔리드)
  2. `punch_circular_hole(center, n, …)` → entry ring-with-hole + hole loop E (HE order)
  3. `punch_circular_hole(center − n·depth, n, …)` → exit ring-with-hole + hole loop B
  4. B reverse + (u,v) nearest 로 k 정렬 → b_rev[k+i] ↔ e_loop[i]
  5. bridge: N quads `[a2, a, b, b2]` → 4 edge 모두 twin-pair (top = entry inner
     HE twin, bottom = exit inner HE twin, verticals = 인접 quad twin) → manifold
- WASM `drillThroughHole` (snapshot wrap + Err 시 `restore_scene_snapshot` —
  ADR-190 P0.2 atomic). TS bridge `drillThroughHole(center, normal, radius,
  segments)` graceful −1.

**회귀 +5** (axia-geo 1704 → 1709, 절대 #[ignore] 금지):
- `adr194_b2_drill_through_box_manifold` (22 face = 6 box + 16 tube, manifold, depth 200)
- `adr194_b2_drill_caps_have_hole_loop` (entry/exit 각 1 inner loop, 12 tube quads)
- `adr194_b2_drill_no_opposite_wall_errors` (standalone face → Err)
- `adr194_b2_drill_degenerate_inputs_error` (radius≤0 / segments<3 / degenerate normal → Err)
- `adr194_b2_two_drills_face_with_existing_hole_manifold` (pre-existing hole 캡 위 2차 drill,
  38 face, 각 캡 2 hole loop — lens-4 coverage)

**워크스페이스**: axia-geo **1709** / axia-core **352** / transaction **5** — 0
failed, 0 ignored. tsc 0.

**브라우저 시연 게이트 (ADR-087 K-ζ — 첫 사용자 facing Phase 2 geometry)**:

| 시나리오 | 결과 |
|---|---|
| 박스 200³ drill (center [0,0,200], +Z, r=50, 24seg) | tubeCount=24, **faces 30 (6+24)**, verts 60, **manifold 0 violations** ✅ |
| 렌더 | tessellation error 0, annulus 캡 tessellation 작동 (punchHole 캡 tris +8 검증) |

**적대적 검토 (5-lens workflow, 1.4M tokens)**: **핵심 알고리즘 (winding /
k-alignment / rollback-atomicity) 은 MVP 범위에서 PROVEN CORRECT** —
- winding: axis-aligned convex box 의 4-edge twin-pair 추적 확인. tilted axis 는
  punch host gate (`|n·nh|>0.999`) 가 quad 생성 전 reject (가설 refute).
- k-alignment: entry CCW(+n) / exit reverse → CCW(+n) 동방향, exact mirror 매칭
  (off-by-one 없음).
- rollback: WASM `restore_scene_snapshot` 가 부분 mutation 완전 복원 (snapshot
  = 전체 Mesh storage 교체, orphan leak 0).

3 minor finding (모두 *unguarded out-of-scope 가 silently Ok* — `debug_verify_
invariants` 가 release no-op 이라) → **3 fix (silent garbage → explicit Err,
메타-원칙 #5/#6)**:
- **Fix A** (lens 1 winding): exit wall 이 anti-parallel 아니면 (non-convex
  up-facing ledge) `bail!` — `exit_n.dot(n) > -0.5` guard.
- **Fix B** (lens 1/4 defensive): post-bridge `verify_face_invariants()` →
  non-manifold 이면 `bail!` (release no-op `debug_verify` 대체).
- **Fix C** (lens 4 hole-extraction): radial band `radius*0.1` → `radius*0.01 +
  1e-3` (다른 radius pre-existing hole reject) + inners()[0] 순서 의존 문서화.
- 회귀 재검증: axia-geo 1709 (+1 lens-4 test) / 시연 box drill manifold 0
  violations 유지 (guard 가 happy-path 무영향).

**알려진 한계 (MVP, 후속 sub-step)**:
- circular through-hole 만. rect through-hole + pocket(recess) 은 후속.
- straight perpendicular **convex** solid (Z-up box 검증). **비-convex single
  solid** (lens 5): −n ray 가 내부 anti-parallel wall 을 먼저 hit 하면 거기까지만
  drill (manifold-valid 부분 관통) — Fix A 가 same-sign ledge 는 reject, anti-
  parallel 내부 wall 은 first-exit 로 허용 (Stage A 에서 outermost-wall 처리).
  tilted axis / multi-solid 은 Stage A.
- Cylinder analytic surface attach 미적용 (tube 면 kind 0) — 후속 (smooth render + re-op).
- 명시 op only (auto-carve dispatch + default ON/OFF 는 β-4).

**β-2 UI 노출 — Hole 도구 (2026-06-10, 사용자 결재 A)**: 기존 `DrawHoleTool`
(메뉴 "⊘ 구멍 (Hole)", `punchHole` 2D 면 구멍) 의 `commitHole` 을 **drill-first +
punch-fallback** 으로 wiring — `drillThroughHole` 시도(반환 >0 = 관통 tube count)
→ 솔리드면 관통, 단일 sheet 면(반대 벽 없음 → −1, snapshot 복원)이면 `punchHole`
2D fallback. Toast "관통 구멍" / "면 구멍" 구분. **엔진/WASM 변경 0** (drillThroughHole
기존). 회귀 vitest DrawHoleTool 8 → 11 (drill 성공 / sheet fallback / 둘 다 실패
error / radius 가드). 브라우저 시연: 박스 면 → faces 30 관통 manifold / sheet 면 →
verts +24 2D 구멍 manifold. β-2 가 드디어 사용자 facing (메뉴 클릭 → 진짜 관통 구멍).

**다음**: β-3 (Pocket) / β-4 (dispatch + default) / Cylinder surface attach. 모두
별도 결재.
