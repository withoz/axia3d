//! **Boundary Kernel** (ADR-186) — Production-grade 2D planar subdivision.
//!
//! AixiAcad `xia-form/src/boundary_kernel` (ADR-057 유도면 모델 / Derived-Face
//! Model) 의 port. 우리 AxiA 3D 엔진을 *저장-후-패치* (incremental DCEL) 에서
//! **edge graph 단일 진실원천 + 면 재유도** 모델로 전환하는 핵심 인프라.
//!
//! ## 사상
//!
//! ```text
//! Loop Input → Edge Graph → Intersection Resolve → Planar Partition → Face Reconstruction
//! ```
//!
//! 단일 알고리즘이 모든 닫힌 boundary 케이스 처리 (Rectangle / Line cycle /
//! Circle / BOUNDARY). 면은 *저장* 하지 않고 edge graph 의 **함수** 로 매번
//! 유도 → 생성·분할·병합·삭제가 *구조적으로 보장* (case-by-case 패치 불필요).
//!
//! ## 모듈 구조 (port 진행)
//!
//! - [`geom2`] — 2D 기하 primitives (Vec2, segment intersection, point-in-polygon).
//!   **β-1 완료 (ADR-186 Phase 3)**.
//! - [`planar`] — PlanarGraph (양자화 weld) + Lineage (edge split 추적) +
//!   dedup_parallel_edges (error01 과병합 fix). **β-2 완료**.
//! - [`region`] — Half-edge 기반 region (face) 추출 (self-touching split) +
//!   containment nesting (`extract_regions_nested` → annulus+disk). **β-3 완료**.
//! - [`bentley_ottmann`] — robust sweep-line intersection resolve
//!   (O((N+K)logN), closed-chain self-intersect 정확). **β-4 완료**.
//! - [`robust_split`] — 3-branch classification (shares_edge / all_inside /
//!   fluke) → material/surface 상속 + `robust_split_2d` entry. **β-4 완료**.
//!
//! ## 정합 (ADR-035)
//!
//! 외부 deps 0 — 표준 라이브러리만. Deterministic kernel invariant (BTreeMap).
//! kernel 내부는 자체 [`geom2::Vec2`] 사용 (외부 인터페이스 glam `DVec3` 는 DCEL
//! 통합 단계 Phase 4 에서 plane projection 으로 변환).

pub mod analytic_arrange;
pub mod bentley_ottmann;
pub mod geom2;
pub mod planar;
pub mod region;
pub mod robust_split;

pub use analytic_arrange::{arrange, ArrFace, Freeform2D, InputCurve, SubCurve};
pub use bentley_ottmann::{bentley_ottmann_resolve, find_intersections_by_sweep, Intersection};
pub use geom2::{eps_from_scale, orient2d_sign, Pip, SegIsect, Vec2};
pub use planar::{Edge, EdgeId, Lineage, PlanarGraph, Vertex};
pub use region::{extract_regions, extract_regions_nested, Region, RegionWithHoles};
pub use robust_split::{resolve_and_extract_nested, robust_split_2d, DirtyFaceInfo, FaceOut};
