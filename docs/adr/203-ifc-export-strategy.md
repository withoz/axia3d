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
