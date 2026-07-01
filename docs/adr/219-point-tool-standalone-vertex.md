# ADR-219 — Point Tool (standalone vertex, Form-citizen Shape)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Roadmap ② Point 도구 (Dimension 확장 이후) / Foundation
- **Depends on**: ADR-049/050 (Two-Layer Citizenship — Shape) / ADR-091 §E L1
  (Scene-level map, bincode lesson) / ADR-095 (Reference / isolated-vertex snapshot
  precedent) / ADR-103 (Z-up) / LOCKED #5 (spatial-hash) / LOCKED #63/#7 (cardinal)

## 1. Context

작도 점(standalone vertex) 도구. 사용자가 "Point 도구 — orphan cleanup 정합 필요"로
defer 했던 항목. de-risk (5 서브시스템 병렬 조사 + 직접 검증)가 **핵심 위험을 코드로
확정**: `Mesh::remove_isolated_verts`(mesh.rs)는 pin 가드 없이 edge 없는 vertex를 전부
제거하고, **DrawRect/DrawCircle finalizer**(scene.rs `cleanup_dangling_topological_
edges` → `remove_isolated_verts`)가 이를 호출 → **점을 찍은 뒤 도형을 그리면 점이 사라짐**.

인프라 90%는 이미 존재 (Pattern-12): `Reference::PointCloud`(ADR-095) + isolated-vertex
snapshot round-trip 테스트 + `tool-point` UI 예약 (AxiaCommands/MenuBar/index.html).

**사용자 결재 (2026-06-23)**: Q1=**B Form citizen** (Shape가 standalone vertex 소유) /
Q2=**a Mesh pinned_verts** (transient, restore 시 재구축) / Q3=**연속 모드** (Esc 종료) /
Q4=**endpoint snap 재사용** (MVP, node snap 재도입은 별도).

**Mechanism 정정** (ADR-091 D-β / §E L1 canonical lesson): B의 의도(Point=Form Shape)는
보존하되 `Shape` struct에 필드를 추가하면 bincode positional encoding이 legacy V4
snapshot을 깬다 → **Scene-level map `shape_to_standalone_vertex`**로 분리 (Shape struct
UNCHANGED).

## 2. Decision

**Point = Form-citizen Shape (Scene map) owning a PINNED isolated vertex** (Pattern-12).

- **Engine** (`mesh.rs`): transient `pinned_verts: FxHashSet<VertId>` (`#[serde(skip)]`) +
  `pin_vertex` / `unpin_vertex` / `is_vert_pinned` / `pinned_vertex_ids`.
  `remove_isolated_verts`가 pinned vertex를 **skip** → 모든 cleanup 사이트(draw finalizer
  / normalize_for_import / orphan recovery)가 단일 가드로 자동 커버.
- **Scene** (`scene.rs`): `shape_to_standalone_vertex: HashMap<ShapeId, VertId>` +
  `create_point_shape(pos)` (add_vertex → pin → 빈 Shape + map) + `delete_shape` unpin +
  `standalone_point_verts()` (render용). 추가 snapshot **section 10** (additive, offset
  guard, version bump 없음 — ADR-091 7d 선례) + restore 시 **re-pin**. `Command::
  DrawPointAsShape { pos }` + `exec_draw_point_as_shape` (transaction-aware, 단일 Undo).
- **WASM**: `drawPointAsShape(x,y,z) -> f64` (ShapeId / -1) + `standalonePointVerts() ->
  Vec<f64>` (flattened [x,y,z,...]).
- **Bridge** (`WasmBridge.ts`): `drawPointAsShape` + `getStandalonePointVerts` (graceful).
- **Tool** (`DrawPointTool`): 연속 단일 클릭, endpoint snap (get3DPoint + getSnappedPoint),
  Esc 종료 (ToolManager). 클릭마다 1 점 (stateless).
- **Render** (`Viewport.updateStandalonePoints`): 전용 THREE.Points 레이어 (Point vertex는
  mesh buffer에서 0 emit). syncMesh가 매 동기화 시 `getStandalonePointVerts` → 갱신.
  size 8 / depthTest false / renderOrder 998 (SelectionManager xiaDotPoints 패턴).
- **UI**: `tool-point`은 이미 예약 (AxiaCommands:98 / MenuBar:394 / index.html:1890, Draw
  메뉴 ADR-210) → ToolManager `tools.set('point', ...)` 등록만 추가.

## 3. Lock-ins

- **L-219-1** Q1=B Form citizen — Point는 Shape, **Scene map** `shape_to_standalone_vertex`
  (Shape struct UNCHANGED, ADR-091 §E L1 bincode lesson).
- **L-219-2** Q2=a Mesh `pinned_verts` (transient) — `remove_isolated_verts` skip. 모든
  cleanup 사이트가 단일 함수로 funnel → 단일 가드.
- **L-219-3** Snapshot section 10 (additive, offset guard) + restore re-pin (pin set은
  transient라 restore 시 `shape_to_standalone_vertex`에서 재구축).
- **L-219-4** `exec_draw_point_as_shape` transaction-aware (own = `!is_recording()`) — 단일
  Undo.
- **L-219-5** add_vertex (spatial-hash dedup) — 빈 공간 클릭=새 vertex, 기존 vertex 위
  클릭(snapped)=재사용. pin은 항상 (edge 있으면 무해).
- **L-219-6** Render = 전용 THREE.Points 레이어 (mesh buffer 무영향, export_buffers 0 emit).
- **L-219-7** Q3 연속 모드 (stateless 단일 클릭) + Q4 endpoint snap 재사용 (node snap 별도).
- **L-219-8** UI additive — tool-point 이미 예약, ToolManager 등록만 (ADR-046 P31 #4).
- **L-219-9** LOCKED #63/#7 cardinal은 tool get3DPoint가 처리 (bridge 점 snap 불요).
- **L-219-10** 절대 #[ignore] 금지.

## 4. 회귀 + 검증

- **회귀**: axia-geo +2 (`adr219_pinned_vertex_survives_remove_isolated` /
  `adr219_pin_does_not_disturb_edge_topology`) · axia-core +2 (`adr219_point_survives_
  draw_snapshot_and_delete` — DrawRect cleanup 생존 + snapshot round-trip re-pin + delete
  reclaim / `adr219_point_undo_removes_it`) · axia-wasm +2 baseline (drawPointAsShape /
  standalonePointVerts) · vitest +9 (DrawPointTool 6 + WasmBridge 3). 절대 #[ignore] 금지.
  tsc clean. 전체 axia-geo 1988 / axia-core 399 PASS, ToolManager 139 PASS (무회귀).
- **브라우저** (real rebuilt WASM):
  - drawPointAsShape(5,5,0)→shapeId 1, (-3,2,0)→2. standalonePointVerts [5,5,0,-3,2,0].
  - **★ DrawRect 후 두 점 모두 생존** (orphan 정합 가드 end-to-end).
  - 렌더: `standalone-points` THREE.Points 레이어 2 points (size 8 / depthTest false /
    renderOrder 998). point 도구 등록 확인.

## 5. 후속 (별도 ADR / future)

- Point 선택/삭제 UX (picked vertex → ShapeId, 기존 shape selection 재사용)
- Node snap 재도입 (ADR-146 β-1 deprecated, getNodeSnapPositions API)
- Point → Edge/Face 승격 (Form citizen 자연 연장), 좌표 입력(VCB)으로 점 배치
- 24-도구 Phase 5+ (Sweep → Loft → Hole → 3P-Plane → NURBS surface → Wall → Window)

## 6. Cross-link

- ADR-049/050 (Two-Layer Citizenship — Shape form citizen)
- ADR-091 §E L1 / D-β (Scene-level map, bincode struct field 금지 lesson)
- ADR-095 (Reference PointCloud / isolated-vertex snapshot 선례)
- ADR-103 (Z-up) / ADR-191 LOCKED #79 (transaction-aware exec)
- ADR-210 (Draw 메뉴) / ADR-046 P31 #4 (additive) / ADR-087 K-ζ (시연 게이트)
- LOCKED #5 (spatial-hash dedup) / LOCKED #7 #63 (cardinal SSOT)
- LOCKED #44 (Complete Meaning per Merge) / 메타-원칙 #4 #5 #6 #13 #14
