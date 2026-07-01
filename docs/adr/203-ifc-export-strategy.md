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
