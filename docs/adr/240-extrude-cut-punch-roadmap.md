# ADR-240 — Extrude / Cut / Punch Completion & Expansion Roadmap

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: Solid-editing 패밀리 (Push/Pull + Slice/Cut + Punch/Hole) 완성·확장 — roadmap only (구현 0, docs-only)
- **Depends on**: ADR-079 (create_solid) / ADR-190~196 (Push/Pull 트랙) / ADR-194 (carve/drill) /
  ADR-197/198/204/205 (Boolean) / ADR-101 (auto-intersect) / ADR-016 Q2 (multi-loop policy)

## 1. Context

사용자 요청 "extrude/cut/punching 관련 기능의 완벽한 구현 및 확장에 대한 상세한 검토 및 계획".
3-agent + 직접 코드 audit (2026-06-24) 결과 — **이 패밀리는 이미 매우 성숙**. 본 ADR 은 audit 기반
현 상태 + gap 인벤토리(bail!/NotYetSupported/deferred 실측) + 5-phase 완성·확장 로드맵을 고정한다
(ADR-190 Push/Pull 로드맵 패턴 — docs-only). 각 phase 는 별도 ADR + 결재 + Path Z atomic.

## 2. 현재 상태 (audit, file:line)

### 🟢 EXTRUDE (Push/Pull)
- `CreateSolidMode`: Extrude / Revolve / Sweep / Loft (`create_solid.rs:44`).
- Extrude 라우팅: Box / Cylinder / Mixed(Arc+Line) / 닫힌 Bezier·BSpline·NURBS sweep
  (`extrude_planar_box/cylinder/mixed`, `extrude_closed_curve_general_kernel_native`).
- Smooth-group offset: Cylinder/Sphere/Cone/Torus (`offset_smooth_group_*`).
- push_pull: MoveOnly(밀기/넣기 + inward clamp ADR-196) / CreateFace (`push_pull.rs:39-1122`).
- Live extrude: `begin/update/commit/cancel_live_extrude` (scene.rs, ADR-193).
- exec_create_solid dispatch: multi-loop→push_pull (P1.2) / is_move_only→push_pull / ADR-102 cleave
  pre-step / Q3 NotYetSupported→push_pull fallback (scene.rs:6991-7224).

### 🟢 CUT / SLICE
- **평면 솔리드 절단(2 볼륨)**: `slice.rs:98 slice_volume_by_plane` (vertex classify → split_edge →
  split_face → cut loop → 양면 cap). SliceTool 3-point 평면 + vertical quick-mode.
- **곡면 솔리드 trim(above/below)**: `cutCurvedByZPlane` (수평 Z-plane, ADR-197 β-3-n).
- face 분할: `face_split.rs:265 split_face_by_line` (Phase A-G, hole-aware Case a/b).
- auto-intersect coplanar: `coplanar.rs` (그린 도형이 coplanar 면 분할, ADR-101).

### 🟢 PUNCH / HOLE
- 원형 hole(ring-with-hole, 다중 hole 허용): `mesh.rs:10038 punch_circular_hole`.
- 사각 window: `mesh.rs:10348 punch_rect_hole`.
- **원형 관통 drill(entry+exit+tube)**: `carve.rs:177 drill_circular_through_hole` +
  `detect_carve_intent`(Through/Pocket/Add) (ADR-194).
- smooth hole render: per-segment Arc (ADR-230).
- Boolean subtract(primitive 곡면): sphere/cyl/cone/torus halfspace·slab·subtract·drill·dimple·
  countersink (ADR-197/198/204/205).
- UI: DrawHoleTool / DrawWindowTool / SliceTool 모두 wired.

## 3. Gap 인벤토리 (실측 bail!/NotYetSupported/deferred)

### EXTRUDE
- **E1** 부분 Revolve(<360°) — `create_solid.rs:1651` deferred (W-4-β). 반회전/lathe 섹터.
- **E2** Loft 정점수 불일치 auto-resample — `:1816` deferred (W-3-β follow-up).
- **E3** Multi-loop profile(구멍 면) extrude — ADR-016 Q2, 현재 push_pull fallback (`:1675/1781/1891`).
- **E4** 닫힌-곡선 sweep 결과 Boolean — side face legacy ring 스키마 → boolean `inners()` 거부
  (`push_pull.rs:1273`; project_pushpull_track §3.2 shared Cylinder Path B latent parity).
- **E5** NURBS profile top = 근사 Plane(W-3-ε) / Sweep NURBS-path / 2-rail / variable section deferred.

### CUT / SLICE
- **C1** Non-convex 면 slice(>2 교차) — `slice.rs:32-34` bail (L-shape wall).
- **C2** 구멍 있는 솔리드 slice — `slice.rs:111` 거부 (face.inners non-empty).
- **C3** 곡면 솔리드의 임의(비수평) 평면 slice — 현재 수평 Z-plane만.
- **C4** 다각 절단(평면이 >2 조각) / **C5** 폴리곤 trim(한쪽 유지, 현재 곡면만).
- **C6** 곡선/freeform cut(스케치 곡선 따라 절단).

### PUNCH / HOLE
- **P1** 사각 관통 drill (현재 원형 관통만; carve.rs는 circular only).
- **P2** 단차 hole (countersink/counterbore) — Boolean엔 일부, punch엔 없음.
- **P3** Slot/obround(둥근 사각) hole.
- **P4** 곡면(원통/구 벽) 위 hole.
- **P5** 임의 프로파일 관통 cut (원/사각 외 닫힌 곡선).
- **P6** 복잡 다면 솔리드 관통 (ray 상 >2 면).

## 4. 5-Phase 로드맵

각 phase = 별도 ADR + 결재 + Path Z atomic (de-risk-first, 사용자 시연 게이트, 절대 #[ignore] 금지).
우선순위 = 사용자 가치 × 낮은 위험.

| Phase | 묶음 | 내용 | 위험 | ADR(가칭) |
|---|---|---|---|---|
| **1. Slice 견고화** | C1+C2+C5 | non-convex 면 slice / 구멍 솔리드 slice / 폴리곤 trim(한쪽 유지) | M | ADR-241 |
| **2. Punch 확장** | P1+P5+P6 | 사각 관통 / 임의-프로파일 관통 / 다면 관통 | M | ADR-242 |
| **3. Extrude 완성** | E1+E2+E3 | 부분 Revolve / Loft resample / multi-loop profile extrude | M-L | ADR-243 |
| **4. 고급 carving** | P2+P3+P4 | 단차 hole / slot / 곡면 hole | L | ADR-244 |
| **5. 곡면 cut + Boolean parity** | C3+C6+E4 | 임의평면 곡면 slice / freeform cut / swept-Boolean 스키마 통일 | L-XL (deep SSI) | ADR-245+ |

**시작 = Phase 1 (Slice 견고화)** — 사용자 결재. 가장 직접적 가치(L자 벽/구멍 솔리드 절단은 흔함),
`split_face`·`slice_volume_by_plane` 기존 자산 확장, deep-SSI 불요.

## 5. Lock-ins

- **L-240-1** 본 ADR = roadmap only (구현 0, 회귀 0). 각 phase 가 구현/회귀/시연.
- **L-240-2** 패밀리 3 축(extrude/cut/punch) 의 현 자산은 **재사용 우선**(Pattern-12) — 새 알고리즘
  최소화. slice_volume_by_plane / split_face / carve drill / push_pull / create_solid 확장.
- **L-240-3** ADR-016 Q2 (multi-loop face) 정책 변경(E3 multi-loop profile extrude / C2 구멍 솔리드
  slice)은 **명시 결재 필수** (ADR-191 P1.2 선례 — per-op 완화).
- **L-240-4** 메타-원칙 #16 (휴리스틱 자동화 antipattern) — 모든 cut/punch 는 **명시 trigger**
  (자동 아님). ADR-194 drill 답습.
- **L-240-5** 각 phase 별도 ADR + 결재 + de-risk-first + 사용자 시연 게이트(ADR-087 K-ζ).
- **L-240-6** ADR-046 P31 #4 additive — 기존 도구/명령 surface 보존.
- **L-240-7** Phase 5 deep-SSI (곡면 임의평면 slice / swept-Boolean) 는 별도 multi-week atomic
  (project_boolean_track open SSI 케이스와 연계).
- **L-240-8** 절대 #[ignore] 금지.

## 6. Lessons (audit)

- **L1** 패밀리 성숙도 과소평가 위험 — audit 전 "extrude/cut/punch 완성 필요" 가정했으나 실제는
  대부분 구현됨(slice_volume_by_plane / drill_circular_through_hole 등). audit-first 가 정확한 계획의
  전제 (메타-원칙 #6, ADR-125 답습).
- **L2** Gap = bail!/NotYetSupported/deferred 의 실측 — 추측 아닌 file:line 근거로 phase 분할.
- **L3** Pattern-12 (engine-already-robust) 의 확장 변형 — 완성도 gap 대부분이 기존 함수의 scope
  확장(non-convex / 구멍 / 사각 / 다면)이라 reuse 높음.

## 7. Cross-link

- ADR-079 (create_solid) / ADR-190 (Push/Pull 로드맵 — 본 ADR 패턴 source) / ADR-191(ring P1.2) /
  ADR-192(closed-curve sweep) / ADR-193(live) / ADR-196(MoveOnly 밀기/넣기) / ADR-194(carve/drill) /
  ADR-197/198/204/205(Boolean) / ADR-101(auto-intersect) / ADR-230(smooth hole) / ADR-016 Q2(multi-loop).
- 메타-원칙 #6(Preventive·audit-first) / #16(명시 trigger) / ADR-046 P31 #4(additive) /
  ADR-087 K-ζ(시연 게이트) / LOCKED #44(Complete Meaning per Merge) / #78~83(Push/Pull 트랙).
- 후속 ADR-241(Phase 1 Slice) / 242(Phase 2 Punch) / 243(Phase 3 Extrude) / 244(Phase 4 고급) /
  245+(Phase 5 곡면 cut+Boolean parity) — 가칭, 각 별도 결재.
