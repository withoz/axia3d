# ADR-203 — IFC4.3 Export Strategy (axia-ifc, α spec)

- **Status**: Accepted — α spec (사용자 결재 2026-06-18: 빌드 착수 + 진짜 IFC
  emit + true-IFC self-validation). β-1 (STEP-21 writer 기반 + IfcFacetedBrep
  cube) 후속 커밋.
- **Date**: 2026-06-18 (α spec)
- **Track**: AixxiA 심화전략 1순위 (BIM interop) — 우리 analytic kernel →
  IFC4.3 직렬화. `feedback_aixxia_engine_compare.md` / `feedback_ifc_export_feasibility.md`.
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

우리 엔진은 STEP/IGES **import** (axia-foreign + occt.js) 는 있으나 어떤
**export** 도 없다. AixxiA Engine(D:\AixiAcad\engine) 비교에서 IFC4.3 export 가
P1 (건축/디자인) BIM interop 의 첫 unlock 으로 도출됐다. ADR-035 §P20.A 는 IFC 를
"별도 ADR" 로 명시 defer — 본 ADR 이 그 신 scope 를 확립한다 (ADR-035 Export
non-goal 의 *역전이 아니라* 신규 scope, 메타-원칙 #10 정합).

### 1.1 Feasibility 시뮬레이션 (2026-06-18, building block 5종 코드 검증)

| building block | 상태 | 근거 |
|---|---|---|
| AnalyticSurface 8종 | full | `surfaces/mod.rs:73-144` — 전 variant 데이터 접근 |
| AnalyticCurve | full | `curves/mod.rs` — Line/Circle/Arc/Bezier/BSpline/NURBS |
| DCEL face 순회 | full | `collect_loop_verts`/`collect_loop_hes` production-ready |
| citizenship/material | full | Xia(face_ids+material)/Shape, MaterialLibrary |
| **axia-foreign 골격** | **MISSING** | **import 전용** — STEP-21 writer 0건 |

**핵심 truth-over-estimate**: 이전 추정의 "axia-foreign reverse-export 50% 보유" 는
**반증**됐다. `promote_step_*` (parse→Analytic) + classify dispatch 만 존재,
`to_step_string`/writer/Serialize-to-STEP = 0 (`grep export|emit|to_step` = 0).
→ STEP-21 writer + IFC entity 레이어는 **처음부터 빌드** (가장 큰 gap). 단
geometry 추출 (가장 어려운 부분) 은 80% 검증 완료 → **feasibility YELLOW** (실제
gap 이나 scoped vertical slice), **MVP 8–12주**.

### 1.2 핵심 결정 — 진짜 IFC emit (사용자 결재 2026-06-18)

β-1 design 시뮬레이션이 강제 fork 를 드러냈다: **axia-foreign(STEP 파서)는 IFC
엔티티명(`IFCCARTESIANPOINT`)을 re-import 못 한다** (`classify_*_entity` 는 STEP
AP203/214/242 대문자 정확 매치, IFC4X3 미인식 → Unsupported). 따라서:

- **(A) 진짜 IFC** (채택): `IFCCARTESIANPOINT`/`IFCFACETEDBREP`/`IFCWALL`
  (IFC4X3 schema). Revit/ArchiCAD/FreeCAD 에서 열림 = BIM interop 목표 직결.
  axia-foreign round-trip 불가 (IFC명 미인식, 게다가 곡선/곡면 geometry 만
  promote 하지 brep topology 는 아님) → **자체 구조 well-formedness 검증** + 외부
  IFC 도구 검증(ε).
- (B) STEP AP203 (거부): `CARTESIAN_POINT` — axia-foreign round-trip 되나 진짜
  IFC 아니라 BIM 도구에서 안 열림 (목표와 괴리).

## 2. Decision

**신 crate `crates/axia-ifc`** 가 우리 DCEL + AnalyticSurface/Curve + Shape/Xia
+ Material 를 **IFC4.3 STEP-21** 로 직렬화한다. β-1 = **IfcFacetedBrep** (point +
polyloop, analytic 없음) writer 기반, MVP target = **IfcAdvancedBrep** (analytic
Plane/Cylinder/... + IfcEdgeCurve). 진짜 IFC4X3 엔티티명, 결정적 byte-identical
출력, MVP 전부 IfcWall (origin_hint 후속).

### 2.1 Lock-ins

- **L-203-1** 신 crate `axia-ifc` (axia-foreign 는 **import 전용 보존** — parser/
  writer 분리). axia-geo AnalyticCurve/Surface + axia-core Scene/Xia/Material 만
  의존, axia-foreign 미의존.
- **L-203-2** **결정적 byte-identical 출력**: 순차 `EntityRef(#N)` (insertion-
  ordered `Vec`, HashMap 금지), 결정적 GUID seed (OS time/getrandom 금지),
  hardcoded formatter. 같은 모델 → 같은 `.ifc` 바이트.
- **L-203-3** **StepValue format SSOT**: `fmt_real`/`fmt_string`/`fmt_enum`/
  `fmt_ref`/`fmt_list`/`fmt_unset`/`fmt_derived`/`fmt_typed` 단일 모듈. REAL =
  trailing-dot (`1.` for int-valued) + sci-notation (abs<1e-6 \|\| abs≥1e6),
  STRING = `''` escape + 비-ASCII `\X2\HHHH\X0\`, ENUM = `.IDENT.`, REF = `#N`.
- **L-203-4** **진짜 IFC4X3 엔티티명** (IFCCARTESIANPOINT 등), `FILE_SCHEMA
  (('IFC4X3'))`. STEP AP203 명 혼용 금지 (시뮬레이션 예시의 schema/엔티티명
  불일치 차단).
- **L-203-5** **true-IFC self-validation**: β-1 검증 = 자체 구조 well-formedness
  (모든 #N 참조 해소 + in-range, header 유효, 닫힌 entity graph) + geometric
  정확성 (cube 8 좌표). 외부 IFC 도구(IfcOpenShell/Revit) 검증은 ε.
- **L-203-6** **Path Z ATOMIC**: 각 sub-step 은 complete shippable 의미 단위
  (LOCKED #44). β-1 = StepWriter + formatters + IfcEntity trait + IfcFacetedBrep
  cube emitter + self-validation 테스트 + Cargo.toml 전부.
- **L-203-7** **ADDITIVE 신규 scope** — LOCKED 역전 0 (ADR-035 §P20.A 의 IFC
  defer 를 충족, Export non-goal 정책 자체 변경 아님). 메타-원칙 #10 정합.
- **L-203-8** **절대 #[ignore] 금지**. 회귀 자산 단조 증가.
- **L-203-9** **MVP 한계 수용** (out-of-scope, §3): assembly/composition, PMI/
  GD&T, Material cost/thickness (필드 부재), origin_hint (전부 IfcWall), trim
  boundary 시맨틱.

## 3. Scope / Out-of-scope

**In-scope (MVP)**: IfcFacetedBrep(β-1) + IfcAdvancedBrep(analytic Plane/Cylinder/
Sphere/Cone/Torus/BSpline/NURBS surface) + IfcEdgeCurve(Line/Circle/Arc/NURBS) +
deterministic GUID + Xia→IfcWall + IfcMaterial(name) + 최소 spatial(Project/Site/
Building/Storey 단일) + 단위/OwnerHistory.

**Out-of-scope** (후속 ADR): assembly/composition hierarchy, Drawing view, PMI/
GD&T/annotation, Material cost·thickness (`material.rs` 필드 부재 → IfcCostValue
미지원), element-type 구분 (Shape/Xia 에 origin_hint 없음 → MVP 전부 IfcWall),
trim loop 시맨틱 (curve 는 edge geometry 로만, IfcRectangularTrimmedSurface 후속).

## 4. Path Z 로드맵 (~8주)

| sub-step | 산출물 | 추정 |
|---|---|---|
| **α** (본 커밋) | spec + lock-in + roadmap + 진짜-IFC 결재 | docs only |
| **β-1** | crate `axia-ifc` scaffold + StepWriter + StepValue/formatter SSOT + IfcEntity trait + ~22 core type + **IfcFacetedBrep cube emitter** + self-validation 테스트 | 6일 |
| **β-2** | IfcAdvancedBrep + analytic surface (IfcPlane/Cylindrical/Spherical/... + IfcAxis2Placement3D + IfcDirection) + IfcEdgeLoop/OrientedEdge/EdgeCurve(LINE) | 5일 |
| **β-3** | AnalyticCurve → IfcLine/Circle/TrimmedCurve/BSpline/RationalBSpline edge curve + Bezier→BSpline knot 합성 | 7일 |
| **γ** | Xia 열거 → IfcWall + IfcMaterial(name) + IfcProductDefinitionShape/ShapeRepresentation | 3일 |
| **δ** | spatial hierarchy (Project→Site→Building→Storey→Element) + IfcRelContainedInSpatialStructure + IfcRelAggregates | 4일 |
| **ε** | 외부 IFC 검증 (IfcOpenShell/Revit/FreeCAD), corpus round-trip, 사용자 시연 (ADR-087 K-ζ) | 5일 |

각 sub-step 별도 atomic PR + 사용자 결재 (메타-원칙 #5/#16).

## 5. β-1 설계 (검증된 구체안)

**Crate** `crates/axia-ifc` (deps: axia-geo, axia-core, glam, anyhow). 모듈:
`lib.rs` / `step_writer.rs` (StepWriter, register/emit) / `step_value.rs`
(StepValue + formatter SSOT) / `ifc_entity.rs` (IfcEntity trait + core impl) /
`ifc_facetedbrep.rs` (cube emitter) / `guid.rs` (결정적 22-char GUID).

```rust
pub struct StepWriter { entities: Vec<(EntityRef, Entity)>, next_id: u32, /* header */ }
pub struct EntityRef(u32);
pub enum StepValue { Ref(EntityRef), Int(i64), Real(f64), Str(String), Enum(String),
                     List(Vec<StepValue>), Unset, Derived, Typed(String, Vec<StepValue>) }
pub trait IfcEntity { fn type_name(&self) -> &'static str; fn attrs(&self, w: &mut StepWriter) -> Vec<StepValue>; }
// register<E: IfcEntity>(&mut self, e: E) -> EntityRef  → 순차 #N, 결정적.
```

**IfcFacetedBrep cube** (~43 entity): IFCCARTESIANPOINT ×8 → IFCPOLYLOOP ×6 →
IFCFACEOUTERBOUND ×6 → IFCFACE ×6 → IFCCLOSEDSHELL → IFCFACETEDBREP + 최소
spatial/owner/unit. 진짜 IFC4X3:

```
ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('AXiA IFC4.3 unit cube (IfcFacetedBrep, β-1)'),'2;1');
FILE_NAME('cube.ifc','2026-06-18T00:00:00',('AXiA'),('AXiA 3D'),'axia-ifc','axia-ifc','');
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCCARTESIANPOINT((0.,0.,0.));
... (8 점)
#11=IFCPOLYLOOP((#1,#2,#3,#4));
... (6 loop)
#21=IFCFACEOUTERBOUND(#11,.T.);
#31=IFCFACE((#21));
#41=IFCCLOSEDSHELL((#31,#32,#33,#34,#35,#36));
#42=IFCFACETEDBREP(#41);
... (spatial/owner/unit)
ENDSEC;
END-ISO-10303-21;
```

**β-1 self-validation 테스트**: emit cube → (1) 모든 `#N` 참조 in-range·해소, (2)
header/DATA 구조 유효, (3) 8 IFCCARTESIANPOINT 좌표 = unit cube, (4) byte-
identical (2회 emit 동일). 외부 IFC 도구는 ε.

## 6. Risks

- **R1 formatter 정확성** (REAL trailing-dot/sci-notation, STRING escape) — SSOT +
  단위 테스트로 차단.
- **R2 결정성 깨짐** (register 순서 비결정) — caller(DCEL 순회) 가 결정적 순서
  보장 (`collect_loop_hes` 이미 결정적).
- **R3 GUID 비결정** (RNG seed) — geometry hash seed (OS time 금지).
- **R4 IFC 시맨틱 검증 gap** — β-1 self-validation 은 구조만, 진짜 IFC
  적합성은 ε 외부 도구. β-1 은 "well-formed STEP-21 + 진짜 IFC4X3 엔티티명"
  까지만 보장.
- **R5 reverse-ref 무결성** (IfcWall→Shape→Repr→Brep) — γ/δ 에서 entity graph
  closure 테스트.

## 7. Cross-link

- ADR-035 §P20.A (IFC defer to 별도 ADR — 본 ADR 이 충족), §P20.B (Export
  non-goal — 신규 scope, 역전 아님)
- ADR-036 P21.1/P21.2 (curve/surface 매핑 11+12 — IFC emit 의 역방향 참조 테이블)
- ADR-031 Phase D (AnalyticSurface 8종), ADR-027~033 (NURBS kernel)
- ADR-050 (Shape/Xia citizenship), ADR-098/099 (material)
- LOCKED #44 (Complete Meaning per Merge — Path Z atomic), #66 (STATUS-POLICY +
  catalog CI), 메타-원칙 #4 (SSOT) / #5 / #6 / #10 / #16
- `web/src/export/DxfWriter.ts` (exporter+writer 패턴 precedent)
- `feedback_ifc_export_feasibility.md` / `feedback_aixxia_engine_compare.md` (memory)

## 8. β-1.5 Acceptance (2026-07-19) — live-scene FacetedBrep + app wiring

첫 **동작하는** IFC export. β-1(box writer) 위에서 실제 씬을 내보낸다.

- **axia-ifc**: box emitter 를 공유 `emit_brep(points, face_loops)` 코어로
  리팩터 + `emit_faceted_brep(positions, tris)` 추가 (crate 는 `glam`-only 유지 —
  DCEL 은 wasm 이 이미 테셀레이트한 렌더 버퍼(`cached_positions_f64`/`cached_
  indices`)로 넘김, 곡면도 OBJ/STL 처럼 faceted).
- **axia-wasm**: `export_ifc(name) -> String` (engine mm → IFC metre ×0.001,
  빈 씬 → ""). axia-ifc 를 workspace + axia-wasm dep 로 승격 (이전엔 런타임 dead).
- **web**: WasmBridge `exportIfc` + 파일▸내보내기▸IFC + `ExportUtils.downloadText`
  + ActionCatalog/CommandCatalog `export-ifc` (AC⊇CC, ADR-133).
- **최초 DCEL→interchange 경로** (기존 4 exporter 는 Three.js 씬 소스).
- **라이브 검증** (박스 2×3×4m, ADR-087 K-ζ): 3611-byte IFC4X3, 0 dangling
  ref (모든 #N 해소), 좌표 metre, 25 pt / 12 폴리루프 / IFCFACETEDBREP +
  IFCWALL, bare-STEP 아님 → Revit/ArchiCAD 호환.
- **회귀**: axia-ifc +3 (`faceted_brep_tetrahedron_well_formed` /
  `faceted_brep_byte_identical` / `box_via_emit_brep_matches_faces`),
  CatalogConsistency 187→188. 절대 #[ignore] 금지.
- **한계 (후속)**: 곡면이 flat facet — analytic IfcAdvancedBrep 은 β-2; 전부
  단일 IfcWall — element-type(origin_hint) 후속; tri-soup 이라 planar face 도
  삼각화 — analytic Plane→clean IfcFace 는 β-2.

## 9. β-2 Acceptance (2026-07-19) — IfcAdvancedBrep emitter + analytic surfaces

β-1(FacetedBrep emitter)에 이어 **analytic B-rep emitter** 를 추가. β-1→β-1.5
리듬 답습 — **본 커밋은 순수 emitter + 테스트 (crate-only), WASM/앱 wiring 은
β-2.5** (DCEL 순회 export 신설, 별도 원자 단계).

- **axia-ifc**: 신 `ifc_advancedbrep.rs` — `emit_advanced_brep(faces, scale, name)
  -> Result<String,String>`. 각 `AdvancedFace` = { `surface: AnalyticSurface`,
  `outer`, `inners`, `same_sense` } (engine mm, emitter 가 `scale` 로 metre 변환).
  - **surface 매핑 (SSOT, axia-geo `AnalyticSurface` 직접 사용, L-203-1)**:
    Plane→`IFCPLANE`, Cylinder→`IFCCYLINDRICALSURFACE`, Sphere→`IFCSPHERICALSURFACE`,
    Cone→`IFCCONICALSURFACE`(apex→reference-plane 변환, v_range 기반 R+semiangle),
    Torus→`IFCTOROIDALSURFACE`. 각 surface 는 `IFCAXIS2PLACEMENT3D`(실제 축) 부여.
  - **경계 (β-2 = LINE only)**: `IFCEDGELOOP`(`IFCORIENTEDEDGE`→`IFCEDGECURVE`(
    `IFCLINE`+`IFCVECTOR`) + `IFCVERTEXPOINT`). `IFCADVANCEDFACE(bounds, surface,
    same_sense)` → `IFCCLOSEDSHELL` → `IFCADVANCEDBREP`.
  - **NURBS-class surface (BezierPatch/BSpline/NURBS) → Err** (β-3 의
    `IfcBSplineSurfaceWithKnots` 필요).
- **공유 스캐폴드 SSOT**: owner/units/context prologue + product/spatial epilogue
  + `pt`/`dir`/`placement`/`placement_axes` 를 신 `ifc_common.rs` 로 추출. faceted
  brep(β-1.5)+advanced brep(β-2) 공유. `RepresentationType` 만 `"Brep"` vs
  `"AdvancedBrep"` 로 분기. 리팩터 후 faceted brep **byte-identical 보존**
  (기존 회귀 유지).
- **핵심 값** — planar face 는 **기하학적으로 정확**: box 가 β-1.5 의 12 삼각형
  대신 **6 clean `IfcAdvancedFace(IfcPlane)`** (4-edge loop) 로 export. 곡면 face 는
  surface 는 정확하나 trim edge 는 LINE 근사 (곡선 edge = β-3).
- **회귀**: axia-ifc **+10** (24→34, 절대 #[ignore] 금지):
  `advanced_box_six_planar_faces`(6 face/plane/edgeloop + 24 edge/line/vertexpoint
  + refs resolve + not-faceted) / `advanced_brep_byte_identical` /
  `scale_converts_mm_to_metre` / `surface_{cylinder,sphere,cone,torus}_maps_*`(4) /
  `nurbs_surface_rejected` / `degenerate_loop_rejected` / `empty_faces_rejected`.
  axia-wasm 무회귀 (public API `emit_faceted_brep` 불변).
- **검증**: crate 단위 테스트 (구조 well-formedness + surface 매핑 + 단위 변환 +
  결정성). 실제 앱 export 는 β-2.5 wiring 후 (ADR-087 K-ζ 시연).
- **한계 (후속)**:
  - **β-2.5 wiring** — WASM 이 active DCEL face 를 순회하며 (`collect_loop_verts`
    outer+inner + `getFaceSurfaceJson`/face surface read + same_sense 계산)
    `emit_advanced_brep` 호출; 모든 face 가 지원 surface 면 advanced, 아니면 β-1.5
    faceted fallback.
  - **β-3** — 곡면 face 의 곡선 edge (`IfcCircle`/`IfcTrimmedCurve`/`IfcBSplineCurve`)
    → 곡면도 기하학적으로 정확. self-loop rim(Path B) 도 이때.
  - edge sharing (현재 per-face island — watertight 위상 공유는 후속),
    BSpline/NURBS surface (`IfcBSplineSurfaceWithKnots`).

## 10. β-2.5 Acceptance (2026-07-19) — live DCEL → IfcAdvancedBrep wiring

β-2 emitter 를 실제 앱에 연결. β-1→β-1.5 와 동일 리듬 (emitter → wiring).
**파일▸내보내기▸IFC** 가 이제 analytic advanced brep 을 우선 시도하고, 비지원
face 가 있으면 β-1.5 faceted 로 자동 fallback.

- **axia-ifc**: `emit_advanced_brep_from_mesh(&Mesh, scale, name) -> Result<..>` —
  live DCEL 을 직접 순회. active face 마다 `mesh.face_surface` +
  `collect_loop_verts`(outer + `face.inners()`) + `mesh.vertex_pos` 로
  `AdvancedFace` 구성, `same_sense` 는 Newell(outer) · `AnalyticSurface::
  normal_at_world_pos`(경계 정점) 로 계산 (ADR-140 SSOT). extraction 이
  axia-ifc 에 있어 `Mesh::create_box` 로 **Rust 단위 테스트 가능** (CI 커버).
  - **All-or-nothing**: active face 하나라도 지원 surface(Plane/Cyl/Sph/Cone/
    Torus)가 없거나 straight-edge loop 불가(**Path B 곡면 rim = 1-vertex
    self-loop** → 곡선 edge β-3 필요)면 Err → caller 가 faceted fallback.
    Planar 모델(box/extrude polygon)은 여기서 정확한 advanced 로 export.
- **axia-wasm**: `exportIfcAdvanced(name) -> String` — `self.scene.mesh` 를
  `emit_advanced_brep_from_mesh` 에 전달, Err/empty → "".
- **web**: `WasmBridge.exportIfcAdvanced` + MenuBar `export-ifc` 가
  `advanced ?? faceted` 로 라우팅 + Toast "IFC 내보내기 완료 (analytic)" vs
  faceted. 새 action/catalog entry 없음 (기존 `export-ifc` 업그레이드,
  additive). i18n +1.
- **라이브 검증** (preview, real WASM, ADR-087 K-ζ):
  - box 2×3×4m → `exportIfcAdvanced` = **IfcAdvancedBrep, 6 IFCADVANCEDFACE
    + 6 IFCPLANE + 6 IFCEDGELOOP + 24 IFCEDGECURVE, faceted 없음, IFC4X3,
    IfcWall** (β-1.5 의 12 삼각형 → clean 6 planar face).
  - box + Path B sphere → `exportIfcAdvanced` = **""** (곡면 self-loop rim
    비지원) → MenuBar 가 `exportIfc` = **IfcFacetedBrep** 로 fallback.
- **회귀**: axia-ifc **+4** (34→38, 절대 #[ignore] 금지):
  `box_mesh_exports_six_planar_advanced_faces` / `box_mesh_faces_are_same_sense_
  outward`(전 face `.T.`) / `empty_mesh_errors_for_faceted_fallback` /
  `box_mesh_export_byte_identical`. web: WasmBridge/CatalogConsistency/i18n
  guards green, vite build green.
- **한계 (후속)**: 곡면 모델은 아직 faceted (β-3 곡선 edge → 곡면 advanced),
  mixed advanced+faceted brep (현재 all-or-nothing), per-face island edge
  (watertight 위상 공유 후속), 전부 단일 IfcWall (γ).

## 11. β-3 Acceptance (2026-07-19) — curved edge curves (IfcCircle) → 곡면 정확 export

β-2.5 의 곡면 faceted fallback 제거. 경계 **edge 의 AnalyticCurve** 를 IFC 곡선
으로 매핑 → cylinder/sphere/cone/torus 가 **정확한 IfcAdvancedBrep** 으로 export.

- **axia-ifc**: `AdvancedFace` 를 vertex-polygon 에서 **edge-loop** (`Vec<IfcEdge>`)
  으로 진화. `IfcEdge { start, end, curve: EdgeCurve }`, `EdgeCurve = Line |
  Circle{center,radius,normal,basis_u} | Arc{..,ccw}`. `AdvancedFace::planar(
  verts)` 시그니처 보존 (내부적으로 Line edge loop 생성 → 기존 planar 테스트 무변경).
  - **curve 매핑**: Line→`IFCLINE`, Circle/Arc→`IFCCIRCLE` (edge 의 start/end 정점
    이 trim — **self-loop rim = whole circle**). Arc 는 `ccw`→`IfcEdgeCurve.
    SameSense`. Bezier/BSpline/NURBS edge → Err (β-3b, `IfcBSplineCurveWithKnots`).
  - **mesh 추출**: `loop_edges` 가 `collect_loop_hes` 로 경계 half-edge 순회 —
    각 he 의 `dst` (정점) + `edge_curve(he.edge())` 로 `IfcEdge` 구성.
    self-loop rim (1 half-edge, dst==anchor) → `start==end` 의 Circle edge.
  - Path B cylinder = base(Plane)+top(Plane)+side(Cylinder), rim = `IFCCIRCLE`
    self-loop → **3 IfcAdvancedFace 정확 export**. sphere = 2 hemisphere(Sphere)
    + 적도 circle.
- **WASM/web 변경 0** — `exportIfcAdvanced` 가 이미 `emit_advanced_brep_from_mesh`
  호출, 본 커밋은 emitter 가 곡면을 지원하도록 개선만 (β-2.5 wiring 재사용).
- **라이브 검증** (real WASM, ADR-087 K-ζ):
  | 모델 | 결과 (β-2.5 → β-3) |
  |---|---|
  | Path B cylinder | faceted fallback → **1 IFCCYLINDRICALSURFACE + 2 IFCPLANE + 5 IFCCIRCLE, 0 line, faceted 없음** |
  | Path B sphere | faceted fallback → **2 IFCSPHERICALSURFACE + 적도 IFCCIRCLE** |
  | box+cyl+sphere 혼합 | 전부 faceted → **11 IfcAdvancedFace: 8 IFCPLANE + 24 IFCLINE(box) + 3 곡면 surface + 8 IFCCIRCLE, faceted 없음** |
- **회귀**: axia-ifc **+4** (38→42, 절대 #[ignore] 금지):
  `emitter_circle_self_loop_disk`(단일 closed circle edge → IFCCIRCLE, 0 line) /
  `path_b_cylinder_exports_analytic_circles`(3 face + 1 cyl + 2 plane + circle,
  not faceted) / `bezier_bspline_nurbs_edge_curves_deferred_to_beta3b`(β-3b Err) /
  `arc_edge_maps_to_circle_with_sense`. 기존 box planar 테스트(Line edge) 무변경.
  axia-wasm/web 무회귀 (Rust-only, API 시그니처 불변).
- **한계 (후속)**: **β-3b** Bezier/BSpline/NURBS edge (`IfcBSplineCurveWithKnots`
  /`IfcRationalBSplineCurveWithKnots` + Bezier→BSpline knot 합성) → closed-Bezier/
  BSpline/NURBS face 모델. BSpline/NURBS *surface* (`IfcBSplineSurfaceWithKnots`).
  per-face island edge (watertight 위상 공유). 전부 단일 IfcWall (γ).

## 12. γ Acceptance (2026-07-19) — 부재별 IfcWall + IfcMaterial (semantic BIM)

β-3 까지 전체 모델 = **단일** IfcWall 이었으나, γ 는 **부재별 IfcWall** + 재질 로
분해 — 진짜 BIM 워크플로우 (Revit/ArchiCAD 에서 부재·재질 인식).

- **axia-ifc**: 신 `ifc_model.rs` — `emit_ifc_model(mesh, elements: &[IfcElement],
  scale, project_name)`. `IfcElement { name, material_name: Option<String>,
  face_ids }`. 하나의 `Project→Site→Building→Storey` 아래에 element 마다:
  advanced geometry (β-3, 자기 face subset) + `IfcShapeRepresentation`/
  `IfcProductDefinitionShape` + `IFCWALL` + (재질 있으면) `IFCMATERIAL` +
  `IFCRELASSOCIATESMATERIAL`. 재질은 name 으로 dedup (한 `IfcMaterial` → 여러 wall).
  모든 wall 은 하나의 `IfcRelContainedInSpatialStructure` 로 storey 에 포함.
  - `emit_advanced_geometry` (geometry-only) + `advanced_faces_filtered`
    (face subset) 를 β-3 emitter 에서 추출 — 단일-wall 경로 (β-1.5/β-3) **byte-
    identical 보존**. axia-ifc 는 Scene-agnostic (generic `IfcElement`).
- **axia-wasm**: `exportIfcModel(name)` — `self.scene` 에서 element 열거:
  **Xia** (id 정렬) → named wall + `material_library.get(xia.material).name`
  (FORM_MATERIAL(0) → None), **Shape** (id 정렬) → named wall (무재질),
  나머지 active face → 단일 "Model" wall. face 는 claimed-set 으로 정확히 1
  element 에만 (중복 0). 결정적 순서 (id 정렬 + SlotStorage).
- **web**: `WasmBridge.exportIfcModel` + MenuBar `export-ifc` 가 `exportIfcModel
  ?? exportIfc` 로 라우팅 (부재별 우선, 비지원 → faceted single wall fallback).
  additive (기존 export-ifc 업그레이드, 신규 action/catalog/i18n 0).
- **라이브 검증** (real WASM, ADR-087 K-ζ):
  - rect Shape (form) → `IFCWALL('Rectangle')`, 재질 0 (Xia 아님 → 무재질).
  - rect → `create_solid_extrude` (solid) → `promoteShapeToXia(_, 1)` (강철) →
    `exportIfcModel` = **1 IFCWALL + 1 IFCMATERIAL('강철') + 1 IFCRELASSOCIATES
    MATERIAL**, `IFCMATERIAL('\X2\AC15CCA0\X0\')` = 한글 "강철" 정확 인코딩,
    wall↔material ref 연결, advanced brep, faceted 없음.
- **회귀**: axia-ifc **+5** (42→47, 절대 #[ignore] 금지):
  `two_elements_two_walls_two_materials`(2 wall + 2 material + assoc + 1 storey) /
  `shared_material_deduplicated`(1 material, 2 assoc) / `element_without_material_
  has_no_association` / `deterministic_byte_identical` / `empty_elements_rejected`.
  단일-wall byte-identical 테스트 무변경. axia-wasm 85 pass, web guards
  (WasmBridge/CatalogConsistency/i18n/MenuBar 669) + vite build green. Cargo.lock
  변경 0.
- **한계 (후속)**: element-type 은 전부 `IfcWall` (Shape/Xia 에 origin_hint 없음
  → IfcSlab/IfcColumn/IfcBeam 구분 별도 ADR). per-element advanced-or-nothing
  (한 element 라도 비지원 → 전체 faceted fallback; per-element faceted 는 후속).
  재질은 name 만 (색상/물성/layered → IfcMaterialProperties 후속). β-3b NURBS
  edge/surface. 외부 IFC 검증 (ε).

## 13. ε Acceptance (2026-07-20) — 외부 IFC 엔진 검증 (L-203-5 closure)

β-1.5~γ 의 검증은 전부 **우리가 생각하는 IFC** 기준 (자체 구조 well-formedness
+ 라이브 엔진 카운트) 이었다. ε 는 파일을 **독립 구현체** 에 넘겨 실제로 읽히는지
확인한다 — L-203-5 가 ε 로 명시 defer 했던 항목.

- **외부 엔진**: `web-ifc` 0.0.77 (IFC.js / ThatOpen 뷰어의 C++ IFC 엔진, npm).
  우리 코드와 완전 독립. *환경 제약*: Revit/ArchiCAD/FreeCAD 는 데스크톱 상용/
  GUI 라 CI 불가, `ifcopenshell` 은 이 환경의 Python 3.14 용 wheel 부재 →
  **web-ifc 가 현실적으로 가장 강한 독립 검증**. 상용 BIM 실검증은 사용자 시연 몫.
- **하네스** (재현 가능, `npm run validate:ifc` / CI `.github/workflows/ifc.yml`):
  `scripts/ifc-external-validate.mjs` 가 **실엔진(wasm-pack --target nodejs)** 으로
  코퍼스를 헤드리스 생성 → web-ifc 로 파싱 → 스키마/계층/이름/**기하 tessellation**
  까지 assert. 진짜 round-trip (engine → .ifc → foreign parser). `web-ifc` 는
  루트 devDependency (0.0.77 핀).
- **결과 — 통과 (전 항목)**:
  | 코퍼스 | 외부 파서 결과 |
  |---|---|
  | box (평면 solid) | 열림 · **schema IFC4X3 인식** · Project/Site/Building/Storey 각 1 · IfcWall "Box" · 6 IfcPlane · **기하 12 삼각형 (박스 정확)** |
  | curved (Path B 원통+구) | 열림 · IfcWall "Cylinder"/"Sphere" · IfcCylindricalSurface + IfcSphericalSurface + IfcCircle 파싱 · 기하 62 삼각형 |
  | bim (Xia+재질) | 열림 · IfcWall "Rectangle" · **IfcMaterial "강철" — 한글 `\X2\` 인코딩이 외부 리더에서 정확 역디코딩** · IfcRelAssociatesMaterial 1 |
  - 세 코퍼스 모두 `IfcAdvancedBrep` (faceted fallback 0) 로 나가고 외부 커널이
    tessellate 함 → **analytic B-rep 이 실제로 소비 가능**함이 처음 증명됨.
- **발견 — 외부 커널 surface 지원 매트릭스** (per-surface 격리 측정):
  | surface | web-ifc |
  |---|---|
  | `IfcPlane` | ✅ tessellate |
  | `IfcCylindricalSurface` | ✅ tessellate (42 tri) |
  | `IfcSphericalSurface` | ⚠️ `GetSurface() unexpected surface type` → skip |
  | `IfcConicalSurface` | ⚠️ 동일 |
  | `IfcToroidalSurface` | ⚠️ 동일 |
  - **우리 파일은 spec-valid** (IFC4 elementary surface 를 `IfcAdvancedFace.
    FaceSurface` 로 정상 사용). web-ifc 의 geometry kernel 이 Plane/Cylinder 만
    구현한 **downstream 커버리지 갭**. 파싱·엔티티 열거는 정상.
  - **결정: fidelity 다운그레이드 안 함** — 한 도구의 갭 때문에 β-3 이 확보한
    analytic 정밀도를 faceted 로 되돌리지 않는다. "interop 우선 모드"(구/원뿔/
    토러스만 faceted) 가 필요하면 별도 ADR (fidelity ↔ interop trade-off).
- **회귀**: 신규 CI job `External IFC validation (web-ifc)` — `crates/axia-ifc`
  /`axia-wasm`/`axia-core`/`axia-geo`/스크립트/package.json 변경 시 자동 실행.
  IFC exporter 의 spec 회귀를 외부 구현체로 상시 감시 (기존 axia-ifc 47 단위
  테스트는 우리 기준, 본 job 은 남의 기준).
- **한계 (후속)**: 상용 BIM (Revit/ArchiCAD) 실오픈 미검증 (사용자 시연 게이트).
  `ifcopenshell` 교차 검증 (Python wheel 가용 시 두 번째 독립 구현체). 곡면
  interop 모드. IFC **import** 는 여전히 미착수 (별도 트랙, 계획은
  `docs/plans/IFC-IMPORT-EXPORT-PLAN-2026-07-19.html`).

## 14. β-3b Acceptance (2026-07-20) — spline edges + NURBS-class surfaces

β-3 까지 Bezier/BSpline/NURBS 는 **거부 → faceted fallback** 이었다. β-3b 로
export 기하의 마지막 gap 을 닫는다 — 이제 모든 axia-geo curve/surface variant
가 analytic IFC 로 나간다.

- **Edges**: `AnalyticCurve::{Bezier, BSpline, NURBS}` → 통합
  `EdgeCurve::BSpline { control_pts, knots, degree, weights }` →
  `IFCBSPLINECURVEWITHKNOTS` (weights 있으면 `IFCRATIONALBSPLINECURVEWITHKNOTS`).
  Bezier 는 clamped knot vector 합성 (`degree = n-1`, `[0]×(d+1) ++ [1]×(d+1)`).
- **Surfaces**: `BezierPatch` / `BSplineSurface` / `NURBSSurface` →
  `IFCBSPLINESURFACEWITHKNOTS` / `IFCRATIONALBSPLINESURFACEWITHKNOTS`.
  BezierPatch 는 grid 크기에서 degree 유도 + clamped knots 합성.
- **Knot 표현 변환**: 엔진은 flat knot vector (반복 포함), IFC 는 *distinct
  values + multiplicities* → `compress_knots` 로 변환
  (`[0,0,0,.5,1,1,1]` → `(0,.5,1)` + `(3,1,3)`).
- **방어**: weights/control_pts 길이 불일치, ragged control grid, 빈 knots,
  control point < 2 → 모두 Err (조용한 잘못된 export 대신 faceted fallback).
- **외부 검증** (ε 인프라 재사용): closed-Bezier / closed-BSpline face 를
  실제 엔진으로 생성 → web-ifc 가 `IFCBSPLINECURVEWITHKNOTS` 를 **인식하고
  기하까지 생성** (`unexpected surface type` 에러 0). β-3b 이전에는 같은
  모델이 `exportIfcModel` = `""` (faceted fallback) 이었다.
- **ε 게이트 확장**: `scripts/ifc-external-validate.mjs` 에 `spline.ifc` 코퍼스
  추가 — 외부 파서가 spline curve 를 읽는지 + **line 으로 격하되지 않았는지**
  (`IFCLINE` 0) 회귀 검사.
- **회귀**: axia-ifc **+5** (47→52, 절대 #[ignore] 금지): spline edge 매핑
  (Bezier clamped knot 합성 / BSpline passthrough / NURBS weights / 길이 불일치
  거부) / knot 압축 / `IFCBSPLINECURVEWITHKNOTS` emit / rational weights emit /
  NURBS surface → rational surface / BSpline+Bezier surface / ragged grid 거부.
  기존 "거부" 테스트 2건은 **지원** 단언으로 재작성 (동작 변경 명시).
- **한계 (후속)**: `NURBSSurface.trim_loops` 는 미emit (경계는 DCEL edge loop
  이 담당 — `IfcRectangularTrimmedSurface` / pcurve 는 별도 트랙). 상용 BIM
  실오픈 미검증. IFC **import** 미착수.

## 15. I-1 Acceptance (2026-07-20) — IFC import 착수: 파일을 *읽는다*

Import 트랙의 첫 원자 단계. 계획(`docs/plans/IFC-IMPORT-EXPORT-PLAN-2026-07-19.html`)
의 de-risk 가 성립함을 코드로 확인 — **새 파서는 필요 없다**.

- **de-risk 확정**: `axia-foreign::step_parser::parse` 는 완전 schema-agnostic
  (ISO envelope strip → tokenize → `#N=TYPE(args);`). `FILE_SCHEMA` 는 단순
  헤더 엔티티일 뿐 gate 가 아니다 → IFC4X3 파일이 그대로 파싱된다.
- **axia-ifc `ifc_analyze.rs`**: `analyze_ifc(src) -> IfcAnalysis`
  { schema, description, entity_count, type_counts(BTreeMap=결정적) } +
  `count()` / `top_types()` / `to_json()`. IFC 지식은 axia-ifc 에 두고
  **axia-foreign 는 IFC-unaware 인 채로 재사용** (파서만 빌려옴).
- **axia-wasm** `analyzeIfc(text) -> JSON` (read-only, scene 무변경).
  **web**: `WasmBridge.analyzeIfc` + `IfcImportHandler.ts` (DxfImportHandler
  패턴 답습) + MenuBar `import-ifc` 의 "준비중" placeholder 제거.
- **정직한 UX**: Toast 가 스키마/엔티티 수/부재 카운트를 보여주고
  "현재는 내용 확인만 가능 (형상 가져오기는 준비 중)" 을 명시 — 모델이
  들어온 것처럼 오해시키지 않는다.
- **라운드트립**: 우리 exporter 출력이 우리 parser 로 읽힌다 (가장 강한 스모크).
  라이브 확인 — box/curved/bim/bezier 코퍼스 4종 모두 `ok=true schema=IFC4X3`,
  walls/materials/advancedBreps 정확, 쓰레기 입력은 `ok:false` 로 안전 거부.
- **CI 갭 2건 해소**: `cargo test -p axia-ifc` 에 이어 **`-p axia-foreign`**
  (138 tests) 도 CI 미실행이었다 → ci.yml 에 추가.
- **회귀**: axia-ifc **+6** (52→58, 절대 #[ignore] 금지): 자체 export
  라운드트립 / 시맨틱 모델(walls+materials+advancedBrep) / top_types 빈도순+
  결정성 / 태그 대소문자 무관 / 쓰레기 입력 거부 / JSON escape.
- **다음 (I-2~I-5)**: entity classifier (IfcWall/IfcSlab → 부재) → brep→DCEL
  promote (IfcAdvancedBrep/IfcFacetedBrep → mesh) → spatial → Scene 배치.

## 16. I-2 Acceptance (2026-07-20) — 부재 분류 (element classifier)

I-1 이 파일을 히스토그램으로 요약했다면, I-2 는 **부재 목록**으로 답한다 —
각 부재의 타입·이름·재질과, 그것이 가리키는 형상까지 IFC 참조 사슬을 따라간다.

```
IfcWall ─Representation→ IfcProductDefinitionShape
         ─Representations→ IfcShapeRepresentation ─Items→ IfcAdvancedBrep / …
IfcRelAssociatesMaterial ─RelatedObjects→ 부재  ─RelatingMaterial→ IfcMaterial
```

- **axia-ifc `ifc_elements.rs`**: `classify_ifc(src) -> ElementReport`.
  `ImportedElement { id, ifc_type, name, global_id, material, geometry }` +
  `GeometryRef { id, kind, representation_type, supported }`.
  17 IFC product 타입 인식 (Wall/Slab/Beam/Column/Door/Window/… + Proxy).
- **정직한 미지원 보고**: I-3 가 변환 가능한 것은 `IfcAdvancedBrep` /
  `IfcFacetedBrep` 뿐. 나머지(예 `IfcExtrudedAreaSolid`)는 조용히 버리지 않고
  `unsupportedGeometry: {tag: count}` 로 노출 + `convertible / total` 집계.
- **결정성**: 부재는 entity id 로 정렬 (HashMap 순회는 무순서).
- **axia-wasm** `classifyIfc(text) -> JSON` (read-only). **web**:
  `WasmBridge.classifyIfc` + Toast 가 "가져올 수 있는 형상: M / N 부재" 와
  부재 미리보기(이름/재질)를 표시.
- **발견하고 고친 진짜 결함 — STEP 문자열 디코딩**: 우리 exporter 는 한글을
  ISO-10303-21 `\X2\HHHH\X0\` 로 쓰는데(외부 web-ifc 는 이를 해독했다),
  **우리 lexer 는 `''` 만 처리하고 제어 지시자를 미해독**해 재질명이
  `\X2\AC15CCA0\X0\` 원문으로 나왔다. 표준 위반이자 STEP import 에도 있던
  버그 → `axia-foreign` lexer 에 `\X2\`(UTF-16, surrogate pair 포함) /
  `\X\HH`(ISO 8859-1) / `\S\c` 해독 추가. 미지·불량 지시자는 **문자를 잃는
  대신 원문 유지**. 라이브 재확인: `Rectangle/강철`.
- **회귀**: axia-ifc **+8** (58→66) — 부재/재질/형상 분류, faceted 경로,
  결정적 정렬, 미지원 형상 보고, 비-부재 무시, JSON 형태, 한글 라운드트립,
  쓰레기 거부. axia-foreign **+3** (138→141) — `\X2\`/`\X\`/`\S\` 해독 +
  미지 지시자 보존. 절대 #[ignore] 금지.
- **다음 (I-3)**: `GeometryRef.supported` 인 항목을 DCEL 로 승격 —
  `IfcAdvancedBrep`/`IfcFacetedBrep` → face/edge/vertex. 형상이 실제로
  들어오는 단계.

---

## 17. I-3 Acceptance — B-rep → DCEL (형상이 실제로 들어온다)

I-2 가 "무엇이 있는지" 를 말했다면, I-3 은 그것을 **씬에 올린다**. IFC 파일을
열면 부재의 면이 엔진의 면이 된다.

```
IfcFacetedBrep  → IfcClosedShell → IfcFace         → IfcFaceOuterBound/Bound → IfcPolyLoop
IfcAdvancedBrep → IfcClosedShell → IfcAdvancedFace → IfcFaceOuterBound/Bound → IfcEdgeLoop
                                                                              → IfcOrientedEdge
                                                                              → IfcEdgeCurve → IfcVertexPoint
```

- **axia-ifc `ifc_geometry.rs`**: `import_ifc_geometry(src) -> GeometryImport`
  (`elements[] { element_id, name, material, faces[] { outer, inners } }`,
  `scale_to_mm`, `warnings`). 단위는 `IfcSIUnit` 의 접두어(MILLI/CENTI/KILO…)
  에서 읽어 mm 로 환산 — 단위가 없으면 metre 로 가정하고 경고를 남긴다.
- **면은 평면을 달고 들어온다** — `FaceLoops::plane()` (Newell). surface 없는
  면은 Push/Pull·Boolean·Offset·advanced 재-export 가 **모두 거부**한다
  (ADR-087 K-ε, LOCKED #34). Newell 을 쓴 이유는 비볼록 loop 와 첫 세 점이
  일직선인 loop 에서도 법선이 정확해서다.
- **axia-wasm `importIfc(text) -> JSON`**: 트랜잭션 1건으로 감싸 **Undo 한
  번**에 통째로 사라진다. 면이 하나도 못 들어오면 스냅샷 복원 + 취소 →
  **씬 무변경**. 성공 시 `{elements, faces, vertices, scaleToMm, warnings}`.
- **web**: `WasmBridge.importIfc` + `IfcImportHandler` 가 분석(I-1) → 분류(I-2)
  → 가져오기(I-3) 후 `syncMesh()`. 실패는 "가져왔습니다" 대신 **이유를 말한다**.

### 발견하고 고친 진짜 결함 — 구멍 뚫은 면이 surface 를 잃었다

라운드트립을 실측하다 **구멍 뚫은 상자를 IFC 로 내보내면 빈 파일**이 나오는
것을 발견했다. 원인은 IFC 가 아니라 엔진이었다 — `punch_rect_hole` /
`punch_circular_hole` / `punch_polygon_hole` 세 곳 모두 host 면을 지우고
다시 만들면서 **material 만 물려주고 analytic surface 를 버렸다**.
ADR-089 A-χ(분할 면의 surface 상속)가 punch 계열에는 적용돼 있지 않았다.

영향은 IFC 보다 넓다. surface 없는 면은 Push/Pull 이 `NoProfileSurface` 로
거부하고(ADR-190 P0.1), Boolean·Offset 도 마찬가지다 — **창을 뚫은 벽은 그
뒤로 밀 수 없었다**. 세 곳에 surface + `face_surface_owner_id` 상속을 추가.
회귀 `punching_a_hole_keeps_the_host_surface` 는 수정을 제거하면 실제로
실패하는 것까지 확인했다.

내보내기 쪽도 같이 정직해졌다. `exportIfcModel` 이 실패하면 빈 문자열을
돌려주는데 `??` 는 빈 문자열을 잡지 못해 "내보낼 형상이 없습니다" 라는
틀린 메시지가 떴다 → **faceted 로 실제 fallback** 하도록 고쳤다.

### 라이브 검증 (실제 엔진, node WASM)

| 항목 | 결과 |
|---|---|
| 좌표 정확도 (박스) | src bbox == dst bbox **완전 일치** |
| 커널 연산 | 가져온 면에 Push/Pull **동작** (bbox 이동, `isClosedSolid=true`, 위반 0) |
| 구멍 | src 6면(inner 1) → dst 6면(inner 1), bbox 동일 |
| 다각형 원통 (24분할) | 26면/48정점 → **26면/48정점** |
| 커널-네이티브 원 (Path B) | 거부 + 이유 명시, **씬 무변경** (0면/0정점) |
| 재-export | 9,430 bytes, `convertible 1/1` |
| Undo | 1회로 전부 복원 (0면/0정점) |
| 쓰레기 입력 | `ok:false` + 씬 무변경 |

### 알려진 한계 (정직하게)

- **커널-네이티브 곡선 경계는 못 읽는다.** `IfcCircle` 은 끝점으로만 읽혀
  자기-루프 rim 이 한 점으로 붕괴 → 그 면은 **버리고 경고**한다. 다각형화된
  곡면은 그대로 들어온다.
- **공간 계층은 아직 안 쓴다** (I-4/I-5). 부재는 좌표 그대로 놓인다.
- `IfcExtrudedAreaSolid` 등 비-B-rep 형상은 I-2 가 보고만 한다.

- **회귀**: axia-ifc **+11** (66→77) — 폴리루프/구멍/축척/단위 없음/퇴화 거부
  (+경고 명시)/평면 유도(Newell·일직선 3점·퇴화)/누락 참조. axia-geo **+1**
  (2270→2271) — punch 3종 surface 상속. 절대 #[ignore] 금지.
- **다음 (I-4)**: `IfcLocalPlacement` 체인을 읽어 부재를 제자리에 놓기.

---

## 18. I-4 Acceptance — 부재를 제자리에 (IfcLocalPlacement)

I-3 은 B-rep 점을 **월드 좌표로** 읽었다. 우리 파일에는 맞다 — 우리는 월드
좌표를 구워 넣고 identity placement 를 쓴다(실측 확인:
`#16=IFCLOCALPLACEMENT($,#13)`, `#13` 은 원점·단위축). **실제 BIM 파일은 다르다.**
Revit/ArchiCAD 는 형상을 부재 자신의 좌표계로 쓰고 placement 체인으로 위치를
준다. 체인을 안 읽으면 **모든 부재가 원점에 쌓인다**.

```
IfcWall.ObjectPlacement → IfcLocalPlacement ─RelativePlacement→ IfcAxis2Placement3D
                                            └PlacementRelTo→ IfcLocalPlacement (층)
                                                             └PlacementRelTo→ … (건물, 대지)
```

- **axia-ifc `ifc_placement.rs`**: `Placement { origin, x, y, z }` (강체 —
  IFC placement 은 스케일·전단이 없다) + `resolve_placement(file, id, scale)`
  가 루트까지 걸어 합성. `axis_placement` 은 `Axis`(Z)/`RefDirection`(X) 이 둘 다
  optional 인 규격을 따르고, **RefDirection 은 축에 수직으로 투영**한다(규격이
  말하는 X 는 RefDirection 자체가 아니라 축에 직교하는 성분이다).
- **정상적으로 실패한다**: 끊긴 참조·순환(깊이 64 cap)·`IfcGridPlacement` 는
  **에러가 아니라 identity** 로 떨어진다 — placement 가 깨진 파일도 형상은
  들어와야 한다.
- **속성 순서는 우리 emitter 를 SSOT 로 검증**: `IFCWALL('guid',#5,'Box',$,$,#16,
  #266,$,$)` → `IfcProduct.ObjectPlacement` = **index 5**.
- **`placed` 카운트**: 실제로 움직인 부재 수를 `importIfc` JSON 과 Toast 에
  노출. "원점에 다 쌓였다" 와 "배치대로 놓았다" 가 사용자 눈에 구분된다.
- **WCS 는 적용하지 않되 침묵하지 않는다**: `IfcGeometricRepresentationContext`
  의 `WorldCoordinateSystem`(index 4) 이 identity 가 아니면 **경고**한다. 대부분
  identity 라 적용 위험을 지기보다 드러내는 쪽을 골랐다.

### 라이브 검증 (실제 엔진, node WASM)

| 파일 | 결과 |
|---|---|
| Revit 형 (local 형상 + 체인: 층 z=3m, 벽 x=10m) | bbox **x∈[10000,11000], z=3000** — 체인대로. `placed:1` |
| 우리 파일 (identity + world 좌표) | bbox **완전 동일**, `placed:0` — 이중 변환 없음 |

### 회귀

- axia-ifc **+13** (77→90): placement 10 — identity/평행이동+단위/Z축 회전/
  체인 합성/**회전한 부모가 자식 오프셋을 회전**/비수직 RefDirection 투영/
  축과 평행한 RefDirection/끊긴·순환 체인/비-LocalPlacement/WCS 보고.
  geometry 3 — **체인대로 배치**(변환을 빼면 정확히 원점으로 무너지는 것까지
  mutation 확인)/**우리 파일 무변경**(이중 변환 가드)/WCS 경고.
- 절대 #[ignore] 금지. 워크스페이스 **2957 passed / 0 failed / 0 ignored**,
  vitest 2935, tsc·build·ADR 카탈로그 모두 통과.

### 알려진 한계

- **WCS 미적용** (경고만). 적용은 별도 판단.
- 공간 구조(`IfcRelContainedInSpatialStructure`) 로 층·건물을 **그룹으로 만들지
  않는다** — 좌표만 맞춘다. 그룹화는 I-5.
- `IfcGridPlacement` 미지원 (identity 처리).

- **다음 (I-5)**: 공간 계층을 씬 그룹으로 — 층/건물별로 묶어서 보이기.

---

## 19. I-5 Acceptance — 공간 구조가 씬 그룹이 된다

I-4 로 부재는 제자리에 놓였지만 여전히 **면 더미**였다. 어느 층에 속한 벽인지
씬이 모르니 층을 숨길 수도, 벽 하나를 통째로 고를 수도 없었다. IFC 는 그
구조를 이미 두 관계로 들고 있다.

```
IfcProject ─IfcRelAggregates→ IfcSite ─IfcRelAggregates→ IfcBuilding
                                                        └→ IfcBuildingStorey
IfcBuildingStorey ←IfcRelContainedInSpatialStructure─ IfcWall, IfcSlab, …
```

- **axia-ifc `ifc_spatial.rs`**: `spatial_tree(file) -> SpatialTree`
  (`nodes: {id → SpatialNode{ifc_type, name, parent}}`, `container_of:
  {element → container}`). 속성 순서는 우리 emitter 로 확인 —
  `IfcRelAggregates(…,4 RelatingObject,5 RelatedObjects)` /
  `IfcRelContainedInSpatialStructure(…,4 RelatedElements,5 RelatingStructure)`.
- **`topological()`** 이 부모-먼저 순서를 준다 → 소비자가 그룹을 만들고 곧바로
  이미 만들어진 부모에 붙일 수 있다. 순환이 있어도 **노드를 잃지 않는다**
  (남은 것은 id 순으로 방출).
- **부재도 그룹이 된다.** 이게 실제 가치 — "벽 하나 통째 선택" 과 "이 층만
  숨기기" 가 가능해진다. 컨테이너 그룹은 비어 있고 면은 부재 그룹을 통해 붙는다.
- **단일 Undo**: 그룹 생성이 `scene.execute` 가 아니라 `scene.groups` 직접
  호출이라 임포트 트랜잭션 **안에서** 일어난다 → Undo 한 번에 형상과 그룹이
  함께 사라진다.
- **없는 구조는 만들지 않는다**: 관계가 없는 부재는 컨테이너 없이 최상위로
  간다. 파일이 말하지 않은 층을 지어내지 않는다.

### 라이브 검증 (실제 엔진, node WASM)

2층 파일 (층마다 벽 하나, 형상은 local + I-4 체인):

```
Tower(project) → Site → Building ├ Level 1 → Wall A (1면)
                                 └ Level 2 → Wall B (1면)
```
`{"elements":2,"faces":2,"placed":1,"groups":7}` · Undo → 면 0 / 그룹 0.

우리 자신의 export (박스 2개): `groups:6` — Own → Site → Building → Storey →
Box(6면) × 2. `get_group_for_face(0)` → 5 로 면↔그룹 역인덱스도 정상.

### 회귀

- axia-ifc **+9** (90→99): spatial 7 — 체인·소속 읽기 / 부모-먼저 정렬 /
  ancestry / **이름 없는 컨테이너 라벨** / 관계 없는 파일 / **순환 aggregation
  이 멈추고 노드를 잃지 않음** / 자기-aggregation 무시. geometry 2 — 부재가
  컨테이너를 안다(mutation 확인) / 컨테이너 없는 부재는 그대로 둠.
- 작성한 테스트가 **진짜 버그 2개**를 잡았다: 태그를 대문자로 저장해 CamelCase
  분해가 무의미했던 라벨(고정 표 조회로 교체), ancestry 가드 off-by-one(길이
  상한 대신 방문 노드 검사로 교체).
- 워크스페이스 **2966 passed / 0 failed / 0 ignored**, vitest 2935, tsc·build·
  ADR 카탈로그 통과. 절대 #[ignore] 금지.

### 알려진 한계

- `IfcSpace` 는 컨테이너로 읽지만 **면적·용도 등 속성은 안 읽는다**.
- 그룹 가시성/잠금은 기존 UI 를 그대로 쓴다 — 임포트가 따로 설정하지 않는다.
- `IfcRelReferencedInSpatialStructure`(참조 소속) 미지원 — 주 소속만.

- **다음 (δ)**: 부재 타입 분화 — 지금은 모두 `IfcWall` 로 나간다. 슬래브·기둥·
  보를 제 타입으로 내보내기.

---

## 20. δ Acceptance — 부재가 제 종류로 나간다

여기까지 내보낸 모든 부재는 `IfcWall` 이었다. 바닥 슬래브도 벽, 기둥도 벽.
Revit 이나 ArchiCAD 에서 열면 **온통 벽으로 지은 건물**로 보였다. 형상은
맞고 의미가 틀린 상태였다.

- **`axia-ifc/ifc_element_kind.rs`**: `IfcElementKind` 13종 — Wall / Slab /
  Column / Beam / Roof / Stair / Ramp / Railing / Covering / Member / Plate /
  Footing / **Proxy**(= "분류하지 않음" 을 IFC 답게 말하는 방법). 모두
  `IfcWall` 과 **속성 형태가 같은**(9개) 타입이다.
- **`IfcDoor` / `IfcWindow` 는 일부러 뺐다.** 이 둘은 속성이 4개 더 있어
  (`OverallHeight` / `OverallWidth` / `OperationType` /
  `UserDefinedOperationType`) 같은 자리에 넣으면 **깨진 엔티티**가 나온다.
  추측 대신 후속으로 남긴다 — `from_tag("IFCDOOR")` 은 `None` 이고, 회귀가
  그것을 잠근다.
- **엔진은 IFC 를 모른다.** `Scene` 은 `xia_element_kind` /
  `shape_element_kind` 에 **정규화된 소문자 키**(`"slab"`)만 담는다. 키 →
  IFC 태그 매핑은 파일 포맷의 몫이라 `axia-core` 는 `axia-ifc` 의존이
  생기지 않는다. 필드가 아니라 **Scene-level BTreeMap**(ADR-091 §E L1 /
  ADR-098 L1) + **스냅샷 섹션 11**(additive).
- **거부하되 저장하지 않는다**: `setXiaElementKind` 는
  `IfcElementKind::from_tag` 로 검증하고 **정규 키만** 저장한다. 모르는
  값이면 `false` — 저장해 두고 조용히 벽으로 나가는 일이 없다.
- **UI**: Inspector "부재 종류" 드롭다운. 목록은 **엔진이 준다**
  (`ifcElementKinds`) — UI 와 엔진이 서로 다른 어휘로 갈라지지 않도록.
  Xia 와 Shape 둘 다 지정 가능 (`getShapeForFace` 신규 — 역인덱스는 ADR-079
  W-1 로 이미 있었고 노출만 안 돼 있었다).

### 만들면서 잡은 것

- **UI 가 조용히 아무 일도 안 했다.** 드롭다운 핸들러가 Inspector 의 캐시
  (`currentFaceIds`)를 읽는데, 그건 다시 그릴 때 갱신된다 — 선택 직후 종류를
  바꾸면 무시됐다. **현재 선택을 직접 읽도록** 고쳤고, 브라우저에서 확인했다.
- **legacy strip 테스트 누적 갱신**(ADR-098 L4): 섹션 11 을 더하니
  `adr091_..._without_section_7d` 가 깨졌다 — 이 테스트는 뒤 섹션을 잘라
  legacy 를 흉내 내는데 **섹션 10(ADR-219)도 이미 빠져 있었다**. 섹션 경계에
  정확히 맞춰 고쳤다. 새 섹션을 추가하면 여기도 같이 고쳐야 한다.
- **i18n 가드가 빠뜨린 문자열 2개를 잡았다** (드롭다운 기본 항목, tooltip).

### 라이브 검증

엔진 (node WASM): `slab` / `IFCCOLUMN`(태그형) 모두 수용 → 정규 키 저장,
`doorway` 는 `false`, 내보내기 **IFCSLAB ×1 / IFCCOLUMN ×1 / IFCWALL ×1**
(미지정), 우리 분류기 왕복도 그대로, 해제하면 벽 복귀.

브라우저 (실제 앱, 포트 4188): 드롭다운 13종이 엔진 목록으로 채워짐 →
"슬래브" 선택 → `IFCSLAB ×1 / IFCWALL ×0` → 해제 → 벽 복귀. 저장·복원 후에도
`column` 유지 → IFCCOLUMN. 콘솔 오류 0.

**외부 IFC 커널 (web-ifc, ε 게이트)**: 새 `typed.ifc` 코퍼스 — 슬래브 1 +
기둥 1, **벽 0**, 그리고 외부 커널이 그 둘을 **실제로 삼각분할**(24 tri).
기존 "IfcWall 하나 이상" 검사는 13종을 세는 **부재 일반** 검사로 바꿨다.

### 회귀

- axia-ifc **+5** (99→104) — 태그·키 왕복 / 중복 없음 / StandardCase 흡수 /
  **미지·Door·Window 거부** / 기본값이 Wall.
- axia-core **+2** — 분류 스냅샷 왕복 / 미분류 씬도 무해.
- axia-wasm **+1** (step6) — 5 엔드포인트 존재 + **정규화 강제** + 내보내기가
  실제로 읽는지.
- vitest **+5** — Inspector 픽커 (엔진 목록 사용 / 라이브 선택 / Shape+Xia /
  거부 시 되돌림 / 템플릿).
- 워크스페이스 **3147 passed / 0 failed / 1 ignored**(선재 slow-channel),
  vitest **2940**, tsc·build·ADR 카탈로그·외부 IFC 검증 모두 통과.

### 알려진 한계

- **`IfcDoor` / `IfcWindow` 미지원** (속성 형태가 다름) — 별도 후속.
- 종류별 `PredefinedType`(예 `IfcSlabTypeEnum.FLOOR`)은 아직 `$`.
- 자동 추론은 하지 않는다 — 사용자가 지정한 것만 쓴다 (메타-원칙 #16).
- 미지정 부재는 여전히 벽으로 나간다 (기존 파일 동작 보존).

---

## 21. δ-2 Acceptance — 문과 창

δ 는 문·창을 **일부러 거부**했다. `IfcDoor` / `IfcWindow` 는 속성이 9개가
아니라 **13개**라, 벽 모양 자리에 그대로 넣으면 **어떤 IFC 리더도 받지 않는
엔티티**가 나온다. 추측 대신 거부하고 후속으로 남겼던 것 — 그 형태를 맞췄다.

```
공통 8   GlobalId OwnerHistory Name Description ObjectType ObjectPlacement Representation Tag
9-arg    + PredefinedType
Door     + OverallHeight OverallWidth PredefinedType OperationType         UserDefinedOperationType
Window   + OverallHeight OverallWidth PredefinedType PartitioningType      UserDefinedPartitioningType
```

- **`IfcElementKind::attribute_count()` / `has_overall_size()`** 신설. emitter
  가 하드코딩된 9 대신 **종류에게 물어본다**. 나머지 11종은 9로 고정 —
  회귀가 그것을 잠근다 (여기가 흔들리면 지금까지 쓴 모든 벽·슬래브의 형태가
  조용히 바뀐다).
- **`OverallHeight` / `OverallWidth` 를 실측해서 채운다.** BIM 도구가 문·창
  크기로 보여주는 값이라 `$` 로 두면 합법이지만 쓸모가 없다. 높이 = **Z
  extent**(Z-up 이라 모호하지 않음, LOCKED #43), 폭 = **큰 쪽 수평 extent**
  (작은 쪽은 판 두께). 이건 *의도 추론* 이 아니라 **사용자가 이미 만든 형상의
  측정** 이다 — 메타-원칙 #16 과 무관.
- **퇴화 형상은 `$`**: `IfcPositiveLengthMeasure` 는 0 이 불법이라, 0 을 실제
  치수처럼 쓰지 않고 생략한다.
- 9-arg 경로는 **손대지 않았다** — 기존 바이트 동일 테스트 그대로 통과.

### 만들면서 잡은 것

**테스트가 실패했는데 emitter 가 아니라 내 fixture 가 틀렸다.** 창을
`create_box(c, 1200, 100, 900)` 로 만들고 높이 0.9m 를 기대했는데 0.1m 가
나왔다 — `create_box(center, width→X, height→Z, depth→Y)` 라 내가 축을 잘못
알고 있었다. 코드를 고치기 전에 시그니처를 읽어서 **fixture 를** 고쳤다.

### 라이브 검증

| 층위 | 결과 |
|---|---|
| 엔진 회귀 | `IFCDOOR` **13 속성**, `IFCWALL` 은 **9 로 불변**; 창 = 높이 0.9 / 폭 1.2 (두께 0.1 아님) |
| 실제 앱 (:4188) | 픽커에 문·창 추가 → "문" 선택 → `#267=IFCDOOR(...,$,2.1,0.9,$,$,$)` **13 속성 + 실측 2.1×0.9 m**, 콘솔 오류 0 |
| 우리 왕복 | 분류기 `IFCDOOR` / `IFCWINDOW`, 분석기 doors 1 / windows 1, convertible 2/2 |
| **외부 커널 (web-ifc)** | `typed.ifc` 코퍼스에 문·창 추가 → **IfcDoor 1 / IfcWindow 1 / 벽 0**, 4 부재 **48 삼각형 삼각분할** |

부재 카운트 검사에 `IFCDOOR`/`IFCWINDOW` 가 빠져 있어 4개 중 2개만 세던 것도
같이 고쳤다.

### 회귀

- axia-ifc **+3** (104→107): 문·창 13속성 선언 + 나머지 11종 9 고정 /
  `IFCDOOR` 13 · `IFCWALL` 9 동시 검증 / 창의 실측 치수(두께가 아니라 폭).
- 워크스페이스 **3150 passed / 0 failed / 1 ignored**(doc fence, `#[ignore]`
  아님), vitest **2940**, tsc·build·ADR 카탈로그·외부 IFC 검증 모두 통과.

### 남은 것

- `PredefinedType`(`IfcDoorTypeEnum.DOOR` 등) · `OperationType`(여닫이 방향)
  · `PartitioningType` 은 아직 `$` — 엔진에 그 정보가 없다.
- 문·창을 **벽의 개구부에 연결**(`IfcRelFillsElement` / `IfcOpeningElement`)
  하지 않는다. 지금은 독립 부재다. Window·Door 도구가 만드는 실제 개구부와
  묶는 것은 별도 트랙.

---

## 22. I-3-arc Acceptance — 곡선 경계를 걷는다 (직선 현이 아니라)

I-3 은 `IfcEdgeCurve` 를 그 **두 끝점** 으로 읽었다. 기하가 `IfcCircle` (보통
`IfcTrimmedCurve` 로 감싼) 인 엣지는 호(arc) 인데, 끝점만 읽으면 **직선 현**
이 된다 — 면은 멀쩡히 들어오고 그럴듯해 보이지만 **틀린 형상** 이다. 아무
경고도 없으니 버려지는 것보다 나쁘다. 2-엣지 루프(지름을 잇는 반원)는 점이
2개뿐이라 아예 **퇴화로 버려졌다**.

- **`arc_interior_points`**: `IfcCircle` 을 읽어 호를 chord tolerance(0.02mm,
  렌더 값 LOCKED #40 과 동일)로 샘플링해 **끝점 사이 내부 점** 을 채운다.
  면은 이제 진짜 곡선을 따르는 매끈한 다각형으로 들어온다.
- **방향은 트림 점에서** (`trim_angle`): **끝점만으로는 어느 호인지 알 수
  없다** — 지름만큼 떨어진 두 점은 서로 다른 두 반원으로 이어진다. 오직
  `IfcTrimmedCurve` 의 `Trim1` / `Trim2` / `SenseAgreement` 만이 정확한 sweep 을
  정한다. 그래서 엣지 플래그로 방향을 *추측* 하지 않고 트림 점을 **읽는다**.
  나온 호는 루프의 start→end 순회 방향에 맞춰 정렬한다.
- **파라미터 fallback**: 트림이 `IfcCartesianPoint` (기하학적으로 정확) 면 그것을
  쓰고, 없으면 `IfcParameterValue` (원의 경우 라디안 각) 를 쓴다.

### 실파일로 검증 (`D:\AixiAcad\engine`, 외부 `AixxiA Engine` 이 만든 파일)

`advanced_brep_demo.ifc` — 이전엔 곡선 경계 면 1개를 통째로 버렸다(2면). 이제
**3면 전부** 들어오고 호가 매끈하게 렌더된다 (`{cap:2 annulus:1}`, 경고 0,
invariants valid). `untitled.ifc` (96면)는 호가 없어 영향 없음 — 그대로.

### 정직한 정정

처음엔 이 데모의 면 겹침(면적 합 26.4 > 외곽 22)을 **방향 버그** 로 봤다.
트림 점을 읽고 브렙이 **z=0 완전 평면·부피 0** 임을 확인한 뒤 정정한다 —
이 파일은 겹치는 3개 동일평면 면을 가진 **2D cut-circle 데모** 이고, 그 겹침은
파일 자체의 내용이지 임포트 오류가 아니다. 이 파일은 엣지 플래그가 이미 트림과
일치해서 수정 전후 기하가 동일하다. 수정의 가치는 *플래그로 방향이 결정되지
않는* 경우(반원)에 있고, 그것은 회귀로 증명했다.

### 회귀

- axia-ifc **+2** (107→109): 호가 현이 아니라 곡선으로 걸린다(내부 점이 모두
  r=1500 원 위) / **트림 sense 가 어느 반원인지 정한다** (같은 끝점, `.T.`→
  오른쪽 반원, `.F.`→왼쪽 반원 — 끝점·플래그로는 못 얻는 것). mutation 확인:
  트림 sense 를 무시(항상 CCW)하면 `.F.` 케이스가 실제로 실패한다.
- 워크스페이스 **3152 passed / 0 failed / 1 ignored**(doc fence), vitest **2940**,
  tsc·build 통과.

### 남은 한계

- **닫힌 원 자기-루프**(rim 전체가 한 self-loop 엣지, ADR-089 Path B)는 여전히
  점 하나로 붕괴 → 면 버림 + 경고. 이 수정은 **두 정점 사이의 열린 호** 만
  다룬다.
- 호는 다각형으로 테셀레이트된다 — 임포트된 엣지에 `AnalyticCurve::Arc` 를
  붙이는 것(진짜 kernel-native 재구성)은 별도 트랙.
- 타원(`IfcEllipse`) / 스플라인(`IfcBSplineCurve`) 경계는 아직 현으로 들어온다.

---

## 23. I-3-arc-closed Acceptance — 닫힌 원 (self-loop) 을 링으로

메모리에 "닫힌 원 self-loop 는 점으로 붕괴, 면 버림" 이라 적혀 있었다.
측정해 보니 **이미 들어온다** — §22 의 아크 수정이 `IfcEdgeCurve` 의
`EdgeStart == EdgeEnd`(자기 루프, 원 전체를 한 엣지로) 도 부수적으로 처리했다.
`sweep = a1 - a0 = 0` 이 아래의 `while sweep <= 1e-9 { sweep += TAU }` 로
굴러떨어져 한 바퀴가 됐다. 노트가 stale 이었다.

문제는 그게 **우연** 이었다는 점이다. 누가 "길이 0 아크 = skip" 가드를 넣으면
모든 닫힌 원이 조용히 깨진다. 그래서 이 작업의 산출물은 새 기능이 아니라
**우연을 의도로** 바꾸는 것이다.

- **`closed` 를 명시**: `arc_interior_points` 가 `start == end` 를 먼저 감지해
  full turn (`±TAU`) 을 sweep 하도록 했다. 동작은 동일(둘 다 TAU) 하지만 이제
  roll-over 우연에 의존하지 않는다.
- **회귀로 잠금** (이전엔 닫힌 원 임포트 테스트가 **하나도 없었다**):
  - `a_closed_circle_self_loop_becomes_a_full_ring` — bare 원 + trim-동일점 두
    형태 모두, 링(정점 다수) 이 되고 모든 점이 r=1500 위 + ±X·±Y 도달.
  - `a_circular_hole_self_loop_imports_as_an_inner_ring` — 원형 구멍이 inner
    링으로 (점 하나로 안 무너짐).
  - `an_open_arc_is_not_turned_into_a_full_circle` — 반대 방향 가드: 열린 호가
    닫힌 원으로 삼켜지지 않음.

### 실파일·왕복 검증

Path B 원 → 우리 export → 재-import: **1면 352정점, 면적 785356 ≈ π·500²**
(0.005% 오차), invariants valid. trimmed full circle (512정점, π·1500² 0.0025%)
+ 원형 구멍 (inner 링 512정점) 도 정상. `advanced_brep_demo.ifc` (열린 호) 는
그대로 3면 경고 0.

### 회귀

- axia-ifc **+3** (109→112). mutation 확인: 내부 아크 점을 안 내보내면
  (호=현) 5개 테스트가 실제로 실패한다.
- 워크스페이스 **3155 passed / 0 failed / 1 ignored**(doc fence), vitest **2940**,
  tsc·build·ADR 카탈로그 통과.

### 남은 한계

- 원은 **다각형으로 테셀레이트** (352~512정점) — 임포트 엣지에 `AnalyticCurve::
  Circle` 을 붙이는 진짜 kernel-native 재구성은 별도 트랙. 정점 수는 렌더
  tolerance (0.02mm) 와 동일 밀도.
- **타원 self-loop** (`IfcEllipse`) / 스플라인 self-loop 는 아직 점으로 붕괴 →
  면 버림. 이 작업은 `IfcCircle` self-loop 만.

---

## 24. I-3-spline Acceptance — 타원·스플라인 self-loop 을 걷는다

곡선 임포트의 마지막 구멍: Bezier / B-spline / NURBS, 그리고 **타원**. 측정해
보니 우리 exporter (그리고 대부분의 도구) 는 이들을 전부
`IfcBSplineCurveWithKnots` (weight 있으면 `RATIONAL` 형) 로 쓴다 — 타원조차
`IFCRATIONALBSPLINECURVEWITHKNOTS` (ADR-158, 타원 = NURBS). 자기 루프
(`EdgeStart == EdgeEnd`) 로 오면 정점 하나로 붕괴 → 면 **통째로 버려짐**, 게다가
**경고도 없었다**.

- **`spline_interior_points`**: 엣지 기하가 `IfcBSplineCurveWithKnots` /
  `RATIONAL` 이면 degree · control points · **distinct knots + multiplicities**
  (exporter 의 `compress_knots` 역함수로 flat vector 복원) · (rational 이면)
  weights 를 읽어 **엔진의 테셀레이터** (`axia_geo::curves::bspline` /
  `nurbs`) 로 건다. 임포트된 스플라인이 그려진 스플라인과 **정확히 같은 밀도** 로
  샘플링된다 (SSOT).
- **자기 루프 = 전체 링**: 열린 스플라인은 start↔end 매칭으로 방향 정렬, 닫힌
  것은 전체 곡선을 emit.
- **malformed 는 거부**: `knots.len() != control.len() + degree + 1` 이거나
  weights 길이 불일치면 `None` → 면은 surface 없이 남지 조작된 형상을
  만들지 않는다.

### 실파일·시각 검증

네 곡선족 모두 export → re-import 왕복 (전부 `faces=1 valid viol=0`):
- **Ellipse 800×400**: bbox 정확 `[-800,-400,0]~[800,400,0]`, 실제 앱에서
  매끈한 타원 면으로 렌더 (이전엔 버려짐).
- closed Bezier / BSpline / NURBS: 전부 링, invariants valid.

### 정점 밀도 (정직하게)

임포트된 스플라인은 **4096정점** (엔진의 bspline 테셀레이터가 0.02mm 에서
`init_n` cap 4096 에 도달). 원본 엔진이 같은 타원을 렌더할 때도 **4097정점** —
즉 import 는 엔진 자체 밀도와 **일치** (SSOT). 원 (352) 과 차이나는 건 엔진이
원엔 별도 (원 전용) 경로를 쓰기 때문이지 import 결정이 아니다. 밀도를 낮추면
그려진 곡선과 임포트된 곡선이 달라져 SSOT 가 깨진다 — 유지.

### 회귀

- axia-ifc **+1** (112→113): `a_closed_spline_self_loop_becomes_a_ring` —
  엔진으로 닫힌 bspline/nurbs 면을 만들어 emit→import, plain·rational 두 형태
  모두 링이 되고(붕괴 X) X·Y 로 퍼지며 control hull 밖으로 안 튄다. mutation
  확인: 스플라인 walk 를 없애면 실제로 실패한다.
- 워크스페이스 **3156 passed / 0 failed / 1 ignored**(doc fence), vitest **2940**,
  tsc·build·ADR 카탈로그 통과.

### 🎉 곡선 경계 임포트 완결 (다각형 tessellation 수준)

| 곡선 | import |
|---|---|
| 열린 호 (Circle/Arc) | ✅ §22 (방향 트림 점) |
| 닫힌 원 self-loop | ✅ §23 (전체 링) |
| **타원 / Bezier / BSpline / NURBS self-loop** | ✅ §24 (엔진 테셀레이터) |

### 남은 한계

- 전부 **다각형으로 tessellate** — 임포트 엣지에 `AnalyticCurve` 를 붙이는 진짜
  kernel-native 재구성 (원 = 1 self-loop, 스플라인 = curve metadata) 은 별도
  트랙. 지금은 렌더 밀도 (352~4096정점) 가 DCEL 경계로 구워진다.
- 외부 파일의 `IFCELLIPSE` (bspline 아닌 직접 형) / `IFCPOLYLINE` 곡선 경계는
  아직 미지원 — 우리 파일은 전부 bspline 형이라 해당 없음.

---

## 25. I-3-kernel-native Acceptance — 임포트된 곡선을 진짜 곡선으로

§22–§24 는 곡선 경계를 임포트했지만 **다각형** 으로 — 원 352정점, 타원·스플라인
4096정점이 DCEL 경계로 구워졌다. 그려진 곡선 (Path B: 1 anchor + 1 self-loop
edge + `AnalyticCurve`) 과 임포트된 곡선이 서로 다른 것이었다. 이제 같아진다.

- **`FaceLoops.closed_curve: Option<AnalyticCurve>`**: 면이 **단일 닫힌곡선
  disk** (bound 1개 · edge loop 1개 · self-loop edge 1개 · 홀 없음) 이면
  `single_closed_curve` 가 정확한 `AnalyticCurve` 를 만든다 — `IFCCIRCLE` →
  Circle, `IFCBSPLINECURVEWITHKNOTS` → BSpline, `RATIONAL` → NURBS (타원 포함).
  `parse_bspline` 는 §24 의 파싱을 공유 (tessellation 과 curve 재구성 둘 다).
- **`importIfc` kernel-native 분기**: `closed_curve` 가 Some 이면 anchor 1개 +
  `add_face_closed_curve` (그려진 곡선과 **같은 엔진 API**). 실패하면 폴리곤
  경로로 그대로 fall-through — 안전.
- **철저히 additive**: `closed_curve` 는 곡선 disk 에만 Some. 박스·홀 있는 면·
  다중 엣지 경계는 전부 None → 폴리곤 경로 **무변경** (회귀 검증: 박스 6면/
  8정점, 실파일 96면·3면 그대로).
- **비-identity 배치는 폴리곤**: 곡선은 placement 로 이동 안 하므로, 배치가
  identity 아니면 `closed_curve = None` 으로 (이미 이동된) 폴리곤 사용.

### 왕복 검증 (실측)

| 곡선 | 이전 (다각형) | 이제 (kernel-native) | 재-export |
|---|---|---|---|
| Circle r500 | 352정점 | **1정점** valid | `IFCCIRCLE` |
| Ellipse 800×400 | 4096정점 | **1정점** valid | `IFCRATIONALBSPLINE…` |
| closed BSpline | 4096정점 | **1정점** valid | `IFCBSPLINE…` |
| closed NURBS | 4096정점 | **1정점** valid | `IFCRATIONALBSPLINE…` |

임포트된 원/타원/스플라인이 **그려진 것과 동일** (1 self-loop + AnalyticCurve),
재-export 가 원래 곡선 엔티티를 그대로 생성, 실제 앱에서 매끈하게 렌더.
Push/Pull·Boolean 등 kernel op 가 다각형 근사가 아닌 **정확한 곡선** 위에서 동작.

### 회귀

- axia-ifc **+1** (113→114): `a_curve_disk_carries_its_exact_curve_a_box_does_
  not` — 원 disk → `Circle` (반지름 보존), rational 스플라인 disk → `NURBS`,
  박스 면 → 전부 None (kernel-native 경로가 일반 형상으로 새지 않음). mutation
  확인: 감지를 끄면 실패.
- axia-wasm step6 **+1**: `importIfc` 가 `closed_curve` 를 읽고
  `add_face_closed_curve` 를 쓰며 폴리곤 경로도 유지하는지 source guard.
- 워크스페이스 **3158 passed / 0 failed / 1 ignored**(doc fence), vitest **2940**,
  tsc·build·ADR 카탈로그 통과.

### 🎉 곡선 임포트 완전 완결

| | import |
|---|---|
| 열린 호 (Circle/Arc) | ✅ §22 (트림 점 방향) |
| 닫힌 원 self-loop | ✅ §23 → **§25 kernel-native** |
| 타원 / spline self-loop | ✅ §24 → **§25 kernel-native** |
| 폴리곤 곡면 경계 (홀 등) | ✅ §22–§24 (다각형, 정확) |

### 남은 한계

- **홀·다중 엣지 경계의 곡선** 은 여전히 다각형 (kernel-native 는 단일
  closed-curve disk 전용 — `add_face_closed_curve` 의 계약). 곡선 홀의
  kernel-native 화는 별도 트랙.
- 외부 파일의 bare `IfcEllipse` / `IfcPolyline` 형은 미지원 (우리 파일은 전부
  bspline 형).
